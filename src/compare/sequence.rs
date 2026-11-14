//! The tokens that a diff aligns.

use std::borrow::Cow;
use std::ops::Range;

use crate::diff;
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
        let indices: Vec<usize> = diff::indices_outside(flow.tokens.len(), ranges).collect();
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
