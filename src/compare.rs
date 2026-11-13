//! Comparing two documents: from what they read as to a [`Comparison`].

mod partners;
mod sequence;
mod sides;
#[cfg(test)]
mod tests;

use self::sequence::Sequence;
use crate::BySide;
use crate::change::Comparison;
use crate::diff;
use crate::filter::Filters;
use crate::flow::{self, RawFlow};

impl Comparison {
    pub fn of(flows: BySide<RawFlow>, filters: &Filters) -> Self {
        let [old, new] = flow::prepare([flows.old, flows.new], filters);
        let flows = BySide { old, new };
        let sequences = flows.as_ref().map(Sequence::whole);
        let hunks = diff::diff(&sequences.as_ref().map(Sequence::keys));
        let page_partners = partners::page_partners(&sequences, &hunks);
        let changes = hunks
            .iter()
            .map(|hunk| sides::change_at(hunk, &sequences, &page_partners))
            .collect();
        Self {
            flows,
            changes,
            page_partners,
        }
    }
}
