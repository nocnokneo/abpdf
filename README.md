# abpdf

Compare `a.pdf` with `b.pdf` and get a side-by-side redline PDF.

abpdf compares the text and the figures of two PDFs as one continuous flow, so content that moves to another page because something was inserted before it is not reported as a change.
It is meant for revisions of generated documents: specifications, manuals, reports.

**Status: under construction.**
[docs/design.md](docs/design.md) describes what is planned and why, and its order of work shows what is done.

## Usage

    abpdf old.pdf new.pdf -o redline.pdf

abpdf lists the changes on standard output, in the order of the new document:

    @ old p. 3 / new p. 4
    - shall be calibrated annually
    + must be verified every month
    @ old p. 2 / new p. 7
    > A paragraph that moved from the second page to the seventh.
    @ old p. 9 / new p. 9
    ~ size 10 pt → 12 pt: A sentence that is set larger now.
    @ old p. 11 / new p. 11
    - [image, 640 × 480 pixels]
    + [image, 640 × 480 pixels]
    4 changes

Like `diff`, it exits with 0 if there are none, with 1 if there are some and with 2 if the comparison failed.

| Option | Effect |
| -- | -- |
| `-o`, `--output FILE` | Write the redline to `FILE`. |
| `--only-if-different` | Write the redline only if there are changes, and remove `FILE` if there are none. |
| `--audit` | Show in the redline what was left out of the comparison. |
| `--json FILE` | Write the changes to `FILE` as JSON. |
| `--profile FILE` | Read the settings of the comparison from `FILE`. |

### The redline

The redline is a PDF that starts with a summary of the changes.
It is written even if there are none, as a summary that says so, unless `--only-if-different` is given.
The entries of the summary and the bookmarks of the PDF link to the changes.

After the summary come the sheets.
A sheet shows an old page on the left and a new page on the right, with removed text marked in red, added text in green, moved text in blue and text in another style in orange.
A figure is marked like text, with a border around it.
A band across the gutter joins the two sides of a change and has its number in the summary.

- A change is shown on the pages where it starts.
- Any other page that a change is on is shown next to the page of the other document that has most of the same text, or alone if no page has any of it.
  The band of the change ends in the gutter on a sheet that shows one of its sides only.
- Pages without a change are left out, and a note says which.
- A mark covers what changed and nothing else: text that was left out of the comparison is not marked where it stands between words that changed.

With `--audit`, the redline also shows what was left out of the comparison, to check the profile with.
Text that was left out has an outline in a color that tells why, which the summary explains, and the pages without a change are shown too.

### What is compared

abpdf compares words and punctuation marks, and ignores how the text is broken into lines and pages.

Text that the producer of the PDF marked as an artifact is not compared.
In a tagged PDF from Typst that covers headers, footers, the leaders of the table of contents, table headers that are repeated after a page break, and text in `pdf.artifact`.

Where nothing is marked, abpdf tells such page furniture by its place and does not compare it either:

- A running header or footer is a line at the top or the bottom of a page that one of the two pages before or after it has too, at the same height and with the same words.
  A number in it may count the pages.
  Running headers and footers are only looked for in a PDF without artifacts.
- The header of a table that continues from the page before is one of the first lines of a page that also stand on the page before, with the same words at the same places across the page.

Text at such a place is still compared where the other document has the same text and not as page furniture.

Text that stands at another place in the new document is reported as moved, if it has at least 20 characters.
Where moved text also changed a little, what changed is reported as a change of its own.

Text that both documents have is reported as restyled where its style differs:

- the font, by its name
- the font size
- the color
- the rotation
- the weight and the slant, where the font has the same name

Words in a row that differ in the same way are one change.

Figures are compared like words.
A figure is a picture that the producer of the PDF keeps together:

- an image
- a drawing that the PDF holds as one object, which is what a PDF file that a document includes becomes
- in a tagged PDF, the shapes that one figure tag holds

A page can also draw a diagram shape by shape, with nothing that keeps the shapes together.
That is how a diagram arrives that is made with the drawing commands of Typst.
Such shapes are one drawing if the page draws them one after another, with nothing between them but text that stands in one of them, such as the label of a box.
Shapes that only rule and box text are no drawing: the lines of a table, the line under underlined text, the box behind highlighted text and the frame around a note.
Boxes with text in them are a drawing if lines join them, or if a shape without text is drawn with them, such as the head of an arrow.

