//! Leaving page furniture and text that is derived from pagination out of the
//! comparison.

use std::ops::Range;

use regex::Regex;

use crate::flow::{Exclusion, Gap, HyphenKind, RawFlow, RawToken};

/// What to leave out of a comparison, besides what the producer marked as
/// artifacts.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    /// Text that lies entirely within this distance of the top edge of its
    /// page is left out.
    pub header_height: f32,
    /// Text that lies entirely within this distance of the bottom edge of
    /// its page is left out.
    pub footer_height: f32,
    /// Text that one of these matches is left out.
    ///
    /// A pattern is searched in the text of each page, in which tokens are
    /// separated as on the page: by a space or, between lines, by a newline.
    /// If the pattern has capture groups, only what the groups capture is
    /// left out, and the rest of the match only says where to look.
    /// A token is left out as a whole, even if only part of it is.
    pub ignore_patterns: Vec<Regex>,
}

impl Filters {
    /// For each token of a flow, the first rule that leaves it out, if one
    /// does.
    ///
    /// The patterns see all text, also what another rule leaves out, so a
    /// profile works the same whether or not the producer marks artifacts.
    pub(crate) fn exclusions(&self, flow: &RawFlow) -> Vec<Option<Exclusion>> {
        let by_position = |token: &RawToken| {
            let page_height = flow.page_sizes[page_of(token)].height;
            self.exclusion_by_position(token, page_height)
        };
        let mut exclusions: Vec<Option<Exclusion>> = flow.tokens.iter().map(by_position).collect();
        let page_ranges = page_ranges(&flow.tokens);
        for (pattern_index, pattern) in self.ignore_patterns.iter().enumerate() {
            for page_range in &page_ranges {
                let page_text = PageText::of(&flow.tokens[page_range.clone()]);
                for ignored in ignored_ranges(pattern, &page_text.text) {
                    for token_index in page_text.tokens_in(&ignored) {
                        exclusions[page_range.start + token_index]
                            .get_or_insert(Exclusion::Pattern(pattern_index));
                    }
                }
            }
        }
        exclusions
    }

    fn exclusion_by_position(&self, token: &RawToken, page_height: f32) -> Option<Exclusion> {
        let bounds = token.fragment.bounds;
        if token.is_artifact {
            Some(Exclusion::Artifact)
        } else if self.header_height > 0.0 && bounds.bottom <= self.header_height {
            Some(Exclusion::Header)
        } else if self.footer_height > 0.0 && bounds.top >= page_height - self.footer_height {
            Some(Exclusion::Footer)
        } else {
            None
        }
    }
}

fn page_of(token: &RawToken) -> usize {
    token.fragment.page_index
}

/// The range of tokens of each page that has any.
fn page_ranges(tokens: &[RawToken]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match ranges.last_mut() {
            Some(range) if page_of(&tokens[range.start]) == page_of(token) => range.end = index + 1,
            _ => ranges.push(index..index + 1),
        }
    }
    ranges
}

/// The byte ranges of `text` that `pattern` says to leave out.
fn ignored_ranges(pattern: &Regex, text: &str) -> Vec<Range<usize>> {
    let has_groups = pattern.captures_len() > 1;
    let mut ranges = Vec::new();
    for captures in pattern.captures_iter(text) {
        let first_ignored_group = usize::from(has_groups);
        let groups = captures.iter().skip(first_ignored_group).flatten();
        ranges.extend(groups.map(|group| group.range()));
    }
    ranges.retain(|range| !range.is_empty());
    ranges
}

/// The text of the tokens of one page, and where each token is in it.
struct PageText {
    text: String,
    token_ranges: Vec<Range<usize>>,
}

