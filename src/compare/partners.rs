//! Which pages of the two documents belong together.

use std::collections::BTreeMap;

use super::sequence::Sequence;
use crate::BySide;
use crate::diff::{self, Hunk};
use crate::flow::Token;

/// For each page of each document, the page of the other document that it
/// shares most tokens with that the diff matched.
/// A page that shares none has no partner.
pub(super) fn page_partners(
    sequences: &BySide<Sequence>,
    hunks: &[Hunk],
) -> BySide<Vec<Option<usize>>> {
    let shared_token_counts = shared_token_counts(sequences, hunks);
    let page_counts = sequences
        .as_ref()
        .map(|sequence| sequence.flow.page_sizes.len());
    let mut partners = page_counts.map(|page_count| vec![None; page_count]);
    let mut most_shared = page_counts.map(|page_count| vec![0; page_count]);
    // The counts come in the order of the pages, so of the pages that share
    // equally many tokens with a page, the first one is its partner.
    for (&(old_page, new_page), &count) in &shared_token_counts {
        if count > most_shared.old[old_page] {
            most_shared.old[old_page] = count;
            partners.old[old_page] = Some(new_page);
        }
        if count > most_shared.new[new_page] {
            most_shared.new[new_page] = count;
            partners.new[new_page] = Some(old_page);
        }
    }
    partners
}

/// How many matched tokens each pair of an old and a new page shares.
fn shared_token_counts(
    sequences: &BySide<Sequence>,
    hunks: &[Hunk],
) -> BTreeMap<(usize, usize), usize> {
    let whole = sequences.as_ref().map(|sequence| 0..sequence.tokens.len());
    let mut counts = BTreeMap::new();
    for matched in diff::matches(hunks, &whole) {
        let old_tokens = &sequences.old.tokens[matched.old];
        let new_tokens = &sequences.new.tokens[matched.new];
        for (old_token, new_token) in old_tokens.iter().zip(new_tokens) {
            *counts
                .entry((first_page(old_token), first_page(new_token)))
                .or_default() += 1;
        }
    }
    counts
}

fn first_page(token: &Token) -> usize {
    token.fragments[0].page_index
}
