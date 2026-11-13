"""Throwaway prototype: continuous-flow redline of two PDFs as a side-by-side PDF.

    python redline_proto.py OLD.pdf NEW.pdf -o REDLINE.pdf [--header PT] [--footer PT] [--scale S]

Answers three feasibility questions on real documents: how clean a continuous
word diff gets with region and regex filters, whether the original pages can be
embedded as vectors with highlights and connectors, and how long it all takes.
"""

from __future__ import annotations

import argparse
import bisect
import json
import re
import subprocess
import tempfile
import time
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path

import pymupdf

IGNORE_PATTERNS = [
    r"\(p\.\s*\d+\)",                      # page references
    r"(?m)(?:\.\s*){4,}⁠?\s*\d+$",    # table of contents leaders and page numbers
    r"(?m)^\d+(?:\.\d+)+(?=\s)",           # leading section numbers
    r"(?:Figure|Table)\s+\d+(?:\.\d+)*",   # figure and table numbers
]
MOVE_MIN_TOKENS = 6
INVISIBLE = dict.fromkeys(map(ord, "⁠­​﻿"))

DELETED, INSERTED, MOVED = (0.85, 0.15, 0.15), (0.1, 0.6, 0.2), (0.15, 0.35, 0.85)


@dataclass
class Token:
    page: int
    rect: pymupdf.Rect
    text: str
    line: tuple[int, int, int]
    ignored: bool = False

    @property
    def key(self) -> str:
        return unicodedata.normalize("NFKC", self.text.translate(INVISIBLE))


@dataclass
class Side:
    """One document's part of a change: the tokens, or where they are missing."""
    tokens: list[Token]
    anchor: Token | None  # the token before the change, for a side with no tokens

    def pages(self) -> list[int]:
        if self.tokens:
            return sorted({t.page for t in self.tokens})
        return [self.anchor.page] if self.anchor else [0]

    def line_rects(self, page: int) -> list[pymupdf.Rect]:
        rects: dict[tuple, pymupdf.Rect] = {}
        for token in self.tokens:
            if token.page == page:
                rects[token.line] = rects.get(token.line, pymupdf.Rect(token.rect)) | token.rect
        return list(rects.values())

    def span(self, page: int) -> tuple[float, float] | None:
        """Vertical extent on page, in page coordinates."""
        rects = self.line_rects(page)
        if rects:
            return min(r.y0 for r in rects), max(r.y1 for r in rects)
        if not self.tokens and self.anchor and self.anchor.page == page:
            return self.anchor.rect.y1, self.anchor.rect.y1
        return None

    def text(self) -> str:
        return " ".join(t.text for t in self.tokens)


@dataclass
class Change:
    old: Side
    new: Side
    moved: bool = False
    sheets: list[int] = field(default_factory=list)

    @property
    def color_old(self): return MOVED if self.moved else DELETED
    @property
    def color_new(self): return MOVED if self.moved else INSERTED


def extract(doc: pymupdf.Document, header: float, footer: float) -> list[Token]:
    tokens = []
    for page in doc:
        bottom = page.rect.height - footer
        for x0, y0, x1, y1, text, block, line, _ in page.get_text("words", sort=False):
            if y1 > header and y0 < bottom:
                tokens.append(Token(page.number, pymupdf.Rect(x0, y0, x1, y1), text, (page.number, block, line)))
    _mark_ignored(tokens)
    return tokens


def _mark_ignored(tokens: list[Token]) -> None:
    parts, starts, offset = [], [], 0
    for index, token in enumerate(tokens):
        separator = "" if index == 0 else (" " if token.line == tokens[index - 1].line else "\n")
        offset += len(separator)
        starts.append(offset)
        parts.append(separator + token.text)
        offset += len(token.text)
    text = "".join(parts)
    for pattern in IGNORE_PATTERNS:
        for match in re.finditer(pattern, text):
            first = bisect.bisect_right(starts, match.start()) - 1
            for index in range(first, len(tokens)):
                if starts[index] >= match.end():
                    break
                if starts[index] + len(tokens[index].text) > match.start():
                    tokens[index].ignored = True


def diff(old: list[str], new: list[str]) -> list[tuple[int, int, int, int]]:
    """Changed ranges (old start, old end, new start, new end), by git's histogram diff."""
    with tempfile.TemporaryDirectory() as scratch:
        paths = [Path(scratch, name) for name in ("old", "new")]
        for path, keys in zip(paths, (old, new)):
            path.write_text("".join(key + "\n" for key in keys), encoding="utf-8", newline="\n")
        output = subprocess.run(
            ["git", "diff", "--no-index", "--histogram", "-U0", *map(str, paths)],
            capture_output=True, text=True, encoding="utf-8").stdout
    ranges = []
    for match in re.finditer(r"(?m)^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", output):
        old_start, old_count, new_start, new_count = (int(g) if g is not None else 1 for g in match.groups())
        old_start -= old_count > 0
        new_start -= new_count > 0
        ranges.append((old_start, old_start + old_count, new_start, new_start + new_count))
    return ranges


