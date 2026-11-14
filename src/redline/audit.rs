//! Showing what was left out of a comparison, so that a reader can check
//! the rules that leave text out.

use std::collections::BTreeSet;

use super::{LegendEntry, Mark, MarkStyle, Redline, Rgb};
use crate::Side;
use crate::change::Comparison;
use crate::flow::{Exclusion, Flow};

const ARTIFACT_COLOR: Rgb = Rgb::new(128, 128, 128);
const HEADER_AND_FOOTER_COLOR: Rgb = Rgb::new(153, 102, 51);
const RUNNING_HEADER_AND_FOOTER_COLOR: Rgb = Rgb::new(120, 81, 169);
const REPEAT_COLOR: Rgb = Rgb::new(0, 140, 140);
/// The colors of the ignore patterns, which patterns after the last color
/// start over with.
const PATTERN_COLORS: [Rgb; 4] = [
    Rgb::new(230, 126, 0),
    Rgb::new(199, 21, 133),
    Rgb::new(128, 128, 0),
    Rgb::new(0, 150, 220),
];

/// How far apart two tokens of a line can be for one outline to go around
/// both, as a share of the height of the line.
/// It is more than the space between two words, and less than the space
/// that text which is not left out would need between them.
const MAX_GAP_IN_OUTLINE: f32 = 0.75;

/// Adds to a redline of a comparison what the comparison left out: outlines
/// on the pages of its sheets, and what their colors stand for.
pub(super) fn add_to(redline: &mut Redline, comparison: &Comparison) {
    for sheet in &mut redline.sheets {
        for side in Side::BOTH {
            if let Some(page) = &mut sheet.pages[side] {
                page.marks
                    .extend(marks_on(&comparison.flows[side], page.page_index));
            }
        }
    }
    if let Some(summary) = &mut redline.summary {
        summary.legend = legend_of(comparison);
    }
}

/// Outlines around the tokens on a page that were left out, in the color of
/// the rule that left them out.
/// Tokens that follow each other closely on a line, left out by one rule,
/// share an outline.
fn marks_on(flow: &Flow, page_index: usize) -> Vec<Mark> {
    let mut marks: Vec<Mark> = Vec::new();
    // The line and the rule of the last mark.
    let mut last = None;
    for excluded in &flow.excluded {
        let fragments = excluded.token.fragments.iter();
        for fragment in fragments.filter(|fragment| fragment.page_index == page_index) {
            let line_and_rule = (fragment.line_index, excluded.exclusion);
            let bounds = fragment.bounds;
            match marks.last_mut() {
                Some(mark)
                    if last == Some(line_and_rule)
                        && bounds.left - mark.bounds.right
                            <= MAX_GAP_IN_OUTLINE * bounds.height() =>
                {
                    mark.bounds = mark.bounds.union(&bounds);
                }
                _ => marks.push(Mark {
                    bounds,
                    color: color_of(excluded.exclusion),
                    style: MarkStyle::Outline,
                }),
            }
            last = Some(line_and_rule);
        }
    }
    marks
}

/// What the colors of the outlines stand for: an entry for each kind of rule
/// that left text out of the comparison.
fn legend_of(comparison: &Comparison) -> Vec<LegendEntry> {
    let flows = Side::BOTH.into_iter().map(|side| &comparison.flows[side]);
    let exclusions: BTreeSet<Exclusion> = flows
        .flat_map(|flow| &flow.excluded)
        .map(|excluded| excluded.exclusion)
        .collect();
    let mut legend: Vec<LegendEntry> = exclusions
        .into_iter()
        .map(|exclusion| LegendEntry {
            color: color_of(exclusion),
            text: description_of(exclusion),
        })
        .collect();
    // The header and the footer have one entry.
    legend.dedup();
    legend
}

fn color_of(exclusion: Exclusion) -> Rgb {
    match exclusion {
        Exclusion::Artifact => ARTIFACT_COLOR,
        Exclusion::Header | Exclusion::Footer => HEADER_AND_FOOTER_COLOR,
        Exclusion::RunningHeader | Exclusion::RunningFooter => RUNNING_HEADER_AND_FOOTER_COLOR,
        Exclusion::Pattern(index) => PATTERN_COLORS[index % PATTERN_COLORS.len()],
        Exclusion::Repeat => REPEAT_COLOR,
    }
}

