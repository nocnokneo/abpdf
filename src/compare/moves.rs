//! Text that moved.
//!
//! A diff aligns two documents in one order, so text that stands at another
//! place in the new document is, to the diff, removed at the old place and
//! added at the new one.
//! Moves are found among what the diff leaves unmatched: runs of removed
//! tokens that equal runs of added ones.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::diff::{self, Hunk, Sizes};
use crate::{BySide, Side};

/// Text of fewer characters than this is not taken to have moved: a few
/// words are too likely to be removed at one place and added at another by
/// chance.
const MIN_MOVED_CHARS: usize = 20;

/// Text that stands at one place in the old document and at another in the
/// new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Move {
    /// The positions of the moved tokens in each document.
    pub tokens: BySide<Range<usize>>,
    /// The places inside the moved text where the documents differ.
    pub inner_hunks: Vec<Hunk>,
}

/// An equal run of tokens of the two documents.
type Block = BySide<Range<usize>>;

/// The moves among the tokens that `hunks` leave unmatched.
///
/// `keys` are those of all tokens of the two documents, and `hunks` a diff of
/// them.
pub(super) fn find(keys: &BySide<Vec<&str>>, hunks: &[Hunk]) -> Vec<Move> {
    let mut finder = Finder {
        keys,
        sizes: Sizes::measure(keys),
        is_free: BySide::from_fn(|side| {
            let mut is_free = vec![false; keys[side].len()];
            for hunk in hunks {
                is_free[hunk[side].clone()].fill(true);
            }
            is_free
        }),
    };
    let mut blocks = finder.equal_blocks();
    // The larger a block, the surer it is what it seems to be, so the
    // largest ones take their tokens and their places first.
    blocks.sort_by_key(|block| (Reverse(finder.sizes.of(block)), block.old.start));

    let mut in_place = InPlace::matched_by(hunks, keys);
    let mut moved = Vec::new();
    for block in blocks {
        let Some(block) = finder.free_part(block) else {
            continue;
        };
        finder.take(&block);
        if in_place.is_in_order(&block) {
            in_place.add(&block);
        } else {
            moved.push(block);
        }
    }
    let mut moves = finder.chain(moved);
    moves.retain(|moved| finder.sizes.of(&moved.tokens) >= MIN_MOVED_CHARS);
    moves
}

struct Finder<'a> {
    keys: &'a BySide<Vec<&'a str>>,
    sizes: Sizes,
    /// Whether each token is unmatched and no block has taken it.
    is_free: BySide<Vec<bool>>,
}

