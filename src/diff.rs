//! Finding where two sequences of tokens differ.

use std::iter;
use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput};

use crate::{BySide, Side};

/// A place where the sequences differ: the old range is replaced by the new
/// one.
/// Either range can be empty, but not both.
pub type Hunk = BySide<Range<usize>>;

/// The hunks between two sequences of keys, in order.
///
/// Tokens are matched by the histogram algorithm, which anchors on rare
/// tokens.
/// A match that is small next to the hunks around it is then taken to be a
/// coincidence, such as a comma or `the` inside a rewritten passage, and
/// becomes part of one hunk with them.
pub fn diff(keys: &BySide<Vec<&str>>) -> Vec<Hunk> {
    let mut input = InternedInput::default();
    input.update_before(keys.old.iter().copied());
    input.update_after(keys.new.iter().copied());
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_no_heuristic(&input);

    let matched: Vec<Hunk> = diff
        .hunks()
        .map(|hunk| Hunk {
            old: hunk.before.start as usize..hunk.before.end as usize,
            new: hunk.after.start as usize..hunk.after.end as usize,
        })
        .collect();
    merge_across_small_matches(matched, &Sizes::measure(keys))
}

/// A run of tokens that the sequences have in common: equally many of each,
/// which match one to one.
pub type Match = BySide<Range<usize>>;

/// The runs that `hunks` leave between them in the ranges `within` of two
/// sequences, in order.
pub(crate) fn matches(hunks: &[Hunk], within: &BySide<Range<usize>>) -> Vec<Match> {
    let end = within.as_ref().map(|range| range.end..range.end);
    let mut start = within.as_ref().map(|range| range.start);
    let mut matches = Vec::new();
    for hunk in hunks.iter().chain(iter::once(&end)) {
        if hunk.old.start > start.old {
            matches.push(BySide::from_fn(|side| start[side]..hunk[side].start));
        }
        start = hunk.as_ref().map(|range| range.end);
    }
    matches
}

/// Merges neighboring hunks if the match between them is small next to them.
fn merge_across_small_matches(hunks: Vec<Hunk>, sizes: &Sizes) -> Vec<Hunk> {
    let mut merged: Vec<Hunk> = Vec::with_capacity(hunks.len());
    for mut hunk in hunks {
        // Merging makes the hunk larger, which can make the match before the
        // merged hunk small in turn.
        while let Some(previous) = merged.last() {
            let matched = BySide::from_fn(|side| previous[side].end..hunk[side].start);
            if !sizes.is_small_between(&matched, previous, &hunk) {
                break;
            }
            hunk = Hunk {
                old: previous.old.start..hunk.old.end,
                new: previous.new.start..hunk.new.end,
            };
            merged.pop();
        }
        merged.push(hunk);
    }
    merged
}

/// The sizes of runs of tokens of the two documents, in characters.
pub(crate) struct Sizes {
    /// The size of all keys of each document before each index.
    before: BySide<Vec<usize>>,
}

impl Sizes {
    pub(crate) fn measure(keys: &BySide<Vec<&str>>) -> Self {
        let before = keys.as_ref().map(|keys| {
            let mut before = Vec::with_capacity(keys.len() + 1);
            let mut total = 0;
            before.push(total);
            for key in keys {
                total += key.chars().count();
                before.push(total);
            }
            before
        });
        Self { before }
    }

    /// The size of text that has these tokens in the two documents, which is
    /// that of its larger side.
    pub(crate) fn of(&self, tokens: &BySide<Range<usize>>) -> usize {
        let of_side = |side: Side| {
            let before = &self.before[side];
            before[tokens[side].end] - before[tokens[side].start]
        };
        of_side(Side::Old).max(of_side(Side::New))
    }

    /// Whether what stands between two runs is little next to them: no
    /// larger than either.
    pub(crate) fn is_small_between(
        &self,
        between: &BySide<Range<usize>>,
        first: &BySide<Range<usize>>,
        second: &BySide<Range<usize>>,
    ) -> bool {
        let size = self.of(between);
        size <= self.of(first) && size <= self.of(second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hunks between two texts, whose tokens are separated by spaces.
    fn hunks(old: &str, new: &str) -> Vec<Hunk> {
        let texts = BySide { old, new };
        diff(&texts.map(|text| text.split_whitespace().collect()))
    }

    fn hunk(old: Range<usize>, new: Range<usize>) -> Hunk {
        Hunk { old, new }
    }

    #[test]
    fn matches_are_what_the_hunks_leave_between_them() {
        let hunks = [hunk(0..1, 0..0), hunk(3..4, 2..5)];
        let within = BySide {
            old: 0..6,
            new: 0..7,
        };
        assert_eq!(
            matches(&hunks, &within),
            [hunk(1..3, 0..2), hunk(4..6, 5..7)]
        );
        assert_eq!(matches(&[], &within.map(|_| 2..4)), [hunk(2..4, 2..4)]);
    }

    #[test]
    fn equal_sequences_have_no_hunks() {
        assert_eq!(hunks("one two three", "one two three"), []);
        assert_eq!(hunks("", ""), []);
    }

    #[test]
    fn insertion_has_an_empty_old_side() {
        assert_eq!(hunks("one three", "one two three"), [hunk(1..1, 1..2)]);
    }

    #[test]
    fn removal_has_an_empty_new_side() {
        assert_eq!(hunks("one two three", "one three"), [hunk(1..2, 1..1)]);
    }

    #[test]
    fn replacement_has_both_sides() {
        assert_eq!(hunks("one two three", "one 2 three"), [hunk(1..2, 1..2)]);
    }

    #[test]
    fn everything_differs_from_nothing() {
        assert_eq!(hunks("", "one two"), [hunk(0..0, 0..2)]);
        assert_eq!(hunks("one two", ""), [hunk(0..2, 0..0)]);
    }

    #[test]
    fn what_follows_an_insertion_still_matches() {
        let old = "alpha beta gamma delta epsilon";
        let new = "alpha new words here beta gamma delta epsilon";
        assert_eq!(hunks(old, new), [hunk(1..1, 1..4)]);
    }

    #[test]
    fn separate_changes_are_separate_hunks() {
        let old = "the device shall be calibrated by trained staff";
        let new = "the unit shall be calibrated by skilled staff";
        assert_eq!(hunks(old, new), [hunk(1..2, 1..2), hunk(6..7, 6..7)]);
    }

    #[test]
    fn small_match_inside_a_rewritten_passage_is_part_of_the_change() {
        let old = "before calibrated annually , under supervision after";
        let new = "before verified monthly , without oversight after";
        assert_eq!(hunks(old, new), [hunk(1..6, 1..6)]);
    }

    #[test]
    fn match_larger_than_a_change_next_to_it_is_kept() {
        let old = "a , then wait .";
        let new = "a then wait :";
        assert_eq!(hunks(old, new), [hunk(1..2, 1..1), hunk(4..5, 3..4)]);
    }

    #[test]
    fn merging_goes_on_while_matches_are_small() {
        // After the last two hunks merge across `c`, the match `b` is small
        // next to them.
        let old = "start aaa b ccccc c ddddd end";
        let new = "start xxx b yyyyy c zzzzz end";
        assert_eq!(hunks(old, new), [hunk(1..6, 1..6)]);
    }
}
