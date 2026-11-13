use regex::Regex;

use std::ops::Range;

use crate::BySide;
use crate::change::{Area, Change, ChangeKind, ChangeSide, Comparison};
use crate::filter::Filters;
use crate::testing::{PageBuilder, cell_bounds, comparison_of, raw_flow_of};

fn compare_with(
    old: impl IntoIterator<Item = PageBuilder>,
    new: impl IntoIterator<Item = PageBuilder>,
    filters: &Filters,
) -> Vec<Change> {
    let flows = BySide {
        old: raw_flow_of(old),
        new: raw_flow_of(new),
    };
    Comparison::of(flows, filters).changes
}

fn compare(old: &str, new: &str) -> Vec<Change> {
    comparison_of(old, new).changes
}

/// The kind and the two texts of each change.
fn summaries(changes: &[Change]) -> Vec<(ChangeKind, &str, &str)> {
    changes
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

/// The area of the cells from `columns` on `line` of page `page_index`.
fn cells(page_index: usize, line: usize, columns: Range<usize>) -> Area {
    Area {
        page_index,
        bounds: cell_bounds(line, columns),
    }
}

#[test]
fn equal_documents_have_no_changes() {
    assert_eq!(compare("one two three", "one two three"), []);
}

#[test]
fn inserted_text_is_only_on_the_new_side() {
    let changes = compare("one three", "one two three");
    assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", "two")]);
    assert_eq!(changes[0].sides.new.areas(), [cells(0, 0, 4..7)]);
}

#[test]
fn side_without_text_is_located_after_the_token_before_it() {
    let changes = compare("one three", "one two three");
    let insertion_point = Some(cells(0, 0, 3..3));
    assert_eq!(changes[0].sides.old, ChangeSide::Absent { insertion_point });
}

#[test]
fn insertion_before_all_text_is_located_at_the_first_token() {
    let changes = compare("one", "zero one");
    assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 0..0)]);
}

#[test]
fn side_without_text_is_on_the_page_that_belongs_with_the_text() {
    // The place of the added word is after `two` and before `three`, which
    // are on two pages of the old document.
    let old = || {
        [
            PageBuilder::new().text("one two"),
            PageBuilder::new().text("three four"),
        ]
    };
    let at_page_end = [
        PageBuilder::new().text("one two added"),
        PageBuilder::new().text("three four"),
    ];
    let at_page_start = [
        PageBuilder::new().text("one two"),
        PageBuilder::new().text("added three four"),
    ];
    let place_in_old = |new| {
        let changes = compare_with(old(), new, &Filters::default());
        changes[0].sides.old.areas().to_vec()
    };
    assert_eq!(place_in_old(at_page_end), [cells(0, 0, 7..7)]);
    assert_eq!(place_in_old(at_page_start), [cells(1, 0, 0..0)]);
}

#[test]
fn pages_that_share_most_unchanged_text_are_partners() {
    let flows = BySide {
        old: raw_flow_of([
            PageBuilder::new().text("a b c"),
            PageBuilder::new().text("d e f"),
            PageBuilder::new().text("old only"),
        ]),
        new: raw_flow_of([
            PageBuilder::new().text("a b"),
            PageBuilder::new().text("c d e f"),
        ]),
    };
    let comparison = Comparison::of(flows, &Filters::default());
    assert_eq!(
        comparison.page_partners,
        BySide {
            old: vec![Some(0), Some(1), None],
            new: vec![Some(0), Some(1)],
        }
    );
}

#[test]
fn side_of_a_document_without_text_is_nowhere() {
    let new = || [PageBuilder::new().text("one")];
    let nowhere = ChangeSide::Absent {
        insertion_point: None,
    };
    let empty_page = [PageBuilder::new()];
    let no_pages: [PageBuilder; 0] = [];
    let from_empty_page = compare_with(empty_page, new(), &Filters::default());
    let from_no_pages = compare_with(no_pages, new(), &Filters::default());
    for changes in [from_empty_page, from_no_pages] {
        assert_eq!(changes[0].sides.old, nowhere);
        assert_eq!(changes[0].location(), "new p. 1");
    }
}

