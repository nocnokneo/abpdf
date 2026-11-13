# Existing open-source tools for redline PDFs: gap analysis

Research date: 2026-11-12.
Scope: can an existing FOSS tool replace i-net PDFC for release redlines of our Typst-generated controlled documents (largest: ~650-page SDD)?
Method: repository metadata via the GitHub, GitLab, PyPI and crates.io APIs, reading READMEs and the comparison source code of the serious candidates, vendor pages for the commercial reference tools.
No candidate was executed in this session, so all performance and output-quality statements are either quoted claims or code-reading estimates and are labelled as such.
"Unverified" marks anything not confirmed from a primary source.

## Verdict

**None of the existing tools is close enough to adopt as-is.**
Every candidate fails at least two hard requirements for our documents, most often header/footer noise, missing regex filters, and unproven scale.

- **Closest: redlatex** (MIT, Python).
  It has the right core architecture: continuous word diff over the whole document with near-linear anchoring, vector side-by-side spreads, and figure change detection.
  It lacks header/footer region exclusion, regex filters, style comparison, "only changed pages", connectors and banners.
  Its core is one ~26 KB module by one author, 4 weeks old, with no external contributors yet.
  Contributing upstream is possible, since the project invites pull requests and asks for an issue first on large changes, but nearly all our features would be new.
  In practice the choice is between a **hard fork of redlatex** and a **new tool that reuses its MIT-licensed algorithm**, and those cost about the same.
- **Best-maintained host: PDF4QT** (MIT, C++/Qt, active, responsive maintainer).
  It already has a `PdfTool diff` CLI and continuous text comparison within changed page ranges.
  But its text diff is a full O(n·m) LCS matrix, which by my estimate cannot handle a 650-page document whose page headers change on every page.
  The CLI emits only a difference list, not a PDF.
  The GUI's PDF export concatenates the two documents with annotations instead of laying out side-by-side sheets.
  Contributing the missing pieces upstream would be a substantial C++ project in a single-maintainer codebase.
- **Everything else** is either page-by-page (raster or text), text-only with no PDF output, browser-only, or source-level.