A figure is reported as replaced if it looks different: if it has other pixels, other shapes, or other colors or line widths.
Where on its page it stands does not matter.
A change names a figure as what it is, such as `[image, 640 × 480 pixels]` or `[drawing, 12 parts]`.
Text in a figure is compared like any other text.

Whether shapes are a drawing is a guess, because the PDF does not say, and it has limits:

- A diagram of nothing but thin straight lines is not compared, and neither is one of nothing but boxes with text in them.
- A table whose cells are filled and ruled is taken for a drawing, unless tags say that its text is in table cells.
- A shape in a header or in the background of a page is compared like any other, unless the PDF marks it as part of the page.

In a tagged PDF, a change does not go from one table cell into the next: where the text of several cells changes, each cell is a change of its own.
Without tags nothing tells the cells apart, and changed cells that follow each other are one change.

### Profile

A profile is a TOML file with what else to ignore.
Every setting is optional.

```toml
# Text within these distances of the top and the bottom edge of a page,
# for headers and footers that are not found by themselves.
header_height_points = 95
footer_height_points = 50

# Text that one of these regular expressions matches.
ignore_patterns = [
    # Page references such as "(p. 12)": only the number is ignored.
    '\(p\. (\d+)\)',
    # Section numbers at the start of a line.
    '(?m)^(\d+(?:\.\d+)+) ',
]
```

- A pattern is searched in the text of each page, in which the lines of the page are separated by a newline.
- If a pattern has capture groups, only what the groups capture is ignored, and the rest of the match only says where to look.
- A word is ignored as a whole even if only part of it is matched.
- A word that a line break splits is searched as one word with its hyphen, such as `well-known`.
  The hyphen is left out only where the PDF marks it as no part of the word.
- The syntax is that of the [regex](https://docs.rs/regex/latest/regex/#syntax) crate.

### JSON

```json
{
  "changes": [
    {
      "kind": "replaced",
      "old": {
        "text": "annually",
        "areas": [{ "page": 3, "left": 72.0, "top": 90.5, "right": 110.2, "bottom": 102.5 }]
      },
      "new": {
        "text": "every month",
        "areas": [{ "page": 3, "left": 72.0, "top": 90.5, "right": 131.9, "bottom": 102.5 }]
      }
    }
  ]
}
```

- The changes are in the order of the new document.
- `kind` is `inserted`, `deleted`, `replaced`, `moved` or `restyled`.
- Text that moved has its old place in `old` and its new place in `new`.
- Text that is restyled has a list `style` of what differs, such as `{ "property": "size", "old": 10.0, "new": 12.0 }`.
  The property is `font`, `size`, `weight`, `italic`, `color` or `rotation`.
- A side has one area for each line that its text is on, and one for each figure.
  Pages are numbered from 1, and the edges of an area are distances in points from the left and the top edge of its page.
- A figure is in `text` as what it is, such as `[image, 640 × 480 pixels]`, and its area has `"figure": true`.
- The side of a change that has no text has one area of no width, at the place where the text of the other side would be.
  It has no area if its document has no text at all.

## Development

[mise](https://mise.jdx.dev) installs the toolchain that `mise.toml` pins: Rust, Typst and the PDFium library.

    mise install
    mise run check

`mise run check` is what CI runs.
The dev container in `.devcontainer` has the same toolchain.

abpdf loads PDFium at run time, from the path in `ABPDF_PDFIUM_LIBRARY`.
mise sets that variable for what it runs, including `cargo`, so start a built binary with `cargo run` or `mise exec`.

The tests read the PDFs in `tests/corpus`, which are built from the Typst sources next to them.
After changing a source, rebuild them with `mise run corpus`.

To run the comparison on documents that cannot be checked in, see [tests/private_corpus.rs](tests/private_corpus.rs).
`mise run manual` builds a synthetic manual of about 300 pages in two revisions, to measure on.

The examples show what abpdf reads from a PDF:

    cargo run --example tokens -- document.pdf
    cargo run --release --example token_overlay -- old.pdf new.pdf overlay.pdf

## License

[MIT](LICENSE)

abpdf embeds the DejaVu Sans font, which has [its own license](assets/fonts/DejaVuSans-LICENSE.txt).
