//! Comparing two documents: from what they read as to a [`Comparison`].

mod moves;
mod partners;
mod sequence;
mod sides;
#[cfg(test)]
mod tests;

use std::ops::Range;

use self::sequence::Sequence;
use crate::BySide;
use crate::change::{Change, Comparison};
use crate::diff;
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
        let without_moves;
        let (in_place, hunks) = match moves.is_empty() {
            true => (&whole, whole_hunks),
            false => {
                without_moves = BySide::from_fn(|side| {
                    let mut moved: Vec<Range<usize>> = moves
                        .iter()
                        .map(|moved| moved.tokens[side].clone())
                        .collect();
                    moved.sort_by_key(|tokens| tokens.start);
                    Sequence::without(&flows[side], &moved)
                });
                let hunks = diff::diff(&without_moves.as_ref().map(Sequence::keys));
                (&without_moves, hunks)
            }
        };
        let page_partners = partners::page_partners(in_place, &hunks);

        // Each change with the indices in the new and in the old flow that
        // it is at, which is what the changes are ordered by.
        let mut changes: Vec<((usize, usize), Change)> = Vec::new();
        for hunk in &hunks {
            let place = BySide::from_fn(|side| in_place[side].index_at(hunk[side].start));
            let change = sides::change_at(hunk, in_place, &page_partners);
            changes.push(((place.new, place.old), change));
        }
        for moved in &moves {
            let place = (moved.tokens.new.start, moved.tokens.old.start);
            changes.push((place, sides::moved(moved, &whole)));
            for hunk in &moved.inner_hunks {
                let change = sides::change_at(hunk, &whole, &page_partners);
                changes.push(((hunk.new.start, hunk.old.start), change));
            }
        }
        changes.sort_by_key(|(place, _)| *place);

        Self {
            changes: changes.into_iter().map(|(_, change)| change).collect(),
            page_partners,
            flows,
        }
    }
}
