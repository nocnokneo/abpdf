use regex::Regex;

use std::ops::Range;

use crate::BySide;
use crate::change::{
    Area, Change, ChangeKind, ChangeSide, Comparison, Difference, StyleDifference,
};
use crate::filter::Filters;
use crate::flow::{Exclusion, Flow};
use crate::testing::{
    PARAGRAPH, PageBuilder, REST, bold_difference, cell_bounds, comparison_of,
    comparison_of_built_pages, page_with_bold, raw_flow_of,
};

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
    assert_eq!(
        changes[0].sides.old.areas(),
        [cells(0, 0, 5..8), cells(1, 0, 0..5)]
    );
    assert_eq!(changes[0].sides.new.areas(), [cells(0, 0, 4..4)]);
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

#[test]
fn change_around_text_that_is_left_out_does_not_cover_it() {
    let page = |before: &str, after: &str| {
        PageBuilder::new()
            .text(&format!("{before} "))
            .artifact()
            .text("(p. 3)")
            .body()
            .text(&format!(" {after}"))
    };
    let changes = compare_with(
        [page("old", "words")],
        [page("new", "text")],
        &Filters::default(),
    );
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Replaced, "old words", "new text")]
    );
    assert_eq!(
        changes[0].sides.old.areas(),
        [cells(0, 0, 0..3), cells(0, 0, 11..16)]
    );
}

/// The two pages of a table of four rows, of which `rows_on_first_page` come
/// before the page break and the others after the header that it repeats.
fn table_over_two_pages(header: &str, rows_on_first_page: usize) -> [PageBuilder; 2] {
    let rows = ["a 1", "b 2", "c 3", "d 4"];
    let (before, after) = rows.split_at(rows_on_first_page);
    let first_page = format!("intro text\n{header}\n{}", before.join("\n"));
    let second_page = format!("{header}\n{}\nend", after.join("\n"));
    [
        PageBuilder::new().text(&first_page),
        PageBuilder::new().text(&second_page),
    ]
}

fn excluded_as_repeats(flow: &Flow) -> Vec<&str> {
    let repeats = flow.excluded.iter();
    repeats
        .filter(|excluded| excluded.exclusion == Exclusion::Repeat)
        .map(|excluded| excluded.token.text.as_str())
        .collect()
}

#[test]
fn header_that_a_page_break_repeats_is_not_a_change_where_the_break_moves() {
    let flows = BySide {
        old: raw_flow_of(table_over_two_pages("Key Value", 2)),
        new: raw_flow_of(table_over_two_pages("Key Value", 1)),
    };
    let comparison = Comparison::of(flows, &Filters::default());
    assert_eq!(comparison.changes, []);
    assert_eq!(excluded_as_repeats(&comparison.flows.old), ["Key", "Value"]);
    assert_eq!(excluded_as_repeats(&comparison.flows.new), ["Key", "Value"]);
}

#[test]
fn header_that_only_one_document_repeats_is_not_a_change() {
    let on_one_page = [PageBuilder::new().text("intro text\nKey Value\na 1\nb 2\nc 3\nd 4\nend")];
    let over_two_pages = table_over_two_pages("Key Value", 3);
    assert_eq!(
        compare_with(on_one_page, over_two_pages, &Filters::default()),
        []
    );
}

#[test]
fn header_of_a_table_that_starts_a_page_is_compared() {
    // The header stands at the top of two pages in a row, like a running
    // header, but the other document has it as text.
    let from_the_top_of_a_page = [
        PageBuilder::new().text("intro text"),
        PageBuilder::new().text("Key Value\na 1\nb 2"),
        PageBuilder::new().text("Key Value\nc 3\nd 4\nend"),
    ];
    let changes = compare_with(
        from_the_top_of_a_page,
        table_over_two_pages("Key Value", 2),
        &Filters::default(),
    );
    assert_eq!(changes, []);
}

