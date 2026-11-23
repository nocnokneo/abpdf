//! Shared by the integration tests.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use abpdf::BySide;
use abpdf::change::Comparison;
use abpdf::filter::Filters;
use abpdf::flow::{self, Flow, RawFlow, Token};
use abpdf::reader::Document;
use abpdf::redline::{self, Redline};

/// How many changes the edits of `tests/corpus/revisions.typ` have to be
/// reported as where the revisions are tagged.
pub const REVISIONS_CHANGE_COUNT: usize = 17;

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
pub fn raw_flow_of(pdf: Arc<Vec<u8>>) -> RawFlow {
    RawFlow::read(&Document::open(pdf).expect("the PDF opens"))
}

/// The flow of a PDF as a comparison with itself sees it.
pub fn flow_of(pdf: Arc<Vec<u8>>) -> Flow {
    flow::prepare(both(raw_flow_of(pdf)), &Filters::default()).new
}

/// The same value for both documents.
pub fn both<T: Clone>(value: T) -> BySide<T> {
    BySide {
        old: value.clone(),
        new: value,
    }
}

pub fn compare(pdfs: BySide<Arc<Vec<u8>>>, filters: &Filters) -> Comparison {
    Comparison::of(pdfs.map(raw_flow_of), filters)
}

/// The comparison of two PDFs and the redline that shows it, as a PDF.
pub fn compare_and_redline(
    pdfs: BySide<Arc<Vec<u8>>>,
    filters: &Filters,
) -> (Comparison, Redline, Vec<u8>) {
    let comparison = compare(pdfs.clone(), filters);
    let names = BySide {
        old: "old.pdf",
        new: "new.pdf",
    };
    let redline = Redline::of(&comparison, names);
    let pdf = redline::write(&redline, pdfs).expect("the redline is written");
    (comparison, redline, pdf)
}

/// The text that each page of a PDF reads as, with a space between its
/// tokens.
pub fn page_texts(pdf: Arc<Vec<u8>>) -> Vec<String> {
    let flow = raw_flow_of(pdf);
    let mut pages = vec![Vec::new(); flow.page_sizes.len()];
    for token in &flow.tokens {
        pages[token.fragment.page_index].push(token.text.as_str());
    }
    pages.iter().map(|texts| texts.join(" ")).collect()
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
