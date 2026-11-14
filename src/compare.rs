//! Comparing two documents: from what they read as to a [`Comparison`].

mod moves;
mod partners;
mod sequence;
mod sides;
mod styles;
#[cfg(test)]
mod tests;

use std::ops::Range;

use self::sequence::Sequence;
use crate::BySide;
use crate::change::{Change, Comparison};
use crate::diff::{self, Hunk};
use crate::filter::Filters;
use crate::flow::{self, RawFlow};

impl Comparison {
    pub fn of(flows: BySide<RawFlow>, filters: &Filters) -> Self {
        let flows = flow::prepare(flows, filters);
        let whole = flows.as_ref().map(Sequence::whole);
        let whole_keys = whole.as_ref().map(Sequence::keys);
        let whole_hunks = diff::diff(&whole_keys);
        let moves = moves::find(&whole_keys, &whole_hunks);

        // What did not move is compared again without what did, in place.
        let in_place = BySide::from_fn(|side| {
            let mut moved: Vec<Range<usize>> = moves
                .iter()
                .map(|moved| moved.tokens[side].clone())
                .collect();
            moved.sort_by_key(|tokens| tokens.start);
            Sequence::without(&flows[side], &moved)
        });
        let hunks = match moves.is_empty() {
            true => whole_hunks,
            false => diff::diff(&in_place.as_ref().map(Sequence::keys)),
        };
        let page_partners = partners::page_partners(&in_place, &hunks);

        let all_in_place = in_place.as_ref().map(|sequence| 0..sequence.tokens.len());
        let mut changes = changes_within(&all_in_place, &hunks, &in_place, &page_partners);
        for moved in &moves {
            let place = (moved.tokens.new.start, moved.tokens.old.start);
            changes.push((place, sides::moved(moved, &whole)));
            let inside = changes_within(&moved.tokens, &moved.inner_hunks, &whole, &page_partners);
            changes.extend(inside);
        }
        // A move comes before the changes inside it that start where it does.
        changes.sort_by_key(|(place, _)| *place);

        Self {
            changes: changes.into_iter().map(|(_, change)| change).collect(),
            page_partners,
            flows,
        }
    }
}

/// The indices in the new and in the old flow that a change is at, which is
/// what the changes are ordered by.
type Place = (usize, usize);

/// The changes in the ranges `within` of two sequences, of which `hunks` are
/// a diff: what differs at each hunk, and the styles that differ in the text
/// that the hunks leave matched.
fn changes_within(
    within: &BySide<Range<usize>>,
    hunks: &[Hunk],
    sequences: &BySide<Sequence>,
    page_partners: &BySide<Vec<Option<usize>>>,
) -> Vec<(Place, Change)> {
    let place_of = |positions: &BySide<Range<usize>>| {
        let place = BySide::from_fn(|side| sequences[side].index_at(positions[side].start));
        (place.new, place.old)
    };
    let mut changes = Vec::new();
    for hunk in hunks {
        let change = sides::change_at(hunk, sequences, page_partners);
        changes.push((place_of(hunk), change));
    }
    for matched in diff::matches(hunks, within) {
        for (positions, change) in styles::restyled(&matched, sequences) {
            changes.push((place_of(&positions), change));
        }
    }
    changes
}
