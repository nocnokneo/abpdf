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

        // Each change with the indices in the new and in the old flow that
        // it is at, which is what the changes are ordered by.
        let place_of = |tokens: &BySide<Range<usize>>, sequences: &BySide<Sequence>| {
            let place = BySide::from_fn(|side| sequences[side].index_at(tokens[side].start));
            (place.new, place.old)
        };
        let mut changes: Vec<((usize, usize), Change)> = Vec::new();
        for hunk in &hunks {
            let change = sides::change_at(hunk, &in_place, &page_partners);
            changes.push((place_of(hunk, &in_place), change));
        }
        for moved in &moves {
            let change = sides::moved(moved, &whole);
            changes.push((place_of(&moved.tokens, &whole), change));
            for hunk in &moved.inner_hunks {
                let change = sides::change_at(hunk, &whole, &page_partners);
                changes.push((place_of(hunk, &whole), change));
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