def changes(old_tokens: list[Token], new_tokens: list[Token]) -> list[Change]:
    old = [t for t in old_tokens if not t.ignored]
    new = [t for t in new_tokens if not t.ignored]
    found = []
    for old_start, old_end, new_start, new_end in diff([t.key for t in old], [t.key for t in new]):
        found.append(Change(
            Side(old[old_start:old_end], old[old_start - 1] if old_start else None),
            Side(new[new_start:new_end], new[new_start - 1] if new_start else None)))
    _mark_moves(found)
    return found


def _mark_moves(found: list[Change]) -> None:
    def keys(side: Side) -> tuple[str, ...]:
        return tuple(t.key for t in side.tokens)

    deletions = {keys(c.old): c for c in found if not c.new.tokens and len(c.old.tokens) >= MOVE_MIN_TOKENS}
    for change in found:
        if not change.old.tokens and keys(change.new) in deletions:
            change.moved = deletions[keys(change.new)].moved = True


def sheet_pairs(found: list[Change]) -> list[tuple[int, int]]:
    """The (old page, new page) pairs that together show both sides of every change."""
    pairs = set()
    for change in found:
        old_pages, new_pages = change.old.pages(), change.new.pages()
        for index in range(max(len(old_pages), len(new_pages))):
            pairs.add((old_pages[min(index, len(old_pages) - 1)], new_pages[min(index, len(new_pages) - 1)]))
    return sorted(pairs, key=lambda pair: (pair[1], pair[0]))


