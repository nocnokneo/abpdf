//! Finding where two sequences of tokens differ.

use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput};

/// A place where the sequences differ: `old` is replaced by `new`.
/// Either range can be empty, but not both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// The hunks between two sequences of keys, in order.
///
/// Tokens are matched by the histogram algorithm, which anchors on rare
/// tokens.
/// A match that is small next to the hunks around it is then taken to be a
/// coincidence, such as a comma or `the` inside a rewritten passage, and
/// becomes part of one hunk with them.
pub fn diff(old: &[&str], new: &[&str]) -> Vec<Hunk> {
    let mut input = InternedInput::default();
    input.update_before(old.iter().copied());
    input.update_after(new.iter().copied());
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_no_heuristic(&input);

    let matched: Vec<Hunk> = diff
        .hunks()
        .map(|hunk| Hunk {
            old: hunk.before.start as usize..hunk.before.end as usize,
            new: hunk.after.start as usize..hunk.after.end as usize,
        })
        .collect();
    merge_across_small_matches(matched, &Sizes::measure(old), &Sizes::measure(new))
}

/// Merges neighboring hunks if the match between them is no larger than
/// either of them, where the size of a hunk is that of its larger side.
fn merge_across_small_matches(hunks: Vec<Hunk>, old: &Sizes, new: &Sizes) -> Vec<Hunk> {
    let size_of = |hunk: &Hunk| old.of(&hunk.old).max(new.of(&hunk.new));
    let mut merged: Vec<Hunk> = Vec::with_capacity(hunks.len());
    for mut hunk in hunks {
        // Merging makes the hunk larger, which can make the match before the
        // merged hunk small in turn.
        while let Some(previous) = merged.last() {
            let match_size = old.of(&(previous.old.end..hunk.old.start));
            if match_size > size_of(previous) || match_size > size_of(&hunk) {
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

/// The sizes of runs of keys, in characters.
struct Sizes {
    /// The size of all keys before each index.
    before: Vec<usize>,
}

impl Sizes {
    fn measure(keys: &[&str]) -> Self {
        let mut before = Vec::with_capacity(keys.len() + 1);
        let mut total = 0;
        before.push(total);
        for key in keys {
            total += key.chars().count();
            before.push(total);
        }
        Self { before }
    }

    fn of(&self, range: &Range<usize>) -> usize {
        self.before[range.end] - self.before[range.start]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hunks between two texts, whose tokens are separated by spaces.
    fn hunks(old: &str, new: &str) -> Vec<Hunk> {
        let old: Vec<&str> = old.split_whitespace().collect();
        let new: Vec<&str> = new.split_whitespace().collect();
        diff(&old, &new)
    }

    fn hunk(old: Range<usize>, new: Range<usize>) -> Hunk {
        Hunk { old, new }
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