#[test]
fn change_to_a_running_header_or_footer_is_not_reported() {
    let pages = |revision: &str, page_count: usize| {
        ["one", "two", "three"].map(|body| {
            PageBuilder::new().text(&format!(
                "Manual revision {revision}\n{body}\nof {page_count}"
            ))
        })
    };
    let changes = compare_with(pages("A", 3), pages("B", 4), &Filters::default());
    assert_eq!(changes, []);
}

#[test]
fn change_to_a_repeated_header_is_reported_where_the_header_is_first() {
    let old = table_over_two_pages("Key Value", 2);
    let new = table_over_two_pages("Key Amount", 2);
    let changes = compare_with(old, new, &Filters::default());
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Replaced, "Value", "Amount")]
    );
    assert_eq!(changes[0].location(), "old p. 1 / new p. 1");
}

#[test]
fn line_that_stands_twice_is_not_taken_for_a_repeat() {
    // Where the pages break, the second of two equal lines starts a page.
    let on_one_page = [PageBuilder::new().text("a b\nx y\nx y\nc d")];
    let over_two_pages = || {
        [
            PageBuilder::new().text("a b\nx y"),
            PageBuilder::new().text("x y\nc d"),
        ]
    };
    let flows = BySide {
        old: raw_flow_of(on_one_page),
        new: raw_flow_of(over_two_pages()),
    };
    let comparison = Comparison::of(flows, &Filters::default());
    assert_eq!(comparison.changes, []);
    assert_eq!(excluded_as_repeats(&comparison.flows.new), [""; 0]);

    // The line is a change where the other document does not have it.
    let without_the_line = [PageBuilder::new().text("a b\nx y\nc d\nmore")];
    let with_more = [
        PageBuilder::new().text("a b\nx y"),
        PageBuilder::new().text("x y\nc d\nmore"),
    ];
    let changes = compare_with(over_two_pages(), without_the_line, &Filters::default());
    assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", "more")]);
    let changes = compare_with(with_more, over_two_pages(), &Filters::default());
    assert_eq!(summaries(&changes), [(ChangeKind::Deleted, "more", "")]);
}

#[test]
fn single_token_is_not_taken_for_a_repeat() {
    let old = [
        PageBuilder::new().text("a b\nx"),
        PageBuilder::new().text("c d"),
    ];
    let new = [
        PageBuilder::new().text("a b\nx"),
        PageBuilder::new().text("x\nc d"),
    ];
    let changes = compare_with(old, new, &Filters::default());
    assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", "x")]);
}

#[test]
fn line_at_another_place_across_the_page_is_not_taken_for_a_repeat() {
    let old = [
        PageBuilder::new().text("a b\nx y"),
        PageBuilder::new().text("c d"),
    ];
    let new = [
        PageBuilder::new().text("a b\nx y"),
        PageBuilder::new().text(" x y\nc d"),
    ];
    let changes = compare_with(old, new, &Filters::default());
    assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", "x y")]);
}

/// A page with a row of table cells that have these texts.
fn row(texts: &[&str]) -> PageBuilder {
    let mut page = PageBuilder::new();
    for (cell, text) in texts.iter().enumerate() {
        page = page.cell(cell).text(&format!("{text} "));
    }
    page
}

fn compare_rows(old: &[&str], new: &[&str]) -> Vec<Change> {
    compare_with([row(old)], [row(new)], &Filters::default())
}

#[test]
fn cells_that_change_next_to_each_other_are_a_change_each() {
    let changes = compare_rows(&["one", "two"], &["uno", "dos"]);
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Replaced, "one", "uno"),
            (ChangeKind::Replaced, "two", "dos"),
        ]
    );
    assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 0..3)]);
    assert_eq!(changes[1].sides.old.areas(), [cells(0, 0, 4..7)]);
}

#[test]
fn text_before_a_table_changes_apart_from_the_first_cell() {
    let page = |before: &str, cell: &str| {
        PageBuilder::new()
            .text(&format!("{before}\n"))
            .cell(0)
            .text(cell)
    };
    let changes = compare_with(
        [page("old", "one")],
        [page("new", "uno")],
        &Filters::default(),
    );
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Replaced, "old", "new"),
            (ChangeKind::Replaced, "one", "uno"),
        ]
    );
}

