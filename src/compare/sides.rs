//! What each document has at a place where they differ.

use super::sequence::Sequence;
use crate::BySide;
use crate::change::{Area, Change, ChangeSide};
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
            areas: areas_of(tokens),
        },
    });
    Change { sides }
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

/// The bounds of the tokens, merged per line.
fn areas_of(tokens: &[&Token]) -> Vec<Area> {
    let mut areas: Vec<Area> = Vec::new();
    let mut last_line = None;
    for fragment in tokens.iter().flat_map(|token| &token.fragments) {
        let line = (fragment.page_index, fragment.line_index);
        match areas.last_mut() {
            Some(area) if last_line == Some(line) => {
                area.bounds = area.bounds.union(&fragment.bounds);
            }
            _ => areas.push(Area {
                page_index: fragment.page_index,
                bounds: fragment.bounds,
            }),
        }
        last_line = Some(line);
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