impl Finder<'_> {
    /// The largest equal runs of free tokens around the tokens that are easy
    /// to find a counterpart for: those whose key no other free token of
    /// their document has.
    fn equal_blocks(&self) -> Vec<Block> {
        // Where the last block ends in the old document, for each distance
        // between the positions of a block in the two documents.
        // The anchors ascend, so one that is before that end is in the block.
        let mut last_block_ends: HashMap<isize, usize> = HashMap::new();
        let mut blocks = Vec::new();
        for (old, new) in self.anchors() {
            let distance = new as isize - old as isize;
            let is_in_last_block = last_block_ends.get(&distance).is_some_and(|&end| old < end);
            if !is_in_last_block {
                let block = self.block_around(old, new);
                last_block_ends.insert(distance, block.old.end);
                blocks.push(block);
            }
        }
        blocks
    }

    /// The pairs of an old and a new position whose tokens share a key that
    /// no other free token of either document has.
    fn anchors(&self) -> Vec<(usize, usize)> {
        let lone_positions = BySide::from_fn(|side| {
            let mut occurrences: HashMap<&str, (usize, usize)> = HashMap::new();
            for (position, key) in self.keys[side].iter().enumerate() {
                if self.is_free[side][position] {
                    let (count, last_position) = occurrences.entry(key).or_default();
                    *count += 1;
                    *last_position = position;
                }
            }
            occurrences.retain(|_, (count, _)| *count == 1);
            occurrences
        });
        let mut anchors: Vec<(usize, usize)> = lone_positions
            .old
            .iter()
            .filter_map(|(key, &(_, old))| Some((old, lone_positions.new.get(key)?.1)))
            .collect();
        anchors.sort_unstable();
        anchors
    }

    /// The longest run of equal free tokens through a pair of positions.
    fn block_around(&self, old: usize, new: usize) -> Block {
        let is_equal = |old: usize, new: usize| {
            self.is_free.old[old]
                && self.is_free.new[new]
                && self.keys.old[old] == self.keys.new[new]
        };
        let mut before = 0;
        while before < old.min(new) && is_equal(old - before - 1, new - before - 1) {
            before += 1;
        }
        let ends = self.keys.as_ref().map(Vec::len);
        let mut after = 1;
        while old + after < ends.old && new + after < ends.new && is_equal(old + after, new + after)
        {
            after += 1;
        }
        BySide {
            old: old - before..old + after,
            new: new - before..new + after,
        }
    }

    /// What is left of a block when the tokens that other blocks have taken
    /// are cut off its ends, unless they also took tokens from its middle.
    fn free_part(&self, block: Block) -> Option<Block> {
        let is_free = |offset: &usize| {
            self.is_free.old[block.old.start + offset] && self.is_free.new[block.new.start + offset]
        };
        let mut offsets = 0..block.old.len();
        let first = offsets.find(is_free)?;
        let last = offsets.rfind(is_free).unwrap_or(first);
        (first..=last)
            .all(|offset| is_free(&offset))
            .then(|| BySide {
                old: block.old.start + first..block.old.start + last + 1,
                new: block.new.start + first..block.new.start + last + 1,
            })
    }

    fn take(&mut self, tokens: &BySide<Range<usize>>) {
        for side in Side::BOTH {
            self.is_free[side][tokens[side].clone()].fill(false);
        }
    }

    /// Makes one move of blocks that follow each other in both documents
    /// with little between them, which is then a difference inside the move.
    ///
    /// Little is what is no larger than either of the two: the rule by which
    /// a diff takes a small match between two hunks for a coincidence, the
    /// other way around.
    fn chain(&mut self, mut blocks: Vec<Block>) -> Vec<Move> {
        blocks.sort_by_key(|block| block.old.start);
        let mut moves: Vec<Move> = Vec::new();
        for block in blocks {
            let between = moves
                .last()
                .and_then(|last| self.small_gap(&last.tokens, &block));
            match (moves.last_mut(), between) {
                (Some(last), Some(between)) => {
                    self.take(&between);
                    last.tokens = BySide::from_fn(|side| last.tokens[side].start..block[side].end);
                    if !(between.old.is_empty() && between.new.is_empty()) {
                        last.inner_hunks.push(between);
                    }
                }
                _ => moves.push(Move {
                    tokens: block,
                    inner_hunks: Vec::new(),
                }),
            }
        }
        moves
    }

    /// The free tokens between two runs, if the second follows the first in
    /// both documents and nothing but a few free tokens stands between them.
    fn small_gap(
        &self,
        first: &BySide<Range<usize>>,
        second: &BySide<Range<usize>>,
    ) -> Option<BySide<Range<usize>>> {
        let follows = Side::BOTH
            .into_iter()
            .all(|side| first[side].end <= second[side].start);
        if !follows {
            return None;
        }
        let between = BySide::from_fn(|side| first[side].end..second[side].start);
        if !self.sizes.is_small_between(&between, first, second) {
            return None;
        }
        let is_free = Side::BOTH.into_iter().all(|side| {
            let tokens = &self.is_free[side][between[side].clone()];
            tokens.iter().all(|&is_free| is_free)
        });
        is_free.then_some(between)
    }
}