#[test]
fn short_cell_that_stays_between_cells_that_change_is_no_change() {
    // The diff takes a match as short as this one for a coincidence, and
    // leaves it in the hunk of the words around it.
    let changes = compare_rows(&["alpha", "x", "beta"], &["gamma", "x", "delta"]);
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Replaced, "alpha", "gamma"),
            (ChangeKind::Replaced, "beta", "delta"),
        ]
    );
}

#[test]
fn change_stays_whole_where_its_sides_have_different_numbers_of_cells() {
    let changes = compare_rows(&["one"], &["uno", "dos"]);
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Replaced, "one", "uno dos")]
    );
}

#[test]
fn change_stays_whole_where_a_cell_faces_text_outside_of_a_table() {
    let old = PageBuilder::new().text("old\n").cell(0).text("one");
    let new = PageBuilder::new().cell(0).text("uno\n").body().text("new");
    let changes = compare_with([old], [new], &Filters::default());
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Replaced, "old one", "uno new")]
    );
}

#[test]
fn text_that_stands_at_another_place_is_a_move() {
    let old = format!("{PARAGRAPH}\n{REST}");
    let new = format!("{REST}\n{PARAGRAPH}");
    let changes = compare(&old, &new);
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Moved, PARAGRAPH, PARAGRAPH)]
    );
    let paragraph_columns = 0..PARAGRAPH.chars().count();
    assert_eq!(
        changes[0].sides.old.areas(),
        [cells(0, 0, paragraph_columns.clone())]
    );
    assert_eq!(
        changes[0].sides.new.areas(),
        [cells(0, 1, paragraph_columns)]
    );
}

#[test]
fn text_around_a_move_is_compared_in_place() {
    let old = format!("{PARAGRAPH}\n{REST} for good");
    let new = format!("{REST} for ever\n{PARAGRAPH}");
    let changes = compare(&old, &new);
    // The changes are in the order of the new document.
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Replaced, "good", "ever"),
            (ChangeKind::Moved, PARAGRAPH, PARAGRAPH)
        ]
    );
}

#[test]
fn difference_inside_moved_text_is_a_change_of_its_own() {
    let old = format!("{PARAGRAPH} with one word in its middle\n{REST} and more of it");
    let new = format!("{REST} and more of it\n{PARAGRAPH} with another word in its middle");
    let changes = compare(&old, &new);
    assert_eq!(
        summaries(&changes),
        [
            (
                ChangeKind::Moved,
                "this paragraph stands somewhere else now with one word in its middle",
                "this paragraph stands somewhere else now with another word in its middle",
            ),
            (ChangeKind::Replaced, "one", "another"),
        ]
    );
    // The move is not marked where the word is, which has a mark of its own.
    let word_start = format!("{PARAGRAPH} with ").chars().count();
    let line_end = old.lines().next().unwrap().chars().count();
    assert_eq!(
        changes[0].sides.old.areas(),
        [
            cells(0, 0, 0..word_start - 1),
            cells(0, 0, word_start + "one ".len()..line_end)
        ]
    );
    assert_eq!(
        changes[1].sides.old.areas(),
        [cells(0, 0, word_start..word_start + "one".len())]
    );
    assert_eq!(
        changes[1].sides.new.areas()[0].bounds.top,
        cells(0, 1, 0..1).bounds.top
    );
}

#[test]
fn few_words_that_stand_at_another_place_are_removed_and_added() {
    let old = format!("to be\n{REST}");
    let new = format!("{REST}\nto be");
    let changes = compare(&old, &new);
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Deleted, "to be", ""),
            (ChangeKind::Inserted, "", "to be")
        ]
    );
}

/// The changes from a page of body text to a page with the same text, of
/// which what stands between two `*` is bold.
fn compare_with_bold(old: &str, new: &str) -> Vec<Change> {
    comparison_of_built_pages([page_with_bold(old)], [page_with_bold(new)]).changes
}

