//! The parts of a hunk that are changes of their own.

use std::ops::Range;

use super::figures;
use super::sequence::Sequence;
use crate::BySide;
use crate::diff::Hunk;
use crate::flow::Cell;

/// The parts of a hunk that differ.
///
/// A diff says of a hunk only that its two sides differ.
/// What tokens are says more:
///
/// - A figure of one side faces a figure of the other, and the text
///   before, between and after the figures faces the text there.
/// - Text in a table cell faces the text of a cell, and text outside of
///   every cell faces such text.
///
/// So each side is cut around its figures, and its text is cut where it
/// goes from one cell into another, into a table or out of one.
/// The pieces of the two sides belong together in their order, if there are
/// as many of one side as of the other.
/// Otherwise nothing says which piece belongs to which, and what cannot be
/// cut stays whole.
///
/// A piece that is the same on both sides is no difference: the diff
/// leaves something short that did not change inside a hunk if what is
/// around it changed.
pub(super) fn split(hunk: &Hunk, sequences: &BySide<Sequence>) -> Vec<Hunk> {
    let pieces = around_figures(hunk, sequences)
        .into_iter()
        .flat_map(|part| by_cell(part, sequences));
    pieces
        .filter(|piece| !is_the_same(piece, sequences))
        .collect()
}

/// The figures of a hunk, each facing the figure of the other side, and the
/// text before, between and after them, if one side has any there.
/// The hunk itself if its sides do not have the same number of figures.
fn around_figures(hunk: &Hunk, sequences: &BySide<Sequence>) -> Vec<Hunk> {
    let figures = BySide::from_fn(|side| {
        let is_figure = |position: &usize| sequences[side].tokens[*position].figure.is_some();
        hunk[side].clone().filter(is_figure).collect::<Vec<_>>()
    });
    if figures.old.len() != figures.new.len() {
        return vec![hunk.clone()];
    }
    let mut parts = Vec::new();
    let mut text_start = hunk.as_ref().map(|positions| positions.start);
    let ends = hunk.as_ref().map(|positions| positions.end);
    let figure_pairs = figures.old.iter().zip(&figures.new);
    let stops = figure_pairs
        .map(|(old, new)| BySide {
            old: *old,
            new: *new,
        })
        .chain([ends]);
    for stop in stops {
        let text = BySide::from_fn(|side| text_start[side]..stop[side]);
        if !(text.old.is_empty() && text.new.is_empty()) {
            parts.push(text);
        }
        if stop != ends {
            parts.push(stop.map(|figure| figure..figure + 1));
        }
        text_start = stop.map(|figure| figure + 1);
    }
    parts
}

/// The text of a hunk cell by cell, if its two sides go through as many
/// cells and stretches outside of cells, in the same order.
/// The hunk itself otherwise, and if it is no text.
fn by_cell(hunk: Hunk, sequences: &BySide<Sequence>) -> Vec<Hunk> {
    let stretches = BySide::from_fn(|side| stretches_of(&sequences[side], hunk[side].clone()));
    let pairs = || stretches.old.iter().zip(&stretches.new);
    let belong_together = stretches.old.len() == stretches.new.len()
        && pairs().all(|(old, new)| old.cell.is_some() == new.cell.is_some());
    if !belong_together {
        return vec![hunk];
    }
    pairs()
        .map(|(old, new)| BySide {
            old: old.positions.clone(),
            new: new.positions.clone(),
        })
        .collect()
}

/// Tokens of a sequence that follow each other in one cell, or outside of
/// every cell.
struct Stretch {
    positions: Range<usize>,
    cell: Option<Cell>,
}

/// The stretches that the tokens at `positions` of a sequence fall into.
fn stretches_of(sequence: &Sequence, positions: Range<usize>) -> Vec<Stretch> {
    let mut stretches: Vec<Stretch> = Vec::new();
    for position in positions {
        let cell = sequence.tokens[position].cell;
        match stretches.last_mut() {
            Some(stretch) if stretch.cell == cell => stretch.positions.end = position + 1,
            _ => stretches.push(Stretch {
                positions: position..position + 1,
                cell,
            }),
        }
    }
    stretches
}

/// Whether the two sides of a piece have the same tokens.
fn is_the_same(piece: &Hunk, sequences: &BySide<Sequence>) -> bool {
    let keys = BySide::from_fn(|side| sequences[side].keys_at(piece[side].clone()));
    let tokens = BySide::from_fn(|side| &sequences[side].tokens[piece[side].clone()]);
    let mut pairs = tokens.old.iter().zip(tokens.new);
    keys.old == keys.new && pairs.all(|(old, new)| figures::are_drawn_alike(old, new))
}
