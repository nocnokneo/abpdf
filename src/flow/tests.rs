use super::*;
use crate::filter::Filters;
use crate::testing::{
    CELL_HEIGHT, CELL_WIDTH, LINE_PITCH, PageBuilder, compared_texts, raw_flow_of, texts,
};

/// The flow of a document with these pages, as the comparison sees it.
fn flow_of(pages: impl IntoIterator<Item = PageBuilder>) -> Flow {
    let [flow] = prepare([raw_flow_of(pages)], &Filters::default());
    flow
}

#[test]
fn words_and_punctuation_marks_are_separate_tokens() {
    let flow = flow_of([PageBuilder::new().text("Export. (p. 351)")]);
    assert_eq!(texts(&flow), ["Export", ".", "(", "p", ".", "351", ")"]);
}

#[test]
fn numbers_and_contractions_stay_whole() {
    let flow = flow_of([PageBuilder::new().text("3.2.1 isn't 1,000")]);
    assert_eq!(texts(&flow), ["3.2.1", "isn't", "1,000"]);
}

#[test]
fn gap_is_the_white_space_before_a_token() {
    let flow = flow_of([PageBuilder::new().text("one two.\nthree")]);
    let gaps: Vec<Gap> = flow.tokens.iter().map(|token| token.gap).collect();
    assert_eq!(
        gaps,
        [Gap::LineBreak, Gap::Space, Gap::None, Gap::LineBreak]
    );
}

#[test]
fn fragment_covers_the_characters_of_its_token() {
    let flow = flow_of([PageBuilder::new().text("ab\ncde")]);
    assert_eq!(
        flow.tokens[1].fragments,
        [Fragment {
            page_index: 0,
            line_index: 1,
            bounds: Rect {
                left: 0.0,
                top: LINE_PITCH,
                right: 3.0 * CELL_WIDTH,
                bottom: LINE_PITCH + CELL_HEIGHT,
            },
        }]
    );
}

#[test]
fn pages_continue_one_flow() {
    let flow = flow_of([
        PageBuilder::new().text("one"),
        PageBuilder::new().text("two"),
    ]);
    assert_eq!(texts(&flow), ["one", "two"]);
    assert_eq!(flow.tokens[1].fragments[0].page_index, 1);
    assert_eq!(flow.page_sizes.len(), 2);
}

#[test]
fn artifact_is_split_from_content_it_touches() {
    let page = PageBuilder::new()
        .artifact()
        .text("(p. 3)")
        .body()
        .text(".");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["(", "p", ".", "3", ")", "."]);
    assert_eq!(compared_texts(&flow), ["."]);
    assert_eq!(flow.tokens[0].exclusion, Some(Exclusion::Artifact));
}

#[test]
fn invisible_characters_are_not_text() {
    let flow = flow_of([PageBuilder::new().text("a\u{200B}b \u{2060} c")]);
    assert_eq!(texts(&flow), ["ab", "c"]);
}

#[test]
fn soft_hyphen_inside_a_line_is_not_text() {
    let flow = flow_of([PageBuilder::new().text("co\u{AD}operate")]);
    assert_eq!(texts(&flow), ["cooperate"]);
}

#[test]
fn equal_styles_are_shared() {
    let page = PageBuilder::new()
        .text("plain ")
        .span(|span| span.style.is_italic = true)
        .text("slanted ")
        .body()
        .text("plain");
    let flow = flow_of([page]);
    assert_eq!(flow.styles.len(), 2);
    assert_eq!(flow.tokens[0].style, flow.tokens[2].style);
    assert!(flow.styles[flow.tokens[1].style].is_italic);
}

#[test]
fn structure_path_is_kept_per_token() {
    let page = PageBuilder::new()
        .span(|span| span.structure_path = Some("Document/H1".to_owned()))
        .text("Title\n")
        .body()
        .text("text");
    let flow = flow_of([page]);
    let path_of = |token: &Token| {
        token
            .structure_path
            .map(|index| &*flow.structure_paths[index])
    };
    assert_eq!(path_of(&flow.tokens[0]), Some("Document/H1"));
    assert_eq!(path_of(&flow.tokens[1]), None);
}

#[test]
fn key_folds_compatibility_characters() {
    let flow = flow_of([PageBuilder::new().text("o\u{FB03}ce office")]);
    assert_eq!(flow.tokens[0].text, "o\u{FB03}ce");
    assert_eq!(flow.tokens[0].key(), flow.tokens[1].key());
}

#[test]
fn soft_hyphen_that_the_producer_marks_joins_the_word() {
    let page = PageBuilder::new()
        .text("an inter")
        .actual_text("\u{AD}")
        .line_end_hyphen()
        .body()
        .text("national one");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["an", "international", "one"]);

    let word = &flow.tokens[1];
    let lines: Vec<usize> = word
        .fragments
        .iter()
        .map(|fragment| fragment.line_index)
        .collect();
    assert_eq!(lines, [0, 1]);
    let hyphen_right = 9.0 * CELL_WIDTH;
    assert_eq!(word.fragments[0].bounds.right, hyphen_right);
    assert_eq!(word.line_end_hyphen, None);
}

#[test]
fn hard_hyphen_that_the_producer_marks_stays_in_the_text() {
    let page = PageBuilder::new()
        .text("well")
        .actual_text("-")
        .line_end_hyphen()
        .body()
        .text("known");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["well", "-", "known"]);
    assert_eq!(flow.tokens[1].gap, Gap::None);
    assert_eq!(flow.tokens[2].gap, Gap::None);
    assert_eq!(flow.tokens[1].fragments[0].line_index, 0);
}

