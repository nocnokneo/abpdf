//! Writes two PDFs side by side, page by page, with every token outlined, to
//! see whether abpdf reads the pages the way a viewer shows them.
//! Tokens that the producer marked as artifacts are highlighted in gray.
//!
//!     cargo run --release --example token_overlay -- OLD.pdf NEW.pdf OUTPUT.pdf
//!
//! Also prints how long each stage took and how large the output is.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs};

use abpdf::BySide;
use abpdf::flow::Flow;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{self, Mark, MarkStyle, Redline, Rgb, Sheet, SheetPage};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [old_path, new_path, output_path] = arguments.as_slice() else {
        return Err("usage: token_overlay OLD.pdf NEW.pdf OUTPUT.pdf".into());
    };
    let old_pdf = Arc::new(fs::read(old_path)?);
    let new_pdf = Arc::new(fs::read(new_path)?);

    let started = Instant::now();
    let old_flow = Flow::read(&PdfiumReader::open(old_pdf.clone())?)?;
    let new_flow = Flow::read(&PdfiumReader::open(new_pdf.clone())?)?;
    let read = Instant::now();

    let mut old_pages = sheet_pages(old_path, &old_flow).into_iter();
    let mut new_pages = sheet_pages(new_path, &new_flow).into_iter();
    let mut redline = Redline::default();
    loop {
        let sheet = Sheet {
            pages: BySide {
                old: old_pages.next(),
                new: new_pages.next(),
            },
            ..Sheet::default()
        };
        if sheet == Sheet::default() {
            break;
        }
        redline.sheets.push(sheet);
    }
    let pdfs = BySide {
        old: old_pdf.clone(),
        new: new_pdf.clone(),
    };
    let output = redline::write(&redline, pdfs)?;
    let written = Instant::now();
    fs::write(output_path, &output)?;

    println!(
        "old: {} pages, {} tokens, {} bytes",
        old_flow.page_sizes.len(),
        old_flow.tokens.len(),
        old_pdf.len()
    );
    println!(
        "new: {} pages, {} tokens, {} bytes",
        new_flow.page_sizes.len(),
        new_flow.tokens.len(),
        new_pdf.len()
    );
    println!(
        "output: {} sheets, {} bytes",
        redline.sheets.len(),
        output.len()
    );
    println!(
        "read {:.2} s, write {:.2} s",
        (read - started).as_secs_f64(),
        (written - read).as_secs_f64()
    );
    Ok(())
}

/// One sheet page per page of the document, with a mark per token fragment.
fn sheet_pages(path: &str, flow: &Flow) -> Vec<SheetPage> {
    let file_name = Path::new(path)
        .file_name()
        .map_or(path.into(), |name| name.to_string_lossy());
    let page_count = flow.page_sizes.len();
    let mut pages: Vec<SheetPage> = (0..page_count)
        .map(|page_index| SheetPage {
            page_index,
            label: format!("{file_name} - page {} of {page_count}", page_index + 1),
            marks: Vec::new(),
        })
        .collect();
    for token in &flow.tokens {
        for fragment in &token.fragments {
            pages[fragment.page_index].marks.push(Mark {
                bounds: fragment.bounds,
                color: if token.is_artifact {
                    Rgb::EXCLUDED
                } else {
                    Rgb::MOVED
                },
                style: if token.is_artifact {
                    MarkStyle::Highlight
                } else {
                    MarkStyle::Outline
                },
            });
        }
    }
    pages
}
