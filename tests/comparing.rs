//! The whole comparison, on the revisions of `tests/corpus/revisions.typ`.

mod common;

use std::path::Path;
use std::sync::Arc;

use abpdf::change::{ChangeKind, Comparison};
use abpdf::filter::Filters;
use abpdf::profile::Profile;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{self, Redline};

use common::{corpus, flow_of, texts};

use ChangeKind::{Deleted, Inserted, Replaced};

/// What the edits in the source have to be reported as.
const EDITS: [(ChangeKind, &str, &str); 11] = [
    (Inserted, "", "Monitoring"),
    (
        Inserted,
        "",
        "The second revision of the instrument adds a monitoring interface. It reports the \
         internal temperature and the remaining lifetime of the light source, and it is \
         described in the section on maintenance.",
    ),
    (
        Replaced,
        "shall be calibrated annually",
        "must be verified every month",
    ),
    (
        Inserted,
        "",
        "Monitoring The monitoring interface is read only.",
    ),
    (Deleted, "Notes are set in italic type.", ""),
    (
        Inserted,
        "",
        "Monitoring interface Figure: The monitoring interface",
    ),
    (Replaced, "250", "300"),
    (Inserted, "", "P19 Interval of the monitoring interface 60"),
    (Deleted, ",", ""),
    (Replaced, ".", "!"),
    (Replaced, "Notwithstanding", "Despite"),
];

fn revision(revision: &str, tagging: &str) -> Arc<Vec<u8>> {
    corpus(&format!("revisions-{revision}-{tagging}.pdf"))
}

fn profile() -> Profile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/revisions.toml");
    Profile::load(&path).expect("the profile loads")
}

fn compare(old: Arc<Vec<u8>>, new: Arc<Vec<u8>>, filters: &Filters) -> Comparison {
    let old = PdfiumReader::open(old).expect("the old PDF opens");
    let new = PdfiumReader::open(new).expect("the new PDF opens");
    Comparison::of(&old, &new, filters).expect("the PDFs read")
}

fn compare_revisions(tagging: &str, filters: &Filters) -> Comparison {
    compare(revision("old", tagging), revision("new", tagging), filters)
}

fn summaries(comparison: &Comparison) -> Vec<(ChangeKind, &str, &str)> {
    comparison
        .changes
        .iter()
        .map(|change| {
            (
                change.kind,
                change.old.text.as_str(),
                change.new.text.as_str(),
            )
        })
        .collect()
}

#[test]
fn tagged_revisions_differ_by_their_edits() {
    let comparison = compare_revisions("tagged", &profile().filters);
    assert_eq!(summaries(&comparison), EDITS);
}

#[test]
fn untagged_revisions_also_differ_by_the_table_header_that_a_page_break_repeats() {
    let comparison = compare_revisions("untagged", &profile().filters);
    let header = "Parameter Meaning Default";
    let mut expected = EDITS.to_vec();
    // The table breaks one row later in the new revision, and without tags
    // nothing says that the header after the break is a repeat.
    expected.splice(7..7, [(Inserted, "", header), (Deleted, header, "")]);
    assert_eq!(summaries(&comparison), expected);
}

#[test]
fn numbers_that_follow_from_the_edits_are_reported_without_a_profile() {
    let comparison = compare_revisions("tagged", &Filters::default());
    let summaries = summaries(&comparison);
    assert!(summaries.len() > EDITS.len());
    assert!(summaries.contains(&(Replaced, "1", "2")), "{summaries:?}");
}

#[test]
fn document_does_not_differ_from_itself() {
    for tagging in ["tagged", "untagged"] {
        let pdf = revision("new", tagging);
        let comparison = compare(pdf.clone(), pdf, &Filters::default());
        assert_eq!(comparison.changes, []);
    }
}

#[test]
fn change_is_located_on_its_text() {
    let comparison = compare_revisions("tagged", &profile().filters);
    let replaced = &comparison.changes[2];
    let word = comparison
        .old
        .tokens
        .iter()
        .find(|token| token.text == "calibrated")
        .expect("the old revision has the word");
    let (area, fragment) = (replaced.old.areas[0], word.fragments[0]);
    assert_eq!(replaced.old.areas.len(), 1);
    assert_eq!(area.page_index, fragment.page_index);
    assert!(area.bounds.left < fragment.bounds.left && fragment.bounds.right < area.bounds.right);
    assert_eq!(
        (area.bounds.top, area.bounds.bottom),
        (fragment.bounds.top, fragment.bounds.bottom)
    );
}

#[test]
fn redline_lists_the_changes_and_shows_their_pages() {
    let (old, new) = (revision("old", "tagged"), revision("new", "tagged"));
    let comparison = compare(old.clone(), new.clone(), &profile().filters);
    let redline = Redline::of(&comparison, "old.pdf", "new.pdf");
    let output = redline::write(&redline, old, new).expect("the redline is written");

    let flow = flow_of(Arc::new(output));
    let page_count = flow.page_sizes.len();
    assert_eq!(page_count, 1 + redline.sheets.len());
    let on_page = |page_index: usize| -> Vec<&str> {
        let tokens = flow.tokens.iter();
        texts(tokens.filter(|token| token.fragments[0].page_index == page_index))
    };

    let summary = on_page(0).join(" ");
    assert!(
        summary.starts_with("old.pdf → new.pdf 11 changes on 4 sheets"),
        "{summary}"
    );
    assert!(
        summary.contains("shall be calibrated annually"),
        "{summary}"
    );
    assert!(
        summary.contains("must be verified every month"),
        "{summary}"
    );

    let first_sheet = on_page(1).join(" ");
    assert!(
        first_sheet.contains("old.pdf — page 1 of 3"),
        "{first_sheet}"
    );
    assert!(
        first_sheet.contains("new.pdf — page 1 of 3"),
        "{first_sheet}"
    );
    // The source pages are there as text, both of them.
    assert_eq!(first_sheet.matches("Operating manual").count(), 2);
    let last_sheet = on_page(page_count - 1).join(" ");
    assert!(
        last_sheet.contains("Despite the robustness"),
        "{last_sheet}"
    );
}

#[test]
fn redline_is_the_same_every_time() {
    for tagging in ["tagged", "untagged"] {
        let write = || {
            let (old, new) = (revision("old", tagging), revision("new", tagging));
            let comparison = compare(old.clone(), new.clone(), &profile().filters);
            let redline = Redline::of(&comparison, "old.pdf", "new.pdf");
            redline::write(&redline, old, new).expect("the redline is written")
        };
        assert_eq!(write(), write());
    }
}

#[test]
fn redline_of_equal_documents_is_a_summary_that_says_so() {
    let pdf = revision("old", "tagged");
    let comparison = compare(pdf.clone(), pdf.clone(), &Filters::default());
    let redline = Redline::of(&comparison, "old.pdf", "new.pdf");
    let output = redline::write(&redline, pdf.clone(), pdf).expect("the redline is written");

    let flow = flow_of(Arc::new(output));
    assert_eq!(flow.page_sizes.len(), 1);
    assert!(texts(&flow.tokens).join(" ").ends_with("No differences"));
}
