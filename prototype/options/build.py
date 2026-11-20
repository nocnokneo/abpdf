#!/usr/bin/env python3
"""PROTOTYPE: builds what dashboard.html shows.

Compiles the example documents, compares them with each option switched on,
and cuts the pictures out of the redlines.
Run from the root of the worktree, after `cargo build --release`:

    mise exec -- python3 prototype/options/build.py

The manual pair is taken from `target/corpus` (`mise run manual`), if it is
there.
"""

import os
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = HERE / "out"
ABPDF = ROOT / "target/release/abpdf"

# Where a redline puts its pages on a sheet, in points.
LEFT_PAGE_X = 17.7
PAGE_Y = 34.3
CONNECTOR_WIDTH = 70.0


def run(*command, env=None, check=True):
    environment = dict(os.environ, **(env or {}))
    return subprocess.run(command, env=environment, check=check, capture_output=True, text=True)


def compile_document(name, tagged=True):
    for revision in ("old", "new"):
        suffix = "" if tagged else "-untagged"
        flags = [] if tagged else ["--no-pdf-tags"]
        run("typst", "compile", *flags, "--input", f"revision={revision}",
            HERE / f"{name}.typ", OUT / f"{name}-{revision}{suffix}.pdf")


def compare(output, old, new, *arguments, env=None):
    """Writes the redline, the JSON and the listing of a comparison."""
    result = run(ABPDF, old, new, *arguments, "-o", OUT / f"{output}.pdf",
                 "--json", OUT / f"{output}.json", env=env, check=False)
    (OUT / f"{output}.txt").write_text(result.stdout)
    return result.stdout.strip().splitlines()[-1]


def crop(redline, sheet, name, left, top, right, bottom, dpi):
    """Cuts the rectangle, given in points on the sheet, out of a sheet."""
    scale = dpi / 72
    run("pdftoppm", "-r", str(dpi), "-f", str(sheet), "-l", str(sheet), "-singlefile",
        "-x", str(round(left * scale)), "-y", str(round(top * scale)),
        "-W", str(round((right - left) * scale)), "-H", str(round((bottom - top) * scale)),
        "-png", OUT / f"{redline}.pdf", OUT / name)


def sheet_showing(redline, label):
    """The number of the sheet of a redline that has `label` on it."""
    info = run("pdfinfo", OUT / f"{redline}.pdf", check=False).stdout
    page_count = int(next(line.split()[1] for line in info.splitlines() if line.startswith("Pages")))
    for sheet in range(1, page_count + 1):
        text = run("pdftotext", "-f", str(sheet), "-l", str(sheet), OUT / f"{redline}.pdf", "-").stdout
        if label in text:
            return sheet
    raise SystemExit(f"{redline} has no sheet with {label!r}")


def left_out_text():
    compile_document("leftout")
    old, new = OUT / "leftout-old.pdf", OUT / "leftout-new.pdf"
    profile = ["--profile", HERE / "leftout.toml"]
    split = {"ABPDF_PROTOTYPE_SPLIT_AREAS": "1"}
    compare("leftout-today", old, new, *profile)
    compare("leftout-split", old, new, *profile, env=split)
    compare("leftout-today-audit", old, new, *profile, "--audit")
    compare("leftout-split-audit", old, new, *profile, "--audit", env=split)
    for redline in ("leftout-today", "leftout-split", "leftout-today-audit", "leftout-split-audit"):
        # The header and the two paragraphs of the old page.
        crop(redline, 2, f"{redline}-old", LEFT_PAGE_X + 28, PAGE_Y + 18,
             LEFT_PAGE_X + 312, PAGE_Y + 118, dpi=190)


def table_cells():
    compile_document("tables")
    old, new = OUT / "tables-old.pdf", OUT / "tables-new.pdf"
    compare("tables-today", old, new)
    compare("tables-cells", old, new, env={"ABPDF_PROTOTYPE_CELLS": "1"})
    for redline in ("tables-today", "tables-cells"):
        # The first table of the old page and the connectors next to it.
        crop(redline, 2, f"{redline}-rows", 70, 122, 506, 345, dpi=230)


def figures():
    compile_document("figures")
    compile_document("figures", tagged=False)
    old, new = OUT / "figures-old.pdf", OUT / "figures-new.pdf"
    new_page_x = LEFT_PAGE_X + 419.5 + CONNECTOR_WIDTH
    rows = {
        "image": (110, 189),
        "vector": (201, 285),
        "diagram": (311, 377),
        "label": (393, 459),
        "unchanged": (468, 537),
        "decoration": (535, 555),
    }
    for mode in ("none", "grouped", "all"):
        redline = f"figures-{mode}"
        compare(redline, old, new, env={"ABPDF_PROTOTYPE_FIGURES": mode})
        compare(f"{redline}-untagged", OUT / "figures-old-untagged.pdf",
                OUT / "figures-new-untagged.pdf", env={"ABPDF_PROTOTYPE_FIGURES": mode})
        for row, (top, bottom) in rows.items():
            # The line of text is cut shorter, so that it is shown larger.
            width = 250 if row == "decoration" else 340
            crop(redline, 2, f"{redline}-{row}", new_page_x + 40, top,
                 new_page_x + 40 + width, bottom, dpi=160)
    crop("figures-grouped", 2, "figures-grouped-sheet", 0, 34, 941, 570, dpi=150)


def manual():
    corpus = ROOT / "target/corpus"
    old, new = corpus / "manual-old.pdf", corpus / "manual-new.pdf"
    if not old.exists():
        print("no manual pair in target/corpus: skipped")
        return
    profile = ["--profile", ROOT / "tests/corpus/manual.toml"]
    for mode in ("none", "grouped", "all"):
        count = compare(f"manual-{mode}", old, new, *profile, env={"ABPDF_PROTOTYPE_FIGURES": mode})
        print(f"manual, figures {mode}: {count}")
    sheet = sheet_showing("manual-all", "page 122 of")
    # The diagram at the foot of the new page.
    crop("manual-all", sheet, "manual-all-diagram", 800, 752, 1235, 870, dpi=220)


def main():
    OUT.mkdir(exist_ok=True)
    left_out_text()
    table_cells()
    figures()
    manual()


if __name__ == "__main__":
    main()