impl PageText {
    /// The text as it is printed, with a newline between two lines.
    ///
    /// A word that a line break splits is one word in it, with its hyphen,
    /// so that a pattern for `well-known` finds the word where it breaks.
    /// Only a hyphen that the producer marks as one that is no part of the
    /// word is left out.
    fn of(tokens: &[RawToken]) -> Self {
        let mut text = String::new();
        let mut token_ranges = Vec::with_capacity(tokens.len());
        let mut hyphen_before: Option<HyphenKind> = None;
        for (index, token) in tokens.iter().enumerate() {
            match (hyphen_before, token.gap) {
                _ if index == 0 => {}
                (Some(HyphenKind::Soft), _) => {}
                (Some(HyphenKind::Hard | HyphenKind::Unknown), _) => text.push('-'),
                (None, Gap::None) => {}
                (None, Gap::Space) => text.push(' '),
                (None, Gap::LineBreak) => text.push('\n'),
            }
            token_ranges.push(text.len()..text.len() + token.text.len());
            text.push_str(&token.text);
            hyphen_before = token.line_end_hyphen.map(|hyphen| hyphen.kind);
        }
        Self { text, token_ranges }
    }

    /// The indices of the tokens that have a byte in `range`.
    fn tokens_in(&self, range: &Range<usize>) -> Range<usize> {
        let first = self
            .token_ranges
            .partition_point(|token| token.end <= range.start);
        let end = self
            .token_ranges
            .partition_point(|token| token.start < range.end);
        first..end.max(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{LINE_PITCH, PageBuilder, raw_flow_of};

    /// The texts of the tokens that the filters leave in.
    fn compared(pages: impl IntoIterator<Item = PageBuilder>, filters: &Filters) -> Vec<String> {
        let flow = raw_flow_of(pages);
        let exclusions = filters.exclusions(&flow);
        let tokens = flow.tokens.into_iter().zip(exclusions);
        tokens
            .filter(|(_, exclusion)| exclusion.is_none())
            .map(|(token, _)| token.text)
            .collect()
    }

    fn exclusions(page: PageBuilder, filters: &Filters) -> Vec<Option<Exclusion>> {
        filters.exclusions(&raw_flow_of([page]))
    }

    fn ignoring(patterns: &[&str]) -> Filters {
        Filters {
            ignore_patterns: patterns
                .iter()
                .map(|pattern| Regex::new(pattern).unwrap())
                .collect(),
            ..Filters::default()
        }
    }

    /// A page on which a line break splits a word after `first_part`, at a
    /// hyphen in the span that `hyphen_span` starts.
    fn page_with_broken_word(
        first_part: &str,
        hyphen_span: impl FnOnce(PageBuilder) -> PageBuilder,
        second_part: &str,
    ) -> PageBuilder {
        let page = PageBuilder::new().text(&format!("a {first_part}"));
        hyphen_span(page)
            .line_end_hyphen()
            .body()
            .text(&format!("{second_part} b"))
    }

    #[test]
    fn pattern_finds_a_word_with_its_hyphen_where_a_line_break_splits_it() {
        let page = || page_with_broken_word("well", PageBuilder::body, "known");
        assert_eq!(compared([page()], &ignoring(&["well-known"])), ["a", "b"]);
        assert_eq!(
            compared([page()], &ignoring(&["wellknown"])),
            ["a", "well", "known", "b"]
        );
    }

    #[test]
    fn pattern_finds_a_word_without_the_hyphen_that_is_marked_as_no_part_of_it() {
        let soft = |page: PageBuilder| page.actual_text("\u{AD}");
        let page = || page_with_broken_word("inter", soft, "national");
        assert_eq!(
            compared([page()], &ignoring(&["international"])),
            ["a", "b"]
        );
        assert_eq!(
            compared([page()], &ignoring(&["inter-national"])),
            ["a", "inter", "national", "b"]
        );
    }

    #[test]
    fn nothing_is_left_out_by_default() {
        let page = PageBuilder::new().text("Title\nbody\n1");
        assert_eq!(
            compared([page], &Filters::default()),
            ["Title", "body", "1"]
        );
    }

    #[test]
    fn artifact_is_left_out() {
        let page = PageBuilder::new()
            .artifact()
            .text("Title\n")
            .body()
            .text("body");
        assert_eq!(
            exclusions(page, &Filters::default()),
            [Some(Exclusion::Artifact), None]
        );
    }

    #[test]
    fn text_inside_the_header_and_footer_bands_is_left_out() {
        // Three lines: at the top, in the middle and at the bottom of the page.
        let page = PageBuilder::new()
            .height(3.0 * LINE_PITCH)
            .text("Title\nbody\n1");
        let filters = Filters {
            header_height: LINE_PITCH,
            footer_height: LINE_PITCH,
            ..Filters::default()
        };
        assert_eq!(
            exclusions(page, &filters),
            [Some(Exclusion::Header), None, Some(Exclusion::Footer)]
        );
    }

    #[test]
    fn text_that_reaches_out_of_a_band_stays() {
        let page = PageBuilder::new()
            .height(3.0 * LINE_PITCH)
            .text("Title\nbody\n1");
        let filters = Filters {
            header_height: 0.5 * LINE_PITCH,
            footer_height: 0.5 * LINE_PITCH,
            ..Filters::default()
        };
        assert_eq!(compared([page], &filters), ["Title", "body", "1"]);
    }

    #[test]
    fn pattern_without_groups_leaves_out_its_whole_match() {
        let page = PageBuilder::new().text("See Section 4 (p. 12) for more");
        let filters = ignoring(&[r"\(p\. \d+\)"]);
        assert_eq!(
            compared([page], &filters),
            ["See", "Section", "4", "for", "more"]
        );
    }

    #[test]
    fn pattern_with_groups_leaves_out_only_what_they_capture() {
        let page = PageBuilder::new().text("See Figure 12 and Table 3.4 and 5 more");
        let filters = ignoring(&[r"(?:Figure|Table) (\d+(?:\.\d+)*)"]);
        assert_eq!(
            compared([page], &filters),
            ["See", "Figure", "and", "Table", "and", "5", "more"]
        );
    }

    #[test]
    fn group_that_takes_no_part_in_a_match_is_skipped() {
        let page = PageBuilder::new().text("a1 b2");
        let filters = ignoring(&[r"a(\d)|b(\d)"]);
        assert_eq!(compared([page], &filters), Vec::<String>::new());
    }

    #[test]
    fn pattern_can_anchor_to_the_lines_of_the_page() {
        let page = PageBuilder::new().text("1.2 Scope\nsee 1.2 above\nIntroduction 7");
        let filters = ignoring(&[r"(?m)^(\d+(?:\.\d+)+) ", r"(?m) (\d+)$"]);
        assert_eq!(
            compared([page], &filters),
            ["Scope", "see", "1.2", "above", "Introduction"]
        );
    }

    #[test]
    fn token_is_left_out_as_a_whole() {
        let page = PageBuilder::new().text("revision-B draft");
        let filters = ignoring(&["vision"]);
        assert_eq!(compared([page], &filters), ["-", "B", "draft"]);
    }

    #[test]
    fn pattern_does_not_match_across_pages() {
        let pages = [
            PageBuilder::new().text("page"),
            PageBuilder::new().text("break"),
        ];
        let filters = ignoring(&[r"page\sbreak"]);
        assert_eq!(compared(pages, &filters), ["page", "break"]);
    }

    #[test]
    fn pattern_sees_what_another_rule_leaves_out() {
        let page = PageBuilder::new()
            .text("Scope ")
            .artifact()
            .text(". . . . ")
            .body()
            .text("7");
        let filters = ignoring(&[r"(?:\. ){4}(\d+)"]);
        assert_eq!(compared([page], &filters), ["Scope"]);
    }

    #[test]
    fn first_rule_that_leaves_a_token_out_is_recorded() {
        let page = PageBuilder::new().artifact().text("7").body().text(" 7 7");
        assert_eq!(
            exclusions(page, &ignoring(&["x", "7"])),
            [
                Some(Exclusion::Artifact),
                Some(Exclusion::Pattern(1)),
                Some(Exclusion::Pattern(1))
            ]
        );
    }
}
