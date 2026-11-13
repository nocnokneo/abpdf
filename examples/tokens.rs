//! Prints the tokens of a PDF, one per line, to see what abpdf compares.
//!
//!     cargo run --example tokens -- document.pdf

use std::error::Error;
use std::sync::Arc;
use std::{env, fs};

use abpdf::flow::{Flow, Gap};
use abpdf::reader::pdfium::PdfiumReader;

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: tokens DOCUMENT.pdf")?;
    let reader = PdfiumReader::open(Arc::new(fs::read(path)?))?;
    let mut flow = Flow::read(&reader)?;
    flow.join_broken_words();

    for token in &flow.tokens {
        let first = token.fragments[0];
        let gap = match token.gap {
            Gap::None => "",
            Gap::Space => "_",
            Gap::LineBreak => "/",
        };
        let style = &flow.styles[token.style];
        println!(
            "{}:{}\t{gap}\t{}\t{}\t{}\t{} {}",
            first.page_index + 1,
            first.line_index + 1,
            token.text,
            if token.is_artifact { "artifact" } else { "" },
            token
                .structure_path
                .map_or("", |index| &flow.structure_paths[index]),
            style.font_name,
            style.font_size,
        );
    }
    Ok(())
}