#[test]
fn unmarked_hyphen_is_soft_in_a_font_whose_hard_hyphens_are_marked() {
    let page = PageBuilder::new()
        .text("well")
        .actual_text("-")
        .text("-")
        .body()
        .text("known inter")
        .line_end_hyphen()
        .text("national");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["well", "-", "known", "international"]);
}

#[test]
fn unmarked_hyphen_is_hard_in_a_font_whose_soft_hyphens_are_marked() {
    let page = PageBuilder::new()
        .text("inter")
        .actual_text("\u{AD}")
        .line_end_hyphen()
        .body()
        .text("national well")
        .line_end_hyphen()
        .text("known");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["international", "well", "-", "known"]);
}

#[test]
fn marks_in_another_font_say_nothing_about_an_unmarked_hyphen() {
    let page = PageBuilder::new()
        .span(|span| {
            span.style.font_name = "Heading".to_owned();
            span.actual_text = Some("\u{AD}".to_owned());
        })
        .text("inter")
        .line_end_hyphen()
        .body()
        .text("national inter")
        .line_end_hyphen()
        .text("national");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["international", "international"]);
}

#[test]
fn hyphen_of_unknown_kind_joins_the_word() {
    let page = PageBuilder::new()
        .text("inter")
        .line_end_hyphen()
        .text("national");
    assert_eq!(texts(&flow_of([page])), ["international"]);
}

#[test]
fn hyphen_of_unknown_kind_before_punctuation_stays_in_the_text() {
    let page = PageBuilder::new().text("pre").line_end_hyphen().text("(x)");
    assert_eq!(texts(&flow_of([page])), ["pre", "-", "(", "x", ")"]);
}

#[test]
fn word_joins_over_more_than_one_line_break() {
    let page = PageBuilder::new()
        .text("in")
        .line_end_hyphen()
        .text("ter")
        .line_end_hyphen()
        .text("national");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["international"]);
    assert_eq!(flow.tokens[0].fragments.len(), 3);
}

#[test]
fn word_joins_across_a_page_break_and_its_page_furniture() {
    let first_page = PageBuilder::new()
        .text("an inter\u{AD}\n")
        .artifact()
        .text("Page 1");
    let second_page = PageBuilder::new()
        .artifact()
        .text("Title\n")
        .body()
        .text("national one");
    let flow = flow_of([first_page, second_page]);
    assert_eq!(compared_texts(&flow), ["an", "international", "one"]);

    let pages: Vec<usize> = flow.tokens[1]
        .fragments
        .iter()
        .map(|fragment| fragment.page_index)
        .collect();
    assert_eq!(pages, [0, 1]);
}

#[test]
fn hyphen_does_not_join_a_word_to_page_furniture() {
    // The footer follows the hyphen directly, so a reader takes the hyphen to
    // end a line inside a word.
    let first_page = PageBuilder::new()
        .text("inter")
        .line_end_hyphen()
        .artifact()
        .text("Footer");
    let second_page = PageBuilder::new().text("national");
    let flow = flow_of([first_page, second_page]);
    assert_eq!(texts(&flow), ["international", "Footer"]);
}

#[test]
fn hyphen_at_the_end_of_the_document_is_resolved() {
    let soft = PageBuilder::new().text("inter\u{AD}");
    assert_eq!(texts(&flow_of([soft])), ["inter"]);

    let unknown = PageBuilder::new().text("inter").line_end_hyphen();
    assert_eq!(texts(&flow_of([unknown])), ["inter", "-"]);
}

#[test]
fn hyphen_of_excluded_text_is_only_part_of_its_bounds() {
    let page = PageBuilder::new()
        .artifact()
        .text("head")
        .line_end_hyphen()
        .text("line");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["head", "line"]);
    assert_eq!(flow.tokens[0].fragments[0].bounds.right, 5.0 * CELL_WIDTH);
    assert!(
        flow.tokens
            .iter()
            .all(|token| token.line_end_hyphen.is_none())
    );
}

#[test]
fn hyphen_of_unknown_kind_stays_between_words_that_are_hyphenated_elsewhere() {
    let page = PageBuilder::new()
        .text("a well-known fact is well")
        .line_end_hyphen()
        .text("known");
    let flow = flow_of([page]);
    assert_eq!(
        texts(&flow),
        [
            "a", "well", "-", "known", "fact", "is", "well", "-", "known"
        ]
    );
}

#[test]
fn hyphen_of_unknown_kind_joins_a_word_that_is_also_written_whole() {
    // Both spellings occur, and automatic hyphenation is the likelier reason
    // for a hyphen at a line end.
    let page = PageBuilder::new()
        .text("on-line online on")
        .line_end_hyphen()
        .text("line");
    let flow = flow_of([page]);
    assert_eq!(texts(&flow), ["on", "-", "line", "online", "online"]);
}

#[test]
fn words_of_every_flow_decide_about_a_hyphen_of_unknown_kind() {
    let broken = raw_flow_of([PageBuilder::new()
        .text("well")
        .line_end_hyphen()
        .text("known")]);
    let whole = raw_flow_of([PageBuilder::new().text("well-known")]);
    let [broken, _] = prepare([broken, whole], &Filters::default());
    assert_eq!(texts(&broken), ["well", "-", "known"]);
}

#[test]
fn hyphen_whose_mark_names_other_text_counts_as_unmarked() {
    let page = PageBuilder::new()
        .text("inter")
        .actual_text("other")
        .line_end_hyphen()
        .body()
        .text("national");
    assert_eq!(texts(&flow_of([page])), ["international"]);
}
