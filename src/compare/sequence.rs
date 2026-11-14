//! The tokens that a diff aligns.

use std::borrow::Cow;
use std::iter;
use std::ops::Range;

use crate::flow::{Flow, Token};

/// Tokens of a flow in the order in which they are compared.
pub(super) struct Sequence<'a> {
    pub flow: &'a Flow,
    pub tokens: Vec<&'a Token>,
    /// The index in the flow of each of the tokens.
    indices: Vec<usize>,
    /// The key of each of the tokens.
    keys: Vec<Cow<'a, str>>,
}

impl<'a> Sequence<'a> {
    /// Every token of a flow.
    pub fn whole(flow: &'a Flow) -> Self {
        Self::without(flow, &[])
    }

    /// The tokens of a flow except those in `ranges` of its indices, which
    /// ascend.
    pub fn without(flow: &'a Flow, ranges: &[Range<usize>]) -> Self {
        let indices: Vec<usize> = indices_outside(flow.tokens.len(), ranges).collect();
        let tokens: Vec<&Token> = indices.iter().map(|&index| &flow.tokens[index]).collect();
        Self {
            flow,
            keys: tokens.iter().map(|token| token.key()).collect(),
            tokens,
            indices,
        }
    }

    pub fn keys(&self) -> Vec<&str> {
        self.keys.iter().map(|key| key.as_ref()).collect()
    }

    /// The index in the flow of the token at a position, or of the end of
    /// the flow for the position after the last token.
    pub fn index_at(&self, position: usize) -> usize {
        let end = self.flow.tokens.len();
        self.indices.get(position).copied().unwrap_or(end)
    }

    /// The tokens at a range of positions, each with its index in the flow.
    pub fn indexed_tokens(
        &self,
        positions: Range<usize>,
    ) -> impl Iterator<Item = (usize, &'a Token)> {
        let indices = self.indices[positions.clone()].iter().copied();
        indices.zip(self.tokens[positions].iter().copied())
    }
}

/// The indices below `len` that are in none of `ranges`, which ascend.
fn indices_outside(len: usize, ranges: &[Range<usize>]) -> impl Iterator<Item = usize> + '_ {
    let starts = iter::once(0).chain(ranges.iter().map(|range| range.end));
    let ends = ranges
        .iter()
        .map(|range| range.start)
        .chain(iter::once(len));
    starts.zip(ends).flat_map(|(start, end)| start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_outside_ranges_skip_what_is_in_them() {
        let outside = |ranges: &[Range<usize>]| indices_outside(6, ranges).collect::<Vec<_>>();
        assert_eq!(outside(&[]), [0, 1, 2, 3, 4, 5]);
        assert_eq!(outside(&[0..2, 3..3, 4..6]), [2, 3]);
    }
}