Recommendation: build a new FOSS CLI, or hard-fork redlatex, using redlatex's anchored diff and spread rendering as the starting design.
The candidates were deliberately not run: most are new single-author packages.
Consider pypdfium2 (Apache-2.0/BSD) plus pypdf (BSD) instead of PyMuPDF (AGPL-3.0) if the new tool should be permissively licensed (see [License trap](#license-trap-pymupdf)).

## Requirement keys used in the tables

| Key | Requirement (from the brief) |
| --- | --- |
| R1 | Continuous text-flow comparison; reflow onto later pages does not mark those pages changed |
| R2 | Side-by-side landscape output PDF, original vector pages preserved |
| R3 | Highlights on both sides joined by a marker/connector |
| R4 | "Pages x–y have no differences" banner, or option to export only changed pages |
| R5 | Fixed-height header/footer region exclusion |
| R6 | Regex ignore filters (page refs, TOC leaders, section numbers, figure numbers) |
| R7 | Text style comparison (font, size, colour, style, rotation) |
| R8 | Image and vector-graphics comparison |
| R9 | CLI, no server, Windows and Linux, offline |
| R10 | 650 pages in minutes |
| R11 | Deterministic output |
| R12 | Free/open-source license |

## Ranked top candidates

### Gap table

Legend: **Yes** = meets it, **Part** = partially, **No** = missing, **?** = unknown/unverified.

| Req | 1. redlatex 0.2.0 | 2. PDF4QT 1.7.0.0 | 3. kogo 0.1.7 | 4. pdf-content-diff 0.7.0 | 5. JoshData/pdf-diff 0.9.3 |
| --- | --- | --- | --- | --- | --- |
| R1 | Yes: whole-document anchored word diff [1a] | Part: continuous only inside runs of non-identical pages [2a] | Part: document-level after page alignment, falls back to per-page-pair on large residual diffs [3a] | Yes: whole-document `difflib` token diff [4a] | Yes: whole-document text diff [5a] |
| R2 | Yes: vector spreads via `show_pdf_page` [1b] | Part: GUI export only, concatenated pages plus annotations, CLI gives no PDF [2b] | Part: writes a `side-by-side.pdf` [3b]; vector or raster unverified | Yes: vector, landscape, common scale [4b] | No: one large PNG [5a] |
| R3 | Part: fills, margin bars, carets; no connector [1c] | Part: coloured polygons; no connector [2b] | Part: red/green highlights; no connector [3b] | Part: red/yellow highlights; no connector [4b] | Part: red boxes on PNG [5a] |
| R4 | Part: cover page with per-section change counts; every new page gets a spread [1d] | Part: GUI "show pages with differences" filter [2c] | ? | Yes: only changed pages; single "no changes" page [4b] | Part: renders changed pages only [5a] |
| R5 | Part: drops all-digit lines in bottom 14% of page [1e] | No | No | Part: margin line numbers and centred page numbers [4b] | No |
| R6 | No | No | No | No | No |
| R7 | No: "colours, fonts or spacing are not marked" [1c] | No (text compared as strings) [2d] | Yes: bold, italic, font size [3b] | No [4b] | No |
| R8 | Part: per-figure pixel fingerprint, framed if changed [1c] | Yes, but positional per matched page, so reflowed figures likely flag [2a] | Yes, pixel diff per aligned page pair with text masked [3b] | Part: vector paths yes, raster images no [4b] | No |
| R9 | Yes: `pip`, pure Python plus PyMuPDF wheels [1f] | Part: CLI exists; Windows zip/MSI, Linux AppImage/Flatpak [2e] | Yes: `pip` [3b] | Part: script plus `requirements.txt`, not on PyPI [4b] | Part: needs poppler `pdftotext`/`pdftoppm` on PATH [5a] |
| R10 | ? Claims "200-page document … in a few seconds" [1c]; algorithm near-linear [1a] | No (estimate): O(n·m) bit matrix ≈ 8 GB for 650 pages [2d] | ? `--max-pages` ≤ 2000 [3c]; renders every page for pixel diff | ? quadratic worst case (`autojunk=False`) [4a] | ? |
| R11 | ? no randomness seen in code | ? | ? | ? | ? |
| R12 | MIT code; runtime dep PyMuPDF AGPL-3.0 [1f] | MIT (relicensed 2025-04-27) [2f] | AGPL-3.0-only [3b] | AGPL-3.0-only [4b] | CC0-1.0 [5b] |
| Maintenance | Created 2026-10-16; 18 commits; 1 author; only Dependabot issues [1g] | 1521 commits, ~97% by maintainer; release 2026-11-08; issues answered and fixed [2g] | Created 2026; 34 commits; 1 author; only Dependabot issues [3d] | Created 2026-10-01; 3 commits; 0 stars [4c] | 60 commits; last release 2026-02-08 [5b] |
| Decisive gaps | R5, R6, R7, R4, R3 | R10, R2 (CLI), R5, R6 | R1 at scale, R5, R6, AGPL | R5, R6, R7, R8 images, AGPL, tiny/new | R2 (raster), R5–R8 |

### 1. redlatex: closest design, needs most filter features

- Repository: https://github.com/stormaref/redlatex; PyPI `redlatex` 0.2.0 uploaded 2026-10-16 (https://pypi.org/project/redlatex/).
- Language: Python ≥ 3.10; dependency `pymupdf>=1.28.2` (https://pypi.org/pypi/redlatex/json).
- [1a] Comparison model, from `src/redlatex/diff.py` (https://github.com/stormaref/redlatex/blob/main/src/redlatex/diff.py):
  - Words are extracted with PyMuPDF for the whole document and split into two streams, body text and "small print" (captions, tables, footnotes, by font size), so displaced floats do not look like edits.
  - `matching_blocks` anchors on word 3-grams that occur exactly once in both documents (patience-style), recursing so `difflib` runs only on short stretches; the docstring calls this "near-linear time".
  - Runs of ≥ 6 identical words that were removed and re-added are treated as moved, not edited (`MOVE_MIN = 6`).
  - Words hyphenated across line breaks are rejoined.
- [1b] Output: each spread draws the original old and new pages with `show_pdf_page` (vector, text stays selectable) and adds highlight overlays on copies of the pages (same file).
- [1c] Visualisation, figure handling, limitations and the speed claim: README (https://github.com/stormaref/redlatex#readme).
  - Removed words red on the left, added green on the right, margin bar per changed line, caret where text is missing on the other side.
  - Figures (embedded images and dense vector drawings) are rendered to small thumbnails; a figure without a pixel-identical counterpart is framed.
  - "purely visual changes such as colours, fonts or spacing are not marked."
  - "A 200-page document is compared in a few seconds." (author's claim, unverified for 650 pages).
- [1d] Page pairing (`pair_pages`): each new page is paired with the old page sharing the most matched words; the loop appends a spread for every new page, so unchanged pages are not omitted (same file).
  A cover page lists removed/added word counts per section using PDF bookmarks, which Typst emits.
- [1e] Header/footer handling: `extract` drops only lines that lie below 86% of page height and consist entirely of digits (same file).
  Our header line ("Page 24 of 646", document number, revision) would therefore enter the diff stream on every page and produce noise.
- [1f] License MIT (https://github.com/stormaref/redlatex/blob/main/LICENSE); but see [License trap](#license-trap-pymupdf).
- [1g] Maintenance: 18 commits, single contributor, 9 issues/PRs, all from Dependabot (GitHub API, 2026-11-12).
  CONTRIBUTING invites PRs and asks for "an issue first for anything large" (https://github.com/stormaref/redlatex/blob/main/CONTRIBUTING.md).
- LaTeX-specific parts (latexmk build, `\appendix` lettering, web app) are separable; `--old-pdf/--new-pdf` needs neither git nor LaTeX (README).
- Code quality: compact, dense functional style with short names; readable but untyped and lightly tested (`tests/test_diff.py` is 3 KB).
- Work to reach our bar: header/footer bands, regex token filters, only-changed-pages plus "no differences" banners, style comparison, connector lines, page-size/orientation options, scale testing on 650 pages.

### 2. PDF4QT (Pdf4QtDiff GUI + `PdfTool diff` CLI): good host, wrong algorithm for our scale

- Repository: https://github.com/JakubMelka/PDF4QT; release v1.7.0.0 published 2026-11-08 with `PDF4QT-Windows-1.7.0.0.zip`, MSI, MSIX, AppImage and Flatpak (https://github.com/JakubMelka/PDF4QT/releases/tag/v1.7.0.0).
- [2f] License: "As of April 27, 2025, the project has been relicensed under the MIT License" (README, https://github.com/JakubMelka/PDF4QT#readme); depends on Qt (LGPL) and others.
- [2a] Comparison model, from `Pdf4QtLibCore/sources/pdfdiff.cpp` (https://github.com/JakubMelka/PDF4QT/blob/master/Pdf4QtLibCore/sources/pdfdiff.cpp):
  - Pages are matched with an LCS over page content hashes (`performPageMatching`).
  - For each contiguous run of non-identical pages, the text flows of all pages in the run are concatenated and diffed (`performCompare`), so reflow inside a run is handled as continuous text.
  - Vector graphics, images and shadings are compared per matched page pair by geometry, so content that only moved position is likely to be flagged (code-reading inference, unverified).
  - Because our header contains "Page N of M", any page-count change makes every page hash differ, so the whole document becomes one run.
- [2d] Text diff algorithm: `PDFAlgorithmLongestCommonSubsequence` in `pdfalgorithmlcs.h` fills a full `m_size1 * m_size2` `std::vector<bool>` backtrack matrix (https://github.com/JakubMelka/PDF4QT/blob/master/Pdf4QtLibCore/sources/pdfalgorithmlcs.h).
  - My estimate: 650 pages × ~400 words ≈ 260 k words per side gives ≈ 6.8 × 10¹⁰ cells, ≈ 8.5 GB of bits plus as many string comparisons; character mode is worse.
  - Consistent with issue #132 "PDF diff hangs" on two 1 k-page PDFs, closed without reproduction (https://github.com/JakubMelka/PDF4QT/issues/132).
  - Text is compared as strings; there is no style comparison (`CompareTextsAsVector` instead compares glyph geometry).
- [2e] CLI: `PdfTool diff <left> <right> [--console-format text|xml|html]` prints a table (or XML) of differences with page numbers; it writes no PDF (https://github.com/JakubMelka/PDF4QT/blob/master/PdfTool/pdftooldiff.cpp, https://github.com/JakubMelka/PDF4QT/blob/master/PdfTool/pdftoolabstractapplication.cpp).
- [2b] GUI "Create compare report" saves the viewer's merged document (all left pages followed by all right pages) with semi-transparent polygon annotations (`DifferencesDrawInterface::drawAnnotations`); the side-by-side arrangement exists only as a viewer layout (https://github.com/JakubMelka/PDF4QT/blob/master/Pdf4QtDiff/mainwindow.cpp, https://github.com/JakubMelka/PDF4QT/blob/master/Pdf4QtDiff/utils.cpp).
- [2c] GUI filters are by difference type only: text, vector graphics, images, shading, page movement, and "show pages with differences" (resource names in https://github.com/JakubMelka/PDF4QT/tree/master/Pdf4QtDiff/resources).
  No header/footer region and no regex filters exist in the engine options (`PDFDiff::Option` in `pdfdiff.h`).
- [2g] Maintenance: 1521 commits, 1476 by Jakub Melka; outside PRs merged in 2025–2026 are mostly translations and build fixes (GitHub API).
  The maintainer fixed feature request #278 (overlay scaling in Diff) and #182 (Diff CLI arguments) (https://github.com/JakubMelka/PDF4QT/issues/278, https://github.com/JakubMelka/PDF4QT/issues/182).
  README: "Contributions are welcome!" with no CLA required.
- Work to reach our bar: replace the LCS with a linear-space/anchored diff, add header/footer and regex filters, add a style comparison, add side-by-side PDF composition and expose it in `PdfTool diff`, plus make graphics comparison reflow-tolerant.
  That is a large C++/Qt contribution into a library with one maintainer.

### 3. kogo: richest feature list, but AGPL and page-pair fallback

- Repository https://github.com/ta-061/kogo; PyPI `kogo` 0.1.7 (2026-08-23), AGPL-3.0-only, depends on PyMuPDF, NumPy and OpenCV (https://pypi.org/pypi/kogo/json).
- [3b] README (https://github.com/ta-061/kogo#readme):
  - Word-level text diff, page alignment tolerant to inserted/removed pages, pixel diff of figures with text areas masked, style-only change detection (bold, italic, font size), annotation diff.
  - Outputs `old-highlighted.pdf`, `new-highlighted.pdf`, `side-by-side.pdf`, `result.json`.
- [3a] `_document_text_differences` first matches identical lines, then runs `difflib.SequenceMatcher` on the remaining words of the whole document, but if `len(old_remaining) * len(new_remaining) > 60_000_000` it falls back to per-aligned-page-pair diffs (https://github.com/ta-061/kogo/blob/main/src/kogo/engine/text_diff.py).
  Whether a 650-page SDD revision stays under that threshold depends on how much text survives exact-line matching (unverified).
- [3c] `--max-pages` defaults to 200 and is capped at 2000; `--dpi` 96–180 (https://github.com/ta-061/kogo/blob/main/src/kogo/cli.py).
- [3d] Single author, 34 commits, no human-filed issues (GitHub API).
- Pixel diff runs per aligned page pair (`compare.py`), so reflowed figures are likely to be flagged (inference, unverified).

### 4. pdf-content-diff: same idea as redlatex, smaller and AGPL

- Repository https://github.com/Sang-Hyorin/pdf-content-diff; created 2026-10-01; v0.5.1, v0.6.0 and v0.7.0 all released that day; 3 commits; 0 stars (GitHub API) [4c].
- [4b] README (https://github.com/Sang-Hyorin/pdf-content-diff#readme):
  - "Compares the whole document before selecting changed regions", "does not rasterize pages", landscape review sheets with one common scale, only changed pages, "If there are no text changes, it says so on one page."
  - Filters manuscript margin line numbers and centred page numbers; vector drawing changes outlined in blue; "Raster image changes … are not covered"; "does not detect … font or layout edits".
  - AGPL-3.0-only; not on PyPI; also ships a VS Code extension.
- [4a] `backend.py` uses `difflib.SequenceMatcher(None, av, bv, autojunk=False)` over all tokens, which is quadratic in the worst case (https://github.com/Sang-Hyorin/pdf-content-diff/blob/main/backend.py).

### 5. JoshData/pdf-diff: continuous text diff, raster output

- Repository https://github.com/JoshData/pdf-diff (default branch `primary`); PyPI `pdf-diff` 0.9.3 (2026-02-08), CC0-1.0 [5b] (https://pypi.org/pypi/pdf-diff/json).
- [5a] README: compares the text layers of the two documents, outputs changed-text bounding boxes as JSON, and "Rasterizes the changed pages … to a PNG"; relies on poppler `pdftotext`; uses `fast_diff_match_patch` (https://github.com/JoshData/pdf-diff#readme).
- Useful mainly as a proof that whole-document text diffing with position mapping works; its output format is far from our bar.

## Other candidates surveyed and rejected

### Page-by-page comparison (fails R1)

| Tool | License | Model and output | Status | Source |
| --- | --- | --- | --- | --- |
| vslavik/diff-pdf | GPL-2.0 | Raster per page; `--output-diff` PDF of rasterised pages; `--skip-identical`, `--mark-differences`, `--dpi` | v0.5.3 2026-03-28; README: "provided as-is … not being actively developed" | https://github.com/vslavik/diff-pdf |
| bgeron/diff-pdf-visually | Apache-2.0 / MIT | Raster; pass/fail "whether there is a visible difference" | v1.8.1 2025-04-01 | https://github.com/bgeron/diff-pdf-visually |
| red6/pdfcompare | Apache-2.0 | Java; renders "a page from the expected.pdf and the same page from the actual.pdf" at 300 DPI; rectangular exclusion areas; diff PDF | active, 601 commits | https://github.com/red6/pdfcompare |
| serhack/pdf-diff | MIT | Go; `pdftoppm` + pixel compare to PNGs | last push 2023-05-08 | https://github.com/serhack/pdf-diff |
| JustusRijke/DiffPDF | MIT | Regression gate: hash, page count, text, then pixelmatch; requires equal page counts | v1.2.2 2026-04-09 | https://github.com/JustusRijke/DiffPDF |
| qtrac DiffPDF (FOSS edition) | GPL-2.0+ | Compares page pairs (words, characters or appearance) (from memory, unverified); free edition frozen at 2.1.3 | unmaintained since 2013 per SlackBuilds; distributions ship 2.1.3.1 | https://packages.debian.org/sid/diffpdf, https://slackbuilds.org/repository/15.0/office/diffpdf/ |
| lbellonda/ConfrontaPDF | GitHub reports none (GPL as a DiffPDF derivative, unverified) | DiffPDF-based fork with CLI `confrontapdfc`; per-page | last push 2022-01-31 | https://github.com/lbellonda/ConfrontaPDF |
| menil/pdf-vdiff | MIT | Rust + PDFium; vector side-by-side landscape output, but `diff_documents` "Compares two documents page-by-page" | v0.1.6 2026-11-07 | https://github.com/menil/pdf-vdiff/blob/main/src/diff.rs |
| isomatter-labs/Redliner | GPL-3.0 | Per-pixel visual diff GUI; "Add CLI interface" still a TODO | early development | https://github.com/isomatter-labs/Redliner |
| ImageMagick `compare` | ImageMagick license | Raster per image/page (general knowledge, not re-verified) | — | https://imagemagick.org/script/compare.php |

menil/pdf-vdiff is notable because its output side (PDFium extraction, vector page embedding, landscape composition, Windows/Linux binaries) matches R2 and R9 well; only its diff is per page.

### Text-only, viewer-only or browser-only (fail R2 or R9)

| Tool | License | Why rejected | Source |
| --- | --- | --- | --- |
| Stirling-PDF Compare | MIT core with proprietary directories | Runs in the browser; "checks extracted text only. Layout, formatting and image changes are not detected"; no redline PDF | https://docs.stirlingpdf.com/Tools/Guides/Compare/, https://github.com/Stirling-Tools/Stirling-PDF/blob/main/LICENSE |
| ssibb/PDF-Diff-Viewer | GPL-3.0 | Tkinter GUI, side-by-side word diff, no export; README says it was largely generated with Gemini | https://github.com/ssibb/PDF-Diff-Viewer |
| eraydin/semantic-pdf-diff (`spdfdiff`) | MIT (crates.io) | Rust; JSON/Markdown/HTML reports, no redline PDF; status "compatibility-gate" | https://github.com/eraydin/semantic-pdf-diff, https://crates.io/crates/spdfdiff_cli |
| jinyounghub/docrefract | Apache-2.0 | .NET CLI; JSON + HTML report, CI gate; "early alpha" | https://github.com/jinyounghub/docrefract |
| mli55/pdfdelta | MIT (+PyMuPDF) | Two separately annotated PDFs, no side-by-side; aimed at papers | https://github.com/mli55/pdfdelta |
| EdgarOrtegaRamirez/pdf-redline | (not stated in README) | Redlines the new PDF only plus JSON/Markdown report; README says `pip install pdf-redline` but no such PyPI project exists (PyPI API, 2026-11-12) | https://gitlab.com/EdgarOrtegaRamirez/pdf-redline |
| Labic-ICMC-USP/SemanticDiff | MIT | Reflow-resistant, vector side-by-side PDF, `ignore_repeated_header_footer` option; but built around an LLM review step, YAML-configured, 15 commits | https://github.com/Labic-ICMC-USP/SemanticDiff |
| cascremers/pdfdiff | none stated | `pdftotext` + text diff to terminal; last push 2022-03-31 | https://github.com/cascremers/pdfdiff |
| extrawurst/pdf-text-diff | none stated | Rust toy: raw text diff | https://github.com/extrawurst/pdf-text-diff |
| j-yang/mumford | MIT | Rust library; PDF text line diff with running-header stripping; no PDF output | https://github.com/j-yang/mumford |
| diffoscope | GPL-3.0+ | Compares text dumps; HTML/text report (general knowledge, not re-verified) | https://diffoscope.org/ |
| `pdftotext` + `diff`/`wdiff`/`dwdiff` | GPL | Continuous word diff is easy, but output is text, not a redline of the original pages | — |
| niklasjosq/opendiff, nickhafen/pdf-tools, ctkrug/draftline, tschnorpfeil/docdiff | Apache-2.0 / MIT / MIT / MIT | Browser-only apps, all created in 2026 with 0 stars | GitHub search API |
| houfu/redlines | MIT | Diffs strings; Markdown/HTML/terminal output | https://github.com/houfu/redlines |

### Indirect routes via DOCX/HTML (fail R2)

All of these discard the original PDF pages, so the redline would not look like the released document.
Converting a 650-page Typst PDF to DOCX/HTML is itself lossy for tables, diagrams and headers.

| Route | Status | Source |
| --- | --- | --- |
| LibreOffice headless compare | No CLI for compare; enhancement bug 93891 still NEW; only UNO-scripted `.uno:CompareDocuments` on Writer documents; PDFs import into Draw, where compare is unavailable (unverified) | https://bugs.documentfoundation.org/show_bug.cgi?id=93891 |
| JSv4/Python-Redlines | MIT; v2.0.0 2026-10-24; DOCX in, DOCX tracked changes out | https://github.com/JSv4/Python-Redlines |
| Open-XML-PowerTools WmlComparer | MIT; last push 2024-11-12 | https://github.com/OpenXmlDev/Open-Xml-PowerTools |
| pandoc + pandiff | MIT; last push 2026-01-21; diffs pandoc-readable text formats | https://github.com/davidar/pandiff |
| DaisyDiff / htmldiff | Apache-2.0; last push 2024-08-08 | https://github.com/DaisyDiff/DaisyDiff |

### LLM- or layout-model-based tools

- No docling-based comparator was found; docling itself is a converter (https://github.com/docling-project/docling).
- SemanticDiff's LLM step and the LLM "PDF compare" repositories found on GitHub are non-deterministic and/or need an API, so they fail R11 and the offline requirement.

### Source-level alternative (not a PDF comparator)

- **typdiff** (Apache-2.0, Rust, 86 stars, v0.1.4 on crates.io 2026-10-15) produces a Typst document marking added/deleted text between two `.typ` sources, like latexdiff (https://github.com/sou1118/typdiff, https://crates.io/crates/typdiff).
- **typdiff-git** wraps it for all `.typ` files changed between two Git revisions (https://github.com/shozee/typdiff-git).
- Gap: it diffs source, not the rendered document, so generated content, data-driven tables, diagrams, cross-reference page numbers and headers are not compared as the reviewer sees them, and the output is an inline redline, not side-by-side.
- It may be useful as a complement for reviewers, not as a PDFC replacement.

## License trap: PyMuPDF

- All Python candidates above (redlatex, kogo, pdf-content-diff, pdfdelta, PDF-Diff-Viewer) depend on PyMuPDF, which is "AGPL-3.0-or-commercial" (kogo README credits; https://github.com/ta-061/kogo#license).
- redlatex and pdfdelta are labelled MIT, but anything distributed with PyMuPDF is effectively bound by AGPL-3.0 terms.
- For an internal CI tool this is acceptable (copyleft is fine for a standalone CLI per the brief), but record it.
- A permissive rewrite could use pypdfium2 for word boxes and rendering plus pypdf for placing original pages as Form XObjects (unverified that pypdf's transformed-page merge preserves everything we need).

## Commercial reference bar (not candidates)

| Product | CLI / automation | Notes | Source |
| --- | --- | --- | --- |
| i-net PDFC | Command line in **Plus** ($1,950/yr or $4,950 perpetual, per installation) and higher; Standard ($199/yr) is desktop-only; REST API, Task Planner and JUnit/NUnit libraries from **Server** ($2,950/yr or $5,950 perpetual); "Production use in containers requires a Site or Cloud license" | Content-aware comparison with styles, tables (incl. local AI model for borderless tables); PDF/XLS/RTF report export | https://www.inetsoftware.de/products/pdf-content-comparer/licensing, https://www.inetsoftware.de/products/pdf-content-comparer/features |
| Draftable | Help centre: Draftable Legal "does not support … command-line (CLI), or scripted execution"; automation via cloud or self-hosted (Docker/Kubernetes) API | Side-by-side PDF export; pricing for self-hosted on request | https://help.draftable.com/hc/en-us/articles/Draftable-Legal-FAQ, https://draftable.com/rest-api, https://www.draftable.com/pricing |
| Adobe Acrobat Pro Compare Files | GUI only | Page ranges, "Compare text only"; no documented header/footer or string exclusion | https://helpx.adobe.com/acrobat/using/compare-documents.html |
| ABBYY FineReader PDF Compare | `Comparator.exe /compare …`; silent save needs the Extended CLI licence and a logged-in user | Exports a list of differences (e.g. Word table) | https://help.abbyy.com/en-us/finereader/16/user_guide/commandline_compare |
| PDF-XChange Editor Plus | GUI only (no CLI found) | Text replacements, insertions, deletions and style changes; since 9.3.360 | https://help.pdf-xchange.com/pdfxe11/compare-documents.html |
| Beyond Compare | Scripting with text reports | Converts PDF to plain text; formatting and images ignored | https://www.scootersoftware.com/kb/docxlspdf, https://www.scootersoftware.com/v5help/scripting_reference.html |

None of the commercial tools besides PDFC combines a local CLI with a side-by-side redline PDF, which is the gap a new FOSS tool would fill.

## Unverified items and open questions

- No candidate was run; all R10 and R11 cells are claims or code-reading estimates.
- PDF4QT memory estimate assumes ~400 words per page and that the header change puts every page in one modified run.
- kogo: whether `side-by-side.pdf` embeds vector pages or rendered images.
- PDF4QT: whether the saved compare report sets a two-page viewer layout.
- qtrac DiffPDF, ImageMagick and diffoscope descriptions come from general knowledge and were not re-checked against their current docs.
