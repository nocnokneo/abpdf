//! The whole comparison, on the revisions of `tests/corpus/revisions.typ`.

mod common;

use std::sync::Arc;

use abpdf::change::{ChangeKind, ChangeSide, Comparison};
use abpdf::filter::Filters;
use abpdf::flow::Exclusion;
use abpdf::profile::Profile;
use abpdf::{BySide, Side};

use common::page_texts;
use common::{REVISIONS_CHANGE_COUNT, both, compare, compare_and_redline, corpus, corpus_path};

use ChangeKind::{Deleted, Inserted, Moved, Replaced};

const STORAGE_OLD: &str =
    "Store the instrument in its case when it is not in use, in a dry room that is free of dust.";
const STORAGE_NEW: &str =
    "Store the instrument in its case when it is not in use, in a cool room that is free of dust.";
const CHECK: &str = "Check the supply voltage at the start of every series of measurements.";

/// What the edits in the source have to be reported as.
const EDITS: [(ChangeKind, &str, &str); REVISIONS_CHANGE_COUNT] = [
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
    // Of the two paragraphs that change places, the shorter one moved.
    (Moved, CHECK, CHECK),
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
    // The paragraph that moved to the last section, and the word that
    // changed in it.
    (Moved, STORAGE_OLD, STORAGE_NEW),
    (Replaced, "dry", "cool"),
];

/// The old and the new revision, with or without tags.
fn revisions(tagging: &str) -> BySide<Arc<Vec<u8>>> {
    BySide::from_fn(|side| corpus(&format!("revisions-{side}-{tagging}.pdf")))
}

fn profile() -> Profile {
    Profile::load(&corpus_path("revisions.toml")).expect("the profile loads")
}

fn compare_revisions(tagging: &str, filters: &Filters) -> Comparison {
    compare(revisions(tagging), filters)
}

fn summaries(comparison: &Comparison) -> Vec<(ChangeKind, &str, &str)> {
    comparison
        .changes
        .iter()
        .map(|change| {
            let texts = change.sides.as_ref().map(ChangeSide::text);
            (
                change.kind(),
                texts.old.unwrap_or_default(),
                texts.new.unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn revisions_differ_by_their_edits() {
    for tagging in ["tagged", "untagged"] {
        let comparison = compare_revisions(tagging, &profile().filters);
        assert_eq!(summaries(&comparison), EDITS, "{tagging}");
    }
}

#[test]
fn table_header_that_a_page_break_repeats_is_left_out_without_tags() {
    // The table breaks one row later in the new revision, and without tags
    // nothing but its place says that the header after the break is a repeat.
    let comparison = compare_revisions("untagged", &profile().filters);
    for side in Side::BOTH {
        let excluded = comparison.flows[side].excluded.iter();
        let repeats: Vec<&str> = excluded
            .filter(|excluded| excluded.exclusion == Exclusion::Repeat)
            .map(|excluded| excluded.token.text.as_str())
            .collect();
        assert_eq!(repeats, ["Parameter", "Meaning", "Default"], "{side}");
    }
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
        let comparison = compare(both(revisions(tagging).new), &Filters::default());
        assert_eq!(comparison.changes, []);
    }
}

#[test]
fn change_is_located_on_its_text() {
    let comparison = compare_revisions("tagged", &profile().filters);
    let replaced = &comparison.changes[2];
    let word = comparison
        .flows
        .old
        .tokens
        .iter()
        .find(|token| token.text == "calibrated")
        .expect("the old revision has the word");
    let [area] = replaced.sides.old.areas() else {
        panic!("{:?}", replaced.sides.old);
    };
    let fragment = word.fragments[0];
    assert_eq!(area.page_index, fragment.page_index);
    assert!(area.bounds.left < fragment.bounds.left && fragment.bounds.right < area.bounds.right);
    assert_eq!(
        (area.bounds.top, area.bounds.bottom),
        (fragment.bounds.top, fragment.bounds.bottom)
    );
}

#[test]
fn redline_lists_the_changes_and_shows_their_pages() {
    let (_, redline, output) = compare_and_redline(revisions("tagged"), &profile().filters);

    let pages = page_texts(Arc::new(output));
    assert_eq!(pages.len(), 1 + redline.sheets.len());

    let summary = &pages[0];
    assert!(
        summary.starts_with("old.pdf → new.pdf 14 changes on 5 sheets"),
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

    let first_sheet = &pages[1];
    assert!(
        first_sheet.contains("old.pdf — page 1 of 3"),
        "{first_sheet}"
    );
    assert!(
        first_sheet.contains("new.pdf — page 1 of 4"),
        "{first_sheet}"
    );
    // The source pages are there as text, both of them.
    assert_eq!(first_sheet.matches("Operating manual").count(), 2);
    // The last sheet shows the paragraph that moved, where it was and where
    // it is.
    let last_sheet = pages.last().unwrap();
    assert!(
        last_sheet.contains("old.pdf — page 1 of 3")
            && last_sheet.contains("new.pdf — page 4 of 4"),
        "{last_sheet}"
    );
    assert_eq!(
        last_sheet
            .matches("Store the instrument in its case")
            .count(),
        2
    );
}

#[test]
fn redline_is_the_same_every_time() {
    for tagging in ["tagged", "untagged"] {
        let write = || compare_and_redline(revisions(tagging), &profile().filters).2;
        assert_eq!(write(), write());
    }
}

#[test]
fn redline_of_equal_documents_is_a_summary_that_says_so() {
    let pdfs = both(revisions("tagged").old);
    let (_, _, output) = compare_and_redline(pdfs, &Filters::default());

    let pages = page_texts(Arc::new(output));
    assert_eq!(pages.len(), 1);
    assert!(pages[0].ends_with("No differences"), "{}", pages[0]);
}
