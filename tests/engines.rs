//! Whether the reader of abpdf's own gets out of the test corpus what the
//! PDFium reader gets out of it.

mod common;

use std::sync::Arc;

use abpdf::BySide;
use abpdf::change::{ChangeKind, Comparison};
use abpdf::filter::Filters;
use abpdf::flow::{Cell, FlowBuilder, RawFlow};
use abpdf::profile::Profile;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::reader::{Document, FigureKind, Reader};

use common::{corpus, corpus_path};

const DOCUMENTS: [&str; 10] = [
    "reading-tagged.pdf",
    "reading-untagged.pdf",
    "revisions-old-tagged.pdf",
    "revisions-new-tagged.pdf",
    "revisions-old-untagged.pdf",
    "revisions-new-untagged.pdf",
    "figures-old-tagged.pdf",
    "figures-new-tagged.pdf",
    "figures-old-untagged.pdf",
    "figures-new-untagged.pdf",
];

/// The pairs of revisions, each with the profile that it is compared with
/// if it has one.
const REVISIONS: [(&str, &str, Option<&str>); 6] = [
    (
        "revisions-old-tagged.pdf",
        "revisions-new-tagged.pdf",
        Some("revisions.toml"),
    ),
    ("revisions-old-tagged.pdf", "revisions-new-tagged.pdf", None),
    (
        "revisions-old-untagged.pdf",
        "revisions-new-untagged.pdf",
        Some("revisions.toml"),
    ),
    (
        "revisions-old-untagged.pdf",
        "revisions-new-untagged.pdf",
        None,
    ),
    ("figures-old-tagged.pdf", "figures-new-tagged.pdf", None),
    ("figures-old-untagged.pdf", "figures-new-untagged.pdf", None),
];

fn read_with_pdfium(pdf: Arc<Vec<u8>>) -> RawFlow {
    let reader = PdfiumReader::open(pdf).expect("the PDF opens");
    RawFlow::read(&reader).expect("the PDF reads")
}

fn read_with_own_reader(pdf: Arc<Vec<u8>>) -> RawFlow {
    let document = Document::open(pdf).expect("the PDF opens");
    let mut builder = FlowBuilder::default();
    for page in document.pages() {
        builder.add_page(&page);
    }
    builder.finish()
}

/// What a reader says of a token apart from where it is, which the
/// readers measure a little differently.
type Said<'a> = (&'a str, bool, Option<Cell>, Option<FigureKind>);

fn said_of(flow: &RawFlow) -> Vec<Said<'_>> {
    let tokens = flow.tokens.iter();
    tokens
        .map(|token| {
            let figure = token.figure.as_ref().map(|figure| figure.kind);
            (token.text.as_str(), token.is_artifact, token.cell, figure)
        })
        .collect()
}

#[test]
fn readers_agree_on_whether_a_document_is_tagged_and_on_its_pages() {
    for file_name in DOCUMENTS {
        let pdfium = PdfiumReader::open(corpus(file_name)).expect("the PDF opens");
        let document = Document::open(corpus(file_name)).expect("the PDF opens");
        assert_eq!(document.is_tagged(), pdfium.is_tagged(), "{file_name}");
        assert_eq!(document.page_count(), pdfium.page_count(), "{file_name}");
        assert_eq!(
            read_with_own_reader(corpus(file_name)).page_sizes,
            read_with_pdfium(corpus(file_name)).page_sizes,
            "{file_name}"
        );
    }
}

#[test]
fn readers_read_the_same_tokens() {
    for file_name in DOCUMENTS {
        let own = read_with_own_reader(corpus(file_name));
        let with_pdfium = read_with_pdfium(corpus(file_name));
        assert_eq!(said_of(&own), said_of(&with_pdfium), "{file_name}");
    }
}

#[test]
fn readers_put_tokens_in_the_same_places() {
    // PDFium adds what a glyph draws out of the box that its font reserves
    // for it, which is less than a point.
    const TOLERANCE: f32 = 1.0;
    // The one hyphen that PDFium reports in place of a soft hyphen comes
    // without the box of its font.
    const HYPHEN: &str = "-";
    for file_name in DOCUMENTS {
        let own = read_with_own_reader(corpus(file_name));
        let with_pdfium = read_with_pdfium(corpus(file_name));
        for (token, other) in own.tokens.iter().zip(&with_pdfium.tokens) {
            let (bounds, other_bounds) = (token.fragment.bounds, other.fragment.bounds);
            let differences = [
                bounds.left - other_bounds.left,
                bounds.top - other_bounds.top,
                bounds.right - other_bounds.right,
                bounds.bottom - other_bounds.bottom,
            ];
            let is_close = differences
                .iter()
                .all(|difference| difference.abs() < TOLERANCE);
            assert!(
                is_close || token.text == HYPHEN,
                "{file_name}: {:?} at {bounds:?} and {other_bounds:?}",
                token.text
            );
        }
    }
}

#[test]
fn comparisons_find_the_same_changes_with_either_reader() {
    type Edit = (ChangeKind, Option<String>, Option<String>);
    fn edits(comparison: &Comparison) -> Vec<Edit> {
        let changes = comparison.changes.iter();
        changes
            .map(|change| {
                let texts = change
                    .sides
                    .as_ref()
                    .map(|side| side.text().map(str::to_owned));
                (change.kind(), texts.old, texts.new)
            })
            .collect()
    }
    for (old, new, profile) in REVISIONS {
        let filters = match profile {
            Some(profile) => {
                Profile::load(&corpus_path(profile))
                    .expect("the profile loads")
                    .filters
            }
            None => Filters::default(),
        };
        let pdfs = || BySide {
            old: corpus(old),
            new: corpus(new),
        };
        let own = Comparison::of(pdfs().map(read_with_own_reader), &filters);
        let with_pdfium = Comparison::of(pdfs().map(read_with_pdfium), &filters);
        assert!(!own.changes.is_empty(), "{old}");
        assert_eq!(edits(&own), edits(&with_pdfium), "{old} with {profile:?}");
    }
}
