# C. Building blocks for an open-source PDF redline CLI

Research date: 2026-11-12.
Scope: components, algorithms and stack choice.
Out of scope here: surveys of complete existing tools, and Typst-source-level diffing (separate tasks).

Conventions:

* Bracketed tags like [P1] are links; the URL list is at the end of the file.
* **Unverified** marks a claim I could not confirm from a primary source.
* **Inference** marks my own reasoning, not a sourced fact.
* **Estimate** marks a back-of-envelope number.

## Summary

* Recommended stack: **Rust**, reading with **PDFium** through `pdfium-render`, writing with **krilla** (Typst's PDF writer) using its `pdf` feature, which embeds source pages as form XObjects through `hayro-write`.
* Keep the reader behind a narrow page-model boundary so the pure-Rust `hayro-interpret` can replace PDFium later and make a single static binary possible.
* Diff the whole document as one token stream with the **histogram** algorithm (`imara-diff`), then apply diff-match-patch-style **semantic cleanup**, word-to-character refinement, and a moved-block pass.
* Typst marks page headers, footers and automatic hyphenation as tagged-PDF **artifacts**, so for documents that Typst generates, header/footer exclusion and de-hyphenation can be exact instead of heuristic [TY1].
* Main risks:
  1. Word segmentation and reading order that differ between two layouts of the same text.
  2. PDFium distribution, because prebuilt binaries are dynamic-only.
  3. Coordinate agreement between two PDF parsers (PDFium reads, hayro-write embeds).
  4. Vector-drawing comparison quality.
  5. Maturity of the young pure-Rust crates.

---

## 1. i-net PDFC feature semantics (the reference for the features)

### 1.1 Comparison mode

| Property | Values and default | Semantics | Source |
|---|---|---|---|
| `CONTINUOUS_COMPARE` | `CONTINUOUS` (default) or `STRICT` | The UI shows `CONTINUOUS` as "Continuous documents" and `STRICT` as "Page by Page". | [P1] |
| Continuous mode | — | Compares content across page boundaries. Position-based tolerances do not apply in this mode. | [P2] |
| `CONTINUOUS_DETECT_PAGES` | default `5`; `0` means all at once | If > 0, the comparison may be split. The value is how many pages may be added or inserted before matching fails. Larger is more precise and uses more memory. `0` compares all content at once, which gives the best result at maximum memory. | [P1] |
| `TOLERANCE_PAGE_LEFTCORNER` / `_RATIO` / `_SIZE` | 3 px / 0.01 / 2 px | Strict mode only: tolerances for page margins, aspect ratio and size. | [P1] |

**Inference:** `CONTINUOUS_DETECT_PAGES` suggests PDFC aligns in windows of pages instead of one global sequence alignment by default.
For 650 pages, a global token diff is cheap enough in a compiled language (see §4.1), so the windowing need not be copied.

### 1.2 Filters (`FILTERS` property, comma-separated)

The default `FILTERS` value is `INVISIBLEELEMENTS,TEXTTRANSFORM,SOLVEFALSEREPLACE` [P1].
Filters are described as an optional feature of continuous mode [P1].

| Filter key | Semantics | Related properties | Source |
|---|---|---|---|
| `INVISIBLEELEMENTS` | Heuristic removal of elements that contribute nothing visible. It works on extracted elements, not rendered pixels. It removes: elements outside the page or their clip; text and shapes without visible fill or stroke; zero-size images and very small line differences; duplicate comparable elements; and transparent or white content on a blank background. Structural elements and annotations are kept, and images are never removed as duplicates. | `COMPUTE_CLIPPING` (default false). When true, it computes real visibility from clip paths, occlusion and drawing order: hidden parts are cropped, fully covered elements are removed, and compatible shapes are merged. This is noticeably slower. | [P1] |
| `HEADERFOOTER` | Excludes header and footer bands. Automatic detection only works in non-strict mode. | `FIXED_HEADER_SIZE` and `FIXED_FOOTER_SIZE` in pixels (72 DPI page units). Values ≥ 0 are manual sizes. Negative values mean auto-detect, encoded as `-(size+1)`, so `-1` means auto. The UI sliders range 0–200. Auto mode "analyzes recurring header and footer areas". | [P1] |
| `REGEXP` | Removes content that matches patterns. | See §1.3. | [P1], [P3] |
| `TEXTTRANSFORM` | Normalizes text for comparison only; the displayed text is unchanged. It applies to extracted words and recursively to structure elements. | `TRANSFORM_OPERATIONS` (default `REPLACE_IDENTICAL`). `REPLACE_IDENTICAL` removes Unicode format characters, applies NFKD and removes simple spaces, so ﬁ becomes fi. `REPLACE_CONFUSABLES` applies NFD and then the Unicode TR39 confusables table (m vs rn). | [P1] |
| `SOLVEFALSEREPLACE` | An "internal filter" that, per the 21.10 changelog, "drastically reduces false positives in drawings and shapes with slight position differences due to rounding errors". It has been on by default even for legacy profiles since 21.10. No other public description exists. | none documented | [P5] |
| "Combine large text replacements/differences" (UI option) | Merges large replacements that contain a few coincidental equal words (e.g. "the", "and", punctuation) into one replace. "No content is marked as equal which is not." The raw profile key is **unverified**. | — | [P2], [P4] |
| `BASELINETABLE` | "Optimize Basic Tables": rebuilds a table and compares it cell by cell. It needs visible borders on the table and on every cell, no cell spacing, and at least 2×2 cells. | `IGNORE_REPEATING_HEADER` (default true) drops a table header identical to the header of the last table on the previous page. `TABLE_HEADER_DIFF_RATIO` (0.0). `TABLE_ID_COLUMN`: `false` keys rows on the whole row, `true` on the first column, `auto` on 1–3 key columns if they are unique, and `1..10` are also allowed. `TABLE_USE_TATR` enables optional ML detection with Table Transformer for borderless tables (slow). | [P1] |
| `MULTICOLUMN` | Detects up to 3 text columns from geometry and orders content column by column. It is not meant for tables. | — | [P1] |
| `HIDEROTATEDTEXT` | Ignores rotated text. | — | [P1] |
| `AREA` | Excludes rectangles, or with `AREA_INCLUDE=true` compares only inside them. An element matches only if its bounds are fully inside the rectangle. Rectangles can target odd/even/first-n/last-n pages and a single document. | `AREAFILTER` syntax `x,y,w,h[,ODD|EVEN|PAGE,n|FIRST,n|LAST,n][,F|S]` in 72-DPI units. | [P1] |
| `PAGERANGE` | Compares selected 1-based page ranges per document, optionally dropping N trailing pages. | `PAGERANGE_DOCUMENT1/2`, `PAGERANGE_END_DOCUMENT1/2` | [P1] |
| `ORIGINALORDER` / `USE_PDF_STRUCTURE` | "Group content" setting. The default detects the visual reading order. `ORIGINALORDER` uses content-stream order. `USE_PDF_STRUCTURE` uses the tagged-PDF structure tree when present and has no effect otherwise. | — | [P1], [P11] |
| `OCR`, `CMAPPATCH`, `NOCMAP` | OCR of images, and OCR-based repair of broken ToUnicode maps. | `NORMALIZATION_LEVEL` 0–3 | [P1] |

### 1.3 REGEXP semantics (precise)

* Patterns are Java regexes compiled with `Pattern.compile` without flags; plain-text patterns are quoted with `Pattern.quote` [P1].
* Each active pattern runs with `Matcher.find()` over the **complete extracted page text**, not per PDF text element [P1].
* `.` does not match line breaks unless `(?s)` is set, and patterns are case-sensitive by default [P3].
* Without capture groups, the whole match is removed [P1].
* With capture groups, only the participating groups are removed; the rest of the match acts as an anchor. `(?:…)` groups are not removed [P1], [P3].
* `REGEX_MATCH_WORDS` (default true) removes every word touched by a match. When false, only the matched character ranges are removed. Overlapping ranges are merged [P1].
* Patterns are stored one per line as `pattern|text|active`, and line breaks inside a pattern are encoded as `CR` [P1].
* The documentation's example for page numbers is `Page: (\d+) of (\d+)` [P3].

Design note (**inference**): because matching runs over the page text, the extractor must keep a mapping from text offsets to glyphs.
For cross-references such as `(p. 351)`, a pattern like `\(p\. (\d+)\)` removes only the number and keeps "(p." as an anchor, which is exactly the PDFC capture-group behaviour.

### 1.4 Compare types and style comparison

`COMPARE_TYPES` defaults to `TEXT, LINE, IMAGE, ANNOTATION` [P1].

| Property | Default | Semantics | Source |
|---|---|---|---|
| `COMPARE_TEXT_STYLES` | all | Which properties of **matched** words are compared: `SIZE, COLOR, FONT, STYLES, ROTATION`. | [P1] |
| `TOLERANCE_TEXT_SIZE` | 0.05 | Allowed relative difference in text size. | [P1] |
| `TOLERANCE_COLOR` | 0.01 | Maximum difference per RGB/HSB channel; shared by text and lines. | [P1] |
| `TEXT_ALIGN_RATIO` | 0.15 | Vertical tolerance for grouping glyphs into a line, relative to text height. | [P1] |
| `COMPARE_TEXT_CASE_SENSITIVE` | true | — | [P1] |
| `COMPARE_LINE_STYLES` | true | Compares color, stroke and thickness of matched lines and shapes. | [P1] |
| `TOLERANCE_LINE_POSITION` / `_SIZE` / `_THICKNESS` | 3 px / 2 px / 1 pt | Per-axis position, total length, and stroke width tolerance. | [P1] |
| `TOLERANCE_BOX_ROUND_EDGES` | 3 px | Tolerance for Bézier control points. | [P1] |
| `TOLERANCE_IMAGE_DISTANCE` / `_PIXEL_VALUE` / `_SIZE` | 3 px / 0.05 / 0.1 | Image position, pixel value, and area tolerance. | [P1] |
| `USE_PIXEL_MEDIUM_VALUE` | true | Compares averaged rather than single pixel values. | [P1] |
| `COMPARE_ANNOTATIONS_DETAILED` | false | Summarizes or itemizes annotation differences. | [P1] |

Limits stated by i-net [P10]:

* Diagrams and technical drawings are compared "as composition of lines and text", line by line.
* Glyph shapes of same-named fonts are not compared.
* Different font names are a difference even if they look the same, which matters for subset prefixes.
* A moved object such as a text box or table is a large difference.
* Line breaks and indentation changes are not reported in continuous mode.

### 1.5 PDF export options

| Option | CLI | API parameter | Semantics | Source |
|---|---|---|---|---|
| Create comparison PDF | `-p` or `-export printPDF` | `GET …/result/pdf` | Side-by-side visual result. | [P6], [P7] |
| Page format | — | `pageformat` = `Auto`, A3, A4 (default), A5, B4, B5, Legal, Letter | `Auto` scales output pages to fit the source pages. | [P6] |
| Orientation | `-landscape` | `orientation` = `PORTRAIT` (default) or `LANDSCAPE` | — | [P6], [P7] |
| Sides | `-sides both\|left\|right` | `onlyside` = `LEFT`, `RIGHT` or `BOTH` (default) | — | [P6], [P7] |
| Only pages with differences | `-m`/`-minimize` ("collapses all pages without differences") | `onlydifferent` (default false) | — | [P6], [P7] |
| Header and footer on output | `-header <bool>` (default true per the Java enum) | `headerandfooter` (default FALSE) | Adds an extra header and footer to the output pages. | [P6], [P7], [P8] |
| Scale to fit | `-scaletofit <bool>` (single-sided only) | `fittopage` | — | [P6], [P7] |
| Overlay mode | `-overlap <opacity 0..1>` | — | Overlays both sides. | [P7] |
| Comments | — | `comments` (default TRUE) | "Add the comments to the exported PDF". The whitepaper: "All detected differences are inserted into the PDF as annotations." | [P6], [P9] |
| Export only if different | default; `-exportAlways` overrides | `exportifdiff` | — | [P7], [P8] |
| Difference types | `-types add,remove,replace,attributeDifference,validationError,metadataChanged` | — | — | [P7] |
| Filter highlights | `-highlights annotation,invisible,headerfooter,unicode,multicolumn,regexp` | — | Outlines what each filter excluded. Useful to audit noise filters. | [P7] |
| Font embedding | — | `fontembedding` (default TRUE) | Replaces non-embedded font references with embedded fonts. | [P6] |
| Password | `-exportpw` | `password` | — | [P6], [P7] |
| Difference marker / connectors | — | — | The viewer "synchronizes documents on a virtual line" and shows tooltips. A specific export switch for the blue connector markers is **unverified**. | [P9] |

Other outputs: a report (PDF/XLS/XLSX/ODS/RTF), PNG diff images, XOR images, JSON summary via plugin, and a difference count [P7], [P9].

---

## 2. PDF reading and extraction libraries

### 2.1 Comparison

| Library | License | Language / bindings | Glyph positions | Per-glyph style | Images / vector paths | Tags and marked content | Distribution | Verdict |
|---|---|---|---|---|---|---|---|---|
| **PDFium** | BSD-3-Clause plus Apache-2.0 [L-PDFIUM] | C/C++. Rust `pdfium-render` (MIT OR Apache-2.0) [CR], Python `pypdfium2` (Apache-2.0 OR BSD-3) [PYPI], Go `go-pdfium` (MIT) [GH] | `FPDFText_GetCharBox`, `GetLooseCharBox`, `GetCharOrigin`, `GetMatrix`, `GetCharAngle`. `FPDFText_IsGenerated` flags spaces and line breaks synthesized by PDFium, and `FPDFText_IsHyphen` flags hyphens [H-TEXT]. | `FPDFText_GetFontInfo`, `GetFontSize`, `GetFontWeight`, `GetFillColor`, `GetStrokeColor`. Char → `FPDFText_GetTextObject` → `FPDFTextObj_GetTextRenderMode` (invisible text) and `FPDFFont_*` (flags, italic angle, embedded) [H-TEXT], [H-EDIT]. | `FPDFPageObj_GetType`, `FPDFPath_*` segments and draw mode, `FPDFImageObj_*` (decoded data, rendered bitmap, metadata), `FPDFFormObj_*` [H-EDIT]. | `FPDF_StructElement_*` (type, alt text, ActualText, marked-content IDs) [H-STRUCT]. `FPDFPageObj_CountMarks`/`GetMark` and `GetMarkedContentID` give `/Artifact` marks [H-EDIT]. | Prebuilt **dynamic** libraries for win/linux/mac/musl/wasm; no static archives in the release assets [PDFIUM-BIN]. Not thread-safe; the authors recommend multi-process [PR-README]. | **Best overall reader.** |
| MuPDF / PyMuPDF | **AGPL-3.0** or commercial [GH], [PYPI] | C. Python, Rust `mupdf` crate (AGPL) [CR] | yes | yes | yes | partial | conda-forge, pip | **Licensing trap** for a permissive tool. |
| Poppler | **GPL-2.0-or-later** [CONDA] | C++/glib | yes (`pdftotext -bbox`) | limited | yes | partial | conda-forge | **Licensing trap.** |
| pdf.js | Apache-2.0 [GH] | JavaScript | `TextItem` = `str`, `dir`, `transform`, `width`, `height`, `fontName`, `hasEOL`. Runs, not glyphs. | **No color**; style only gives ascent, descent and family [PDFJS-API] | Operator list | `includeMarkedContent` adds marked-content begin/end items [PDFJS-API] | npm `pdfjs-dist` 6.5 [NPM] | Good for a later web viewer; weak for extraction. |
| pdfminer.six / pdfplumber | MIT [GH] | Pure Python | yes (per char) | font name, size, colors (pdfplumber) | curves, rects, images | partial (`MCID`) | pip | Slow: 3.5 s and 5.6 s average versus 0.1 s for pypdfium2 in the py-pdf benchmark [PYPDF-BENCH]. Fine for prototyping table heuristics only. |
| Apache PDFBox | Apache-2.0 [GH] | Java | `TextPosition` with font, size, direction [PDFBOX-TP] | color through a graphics stream engine | yes | yes | JVM | Complete and permissive, but JVM distribution. |
| lopdf | MIT [CR] | Rust | no text layout | — | raw objects | raw | cargo | Object-level read/write; useful for post-processing output. |
| pdf (pdf-rs) | MIT [CR] | Rust | low-level | — | raw | raw | cargo | Low-level reader. |
| pdf-extract | MIT [CR] | Rust | basic text output | — | — | — | cargo | Plain text only. |
| **hayro** (LaurenzV) | Apache-2.0 OR MIT [CR] | Pure Rust: `hayro-syntax` (parse), `hayro-interpret` (interpreter → `Device` trait), `hayro` (raster), `hayro-svg`, `hayro-write` [HAYRO] | `Device::draw_glyph_run` receives positioned glyphs. `Glyph::as_unicode()` uses ToUnicode, then the Adobe Glyph List, then `uniXXXX` names, and is self-described as "still somewhat experimental" [HAYRO-FONT]. No word or space reconstruction. | Paint and font are available through draw props (**unverified** detail) | `draw_path`, `draw_image`, clip stacks [HAYRO-DEV] | `begin_marked_content(tag, props)` on the main branch [HAYRO-DEV]; whether it is in the released 0.9.0 is **unverified** | cargo, pure Rust | The future single-binary reader. Missing: passwords [HAYRO-SYN], text layout. |
| pdf_oxide | MIT OR Apache-2.0 [CR] | Rust core with many bindings | chars API | **unverified** | images | **unverified** | cargo/pip | Young and self-benchmarked (claims 5× faster than PyMuPDF) [OXIDE]. Watch, don't depend. |
| oxidize-pdf | MIT [CR] | Rust | **unverified** | — | — | — | cargo | Not evaluated in depth. |
| pdfcpu | Apache-2.0 [GH] | Go | **No positioned text extraction**; `extract` modes are image, font, content (raw streams), page, meta [PDFCPU-EX] | — | — | — | Go binary, conda-forge [CONDA] | Writer and manipulator, not an extractor. |
| ledongthuc/pdf | BSD-3 [GH] | Go | basic | font, size | — | — | Go | Too limited. |
| unidoc/unipdf | commercial | Go | — | — | — | — | — | Licensing trap. |
| qpdf / pikepdf | Apache-2.0 / MPL-2.0 [GH], [CONDA] | C++ / Python | **no text extraction** | — | object level | object level | conda-forge, pip | Writer and manipulator (see §3). |

Extraction quality and speed (py-pdf benchmark, plain text, 14 papers) [PYPDF-BENCH]:

| Library | Mean time | Quality score |
|---|---|---|
| pypdfium2 | 0.1 s | 95 % |
| PyMuPDF | 0.1 s | 95 % |
| pdftotext (Poppler) | 0.3 s | 90 % |
| pdfminer.six | 3.5 s | 89 % |
| pdfplumber | 5.6 s | 74 % |

**Estimate:** pypdfium2 took about 0.2 s for the 117-page GeoTopo book [PYPDF-BENCH], so plain-text extraction of 650 pages is on the order of 1–2 s per document.
Glyph-level extraction makes several FFI calls per character.
That is cheap from Rust but likely tens of seconds from pure Python loops (**estimate**).

### 2.2 Licensing traps

* AGPL: MuPDF, PyMuPDF, `mupdf` crate [GH], [CR]; also `dssim` and `dssim-core` for image similarity [GH], [CR].
* GPL: Poppler [CONDA], `diff-pdf` [GH], pHash (**unverified** here; widely documented as GPL-3).
* Commercial: unipdf, i-net PDFC.
* MPL-2.0 (pikepdf) has file-level copyleft and is compatible with an MIT/Apache tool used as a dependency (**inference**).

### 2.3 Tagged PDF matters for Typst inputs

* Typst writes tagged PDFs by default [TY1].
* Typst "automatically tags many layout artifacts such as headers, footers, page back- and foregrounds, and automatic hyphenation as artifacts" [TY1].
* So, with PDFium marked-content access [H-EDIT] or hayro's marked-content callbacks [HAYRO-DEV]:
  * Header/footer exclusion becomes exact (skip `/Artifact` content).
  * Hyphens inserted by line breaking can be dropped exactly.
  * The structure tree gives paragraphs, headings, tables (`TH`/`TD`) and figures, removing most layout heuristics.
* Heuristics remain the fallback for untagged PDFs, as PDFC's `USE_PDF_STRUCTURE` also does [P11].

---

## 3. PDF writing for the side-by-side output

### 3.1 How to embed a page without rasterizing

Wrap the source page's content stream and resources in a form XObject.
Then place it with a `cm` matrix on a new landscape page and draw overlays after it.

| Library | API | License | Notes | Source |
|---|---|---|---|---|
| **krilla** (+ `hayro-write`) | `Surface::draw_pdf_page(&PdfDocument, size, page_idx)` embeds a page as an XObject. `Document::embed_pdf_pages` copies whole pages. Cargo feature `pdf`. | MIT OR Apache-2.0 | High-level drawing (paths, fills, blend modes, text with font subsetting), outlines, page labels, links, tagged PDF, **PDF/A-1…4 and PDF/UA-1 export**. Tested against 6 viewers, the Arlington model and veraPDF. **Annotations are limited to Link and Widget**, so markup annotations need another route. Encryption is out of scope. | [KRILLA], [KRILLA-SRC], [KRILLA-ANN] |
| pdf-writer | Low-level writer used by krilla. Annotation subtypes include Text, Highlight, Square, Line, Underline, StrikeOut. | Apache-2.0 (MIT OR Apache-2.0 on crates.io) | Use directly with `hayro-write` if krilla's annotation gap matters. | [PDFW-ANN], [CR] |
| PDFium | `FPDF_NewXObjectFromPage` plus `FPDF_NewFormObjectFromXObject`; `FPDF_ImportNPagesToOne` (n-up); `FPDFAnnot_*` creation with Popup/IRT links | BSD-3/Apache-2.0 | One library for everything, but the APIs are labelled "Experimental" and give less control over output conformance. | [H-PPO], [H-ANNOT] |
| qpdf / pikepdf | `getFormXObjectForPage(handle_transformations)` and `placeFormXObject`; pikepdf `Page.as_form_xobject`, `add_overlay`, `calc_form_xobject_placement` | Apache-2.0 / MPL-2.0 | `handle_transformations` replicates `/Rotate` and `/UserUnit` in the XObject `/Matrix`. Mature, and handles encrypted input. | [QPDF-H], [PIKE-OV] |
| pdfcpu | `nup`, `grid`, `stamp`, `watermark`, `annotations` commands | Apache-2.0 | Go; usable as a library. | [PDFCPU] |
| PDFBox | `LayerUtility.importPageAsForm(doc, page)` | Apache-2.0 | — | [PDFBOX-LU] |
| pdf-lib | `embedPage`/`drawPage` | MIT | Upstream last released 1.17.1 on 2021-11-06; maintained fork `@cantoo/pdf-lib` 2.11.1 (2026-09) [NPM] | [NPM] |

### 3.2 Pitfalls

* **Annotations and links are lost.**
  A form XObject captures only the content stream.
  pikepdf notes that composing inside the existing document is needed to keep "annotations and hyperlinks" [PIKE-OV].
  Links (TOC, cross-references) would be re-created as Link annotations transformed into output coordinates if wanted (**inference**).
* **Page boxes and rotation.**
  Placement must honour CropBox/TrimBox, `/Rotate` and `/UserUnit`; qpdf exposes these explicitly [QPDF-H].
  When PDFium reads and hayro-write embeds, both must agree on the visible box, or highlights drift (**inference**; a main risk).
* **Resource deduplication and size.**
  Each embedded page drags in its fonts and images.
  Shared objects must be copied once per source document, not once per page.
  Whether hayro-write deduplicates across pages is **unverified**; measure it on a 650-page pair.
  krilla's own fonts are subset [KRILLA].
* **Encrypted inputs.**
  hayro-syntax lists password-protected PDFs as unsupported [HAYRO-SYN].
  PDFium and qpdf decrypt.
  Workaround (**inference**): decrypt with PDFium or qpdf to a temporary file first, or refuse.
* **PDF/A output.**
  krilla can validate PDF/A, but embedding arbitrary source pages may violate PDF/A (e.g. transparency, unembedded fonts).
  It reports a `ValidationError` [KRILLA-SRC].
  Make PDF/A optional.
* **Highlights must not hide text.**
  Draw highlight fills with a `Multiply` blend mode or low alpha over the XObject; krilla supports blend modes [KRILLA].
* **Reviewer-replyable comments.**
  Use markup annotations (Highlight with QuadPoints, plus Popup; replies use `IRT`).
  PDFium [H-ANNOT] and pdf-writer [PDFW-ANN] can write these, but krilla cannot yet [KRILLA-ANN].

---

## 4. Diff algorithms and document alignment

### 4.1 Sequence diff

| Algorithm | Property | Implementations | Source |
|---|---|---|---|
| Myers O(ND) | Minimal edit script; cost grows with the number of differences D | `imara-diff` (with GNU-diff/git heuristics against quadratic blow-up), `similar`, `dissimilar` | [MYERS], [IMARA] |
| Patience | Anchors on tokens unique in both sides, then recurses; avoids aligning on frequent tokens such as "the" | `similar` | [SIMILAR] |
| Histogram | Patience variant from JGit/git that also uses low-occurrence (not only unique) tokens. In `imara-diff` it is 10–100 % faster than Myers. | `imara-diff`, `similar`, git | [IMARA], [SIMILAR], [GITDIFF] |
| Anchored | Forces chosen lines to match (git `--anchored`) | `similar` ("with any algorithm") | [SIMILAR], [GITDIFF] |

Evidence on quality: Nugroho et al. found that Histogram is better suited than Myers for describing code changes, based on manual analysis [NUGROHO].

Crates:

* `imara-diff`: Apache-2.0, Myers plus Histogram, up to 2^31−2 tokens [IMARA], [CR].
* `similar`: Apache-2.0, Myers, Patience, Hunt, Histogram, LCS, anchored; line, word, character and grapheme modes; deadlines [SIMILAR].
* `dissimilar`: Apache-2.0, a diff-match-patch port with semantic cleanup [CR].

`imara-diff` advises against a single fine-grained diff over a large input [IMARA].
Instead, diff coarse tokens first, refine inside changed regions, and skip refinement for pure inserts, pure deletes and very large changes [IMARA].

**Estimate:** 650 pages × about 400 words ≈ 260 k word tokens.
After interning to integer IDs, a histogram diff over that is in the milliseconds to low seconds even with many changes.

Recommended pipeline (**inference**, built from the sources above):

1. Tokenize into words with Unicode word boundaries (UAX #29) [UAX29] and intern them to `u32`.
2. Run a global histogram diff over all pages as one stream; this implements PDFC's continuous mode.
3. Apply semantic cleanup (§4.2).
4. Within each replace pair whose old and new runs are short and similar, refine at the character level (e.g. `3.2.1` → `3.2.4`).
   Otherwise keep it as a word-level replace.
5. Detect moves (§4.3).
6. Compare styles only on equal tokens (§1.4 semantics).

### 4.2 Avoiding false "replace" pairs and coincidental matches

* Neil Fraser's "semantic chaff": diffs of dissimilar texts are "littered with small coincidental equalities".
  The fix: split any equality no longer than the edits on both sides into a delete and an insert, then merge adjacent edits [FRASER].
  This is what `diff_cleanupSemantic` does, and it matches PDFC's "Combine large text differences" [P4].
* Semantic alignment: slide edit boundaries to word or punctuation boundaries, scoring candidate positions [FRASER].
* Patience and Histogram anchor on rare tokens, which avoids aligning on "the" and "," in the first place [GITDIFF], [IMARA].
* Anchoring on structure (**inference**): headings with section numbers stripped, paragraph starts, and figure tokens are rare and stable.
  Use them as anchors (git `--anchored` style [GITDIFF]), or diff paragraph hashes first and then words inside each paragraph pair.
* Keep PDFC's guarantee: never report content as equal when it is not [P4].

### 4.3 Moved blocks

* Heckel (1978) uses tokens that occur once in each file as anchors and extends them, which naturally finds moved blocks [HECKEL].
* Tichy (1984) formalizes edit distance with block moves [TICHY].
* git `--color-moved=blocks` greedily pairs added and removed blocks of at least 20 alphanumeric characters [GITDIFF].
* Practical approach (**inference**):
  1. After the main diff, hash every deleted and inserted run of at least k words (e.g. k=8, case- and whitespace-normalized).
  2. Pair identical hashes, then extend each pair greedily.
  3. Report the pair as "moved" with a link instead of as delete plus insert.

### 4.4 Linearization, paragraphs, headers and footers

* PDF has no paragraph or line-break markup; PDFC treats line breaks as layout, not content, in continuous mode [P10].
* Reading-order options, from most to least reliable:
  1. Structure tree, when tagged [P11], [H-STRUCT].
  2. Content-stream order, when both files come from the same generator [P11].
  3. Geometric layout analysis (e.g. pdfminer's `LAParams`: `char_margin`, `word_margin`, `line_margin`, `boxes_flow` [PDFMINER-LA]).
  4. Docstrum [DOCSTRUM].
  5. Recursive XY-cut (Nagy and Seth 1984, **unverified** citation details).
* Word boundaries: PDFium inserts generated spaces and line breaks, reported by `FPDFText_IsGenerated` [H-TEXT].
  This is the main reason to prefer PDFium over a raw glyph stream.
* Header/footer auto-detection:
  * "Repetition of similar tokens located at similar positions at the top and at the bottom of contiguous pages", with odd/even layouts checked separately (Déjean and Meunier 2006, as summarized by [FNP21]; original paper not fetched).
  * Xerox patent US 7,937,653 describes header/footer zones as having low text variability across pages [US7937653].
  * Implementation sketch (**inference**):
    1. Normalize digits to `#`.
    2. Cluster text lines in the top and bottom 15 % by (normalized text, y ± tolerance) across pages, separately for odd and even pages.
    3. Classify a cluster as header/footer if it appears on at least 50 % of pages.
    4. Return the band edges in PDFC's `FIXED_*_SIZE` form.

### 4.5 Tables

* PDFC's basic table filter needs fully ruled tables, matches rows by full content or key columns, and drops repeated headers [P1].
* pdfplumber's ruled-table detection follows Nurminen's thesis and Tabula [PDFPLUMBER].
* Camelot is MIT [GH].
* Table Transformer (TATR, MIT, now archived) [GH], from Smock et al., PubTables-1M [TATR], is what PDFC offers as optional "AI table detection" [P1].
* For Typst output, `table.header`/`table.footer` are tagged as such [TY1], so the structure tree gives cells (`TD`/`TH`) and repeated headers directly (**inference**).
* Cell-level alignment (**inference**):
  1. Key rows by first-column text, as PDFC does with `TABLE_ID_COLUMN=auto` [P1].
  2. Diff row sequences with patience or histogram.
  3. Then diff cells pairwise.
  4. Represent a table in the main token stream as one structured token so table changes don't smear into the surrounding text.

### 4.6 Hyphenation, ligatures, whitespace

* Ligatures and compatibility forms: apply NFKC or NFKD normalization (UAX #15) [UAX15] for comparison only, as PDFC's `REPLACE_IDENTICAL` does [P1].
* End-of-line hyphens:
  * Drop the hyphen when PDFium's `FPDFText_IsHyphen` is set at a line end [H-TEXT], or when the hyphen is an artifact (Typst) [TY1].
  * pdf-diff converts a trailing `-` into a soft hyphen (U+00AD) and removes it when joining [PDFDIFF-SRC].
* Whitespace: collapse runs and ignore generated line breaks (**inference**).
* Dashes and quotes: optional confusables folding (TR39), as in PDFC's `REPLACE_CONFUSABLES` [P1].

### 4.7 Figures and images

* **Atomic tokens** (**inference**):
  * Insert one token per image (hash of decoded pixels plus size class) and one per vector figure into the word stream at its reading-order position.
  * Equal tokens align figures; a replace pair of figure tokens triggers a visual sub-comparison.
* Image identity: hash decoded pixel data (PDFium `FPDFImageObj_GetImageDataDecoded` or `GetBitmap`) [H-EDIT] rather than encoded bytes, so re-compression is not flagged.
* Perceptual hashes: `image_hasher` (MIT OR Apache-2.0, maintained fork of `img_hash`) [CR] gives "similar but changed" versus "different image".
* Vector drawings:
  * Option A, as PDFC does: compare canonicalized path segments with position, length and stroke tolerances [P1].
    This gets noisy with rounding, which is why PDFC's `SOLVEFALSEREPLACE` exists [P5].
  * Option B (**inference**): render each changed figure region from both sides at the same scale (PDFium rendering or `hayro`) and pixel-diff with anti-aliasing tolerance.
    pixelmatch (ISC) does this with Vyšniauskas' anti-aliased pixel detector and a perceptual color threshold [PIXELMATCH].
    Avoid `dssim` (AGPL).
    `image-compare` (MIT) offers SSIM/RMS [CR].
  * Use A to locate candidates and B to confirm and outline them.

### 4.8 Prior art worth reading

* JoshData `pdf-diff` (CC0) [GH]:
  1. Serializes each whole PDF to one text stream from `pdftotext -bbox`, with optional top/bottom margins.
  2. Marks end-of-line hyphens.
  3. Diffs with the C++ diff-match-patch.
  4. Maps hunks back to boxes, then renders changed pages to PNG with red outlines and emits JSON [PDFDIFF], [PDFDIFF-SRC].
  This is the closest open-source analogue of continuous mode, but it rasterizes and depends on GPL Poppler.
* Yin et al. 2026 (arXiv 2607.14117) [ARXIV-YIN]: decomposes document versions into typed elements (text, tables, formulas, figures) and aligns them "alignment-first" with spatial, content and structural compatibility before type-aware differencing.
  Their reported reflow Match-F1 numbers came from search snippets and are **unverified**.
* A survey of change detection in document images (arXiv 2307.07691) covers image-based methods, which are less relevant for born-digital PDFs [SURVEY-IMG].
* Fraser's "Diff Strategies" is the practical reference for pre- and post-processing [FRASER].
* Myers [MYERS], Heckel [HECKEL] and Tichy [TICHY] are the algorithmic foundations.

---

## 5. Stack options

| Criterion | **A. Rust + PDFium + krilla** | B. Pure Rust (hayro + krilla) | C. Python (pypdfium2 + pikepdf) | D. Go (go-pdfium + pdfcpu) | E. TypeScript (pdf.js + pdf-lib) | F. Java (PDFBox) |
|---|---|---|---|---|---|---|
| Extraction ceiling | Highest: per-char style, generated spaces, hyphen flags, struct tree, marks [H-TEXT] | High rendering fidelity [HAYRO]; text layout to build ourselves; Unicode mapping "experimental" [HAYRO-FONT] | Same as A (same engine) | Same as A | No text color [PDFJS-API] | High [PDFBOX-TP] |
| Vector-preserving output | krilla `draw_pdf_page` [KRILLA-SRC] or PDFium XObjects [H-PPO] | krilla | pikepdf form XObjects [PIKE-OV] | pdfcpu, or PDFium through go-pdfium | pdf-lib `embedPage` | `importPageAsForm` [PDFBOX-LU] |
| Performance for 650 pages | Seconds (**estimate**) | Unknown; hayro says performance is a remaining gap [HAYRO] | Per-char Python loops are slow (**estimate**) | Good with CGO; WebAssembly mode is slower and only compiled on amd64/arm64 [GOPDFIUM] | Moderate | Good after JVM warm-up |
| Licensing | All permissive | All permissive | Permissive plus MPL-2.0 | Permissive | Permissive | Permissive |
| Single-binary distribution | Binary plus `pdfium.dll`/`.so` beside it. A static build means building PDFium ourselves [PDFIUM-BIN], [PR-README]. | **Yes** (`cargo install`) | pip/conda (pypdfium2 and pikepdf are on conda-forge [CONDA]) | Yes with the WebAssembly PDFium inside [GOPDFIUM] | Node or bundled runtime | JVM |
| Ease of contribution | Medium (Rust) | Medium | Highest | Medium | High | Medium |
| Path to interactive or HTML viewer | `pdfium-render` compiles to WASM [PR-README]; diff core to WASM | hayro has a WASM demo viewer [HAYRO] | Separate JS front end | Separate JS front end | Native (pdf.js viewer) | Separate |

### 5.1 Recommendation

**Choose A**, structured so that it can become B:

1. **Core in Rust**, as a library crate plus a CLI crate.
   Rust gives deterministic, fast, memory-safe native code, a single `cargo install`, and WASM for a later browser viewer.
2. **Reader: PDFium through `pdfium-render`**, behind a `PageModel` trait.
   The model holds glyphs with Unicode, quad, font, size, color, weight, italic, angle, render mode and artifact flag, plus images, paths and structure.
   PDFium is the only permissive engine with glyph-level style, synthesized word spaces, hyphen flags, struct-tree and marked-content access, and rendering in one place [H-TEXT], [H-EDIT], [H-STRUCT].
   It is also the engine with the best extraction speed and quality score in the py-pdf benchmark [PYPDF-BENCH].
3. **Writer: krilla with the `pdf` feature**, for landscape sheets, XObject page embedding, highlight and connector drawing with blend modes, subset fonts for banners, outlines (one bookmark per change), page labels, and optional PDF/A [KRILLA], [KRILLA-SRC].
   Add markup annotations either upstream in krilla or with a small `lopdf` post-pass [CR].
   Fall back to PDFium's XObject and annotation APIs if a spike shows coordinate or size problems [H-PPO], [H-ANNOT].
4. **Diff: `imara-diff` (histogram)** for the global word stream [IMARA], plus `similar` for anchored, character-level and inline refinement [SIMILAR].
   Write a semantic cleanup following Fraser [FRASER], or reuse `dissimilar` for character-level cleanup [CR].
5. **Prototype quickly in Python with pypdfium2 + pikepdf** to tune heuristics first.
   It uses the same PDFium semantics, so the logic transfers (**inference**).
6. **Later:** implement a `PageModel` backend on `hayro-interpret` and run both backends against each other in CI (differential testing).
   Switch the default once the outputs agree, to get a pure-Rust static binary.

Distribution for A (**inference**):

* Ship per-platform archives that contain `pdfredline` and `pdfium.{dll,so}` from `bblanchon/pdfium-binaries` [PDFIUM-BIN].
* Publish to conda-forge, where `pypdfium2` already packages PDFium [CONDA].
* `cargo install` needs a PDFium library at run time; `pdfium-auto` downloads one, but not offline [CR].

### 5.2 Main technical risks

| # | Risk | Mitigation (**inference** unless cited) |
|---|---|---|
| 1 | Two layouts of the same text extract into different word sequences (kerning-split words, generated spaces, hyphenation, ligatures) | Normalize (§4.6). Use Typst artifacts and tags [TY1]. Build a golden corpus of old/new pairs with expected diffs. |
| 2 | False replaces and chaff across 260 k tokens | Histogram anchors, semantic cleanup [FRASER], structure anchors, move detection. |
| 3 | PDFium packaging: dynamic-only prebuilt binaries [PDFIUM-BIN]; not thread-safe [PR-README] | Ship the library alongside the binary; parallelize with processes; move to hayro later. |
| 4 | PDFium (reader) and hayro-write (embedder) disagree on page box or rotation, so highlights drift | Normalize to one CropBox-based coordinate system; test rotated and cropped fixtures; fall back to PDFium XObjects. |
| 5 | Output size from per-page resource copies | Measure early on a 650-page pair; deduplicate shared objects. |
| 6 | Vector-drawing diff is noisy (PDFC needed `SOLVEFALSEREPLACE` [P5]) | Treat figures as atomic tokens and confirm with pixel diff (§4.7). |
| 7 | Young crates: hayro 0.9, krilla 0.8, PDFium editing APIs marked "Experimental" | Pin versions; keep backends swappable; contribute upstream. |
| 8 | Encrypted inputs unsupported by hayro [HAYRO-SYN] | Decrypt through PDFium or qpdf before embedding. |

---

## 6. Ideas beyond a PDFC clone

All are **inference**: design ideas made cheap by the components above.

* **Change model as the hub**: one JSON document listing each change with kind (insert, delete, replace, move, style, image, figure), old and new page plus quads, text, and section path.
  Every output (PDF, HTML, CI) renders from it.
* **CI gating**: exit code 0 = no differences, 1 = differences, 2 = error, following GNU `diff` [GNUDIFF].
  Add `--fail-on=text,style,image` and a Markdown summary for a comment on a code review.
* **git integration**:
  * `textconv` gives text-only `git diff`.
  * A `diff.<driver>.command` (or `GIT_EXTERNAL_DIFF`) launches the visual redline [GITATTR].
* **Summary page** at the front: counts per change type and a table of changed sections with internal links to each change.
  Add PDF outline entries per change; krilla supports outlines, links and named destinations [KRILLA].
* **Collapsed unchanged ranges**: banner pages "Pages 1–23 have no differences", as PDFC's `-minimize` does [P7].
* **Moved-text visualization**: same-color brackets on both sides plus a connector, with "moved from p. X" labels.
* **Reviewer workflow**: one Highlight annotation per change, with a Popup carrying the change description, so reviewers can reply (IRT) in any PDF viewer [H-ANNOT], [PDFW-ANN].
* **Noise-filter audit overlay**: outline what each filter excluded, as PDFC's `-highlights` does [P7], to build trust in the ignore regexes.
* **HTML/interactive report**: pdf.js renders both originals, overlays come from the change JSON, scrolling is synchronized, and there is a change list sidebar.
  Alternatively, compile the Rust core to WASM for a fully local viewer [PR-README].
* **Section-aware diff**: report changes per heading path from the structure tree ("§4.2 Calibration: 3 changes").
* **Profile files**: a TOML equivalent of PDFC profiles (filters, regexes, tolerances), checked into the repository next to the document.

---

## Sources

### i-net PDFC

* [P1]: https://docs.inetsoftware.de/pdfc/help/pdfc.programming.profiles/p/comparison-profile
* [P2]: https://docs.inetsoftware.de/pdfc/help/pdfc.gui.profiles/p/comparison-profile
* [P3]: https://docs.inetsoftware.de/pdfc/help/pdfc.gui.regex/p/regular-expressions-filter
* [P4]: https://docs.inetsoftware.de/pdfc/help/diffmerge/p/combine-differences
* [P5]: https://docs.inetsoftware.de/pdfc/help/pluginstore.page.pdfc/p/i-net-pdfc-core
* [P6]: https://docs.inetsoftware.de/pdfc/help/comparisonapi-guid-result/p/guid-result-type
* [P7]: https://docs.inetsoftware.de/pdfc/help/command-line-access?locale=en
* [P8]: https://apidoc.inetsoftware.de/java.pdfc/com/inet/pdfc/PDFC.PROP.html
* [P9]: https://www.inetsoftware.de/en/i-net-pdfc/whitepaper/
* [P10]: https://docs.inetsoftware.de/pdfc/help/pdfc.limitations/p/limitations
* [P11]: https://docs.inetsoftware.de/pdfc/help/parser.pdf.config

### PDF libraries

* [L-PDFIUM]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE
* [H-TEXT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h
* [H-EDIT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_edit.h
* [H-STRUCT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_structtree.h
* [H-PPO]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_ppo.h
* [H-ANNOT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_annot.h
* [PDFIUM-BIN]: https://github.com/bblanchon/pdfium-binaries/releases (release chromium/8152, 2026-11-10)
* [PR-README]: https://github.com/ajrcarey/pdfium-render
* [GOPDFIUM]: https://github.com/klippa-app/go-pdfium
* [PDFJS-API]: https://github.com/mozilla/pdf.js/blob/master/src/display/api.js
* [PDFBOX-TP]: https://javadoc.io/static/org.apache.pdfbox/pdfbox/3.0.6/org/apache/pdfbox/text/TextPosition.html
* [PDFBOX-LU]: https://javadoc.io/static/org.apache.pdfbox/pdfbox/3.0.6/org/apache/pdfbox/multipdf/LayerUtility.html
* [HAYRO]: https://github.com/LaurenzV/hayro
* [HAYRO-SYN]: https://github.com/LaurenzV/hayro/blob/main/hayro-syntax/README.md
* [HAYRO-DEV]: https://github.com/LaurenzV/hayro/blob/main/hayro-interpret/src/device.rs
* [HAYRO-FONT]: https://github.com/LaurenzV/hayro/blob/main/hayro-interpret/src/font/mod.rs
* [KRILLA]: https://github.com/LaurenzV/krilla
* [KRILLA-SRC]: https://github.com/LaurenzV/krilla/blob/main/crates/krilla/src/pdf.rs and https://github.com/LaurenzV/krilla/blob/main/crates/krilla/Cargo.toml
* [KRILLA-ANN]: https://github.com/LaurenzV/krilla/blob/main/crates/krilla/src/interactive/annotation.rs
* [PDFW-ANN]: https://github.com/typst/pdf-writer/blob/main/src/annotations.rs
* [QPDF-H]: https://github.com/qpdf/qpdf/blob/main/include/qpdf/QPDFPageObjectHelper.hh
* [PIKE-OV]: https://pikepdf.readthedocs.io/en/latest/topics/overlays.html
* [PDFCPU]: https://github.com/pdfcpu/pdfcpu
* [PDFCPU-EX]: https://pdfcpu.io/extract/extract
* [OXIDE]: https://github.com/yfedoseev/pdf_oxide
* [PDFMINER-LA]: https://github.com/pdfminer/pdfminer.six/blob/master/pdfminer/layout.py
* [PDFPLUMBER]: https://github.com/jsvine/pdfplumber
* [PYPDF-BENCH]: https://github.com/py-pdf/benchmarks
* [TY1]: https://typst.app/docs/guides/accessibility/

### Package registries

These were queried through their APIs on 2026-11-12 for licenses and versions.

* [GH]: GitHub repository metadata (`gh api repos/<owner>/<repo>`)
* [CR]: https://crates.io/api/v1/crates/<name>
* [PYPI]: https://pypi.org/pypi/<name>/json
* [NPM]: https://registry.npmjs.org/<name>
* [CONDA]: https://api.anaconda.org/package/conda-forge/<name>

### Diff algorithms and tools

* [MYERS]: Myers, "An O(ND) difference algorithm and its variations", Algorithmica 1986, https://doi.org/10.1007/BF01840446 (PDF: http://www.xmailserver.org/diff2.pdf)
* [HECKEL]: Heckel, "A technique for isolating differences between files", CACM 1978, https://doi.org/10.1145/359460.359467
* [TICHY]: Tichy, "The string-to-string correction problem with block moves", ACM TOCS 1984, https://doi.org/10.1145/357401.357404
* [NUGROHO]: Nugroho, Hata, Matsumoto, "How different are different diff algorithms in Git?", EMSE 2019, https://doi.org/10.1007/s10664-019-09772-z (preprint https://arxiv.org/abs/1902.02467)
* [FRASER]: https://neil.fraser.name/writing/diff/
* [IMARA]: https://github.com/pascalkuthe/imara-diff
* [SIMILAR]: https://github.com/mitsuhiko/similar
* [GITDIFF]: https://git-scm.com/docs/git-diff
* [GITATTR]: https://git-scm.com/docs/gitattributes#_defining_an_external_diff_driver
* [GNUDIFF]: https://www.gnu.org/software/diffutils/manual/html_node/Invoking-diff.html
* [PDFDIFF]: https://github.com/JoshData/pdf-diff
* [PDFDIFF-SRC]: https://github.com/JoshData/pdf-diff/blob/primary/pdf_diff/command_line.py
* [PIXELMATCH]: https://github.com/mapbox/pixelmatch

### Document analysis

* [DOCSTRUM]: O'Gorman, "The document spectrum for page layout analysis", IEEE TPAMI 1993, https://doi.org/10.1109/34.244677
* [TATR]: Smock, Pesala, Abraham, "PubTables-1M", CVPR 2022, https://doi.org/10.1109/CVPR52688.2022.00459
* [FNP21]: https://preview.aclanthology.org/fix_video/2021.fnp-1.13.pdf (summarizes Déjean and Meunier 2006)
* [US7937653]: https://patents.google.com/patent/US7937653
* [ARXIV-YIN]: https://arxiv.org/abs/2607.14117
* [SURVEY-IMG]: https://arxiv.org/abs/2307.07691
* [UAX29]: https://www.unicode.org/reports/tr29/
* [UAX15]: https://www.unicode.org/reports/tr15/

[P1]: https://docs.inetsoftware.de/pdfc/help/pdfc.programming.profiles/p/comparison-profile
[P2]: https://docs.inetsoftware.de/pdfc/help/pdfc.gui.profiles/p/comparison-profile
[P3]: https://docs.inetsoftware.de/pdfc/help/pdfc.gui.regex/p/regular-expressions-filter
[P4]: https://docs.inetsoftware.de/pdfc/help/diffmerge/p/combine-differences
[P5]: https://docs.inetsoftware.de/pdfc/help/pluginstore.page.pdfc/p/i-net-pdfc-core
[P6]: https://docs.inetsoftware.de/pdfc/help/comparisonapi-guid-result/p/guid-result-type
[P7]: https://docs.inetsoftware.de/pdfc/help/command-line-access?locale=en
[P8]: https://apidoc.inetsoftware.de/java.pdfc/com/inet/pdfc/PDFC.PROP.html
[P9]: https://www.inetsoftware.de/en/i-net-pdfc/whitepaper/
[P10]: https://docs.inetsoftware.de/pdfc/help/pdfc.limitations/p/limitations
[P11]: https://docs.inetsoftware.de/pdfc/help/parser.pdf.config
[L-PDFIUM]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE
[H-TEXT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h
[H-EDIT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_edit.h
[H-STRUCT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_structtree.h
[H-PPO]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_ppo.h
[H-ANNOT]: https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_annot.h
[PDFIUM-BIN]: https://github.com/bblanchon/pdfium-binaries/releases
[PR-README]: https://github.com/ajrcarey/pdfium-render
[GOPDFIUM]: https://github.com/klippa-app/go-pdfium
[PDFJS-API]: https://github.com/mozilla/pdf.js/blob/master/src/display/api.js
[PDFBOX-TP]: https://javadoc.io/static/org.apache.pdfbox/pdfbox/3.0.6/org/apache/pdfbox/text/TextPosition.html
[PDFBOX-LU]: https://javadoc.io/static/org.apache.pdfbox/pdfbox/3.0.6/org/apache/pdfbox/multipdf/LayerUtility.html
[HAYRO]: https://github.com/LaurenzV/hayro
[HAYRO-SYN]: https://github.com/LaurenzV/hayro/blob/main/hayro-syntax/README.md
[HAYRO-DEV]: https://github.com/LaurenzV/hayro/blob/main/hayro-interpret/src/device.rs
[HAYRO-FONT]: https://github.com/LaurenzV/hayro/blob/main/hayro-interpret/src/font/mod.rs
[KRILLA]: https://github.com/LaurenzV/krilla
[KRILLA-SRC]: https://github.com/LaurenzV/krilla/blob/main/crates/krilla/src/pdf.rs
[KRILLA-ANN]: https://github.com/LaurenzV/krilla/blob/main/crates/krilla/src/interactive/annotation.rs
[PDFW-ANN]: https://github.com/typst/pdf-writer/blob/main/src/annotations.rs
[QPDF-H]: https://github.com/qpdf/qpdf/blob/main/include/qpdf/QPDFPageObjectHelper.hh
[PIKE-OV]: https://pikepdf.readthedocs.io/en/latest/topics/overlays.html
[PDFCPU]: https://github.com/pdfcpu/pdfcpu
[PDFCPU-EX]: https://pdfcpu.io/extract/extract
[OXIDE]: https://github.com/yfedoseev/pdf_oxide
[PDFMINER-LA]: https://github.com/pdfminer/pdfminer.six/blob/master/pdfminer/layout.py
[PDFPLUMBER]: https://github.com/jsvine/pdfplumber
[PYPDF-BENCH]: https://github.com/py-pdf/benchmarks
[TY1]: https://typst.app/docs/guides/accessibility/
[GH]: https://docs.github.com/rest/repos/repos#get-a-repository
[CR]: https://crates.io/
[PYPI]: https://pypi.org/
[NPM]: https://www.npmjs.com/
[CONDA]: https://anaconda.org/conda-forge
[MYERS]: https://doi.org/10.1007/BF01840446
[HECKEL]: https://doi.org/10.1145/359460.359467
[TICHY]: https://doi.org/10.1145/357401.357404
[NUGROHO]: https://doi.org/10.1007/s10664-019-09772-z
[FRASER]: https://neil.fraser.name/writing/diff/
[IMARA]: https://github.com/pascalkuthe/imara-diff
[SIMILAR]: https://github.com/mitsuhiko/similar
[GITDIFF]: https://git-scm.com/docs/git-diff
[GITATTR]: https://git-scm.com/docs/gitattributes#_defining_an_external_diff_driver
[GNUDIFF]: https://www.gnu.org/software/diffutils/manual/html_node/Invoking-diff.html
[PDFDIFF]: https://github.com/JoshData/pdf-diff
[PDFDIFF-SRC]: https://github.com/JoshData/pdf-diff/blob/primary/pdf_diff/command_line.py
[PIXELMATCH]: https://github.com/mapbox/pixelmatch
[DOCSTRUM]: https://doi.org/10.1109/34.244677
[TATR]: https://doi.org/10.1109/CVPR52688.2022.00459
[FNP21]: https://preview.aclanthology.org/fix_video/2021.fnp-1.13.pdf
[US7937653]: https://patents.google.com/patent/US7937653
[ARXIV-YIN]: https://arxiv.org/abs/2607.14117
[SURVEY-IMG]: https://arxiv.org/abs/2307.07691
[UAX29]: https://www.unicode.org/reports/tr29/
[UAX15]: https://www.unicode.org/reports/tr15/
