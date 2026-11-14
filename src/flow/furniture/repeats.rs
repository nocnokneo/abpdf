//! Text that a page repeats at its top: the first lines of a page that also
//! stand on the page before, with the same words at the same places across
//! the page.
//!
//! A table that continues on the next page starts there with its header
//! again.
//! Where the pages break is layout, so without a mark the header would show
//! up as text that is added where the new page break is and removed where
//! the old one was.

use super::{Compared, Line};
use crate::flow::{Exclusion, key_of};

/// Fewer tokens than this are not taken for a repeat: one token alone is too
/// likely to be on both pages by chance.
const MIN_REPEATED_TOKENS: usize = 2;

/// How far apart the left edges of two tokens can be that are drawn at the
/// same place across their pages.
const LEFT_EDGE_TOLERANCE: f32 = 0.5;

/// Marks the tokens that pages repeat at their tops.
///
/// Lines that are marked already do not count as lines of their pages: a
/// running header stands above what a page repeats.
pub(super) fn find(compared: &Compared, furniture: &mut [Option<Exclusion>]) {
    let pages: Vec<Vec<&Line>> = compared
        .lines
        .iter()
        .map(|lines| {
            let unmarked = lines
                .iter()
                .filter(|line| furniture[line.tokens.start].is_none());
            unmarked.collect()
        })
        .collect();
    for pair in pages.windows(2) {
        let (previous, page) = (&pair[0], &pair[1]);
        let repeated_lines = &page[..repeated_line_count(compared, previous, page)];
        let token_count: usize = repeated_lines.iter().map(|line| line.tokens.len()).sum();
        if token_count >= MIN_REPEATED_TOKENS {
            for line in repeated_lines {
                furniture[line.tokens.clone()].fill(Some(Exclusion::Repeat));
            }
        }
    }
}

/// How many lines from the top of a page each repeat a line of the page
/// before, in the order of those lines.
fn repeated_line_count(compared: &Compared, previous: &[&Line], page: &[&Line]) -> usize {
    let mut candidates = previous.iter();
    let repeats_a_candidate =
        |line: &&&Line| candidates.any(|candidate| is_same_line(compared, candidate, line));
    page.iter().take_while(repeats_a_candidate).count()
}

/// Whether two lines have the same tokens at the same places across their
/// pages.
fn is_same_line(compared: &Compared, line: &Line, other: &Line) -> bool {
    let tokens = compared.tokens_of(line);
    let other_tokens = compared.tokens_of(other);
    tokens.len() == other_tokens.len()
        && tokens.iter().zip(other_tokens).all(|(token, other_token)| {
            let left = token.fragment.bounds.left;
            let other_left = other_token.fragment.bounds.left;
            key_of(&token.text) == key_of(&other_token.text)
                && (left - other_left).abs() <= LEFT_EDGE_TOLERANCE
        })
}
