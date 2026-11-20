//! Text that both documents have, in different styles.

use std::ops::Range;

use super::sequence::Sequence;
use super::sides;
use crate::BySide;
use crate::change::{Change, StyleDifference};
use crate::diff::Match;
use crate::flow::Token;
use crate::reader::Style;

/// Font sizes that differ by no more than this many points are the same:
/// a reader computes the size, and no eye tells them apart.
const SIZE_TOLERANCE: f32 = 0.05;

/// Rotations that differ by no more than this many degrees are the same.
const ROTATION_TOLERANCE: f32 = 0.5;

/// The changes of style in a run of tokens that the documents have in
/// common, each with the positions of its tokens.
///
/// Tokens that follow each other and differ in the same way are one change.
pub(super) fn restyled(
    matched: &Match,
    sequences: &BySide<Sequence>,
) -> Vec<(BySide<Range<usize>>, Change)> {
    let pairs = matched.old.clone().zip(matched.new.clone());
    let differences: Vec<Vec<StyleDifference>> = pairs
        .map(|(old, new)| differences_at(BySide { old, new }, sequences))
        .collect();

    let mut changes = Vec::new();
    let mut offset = 0;
    for run in differences.chunk_by(|differences, next| differences == next) {
        if !run[0].is_empty() {
            let positions = BySide::from_fn(|side| {
                let start = matched[side].start + offset;
                start..start + run.len()
            });
            let change = sides::restyled(&positions, sequences, run[0].clone());
            changes.push((positions, change));
        }
        offset += run.len();
    }
    changes
}

/// What differs between the styles of the tokens at a position of each
/// sequence, which have the same text.
///
/// The runs of characters in one style are compared in the order of the
/// text.
/// Where one token has fewer of them than the other, its last one is
/// compared with the rest of the other's.
fn differences_at(positions: BySide<usize>, sequences: &BySide<Sequence>) -> Vec<StyleDifference> {
    let tokens = BySide::from_fn(|side| sequences[side].tokens[positions[side]]);
    // PROTOTYPE: a graphic has no style.
    if tokens.old.styles.is_empty() || tokens.new.styles.is_empty() {
        return Vec::new();
    }
    let style = |side, run: usize| {
        let token: &Token = tokens[side];
        let run = run.min(token.styles.len() - 1);
        &sequences[side].flow.styles[token.styles[run]]
    };
    let run_count = tokens.old.styles.len().max(tokens.new.styles.len());
    let mut differences = Vec::new();
    for run in 0..run_count {
        let styles = BySide::from_fn(|side| style(side, run));
        for difference in differences_between(styles) {
            if !differences.contains(&difference) {
                differences.push(difference);
            }
        }
    }
    differences
}

fn differences_between(styles: BySide<&Style>) -> Vec<StyleDifference> {
    let BySide { old, new } = styles;
    let mut differences = Vec::new();
    if (old.font_size - new.font_size).abs() > SIZE_TOLERANCE {
        differences.push(StyleDifference::Size(styles.map(|style| style.font_size)));
    }
    if old.font_name != new.font_name {
        // The name of a font also tells its weight and its slant.
        differences.push(StyleDifference::Font(
            styles.map(|style| style.font_name.clone()),
        ));
    } else {
        if old.font_weight != new.font_weight {
            differences.push(StyleDifference::Weight(
                styles.map(|style| style.font_weight),
            ));
        }
        if old.is_italic != new.is_italic {
            differences.push(StyleDifference::Italic(styles.map(|style| style.is_italic)));
        }
    }
    if old.fill_color != new.fill_color {
        differences.push(StyleDifference::Color(styles.map(|style| style.fill_color)));
    }
    if (old.rotation_degrees - new.rotation_degrees).abs() > ROTATION_TOLERANCE {
        differences.push(StyleDifference::Rotation(
            styles.map(|style| style.rotation_degrees),
        ));
    }
    differences
}
