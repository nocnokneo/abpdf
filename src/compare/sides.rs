//! What each document has at a place where they differ.

use std::ops::Range;

use super::moves::Move;
use super::sequence::Sequence;
use crate::BySide;
use crate::change::{Area, Change, ChangeSide, Difference, StyleDifference};
use crate::diff::Hunk;
use crate::flow::{Gap, Token};
use crate::geometry::Rect;

/// The change that a hunk of the diff of two sequences is.
pub(super) fn change_at(
    hunk: &Hunk,
    sequences: &BySide<Sequence>,
    page_partners: &BySide<Vec<Option<usize>>>,
) -> Change {
    let tokens = BySide::from_fn(|side| &sequences[side].tokens[hunk[side].clone()]);
    let sides = BySide::from_fn(|side| match tokens[side] {
        [] => {
            // A hunk has tokens on its other side if it has none on this one.
            let text_side = side.other();
            let text_page = tokens[text_side][0].fragments[0].page_index;
            let partner_page = page_partners[text_side][text_page];
            let position = hunk[side].start;
            ChangeSide::Absent {
                insertion_point: insertion_point(&sequences[side], position, partner_page),
            }
        }
        tokens => ChangeSide::Text {
            text: text_of(tokens),
            areas: areas_of(sequences[side].indexed_tokens(hunk[side].clone())),
        },
    });
    Change {
        sides,
        difference: Difference::Text,
    }
}

/// The change that moved text is: the text at its place in each document.
///
/// `sequences` are the whole documents, in which a position is an index of
/// the flow.
pub(super) fn moved(moved: &Move, sequences: &BySide<Sequence>) -> Change {
    let sides = BySide::from_fn(|side| {
        let positions = moved.tokens[side].clone();
        // What differs inside the moved text is a change of its own, and is
        // marked as one.
        let differs = |index: usize| {
            moved
                .inner_hunks
                .iter()
                .any(|hunk| hunk[side].contains(&index))
        };
        let tokens = sequences[side].indexed_tokens(positions.clone());
        ChangeSide::Text {
            text: text_of(&sequences[side].tokens[positions]),
            areas: areas_of(tokens.filter(|(index, _)| !differs(*index))),
        }
    });
    Change {
        sides,
        difference: Difference::Place,
    }
}

/// The change that text in another style is: the text at `positions` of the
/// sequences, which have the same text there.
pub(super) fn restyled(
    positions: &BySide<Range<usize>>,
    sequences: &BySide<Sequence>,
    differences: Vec<StyleDifference>,
) -> Change {
    let sides = BySide::from_fn(|side| {
        let positions = positions[side].clone();
        ChangeSide::Text {
            text: text_of(&sequences[side].tokens[positions.clone()]),
            areas: areas_of(sequences[side].indexed_tokens(positions)),
        }
    });
    Change {
        sides,
        difference: Difference::Style(differences),
    }
}

fn text_of(tokens: &[&Token]) -> String {
    let mut text = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 && token.gap != Gap::None {
            text.push(' ');
        }
        text.push_str(&token.text);
    }
    text
}

/// The bounds of tokens, each given with its index in the flow, merged per
/// line.
/// Tokens that other tokens of the flow stand between are not merged, so
/// that an area does not cover those.
fn areas_of<'a>(tokens: impl IntoIterator<Item = (usize, &'a Token)>) -> Vec<Area> {
    let mut areas: Vec<Area> = Vec::new();
    // The index and the line of the fragment before.
    let mut last = None;
    for (index, token) in tokens {
        for fragment in &token.fragments {
            let line = (fragment.page_index, fragment.line_index);
            let continues_last = last.is_some_and(|(last_index, last_line)| {
                last_line == line && index <= last_index + 1
            });
            match areas.last_mut() {
                Some(area) if continues_last => {
                    area.bounds = area.bounds.union(&fragment.bounds);
                }
                _ => areas.push(Area {
                    page_index: fragment.page_index,
                    bounds: fragment.bounds,
                }),
            }
            last = Some((index, line));
        }
    }
    areas
}

/// Where text would be that came before the token at `position`: at the end
/// of the token before it, or at the start of that token.
///
/// The two are one place in the text, but can be on two pages.
/// Then the place is the one on `preferred_page`, if either is, so that it
/// is shown next to the text that is missing there.
fn insertion_point(
    sequence: &Sequence,
    position: usize,
    preferred_page: Option<usize>,
) -> Option<Area> {
    let previous = position.checked_sub(1).map(|index| sequence.tokens[index]);
    let after_previous = previous.map(|token| {
        let fragment = token.fragments.last().expect("a token has a fragment");
        Area {
            page_index: fragment.page_index,
            bounds: Rect {
                left: fragment.bounds.right,
                ..fragment.bounds
            },
        }
    });
    let before_next = sequence.tokens.get(position).map(|token| {
        let fragment = token.fragments[0];
        Area {
            page_index: fragment.page_index,
            bounds: Rect {
                right: fragment.bounds.left,
                ..fragment.bounds
            },
        }
    });
    let is_preferred = |area: &Area| Some(area.page_index) == preferred_page;
    match (after_previous, before_next) {
        (Some(after_previous), Some(before_next))
            if is_preferred(&before_next) && !is_preferred(&after_previous) =>
        {
            Some(before_next)
        }
        (after_previous, before_next) => after_previous.or(before_next),
    }
}
