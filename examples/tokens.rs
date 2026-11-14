//! Prints the tokens of a PDF that abpdf compares when it compares the PDF
//! with itself, one per line.
//!
//!     cargo run --example tokens -- document.pdf

use std::error::Error;
use std::sync::Arc;
use std::{env, fs};

use abpdf::BySide;
use abpdf::filter::Filters;
use abpdf::flow::{self, Gap, RawFlow};
use abpdf::reader::pdfium::PdfiumReader;

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: tokens DOCUMENT.pdf")?;
    let reader = PdfiumReader::open(Arc::new(fs::read(path)?))?;
    let raw = RawFlow::read(&reader)?;
    let flows = BySide {
        old: raw.clone(),
        new: raw,
    };
    let flow = flow::prepare(flows, &Filters::default()).new;

    for token in &flow.tokens {
        let first = token.fragments[0];
        let gap = match token.gap {
            Gap::None => "",
            Gap::Space => "_",
            Gap::LineBreak => "/",
        };
        println!(
            "{}:{}\t{gap}\t{}",
            first.page_index + 1,
            first.line_index + 1,
            token.text,
        );
    }
    Ok(())
}