/// The runs of tokens that stay where they are, by the position at which
/// each starts in the old document, with the one at which it starts in the
/// new document.
/// They are in the same order in both documents.
struct InPlace(BTreeMap<usize, usize>);

impl InPlace {
    /// The runs between the hunks of a diff.
    fn matched_by(hunks: &[Hunk], keys: &BySide<Vec<&str>>) -> Self {
        let matches = diff::matches(hunks, keys.as_ref().map(Vec::len));
        let starts = matches.iter().map(|run| (run.old.start, run.new.start));
        Self(starts.collect())
    }

    /// Whether a block of tokens that are in none of the runs comes between
    /// the same runs in both documents.
    fn is_in_order(&self, block: &Block) -> bool {
        let before = self.0.range(..block.old.start).next_back();
        let after = self.0.range(block.old.start..).next();
        before.is_none_or(|(_, &new)| new < block.new.start)
            && after.is_none_or(|(_, &new)| block.new.start < new)
    }

    fn add(&mut self, block: &Block) {
        self.0.insert(block.old.start, block.new.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The moves between two texts, whose tokens are separated by spaces.
    fn moves(old: &str, new: &str) -> Vec<Move> {
        let texts = BySide { old, new };
        let keys = texts.map(|text| text.split_whitespace().collect());
        find(&keys, &diff::diff(&keys))
    }

    fn moved(old: Range<usize>, new: Range<usize>) -> Move {
        Move {
            tokens: BySide { old, new },
            inner_hunks: Vec::new(),
        }
    }

    /// Six tokens.
    const PARAGRAPH: &str = "this paragraph stands somewhere else now";
    /// Ten tokens, so more than the paragraph, which the diff therefore
    /// takes for the text that moved around this.
    const REST: &str = "all the other text of the document stays in place";

    #[test]
    fn text_that_is_edited_in_place_did_not_move() {
        assert_eq!(moves("one two three four", "one 2 three four more"), []);
    }

    #[test]
    fn run_that_is_removed_at_one_place_and_added_at_another_moved() {
        let old = format!("{PARAGRAPH} {REST}");
        let new = format!("{REST} {PARAGRAPH}");
        assert_eq!(moves(&old, &new), [moved(0..6, 10..16)]);
    }

    #[test]
    fn few_words_are_not_taken_for_a_move() {
        let old = format!("to be {REST}");
        let new = format!("{REST} to be");
        assert_eq!(moves(&old, &new), []);
    }

    #[test]
    fn of_two_runs_that_swap_places_the_smaller_one_moved() {
        // The diff takes the two for one rewritten passage: each is small
        // next to the other.
        let other = "another paragraph of about that length";
        let old = format!("start {PARAGRAPH} {other} and more end");
        let new = format!("start {other} and more {PARAGRAPH} end");
        assert_eq!(moves(&old, &new), [moved(1..7, 9..15)]);
    }

    #[test]
    fn moved_run_can_differ_a_little_inside() {
        let old = format!("{PARAGRAPH} with one word in its middle {REST} and more of it");
        let new = format!("{REST} and more of it {PARAGRAPH} with another word in its middle");
        assert_eq!(
            moves(&old, &new),
            [Move {
                tokens: BySide {
                    old: 0..12,
                    new: 14..26
                },
                inner_hunks: vec![BySide {
                    old: 7..8,
                    new: 21..22
                }],
            }]
        );
    }

    #[test]
    fn runs_with_much_between_them_moved_one_by_one() {
        let between = "and here come quite a lot of words that only the old text has";
        let second = "the second run that moved as well";
        let rest = format!("{REST} and more of it and still more of it");
        let old = format!("{PARAGRAPH} {between} {second} {rest}");
        let new = format!("{rest} {PARAGRAPH} {second}");
        assert_eq!(
            moves(&old, &new),
            [moved(0..6, 19..25), moved(20..27, 25..32)]
        );
    }
}
