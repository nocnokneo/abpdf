//! The tokens that a diff aligns.

use std::borrow::Cow;

use crate::flow::{Flow, Token};

/// The tokens of a flow in the order in which they are compared.
pub(super) struct Sequence<'a> {
    pub flow: &'a Flow,
    pub tokens: Vec<&'a Token>,
    /// The key of each of the tokens.
    keys: Vec<Cow<'a, str>>,
}

impl<'a> Sequence<'a> {
    /// Every token of a flow.
    pub fn whole(flow: &'a Flow) -> Self {
        Self {
            flow,
            tokens: flow.tokens.iter().collect(),
            keys: flow.tokens.iter().map(Token::key).collect(),
        }
    }

    pub fn keys(&self) -> Vec<&str> {
        self.keys.iter().map(|key| key.as_ref()).collect()
    }
}
