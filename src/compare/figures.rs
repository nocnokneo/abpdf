//! Figures that both documents have, and that are not drawn the same.

use super::sequence::Sequence;
use crate::BySide;
use crate::diff::{Hunk, Match};
use crate::flow::Token;

/// The figures in a run of tokens that the documents have in common which
/// do not look the same in both, as a hunk each.
///
/// The key of a figure says what the figure is drawn with and not where its
/// parts are, so a diff takes two figures for the same that differ in that
/// alone.
pub(super) fn redrawn(matched: &Match, sequences: &BySide<Sequence>) -> Vec<Hunk> {
    let pairs = matched.old.clone().zip(matched.new.clone());
    pairs
        .filter(|(old, new)| {
            !are_drawn_alike(sequences.old.tokens[*old], sequences.new.tokens[*new])
        })
        .map(|(old, new)| BySide {
            old: old..old + 1,
            new: new..new + 1,
        })
        .collect()
}

/// Whether two tokens with the same key look the same, as far as the key
/// does not say: text does, and figures do if they are drawn through points
/// that lie the same way to each other.
pub(super) fn are_drawn_alike(old: &Token, new: &Token) -> bool {
    match (&old.figure, &new.figure) {
        (Some(old), Some(new)) => old.looks_like(new),
        _ => true,
    }
}
