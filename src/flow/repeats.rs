//! Text that a page repeats at its top.
//!
//! A table that continues on the next page starts there with its header
//! again, and a running header starts every page.
//! Where the pages break is layout, so such text is page furniture.
//! A tagged document marks it as an artifact.
//! In an untagged one nothing does, and it would show up as text that is
//! added where the new page break is and removed where the old one was.

use std::borrow::Cow;
use std::ops::Range;

use super::{Exclusion, RawToken, Screened, key_of};
use crate::diff;
use crate::{BySide, Side};

/// Fewer tokens than this are not taken for a repeat: one token alone is too
/// likely to be on both pages by chance.
const MIN_REPEATED_TOKENS: usize = 2;

/// How far apart the left edges of two tokens can be that are drawn at the
/// same place across their pages.
const LEFT_EDGE_TOLERANCE: f32 = 0.5;

/// Leaves out of the comparison what the pages of each document repeat at
/// their tops, where the other document has no text for it.
pub(super) fn exclude(screened: &mut BySide<Screened>) {
    let compared = screened.as_ref().map(Compared::of);
    let repeats = compared.as_ref().map(Compared::repeats);
    let indices_to_exclude = BySide::from_fn(|side| {
        let positions = repeats_without_counterpart(&compared, &repeats, side);
        let indices = positions
            .iter()
            .map(|&position| compared[side].indices[position]);
        indices.collect::<Vec<usize>>()
    });
    for side in Side::BOTH {
        for &index in &indices_to_exclude[side] {
            screened[side].exclusions[index] = Some(Exclusion::Repeat);
        }
    }
}

/// The positions of the repeated tokens of one side that match no text of
/// the other document.
///
/// A line that looks like a repeat can be text that happens to stand twice.
/// Then the other document has it too, and not only as a repeat of its own,
/// so its repeats are left out when looking for a match.
fn repeats_without_counterpart(
    compared: &BySide<Compared>,
    repeats: &BySide<Vec<Range<usize>>>,
    side: Side,
) -> Vec<usize> {
    if repeats[side].is_empty() {
        return Vec::new();
    }
    let keys = BySide::from_fn(|keys_side| match keys_side == side {
        true => compared[keys_side].keys_without(&[]),
        false => compared[keys_side].keys_without(&repeats[keys_side]),
    });
    let hunks = diff::diff(
        &keys
            .as_ref()
            .map(|keys| keys.iter().map(Cow::as_ref).collect()),
    );

    let mut is_repeated = vec![false; compared[side].tokens.len()];
    for repeat in &repeats[side] {
        is_repeated[repeat.clone()].fill(true);
    }
    let unmatched = hunks.iter().flat_map(|hunk| hunk[side].clone());
    unmatched
        .filter(|&position| is_repeated[position])
        .collect()
}

/// The tokens of a flow that no filter leaves out.
struct Compared<'a> {
    tokens: Vec<&'a RawToken>,
    /// The index in the flow of each of the tokens.
    indices: Vec<usize>,
}

/// The tokens of one line of a page, as a range of positions among the
/// compared tokens.
struct Line {
    page_index: usize,
    tokens: Range<usize>,
}

impl<'a> Compared<'a> {
    fn of(screened: &'a Screened) -> Self {
        let (indices, tokens) = screened.compared().unzip();
        Self { tokens, indices }
    }

    /// The keys of the tokens except those in `ranges` of positions, which
    /// ascend.
    fn keys_without(&self, ranges: &[Range<usize>]) -> Vec<Cow<'a, str>> {
        let positions = diff::indices_outside(self.tokens.len(), ranges);
        positions
            .map(|position| key_of(&self.tokens[position].text))
            .collect()
    }

    /// The ranges of tokens at the tops of pages that repeat lines of the
    /// page before.
    fn repeats(&self) -> Vec<Range<usize>> {
        let lines = self.lines();
        let pages: Vec<&[Line]> = lines
            .chunk_by(|line, next| line.page_index == next.page_index)
            .collect();
        let mut repeats = Vec::new();
        for pair in pages.windows(2) {
            let (previous, page) = (pair[0], pair[1]);
            if page[0].page_index != previous[0].page_index + 1 {
                continue;
            }
            let repeated_lines = &page[..self.repeated_line_count(previous, page)];
            let (Some(first), Some(last)) = (repeated_lines.first(), repeated_lines.last()) else {
                continue;
            };
            let tokens = first.tokens.start..last.tokens.end;
            if tokens.len() >= MIN_REPEATED_TOKENS {
                repeats.push(tokens);
            }
        }
        repeats
    }

    fn lines(&self) -> Vec<Line> {
        let mut lines: Vec<Line> = Vec::new();
        let mut last_line = None;
        for (position, token) in self.tokens.iter().enumerate() {
            let line = (token.fragment.page_index, token.fragment.line_index);
            match lines.last_mut() {
                Some(last) if last_line == Some(line) => last.tokens.end = position + 1,
                _ => lines.push(Line {
                    page_index: token.fragment.page_index,
                    tokens: position..position + 1,
                }),
            }
            last_line = Some(line);
        }
        lines
    }

    /// How many lines from the top of a page each repeat a line of the page
    /// before, in the order of those lines.
    fn repeated_line_count(&self, previous: &[Line], page: &[Line]) -> usize {
        let mut candidates = previous.iter();
        let repeats_a_candidate =
            |line: &&Line| candidates.any(|candidate| self.is_same_line(candidate, line));
        page.iter().take_while(repeats_a_candidate).count()
    }

    /// Whether two lines have the same tokens at the same places across
    /// their pages.
    fn is_same_line(&self, line: &Line, other: &Line) -> bool {
        let tokens = &self.tokens[line.tokens.clone()];
        let other_tokens = &self.tokens[other.tokens.clone()];
        tokens.len() == other_tokens.len()
            && tokens.iter().zip(other_tokens).all(|(token, other_token)| {
                let left = token.fragment.bounds.left;
                let other_left = other_token.fragment.bounds.left;
                key_of(&token.text) == key_of(&other_token.text)
                    && (left - other_left).abs() <= LEFT_EDGE_TOLERANCE
            })
    }
}
