//! Changes in a table, cell by cell.

use std::ops::Range;

use super::sequence::Sequence;
use crate::BySide;
use crate::diff::Hunk;
use crate::flow::Cell;

/// The parts of a hunk that differ, each in one table cell on both of its
/// sides or outside of every cell on both.
///
/// A side is cut where its tokens go from one cell into another, and where
/// they go into a table or out of one.
/// The pieces of the two sides belong together in their order if there are
/// as many of one side as of the other and every cell faces a cell.
/// Otherwise nothing says which piece belongs to which, and the hunk stays
/// whole.
///
/// A piece that reads the same on both sides is no difference: the diff
/// leaves a short cell that did not change inside a hunk if the cells
/// around it changed.
pub(super) fn split(hunk: &Hunk, sequences: &BySide<Sequence>) -> Vec<Hunk> {
    let pieces = BySide::from_fn(|side| pieces_of(&sequences[side], hunk[side].clone()));
    let pairs = || pieces.old.iter().zip(&pieces.new);
    let belong_together = pieces.old.len() == pieces.new.len()
        && pairs().all(|(old, new)| old.cell.is_some() == new.cell.is_some());
    if !belong_together {
        return vec![hunk.clone()];
    }
    pairs()
        .map(|(old, new)| BySide {
            old: old.positions.clone(),
            new: new.positions.clone(),
        })
        .filter(|piece| {
            let keys = BySide::from_fn(|side| sequences[side].keys_at(piece[side].clone()));
            keys.old != keys.new
        })
        .collect()
}

/// Tokens that follow each other in one cell, or outside of every cell.
struct Piece {
    positions: Range<usize>,
    cell: Option<Cell>,
}

/// The pieces that the tokens at `positions` of a sequence fall into.
fn pieces_of(sequence: &Sequence, positions: Range<usize>) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    for position in positions {
        let cell = sequence.tokens[position].cell;
        match pieces.last_mut() {
            Some(piece) if piece.cell == cell => piece.positions.end = position + 1,
            _ => pieces.push(Piece {
                positions: position..position + 1,
                cell,
            }),
        }
    }
    pieces
}