class Renderer:
    MARGIN, GUTTER, BANNER = 18, 70, 34

    def __init__(self, old_doc, new_doc, old_name, new_name, scale):
        self.old_doc, self.new_doc, self.old_name, self.new_name, self.scale = old_doc, new_doc, old_name, new_name, scale
        size = new_doc[0].rect
        self.page_w, self.page_h = size.width * scale, size.height * scale
        self.sheet_w = 2 * self.MARGIN + 2 * self.page_w + self.GUTTER
        self.sheet_h = self.BANNER + self.page_h + self.MARGIN
        self.left = pymupdf.Rect(self.MARGIN, self.BANNER, self.MARGIN + self.page_w, self.BANNER + self.page_h)
        self.right = self.left + (self.page_w + self.GUTTER, 0, self.page_w + self.GUTTER, 0)
        self.out = pymupdf.open()

    def _place(self, frame: pymupdf.Rect, rect: pymupdf.Rect) -> pymupdf.Rect:
        return pymupdf.Rect(*(v * self.scale for v in rect)) + (frame.x0, frame.y0, frame.x0, frame.y0)

    def sheet(self, old_page: int, new_page: int, found: list[Change], skipped: str) -> None:
        page = self.out.new_page(width=self.sheet_w, height=self.sheet_h)
        page.show_pdf_page(self.left, self.old_doc, old_page)
        page.show_pdf_page(self.right, self.new_doc, new_page)
        for frame, name, number, total in ((self.left, self.old_name, old_page, len(self.old_doc)),
                                           (self.right, self.new_name, new_page, len(self.new_doc))):
            page.draw_rect(frame, color=(0.6, 0.6, 0.6), width=0.5)
            page.insert_text((frame.x0, 14), f"{name}  -  page {number + 1} of {total}", fontsize=9, color=(0.2, 0.2, 0.2))
        if skipped:
            page.insert_text((self.left.x0, 27), skipped, fontsize=8, color=(0.45, 0.45, 0.45))

        for change in found:
            spans = []
            for frame, side, page_number, color in ((self.left, change.old, old_page, change.color_old),
                                                    (self.right, change.new, new_page, change.color_new)):
                shape = page.new_shape()
                for rect in side.line_rects(page_number):
                    shape.draw_rect(self._place(frame, rect))
                shape.finish(fill=color, color=None, fill_opacity=0.28)
                shape.commit()
                span = side.span(page_number)
                spans.append(None if span is None else (frame.y0 + span[0] * self.scale, frame.y0 + span[1] * self.scale))
            if spans[0] and spans[1]:
                self._connector(page, spans[0], spans[1], MOVED if change.moved else (0.55, 0.35, 0.8))
                change.sheets.append(page.number)

    def _connector(self, page, old_span, new_span, color) -> None:
        x0, x1 = self.left.x1 + 3, self.right.x0 - 3
        middle = (x0 + x1) / 2
        shape = page.new_shape()
        shape.draw_bezier((x0, old_span[0]), (middle, old_span[0]), (middle, new_span[0]), (x1, new_span[0]))
        shape.draw_line((x1, new_span[0]), (x1, new_span[1] + 0.5))
        shape.draw_bezier((x1, new_span[1] + 0.5), (middle, new_span[1] + 0.5), (middle, old_span[1] + 0.5), (x0, old_span[1] + 0.5))
        shape.finish(fill=color, color=color, width=0.6, fill_opacity=0.22, closePath=True)
        shape.commit()

    def summary(self, found: list[Change], seconds: float) -> None:
        rows_per_page = int((self.sheet_h - 110) // 13)
        pages = -(-len(found) // rows_per_page) or 1
        for number in range(pages):
            page = self.out.new_page(pno=number, width=self.sheet_w, height=self.sheet_h)
            if number == 0:
                page.insert_text((self.MARGIN, 36), f"Redline: {self.old_name}  ->  {self.new_name}", fontsize=16)
                moved = sum(c.moved for c in found) // 2
                page.insert_text((self.MARGIN, 56),
                    f"{len(found)} changes ({moved} moved blocks) on {len(self.out) - 1} sheets; "
                    f"red = removed, green = added, blue = moved; computed in {seconds:.1f} s", fontsize=9)
            y = 84
            for index in range(number * rows_per_page, min(len(found), (number + 1) * rows_per_page)):
                change = found[index]
                kind = "moved" if change.moved else "added" if not change.old.tokens else "removed" if not change.new.tokens else "changed"
                where = f"old p.{change.old.pages()[0] + 1} / new p.{change.new.pages()[0] + 1}"
                detail = " | ".join(part for part in ("- " + change.old.text()[:95] if change.old.tokens else "",
                                                      "+ " + change.new.text()[:95] if change.new.tokens else "") if part)
                page.insert_text((self.MARGIN, y), f"{index + 1:>4}  {where:<26} {kind:<8} {detail}", fontsize=7.5, fontname="cour")
                if change.sheets:
                    page.insert_link({"kind": pymupdf.LINK_GOTO, "page": change.sheets[0] + pages, "to": pymupdf.Point(0, 0),
                                      "from": pymupdf.Rect(self.MARGIN, y - 9, self.sheet_w - self.MARGIN, y + 3)})
                y += 13


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("old", type=Path)
    parser.add_argument("new", type=Path)
    parser.add_argument("-o", "--output", type=Path, default=Path("redline.pdf"))
    parser.add_argument("--header", type=float, nargs="+", default=[0], help="height in points of the top region to ignore: one value, or old then new")
    parser.add_argument("--footer", type=float, nargs="+", default=[0], help="height in points of the bottom region to ignore: one value, or old then new")
    parser.add_argument("--scale", type=float, default=1.0, help="scale of the embedded pages")
    parser.add_argument("--json", type=Path, help="also write the changes as JSON")
    args = parser.parse_args()

    started = time.perf_counter()
    old_doc, new_doc = pymupdf.open(args.old), pymupdf.open(args.new)
    old_tokens = extract(old_doc, args.header[0], args.footer[0])
    new_tokens = extract(new_doc, args.header[-1], args.footer[-1])
    extracted = time.perf_counter()
    found = changes(old_tokens, new_tokens)
    compared = time.perf_counter()

    renderer = Renderer(old_doc, new_doc, args.old.name, args.new.name, args.scale)
    previous_new_page = -1
    for old_page, new_page in sheet_pairs(found):
        skipped = ""
        if new_page - previous_new_page > 1:
            skipped = f"Pages {previous_new_page + 2}-{new_page} of {args.new.name} have no differences"
        on_sheet = [c for c in found if old_page in c.old.pages() or new_page in c.new.pages()]
        renderer.sheet(old_page, new_page, on_sheet, skipped)
        previous_new_page = new_page
    renderer.summary(found, time.perf_counter() - started)
    renderer.out.save(args.output, garbage=3, deflate=True)
    finished = time.perf_counter()

    if args.json:
        args.json.write_text(json.dumps([
            {"moved": c.moved, "old_pages": [p + 1 for p in c.old.pages()], "new_pages": [p + 1 for p in c.new.pages()],
             "old": c.old.text(), "new": c.new.text()} for c in found], indent=1, ensure_ascii=False), encoding="utf-8")
    print(f"tokens old/new: {len(old_tokens)}/{len(new_tokens)}; ignored: {sum(t.ignored for t in old_tokens)}/{sum(t.ignored for t in new_tokens)}")
    print(f"changes: {len(found)}; sheets: {len(renderer.out)}")
    print(f"extract {extracted - started:.1f} s, compare {compared - extracted:.1f} s, render {finished - compared:.1f} s, total {finished - started:.1f} s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
