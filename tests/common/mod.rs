//! Shared by the integration tests.
#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use abpdf::filter::Filters;
use abpdf::flow::{Flow, Token, Vocabulary};
use abpdf::reader::pdfium::PdfiumReader;

/// A PDF of the test corpus, which `mise run corpus` builds from the Typst
/// sources next to it.
pub fn corpus(file_name: &str) -> Arc<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(file_name);
    Arc::new(fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
}

/// The flow of a PDF as the comparison sees it.
pub fn flow_of(pdf: Arc<Vec<u8>>) -> Flow {
    let reader = PdfiumReader::open(pdf).expect("the PDF opens");
    let mut flow = Flow::read(&reader).expect("the PDF reads");
    Filters::default().apply(&mut flow);
    let vocabulary = Vocabulary::of([&flow]);
    flow.join_broken_words(&vocabulary);
    flow
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
