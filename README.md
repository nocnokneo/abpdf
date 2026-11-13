# abpdf

Compare `a.pdf` with `b.pdf` and get a side-by-side redline PDF.

abpdf compares the text of two PDFs as one continuous flow, so content that moves to another page because something was inserted before it is not reported as a change.
It is meant for revisions of generated documents: specifications, manuals, reports.

**Status: under construction.**
[docs/design.md](docs/design.md) describes what is planned and why, and its order of work shows what is done.

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

The examples show what abpdf reads from a PDF:

    cargo run --example tokens -- document.pdf
    cargo run --release --example token_overlay -- old.pdf new.pdf overlay.pdf

## License

[MIT](LICENSE)

abpdf embeds the DejaVu Sans font, which has [its own license](assets/fonts/DejaVuSans-LICENSE.txt).
