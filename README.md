# abpdf

Compare `a.pdf` with `b.pdf` and get a side-by-side redline PDF.

abpdf compares the text of two PDFs as one continuous flow, so content that moves to another page because something was inserted before it is not reported as a change.
It is meant for revisions of generated documents: specifications, manuals, reports.

**Status: under construction.**
[docs/design.md](docs/design.md) describes what is planned and why, and its order of work shows what is done.

## Usage

    abpdf old.pdf new.pdf -o redline.pdf

abpdf lists the changes on standard output.
Like `diff`, it exits with 0 if there are none, with 1 if there are some and with 2 if the comparison failed.

| Option | Effect |
| -- | -- |
| `-o`, `--output FILE` | Write the redline to `FILE`. |
| `--json FILE` | Write the changes to `FILE` as JSON. |
| `--profile FILE` | Read the settings of the comparison from `FILE`. |

The redline is a PDF that starts with a summary of the changes.
After it comes a sheet for each pair of pages with a change: the old page on the left and the new page on the right, with removed text marked in red and added text in green.
The entries of the summary and the bookmarks of the PDF link to the changes.

### What is compared

abpdf compares words and punctuation marks, and ignores how the text is broken into lines and pages.

Text that the producer of the PDF marked as an artifact is not compared.
In a tagged PDF from Typst that covers headers, footers, the leaders of the table of contents, table headers that are repeated after a page break, and text in `pdf.artifact`.

### Profile

A profile is a TOML file with what else to ignore.
Every setting is optional.

```toml
# Text within these distances of the top and the bottom edge of a page.
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

- `kind` is `inserted`, `deleted` or `replaced`.
- A side has one area for each line that its text is on.
  Pages are numbered from 1, and the edges of an area are distances in points from the left and the top edge of its page.
- The side of a change that has no text has one area of no width, at the place where the text of the other side would be.

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
