//! Page furniture that only its place on the page gives away.
//!
//! A producer that marks artifacts says what is on a page because of how
//! the text falls onto pages: the headers and footers that run from page to
//! page, and the header that a table starts with again after a page break.
//! Where nothing is marked, the place of a line is all that tells, and
//! [`running`] and [`repeats`] each find the lines that stand where
//! furniture does.
//!
//! Such a line can also be text that happens to stand there.
//! Then the other document has the text too, and not as furniture of its
//! own, so a line is only left out where the other document, without what
//! looks like furniture in it, has no text for it.

mod repeats;
mod running;

use std::borrow::Cow;
use std::ops::Range;

use super::{Exclusion, RawToken, Screened};
use crate::diff;
use crate::geometry::{Rect, Size};
use crate::{BySide, Side};

/// Leaves out of the comparison what stands where page furniture does in
/// each document, where the other document has no text for it.
pub(super) fn exclude(screened: &mut BySide<Screened>) {
    let compared = screened.as_ref().map(Compared::of);
    let furniture = compared.as_ref().map(Compared::furniture);
    // The index in its flow of each token to leave out, with the rule.
    let left_out = BySide::from_fn(|side| {
        let positions = without_counterpart(&compared, &furniture, side);
        let rules = positions
            .iter()
            .map(|&position| (compared[side].indices[position], furniture[side][position]));
        rules.collect::<Vec<_>>()
    });
    for side in Side::BOTH {
        for &(index, rule) in &left_out[side] {
            screened[side].exclusions[index] = rule;
        }
    }
}

/// The positions of the tokens of one side that look like furniture and
/// match no text of the other document.
fn without_counterpart(
    compared: &BySide<Compared>,
    furniture: &BySide<Vec<Option<Exclusion>>>,
    side: Side,
) -> Vec<usize> {
    if furniture[side].iter().all(Option::is_none) {
        return Vec::new();
    }
    // All of this side, and of the other side what does not look like
    // furniture.
    let keys = BySide::from_fn(|keys_side| {
        let tokens = compared[keys_side].tokens.iter().zip(&furniture[keys_side]);
        let kept = tokens.filter(|(_, rule)| keys_side == side || rule.is_none());
        kept.map(|(token, _)| token.key()).collect::<Vec<_>>()
    });
    let hunks = diff::diff(
        &keys
            .as_ref()
            .map(|keys| keys.iter().map(Cow::as_ref).collect()),
    );
    let unmatched = hunks.iter().flat_map(|hunk| hunk[side].clone());
    unmatched
        .filter(|&position| furniture[side][position].is_some())
        .collect()
}

/// The tokens of a flow that no filter leaves out.
struct Compared<'a> {
    tokens: Vec<&'a RawToken>,
    /// The index in the flow of each of the tokens.
    indices: Vec<usize>,
    /// The lines of each page, in reading order.
    lines: Vec<Vec<Line>>,
    page_sizes: &'a [Size],
    /// Whether the producer of the document marks artifacts.
    marks_artifacts: bool,
}

/// The tokens of one line of a page, as a range of positions among the
/// compared tokens.
struct Line {
    tokens: Range<usize>,
    bounds: Rect,
}

impl<'a> Compared<'a> {
    fn of(screened: &'a Screened) -> Self {
        let flow = &screened.flow;
        let (indices, tokens): (Vec<usize>, Vec<&RawToken>) = screened.compared().unzip();
        Self {
            lines: lines_by_page(&tokens, flow.page_sizes.len()),
            tokens,
            indices,
            page_sizes: &flow.page_sizes,
            marks_artifacts: flow.tokens.iter().any(|token| token.is_artifact),
        }
    }

    /// For each token, the rule by which it looks like furniture, if it
    /// does by one.
    ///
    /// A producer that marks artifacts marks its running headers and
    /// footers, so none are looked for among its other text.
    fn furniture(&self) -> Vec<Option<Exclusion>> {
        let mut furniture = vec![None; self.tokens.len()];
        if !self.marks_artifacts {
            running::find(self, &mut furniture);
        }
        repeats::find(self, &mut furniture);
        furniture
    }

    fn tokens_of(&self, line: &Line) -> &[&'a RawToken] {
        &self.tokens[line.tokens.clone()]
    }
}

fn lines_by_page(tokens: &[&RawToken], page_count: usize) -> Vec<Vec<Line>> {
    let mut pages: Vec<Vec<Line>> = (0..page_count).map(|_| Vec::new()).collect();
    let line_of = |token: &RawToken| (token.fragment.page_index, token.fragment.line_index);
    let mut position = 0;
    for line in tokens.chunk_by(|token, next| line_of(token) == line_of(next)) {
        let bounds = line.iter().map(|token| token.fragment.bounds);
        pages[line[0].fragment.page_index].push(Line {
            tokens: position..position + line.len(),
            bounds: bounds
                .reduce(|bounds, next| bounds.union(&next))
                .expect("a line has a token"),
        });
        position += line.len();
    }
    pages
}
