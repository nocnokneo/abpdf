//! Shared by the integration tests.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use abpdf::change::Comparison;
use abpdf::filter::Filters;
use abpdf::flow::{self, Flow, Token};
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{self, Redline};

/// The path of a file of the test corpus.
pub fn corpus_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(file_name)
}

/// A PDF of the test corpus, which `mise run corpus` builds from the Typst
/// sources next to it.
pub fn corpus(file_name: &str) -> Arc<Vec<u8>> {
    let path = corpus_path(file_name);
    Arc::new(fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
}

/// The flow of a PDF as it is read.
pub fn raw_flow_of(pdf: Arc<Vec<u8>>) -> Flow {
    let reader = PdfiumReader::open(pdf).expect("the PDF opens");
    Flow::read(&reader).expect("the PDF reads")
}

/// The flow of a PDF as the comparison sees it.
pub fn flow_of(pdf: Arc<Vec<u8>>) -> Flow {
    let [flow] = flow::prepare([raw_flow_of(pdf)], &Filters::default());
    flow
}

pub fn compare(old_pdf: Arc<Vec<u8>>, new_pdf: Arc<Vec<u8>>, filters: &Filters) -> Comparison {
    Comparison::of(raw_flow_of(old_pdf), raw_flow_of(new_pdf), filters)
}

/// The comparison of two PDFs and the redline that shows it, as a PDF.
pub fn compare_and_redline(
    old_pdf: Arc<Vec<u8>>,
    new_pdf: Arc<Vec<u8>>,
    filters: &Filters,
) -> (Comparison, Redline, Vec<u8>) {
    let comparison = compare(old_pdf.clone(), new_pdf.clone(), filters);
    let redline = Redline::of(&comparison, "old.pdf", "new.pdf");
    let pdf = redline::write(&redline, old_pdf, new_pdf).expect("the redline is written");
    (comparison, redline, pdf)
}

pub fn texts<'a>(tokens: impl IntoIterator<Item = &'a Token>) -> Vec<&'a str> {
    tokens
        .into_iter()
        .map(|token| token.text.as_str())
        .collect()
}

/// The first token with this text.
pub fn token<'a>(flow: &'a Flow, text: &str) -> &'a Token {
    flow.tokens
        .iter()
        .find(|token| token.text == text)
        .unwrap_or_else(|| panic!("no token {text:?}"))
}
