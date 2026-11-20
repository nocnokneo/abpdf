//! Figures as tokens of a flow.

use crate::reader::{self, FigureKind};

/// The text of a token that is a figure: the character that stands for an
/// object in a text.
pub const FIGURE_TEXT: &str = "\u{FFFC}";

/// How far the points of two figures can be from each other for the figures
/// to look the same, in units of page space.
/// A producer rounds the positions that it writes and a reader computes
/// with them, so that they come out a little differently for a figure that
/// stands at another place on its page.
const POINT_TOLERANCE: f32 = 0.05;

/// What a token shows that is a figure and not text.
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    pub kind: FigureKind,
    /// What the figure is drawn with: see [`reader::Figure::content_hash`].
    pub content_hash: u64,
    /// The points that the figure is drawn through, each as how far it is
    /// from the first of them.
    offsets: Vec<(f32, f32)>,
}

impl Figure {
    pub fn of(figure: &reader::Figure) -> Self {
        let (first_x, first_y) = figure.points.first().copied().unwrap_or_default();
        let offsets = figure
            .points
            .iter()
            .map(|(x, y)| (x - first_x, y - first_y));
        Self {
            kind: figure.kind,
            content_hash: figure.content_hash,
            offsets: offsets.collect(),
        }
    }

    /// Whether the two figures look the same, wherever on their pages they
    /// are: they are drawn with the same things, through points that lie
    /// the same way to each other.
    pub fn looks_like(&self, other: &Figure) -> bool {
        let is_close =
            |length: f32, other_length: f32| (length - other_length).abs() <= POINT_TOLERANCE;
        let offsets = self.offsets.iter().zip(&other.offsets);
        self.content_hash == other.content_hash
            && self.offsets.len() == other.offsets.len()
            && offsets.into_iter().all(|(offset, other_offset)| {
                is_close(offset.0, other_offset.0) && is_close(offset.1, other_offset.1)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    const SQUARE: [(f32, f32); 4] = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];

    /// A figure that is drawn through these points.
    fn figure(points: &[(f32, f32)]) -> Figure {
        Figure::of(&reader::Figure {
            bounds: Rect::bounding(points.iter().copied()).expect("a figure has points"),
            kind: FigureKind::Drawing { part_count: 1 },
            is_artifact: false,
            content_hash: 7,
            points: points.to_vec(),
        })
    }

    fn moved(points: &[(f32, f32)], by: (f32, f32)) -> Vec<(f32, f32)> {
        let points = points.iter();
        points.map(|(x, y)| (x + by.0, y + by.1)).collect()
    }

    #[test]
    fn figure_looks_the_same_at_another_place() {
        let elsewhere = moved(&SQUARE, (123.4, 56.7));
        assert!(figure(&SQUARE).looks_like(&figure(&elsewhere)));
    }

    #[test]
    fn figure_with_a_point_at_another_place_looks_different() {
        let mut dented = SQUARE;
        dented[2].1 += 0.5;
        assert!(!figure(&SQUARE).looks_like(&figure(&dented)));
    }

    #[test]
    fn points_that_are_all_but_at_the_same_place_do_not_differ() {
        let mut all_but_square = SQUARE;
        all_but_square[2].1 += 0.01;
        assert!(figure(&SQUARE).looks_like(&figure(&all_but_square)));
    }

    #[test]
    fn figure_with_more_points_looks_different() {
        let mut with_one_more = SQUARE.to_vec();
        with_one_more.push((0.0, 0.0));
        assert!(!figure(&SQUARE).looks_like(&figure(&with_one_more)));
    }

    #[test]
    fn figure_with_other_content_looks_different() {
        let mut other = figure(&SQUARE);
        other.content_hash += 1;
        assert!(!figure(&SQUARE).looks_like(&other));
    }
}
