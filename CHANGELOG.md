# Changelog

What changes in abpdf from one release to the next, for those who use it.
The format is that of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-24

The first release.

### Added

- `abpdf old.pdf new.pdf` compares the text and the figures of two PDFs as one flow, so that what moves to another page because something was inserted before it is no change.
- The changes are listed on standard output, and the exit status says whether there are any: 0 for none, 1 for some and 2 if the comparison failed.
- A page that goes far beyond what honest pages draw, such as forms that draw forms over and over, is not read: the comparison fails with a message that names the page and the limit.
- `-o` writes a redline PDF: a summary of the changes with links to them, and sheets that show an old page next to a new page with the changes marked.
- `--json` writes the changes with their kinds, their texts and their places on the pages.
- Text that stands at another place is reported as moved, and text in another font, size, color or rotation as restyled.
- Figures are compared like words: images, included PDFs, the shapes of a tagged figure, and diagrams that a page draws shape by shape.
- Text that the producer of a PDF marks as an artifact is left out, and without marks running headers and footers and repeated table headers are found by their place.
- A profile, given with `--profile`, leaves out more: text near the top and the bottom edge of a page, and text that a regular expression matches.
- `--audit` shows in the redline what was left out and why.
- abpdf is one program without anything to install next to it, for Linux, Windows and macOS.