fn description_of(exclusion: Exclusion) -> String {
    match exclusion {
        Exclusion::Artifact => "marked as an artifact by the producer of the PDF".to_owned(),
        Exclusion::Header | Exclusion::Footer => {
            "within the height of the header or the footer".to_owned()
        }
        Exclusion::RunningHeader | Exclusion::RunningFooter => {
            "a header or a footer that runs from page to page".to_owned()
        }
        Exclusion::Pattern(index) => format!("matched by ignore pattern {}", index + 1),
        Exclusion::Repeat => "repeated at the top of a page".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;
    use crate::BySide;
    use crate::filter::Filters;
    use crate::testing::{LINE_PITCH, PageBuilder, cell_bounds, raw_flow_of};

    /// The comparison of a document with these pages with itself.
    fn comparison_of(pages: impl Fn() -> Vec<PageBuilder>, filters: &Filters) -> Comparison {
        let flows = BySide::from_fn(|_| raw_flow_of(pages()));
        Comparison::of(flows, filters)
    }

    fn outline(line: usize, columns: std::ops::Range<usize>, color: Rgb) -> Mark {
        Mark {
            bounds: cell_bounds(line, columns),
            color,
            style: MarkStyle::Outline,
        }
    }

    #[test]
    fn tokens_that_follow_each_other_closely_share_an_outline() {
        let page = || {
            vec![
                PageBuilder::new()
                    .artifact()
                    .text("a-b c-d\ne")
                    .body()
                    .text(" text"),
            ]
        };
        let comparison = comparison_of(page, &Filters::default());
        assert_eq!(
            marks_on(&comparison.flows.new, 0),
            [
                outline(0, 0..3, ARTIFACT_COLOR),
                // After the space of the test page, which is a character wide.
                outline(0, 4..7, ARTIFACT_COLOR),
                outline(1, 0..1, ARTIFACT_COLOR),
            ]
        );
    }

    #[test]
    fn tokens_that_different_rules_leave_out_have_outlines_of_their_own() {
        let page = || {
            vec![
                PageBuilder::new()
                    .artifact()
                    .text("x-")
                    .body()
                    .text("7 text"),
            ]
        };
        let filters = Filters {
            ignore_patterns: vec![Regex::new("7").unwrap()],
            ..Filters::default()
        };
        let comparison = comparison_of(page, &filters);
        assert_eq!(
            marks_on(&comparison.flows.new, 0),
            [
                outline(0, 0..2, ARTIFACT_COLOR),
                outline(0, 2..3, PATTERN_COLORS[0]),
            ]
        );
    }

    #[test]
    fn only_the_tokens_of_a_page_are_outlined_on_it() {
        let pages = || {
            vec![
                PageBuilder::new()
                    .artifact()
                    .text("one")
                    .body()
                    .text(" text"),
                PageBuilder::new()
                    .artifact()
                    .text("\ntwo")
                    .body()
                    .text(" more"),
            ]
        };
        let comparison = comparison_of(pages, &Filters::default());
        assert_eq!(
            marks_on(&comparison.flows.old, 1),
            [outline(1, 0..3, ARTIFACT_COLOR)]
        );
    }

    #[test]
    fn legend_has_an_entry_for_each_kind_of_rule_that_left_text_out() {
        // Three lines: at the top, in the middle and at the bottom of the page.
        let page = || {
            vec![
                PageBuilder::new()
                    .height(3.0 * LINE_PITCH)
                    .text("Title\nbody ")
                    .artifact()
                    .text("mark")
                    .body()
                    .text("\n1"),
            ]
        };
        let filters = Filters {
            header_height: LINE_PITCH,
            footer_height: LINE_PITCH,
            ..Filters::default()
        };
        let legend = legend_of(&comparison_of(page, &filters));
        let colors: Vec<Rgb> = legend.iter().map(|entry| entry.color).collect();
        assert_eq!(colors, [ARTIFACT_COLOR, HEADER_AND_FOOTER_COLOR]);
        assert!(legend[0].text.contains("artifact"));
        assert!(legend[1].text.contains("header"));

        let nothing_left_out = || vec![PageBuilder::new().text("body")];
        assert_eq!(
            legend_of(&comparison_of(nothing_left_out, &Filters::default())),
            []
        );
    }

    #[test]
    fn patterns_after_the_last_color_start_over_with_the_colors() {
        assert_eq!(
            color_of(Exclusion::Pattern(PATTERN_COLORS.len())),
            color_of(Exclusion::Pattern(0))
        );
    }
}
