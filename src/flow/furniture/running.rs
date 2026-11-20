//! Headers and footers that run from page to page: lines at the top or the
//! bottom of a page that a page nearby has at the same height and with the
//! same words, where a number may count the pages.

use super::{Compared, Line};
use crate::flow::Exclusion;

/// How many pages away the same header or footer is looked for: two, because
/// left and right pages can each have their own.
const MAX_PAGE_DISTANCE: usize = 2;

/// How much the distances of two lines from the edges of their pages can
/// differ for the lines to stand at the same height.
const HEIGHT_TOLERANCE: f32 = 0.5;

/// Marks the tokens of the running headers and footers.
pub(super) fn find(compared: &Compared, furniture: &mut [Option<Exclusion>]) {
    let edges = [
        (Edge::Top, Exclusion::RunningHeader),
        (Edge::Bottom, Exclusion::RunningFooter),
    ];
    for (edge, exclusion) in edges {
        let pages = PagesFromEdge::of(compared, edge);
        for page_index in 0..pages.lines.len() {
            for line in pages.running_lines(page_index) {
                furniture[line.tokens.clone()].fill(Some(exclusion));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Edge {
    Top,
    Bottom,
}

/// A line of a page, seen from the top or the bottom edge of the page.
struct LineFromEdge<'a> {
    line: &'a Line,
    /// How far from the edge the line starts.
    start: f32,
    /// How far from the edge the line ends.
    end: f32,
}

impl LineFromEdge<'_> {
    fn is_at_the_height_of(&self, other: &Self) -> bool {
        (self.start - other.start).abs() <= HEIGHT_TOLERANCE
            && (self.end - other.end).abs() <= HEIGHT_TOLERANCE
    }
}

struct PagesFromEdge<'a> {
    compared: &'a Compared<'a>,
    /// The lines of each page, from the nearest to the edge to the farthest.
    lines: Vec<Vec<LineFromEdge<'a>>>,
}

impl<'a> PagesFromEdge<'a> {
    fn of(compared: &'a Compared<'a>, edge: Edge) -> Self {
        let pages = compared.lines.iter().zip(compared.page_sizes);
        let lines = pages
            .map(|(lines, page_size)| {
                let from_edge = |line: &'a Line| {
                    let (start, end) = match edge {
                        Edge::Top => (line.bounds.top, line.bounds.bottom),
                        Edge::Bottom => (
                            page_size.height - line.bounds.bottom,
                            page_size.height - line.bounds.top,
                        ),
                    };
                    LineFromEdge { line, start, end }
                };
                let mut lines: Vec<LineFromEdge> = lines.iter().map(from_edge).collect();
                lines.sort_by(|line, other| line.start.total_cmp(&other.start));
                lines
            })
            .collect();
        Self { compared, lines }
    }

    /// The lines of a page that run, which no other line is nearer to the
    /// edge than.
    fn running_lines(&self, page_index: usize) -> impl Iterator<Item = &'a Line> {
        let lines = self.lines[page_index].iter();
        lines
            .take_while(move |line| self.runs(page_index, line))
            .map(|line| line.line)
    }

    /// Whether a page nearby has the line too.
    fn runs(&self, page_index: usize, line: &LineFromEdge) -> bool {
        let first_page = page_index.saturating_sub(MAX_PAGE_DISTANCE);
        let last_page = (page_index + MAX_PAGE_DISTANCE).min(self.lines.len() - 1);
        let mut nearby_pages = (first_page..=last_page).filter(|&nearby| nearby != page_index);
        nearby_pages.any(|nearby_page| {
            let pages_on = nearby_page as i64 - page_index as i64;
            self.lines[nearby_page].iter().any(|nearby| {
                line.is_at_the_height_of(nearby)
                    && self.has_the_words_of(line.line, nearby.line, pages_on)
            })
        })
    }

    /// Whether two lines have the same words, where a number of the second
    /// line may be as much larger as its page is further on: a page number.
    fn has_the_words_of(&self, line: &Line, other: &Line, pages_on: i64) -> bool {
        let tokens = self.compared.tokens_of(line);
        let other_tokens = self.compared.tokens_of(other);
        tokens.len() == other_tokens.len()
            && tokens.iter().zip(other_tokens).all(|(token, other_token)| {
                token.key() == other_token.key()
                    || counts_on(&token.text, &other_token.text, pages_on)
            })
    }
}

/// Whether both texts are numbers, the second one larger by `difference`.
fn counts_on(number: &str, other_number: &str, difference: i64) -> bool {
    match (number.parse::<i64>(), other_number.parse::<i64>()) {
        (Ok(number), Ok(other_number)) => other_number.checked_sub(number) == Some(difference),
        _ => false,
    }
}