fn style_differences(change: &Change) -> &[StyleDifference] {
    match &change.difference {
        Difference::Style(differences) => differences,
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_in_another_style_is_restyled() {
    let changes = compare_with_bold("one two three", "one *two* three");
    assert_eq!(summaries(&changes), [(ChangeKind::Restyled, "two", "two")]);
    assert_eq!(style_differences(&changes[0]), [bold_difference()]);
    assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 4..7)]);
    assert_eq!(changes[0].sides.new.areas(), [cells(0, 0, 4..7)]);
}

#[test]
fn words_in_a_row_that_differ_in_the_same_way_are_one_change() {
    let changes = compare_with_bold("one two three four", "one *two three* four");
    assert_eq!(
        summaries(&changes),
        [(ChangeKind::Restyled, "two three", "two three")]
    );
}

#[test]
fn words_in_a_row_that_differ_in_other_ways_are_separate_changes() {
    let old = PageBuilder::new().text("one two three four");
    let new = PageBuilder::new()
        .text("one ")
        .bold()
        .text("two ")
        .styled(|style| style.is_italic = true)
        .text("three")
        .body()
        .text(" four");
    let changes = comparison_of_built_pages([old], [new]).changes;
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Restyled, "two", "two"),
            (ChangeKind::Restyled, "three", "three")
        ]
    );
    let slant = BySide {
        old: false,
        new: true,
    };
    assert_eq!(
        style_differences(&changes[1]),
        [StyleDifference::Italic(slant)]
    );
}

#[test]
fn word_with_a_part_in_another_style_is_restyled() {
    let old = PageBuilder::new().text("of H2O here");
    let new = PageBuilder::new()
        .text("of H")
        .styled(|style| style.font_size = 7.0)
        .text("2")
        .body()
        .text("O here");
    let changes = comparison_of_built_pages([old], [new]).changes;
    assert_eq!(summaries(&changes), [(ChangeKind::Restyled, "H2O", "H2O")]);
    let size = BySide {
        old: 10.0,
        new: 7.0,
    };
    assert_eq!(
        style_differences(&changes[0]),
        [StyleDifference::Size(size)]
    );
}

#[test]
fn sizes_and_rotations_that_are_all_but_the_same_do_not_differ() {
    let old = PageBuilder::new().text("one two");
    let new = PageBuilder::new()
        .styled(|style| {
            style.font_size += 0.01;
            style.rotation_degrees += 0.1;
        })
        .text("one two");
    assert_eq!(comparison_of_built_pages([old], [new]).changes, []);
}

#[test]
fn name_of_a_font_stands_for_its_weight_and_its_slant() {
    let old = PageBuilder::new().text("one two");
    let new = PageBuilder::new()
        .text("one ")
        .styled(|style| {
            style.font_name = "Body-BoldItalic".to_owned();
            style.font_weight = Some(700);
            style.is_italic = true;
        })
        .text("two");
    let changes = comparison_of_built_pages([old], [new]).changes;
    let font = BySide {
        old: "Body".to_owned(),
        new: "Body-BoldItalic".to_owned(),
    };
    assert_eq!(
        style_differences(&changes[0]),
        [StyleDifference::Font(font)]
    );
}

#[test]
fn style_of_text_that_is_replaced_is_not_compared() {
    let changes = compare_with_bold("one two", "one *2*");
    assert_eq!(summaries(&changes), [(ChangeKind::Replaced, "two", "2")]);
}

#[test]
fn style_of_text_that_moved_is_compared() {
    let old = format!("{PARAGRAPH}\n{REST}");
    let new = format!("{REST}\nthis *paragraph* stands somewhere else now");
    let changes = compare_with_bold(&old, &new);
    assert_eq!(
        summaries(&changes),
        [
            (ChangeKind::Moved, PARAGRAPH, PARAGRAPH),
            (ChangeKind::Restyled, "paragraph", "paragraph")
        ]
    );
    assert_eq!(changes[1].sides.old.areas(), [cells(0, 0, 5..14)]);
    assert_eq!(changes[1].sides.new.areas(), [cells(0, 1, 5..14)]);
}