#[test]
fn location_names_the_page_that_each_side_starts_on() {
    let old = [
        PageBuilder::new().text("keep"),
        PageBuilder::new().text("old words"),
    ];
    let new = [PageBuilder::new().text("keep new words")];
    let changes = compare_with(old, new, &Filters::default());
    assert_eq!(changes[0].location(), "old p. 2 / new p. 1");
}

#[test]
fn deleted_text_is_only_on_the_old_side() {
    let changes = compare("one two three", "one three");
    assert_eq!(summaries(&changes), [(ChangeKind::Deleted, "two", "")]);
}

#[test]
fn replaced_text_is_on_both_sides() {
    let changes = compare("calibrated annually by staff", "verified monthly by staff");
    assert_eq!(
        summaries(&changes),
        [(
            ChangeKind::Replaced,
            "calibrated annually",
            "verified monthly"
        )]
    );
}

#[test]
fn text_keeps_its_spacing() {
    let changes = compare("see p. 3, then", "see pp. 4; then");
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Replaced, "p. 3,", "pp. 4;")]
    );
}

#[test]
fn punctuation_changes_apart_from_its_word() {
    let changes = compare("press Export now", "press Export. now");
    assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", ".")]);
}

#[test]
fn change_has_one_area_per_line() {
    let changes = compare("keep old\nwords here", "keep here");
    assert_eq!(changes[0].sides.old.text(), Some("old words"));
    assert_eq!(
        changes[0].sides.old.areas(),
        [cells(0, 0, 5..8), cells(0, 1, 0..5)]
    );
}

#[test]
fn change_can_span_pages() {
    let old = [
        PageBuilder::new().text("keep old"),
        PageBuilder::new().text("words here"),
    ];
    let new = [PageBuilder::new().text("keep here")];
    let changes = compare_with(old, new, &Filters::default());
    assert_eq!(changes[0].sides.old.page_indices(), [0, 1]);
    assert_eq!(changes[0].sides.new.page_indices(), [0]);
}

#[test]
fn moving_to_other_lines_and_pages_is_not_a_change() {
    let old = [PageBuilder::new().text("one two three four")];
    let new = [
        PageBuilder::new().text("one\ntwo"),
        PageBuilder::new().text("three four"),
    ];
    assert_eq!(compare_with(old, new, &Filters::default()), []);
}

#[test]
fn word_that_breaks_at_another_place_is_not_a_change() {
    let old = [PageBuilder::new()
        .text("an inter")
        .line_end_hyphen()
        .text("national one")];
    let new = [PageBuilder::new().text("an international one")];
    assert_eq!(compare_with(old, new, &Filters::default()), []);
}

#[test]
fn compatibility_characters_compare_as_their_plain_form() {
    assert_eq!(compare("the o\u{FB03}ce", "the office"), []);
}

#[test]
fn what_the_filters_leave_out_is_not_compared() {
    let page = |revision: &str, page_number: &str| {
        PageBuilder::new()
            .artifact()
            .text(&format!("Revision {revision}\n"))
            .body()
            .text(&format!("same text (p. {page_number})"))
    };
    let filters = Filters {
        ignore_patterns: vec![Regex::new(r"\(p\. (\d+)\)").unwrap()],
        ..Filters::default()
    };
    assert_eq!(
        compare_with([page("A", "3")], [page("B", "4")], &filters),
        []
    );
    assert_eq!(
        compare_with([page("A", "3")], [page("B", "4")], &Filters::default()).len(),
        1
    );
}

#[test]
fn change_next_to_text_that_is_left_out_does_not_cover_it() {
    let page = |word: &str| {
        PageBuilder::new()
            .text(&format!("{word} "))
            .artifact()
            .text("(p. 3)")
    };
    let changes = compare_with([page("old")], [page("new")], &Filters::default());
    assert_eq!(summaries(&changes), [(ChangeKind::Replaced, "old", "new")]);
    assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 0..3)]);
}
