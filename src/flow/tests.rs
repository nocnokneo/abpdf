use super::*;
use crate::BySide;
use crate::filter::Filters;
use crate::testing::{
    CELL_HEIGHT, CELL_WIDTH, LINE_PITCH, PageBuilder, excluded_texts, raw_flow_of, texts,
};

/// The flow of a document with these pages, as a comparison with itself sees
/// it.
fn flow_of(pages: impl IntoIterator<Item = PageBuilder>) -> Flow {
    let raw = raw_flow_of(pages);
    let flows = BySide {
        old: raw.clone(),
        new: raw,
    };
    prepare(flows, &Filters::default()).new
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
    assert_eq!(texts(&flow), ["."]);
    assert_eq!(excluded_texts(&flow), ["(", "p", ".", "3", ")"]);
    assert_eq!(flow.excluded[0].exclusion, Exclusion::Artifact);
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
    assert_eq!(texts(&flow), ["an", "international", "one"]);

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
    assert_eq!(texts(&flow), ["international"]);
    assert_eq!(excluded_texts(&flow), ["Footer"]);
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
    assert_eq!(excluded_texts(&flow), ["head", "line"]);
    let head = &flow.excluded[0].token;
    assert_eq!(head.fragments[0].bounds.right, 5.0 * CELL_WIDTH);
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
    let flows = BySide {
        old: broken,
        new: whole,
    };
    let broken = prepare(flows, &Filters::default()).old;
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

#[test]
fn word_joins_across_a_page_break_and_the_header_that_the_page_repeats() {
    let first_page = PageBuilder::new()
        .text("intro\nKey Value\nan inter")
        .line_end_hyphen();
    let second_page = PageBuilder::new().text("Key Value\nnational one");
    let flow = flow_of([first_page, second_page]);
    assert_eq!(
        texts(&flow),
        ["intro", "Key", "Value", "an", "international", "one"]
    );
    assert_eq!(excluded_texts(&flow), ["Key", "Value"]);
    assert_eq!(flow.excluded[0].exclusion, Exclusion::Repeat);
}

/// The texts of the tokens that are left out as running headers and as
/// running footers.
fn running(flow: &Flow) -> (Vec<&str>, Vec<&str>) {
    let texts_of = |exclusion: Exclusion| {
        let excluded = flow.excluded.iter();
        excluded
            .filter(|excluded| excluded.exclusion == exclusion)
            .map(|excluded| excluded.token.text.as_str())
            .collect()
    };
    (
        texts_of(Exclusion::RunningHeader),
        texts_of(Exclusion::RunningFooter),
    )
}

#[test]
fn lines_that_pages_share_at_their_tops_and_bottoms_are_left_out() {
    let flow = flow_of([
        PageBuilder::new().text("The manual\none\nIssue A"),
        PageBuilder::new().text("The manual\ntwo\nIssue A"),
    ]);
    assert_eq!(texts(&flow), ["one", "two"]);
    assert_eq!(
        running(&flow),
        (
            vec!["The", "manual", "The", "manual"],
            vec!["Issue", "A", "Issue", "A"]
        )
    );
}

#[test]
fn page_number_counts_on_in_a_running_footer() {
    let flow = flow_of([
        PageBuilder::new().text("one\nPage 9 of 11"),
        PageBuilder::new().text("two\nPage 10 of 11"),
        PageBuilder::new().text("three\nPage 11 of 11"),
    ]);
    assert_eq!(texts(&flow), ["one", "two", "three"]);
}

#[test]
fn number_that_does_not_count_the_pages_is_not_a_page_number() {
    let flow = flow_of([
        PageBuilder::new().text("one\nItem 9"),
        PageBuilder::new().text("two\nItem 11"),
    ]);
    assert_eq!(texts(&flow), ["one", "Item", "9", "two", "Item", "11"]);
}

#[test]
fn left_and_right_pages_can_have_their_own_running_headers() {
    let flow = flow_of([
        PageBuilder::new().text("2 The manual\none"),
        PageBuilder::new().text("Operation 3\ntwo"),
        PageBuilder::new().text("4 The manual\nthree"),
        PageBuilder::new().text("Operation 5\nfour"),
    ]);
    assert_eq!(texts(&flow), ["one", "two", "three", "four"]);
}

#[test]
fn line_runs_only_at_the_same_height() {
    let flow = flow_of([
        PageBuilder::new().text("same words\none"),
        PageBuilder::new().text("\nsame words\ntwo"),
    ]);
    assert_eq!(running(&flow), (vec![], vec![]));
}

#[test]
fn line_beyond_one_that_does_not_run_stays() {
    let flow = flow_of([
        PageBuilder::new().text("The manual\none\nsame words\nmore of one\nIssue A"),
        PageBuilder::new().text("The manual\ntwo\nsame words\nmore of two\nIssue A"),
    ]);
    assert_eq!(
        texts(&flow),
        [
            "one", "same", "words", "more", "of", "one", "two", "same", "words", "more", "of",
            "two"
        ]
    );
}

#[test]
fn producer_that_marks_artifacts_says_what_its_headers_are() {
    let page = |body: &str| {
        PageBuilder::new()
            .text("Same first line\n")
            .text(body)
            .artifact()
            .text("\nfooter")
    };
    let flow = flow_of([page("one"), page("two")]);
    assert_eq!(running(&flow), (vec![], vec![]));
}
