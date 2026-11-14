//! Which pages the sheets of a redline show next to each other.

use std::collections::BTreeSet;

use crate::change::{Change, ChangeSide, Comparison};
use crate::{BySide, Side};

/// The indices of the pages that a sheet shows.
/// A side has none if nothing of its document belongs on the sheet.
pub(super) type PagePair = BySide<Option<usize>>;

/// The pairs of pages that together show every change and each of `pages`,
/// in the order of their sheets.
///
/// A change is shown as a whole where it starts: the pages on which its two
/// sides start are on one sheet.
/// Any other page is shown next to its partner, the page of the other
/// document that it shares most text with, or alone if it has none.
pub(super) fn page_pairs(
    comparison: &Comparison,
    pages: &BySide<BTreeSet<usize>>,
) -> Vec<PagePair> {
    let mut pairs: Vec<PagePair> = Vec::new();
    for change in &comparison.changes {
        let first_pages = first_pages_of(change);
        if !pairs.iter().any(|pair| shows(pair, &first_pages)) {
            pairs.push(first_pages);
        }
    }
    for side in [Side::New, Side::Old] {
        for &page_index in &pages[side] {
            if !pairs.iter().any(|pair| pair[side] == Some(page_index)) {
                pairs.push(next_to_partner(comparison, side, page_index));
            }
        }
    }
    pairs.sort_by_key(|pair| sheet_order(comparison, pair));
    pairs
}

/// The pages on which the two sides of a change start.
pub(super) fn first_pages_of(change: &Change) -> PagePair {
    change.sides.as_ref().map(ChangeSide::first_page_index)
}

/// Whether a sheet of the pair shows each of `pages` that there is.
pub(super) fn shows(pair: &PagePair, pages: &PagePair) -> bool {
    let shows_page_of = |side: Side| pages[side].is_none() || pages[side] == pair[side];
    Side::BOTH.into_iter().all(shows_page_of)
}

/// The pages of one document that a change is on.
pub(super) fn pages_with_changes(comparison: &Comparison, side: Side) -> BTreeSet<usize> {
    let changes = comparison.changes.iter();
    let areas = changes.flat_map(|change| change.sides[side].areas());
    areas.map(|area| area.page_index).collect()
}

fn next_to_partner(comparison: &Comparison, side: Side, page_index: usize) -> PagePair {
    let partner = comparison.page_partners[side][page_index];
    BySide::from_fn(|pair_side| match pair_side == side {
        true => Some(page_index),
        false => partner,
    })
}

/// What sheets are sorted by: they are in the order of the new document, and
/// those that show one new page in the order of the old document.
///
/// A pair without a page of one document counts as the page of that
/// document near which its other page belongs.
fn sheet_order(comparison: &Comparison, pair: &PagePair) -> (usize, usize) {
    let place_in = |side: Side| {
        let page_or_place = pair[side].or_else(|| {
            let other_side = side.other();
            place_in_other_document(comparison, other_side, pair[other_side]?)
        });
        page_or_place.unwrap_or(0)
    };
    (place_in(Side::New), place_in(Side::Old))
}

/// The page of the other document near which a page of the document on
/// `side` belongs: its partner, or that of the nearest page before it that
/// has one.
fn place_in_other_document(
    comparison: &Comparison,
    side: Side,
    page_index: usize,
) -> Option<usize> {
    let partners = &comparison.page_partners[side][..=page_index];
    partners.iter().rev().find_map(|partner| *partner)
}
