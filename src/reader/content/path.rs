//! The paths that a page constructs and paints.

use super::color::Rgb;
use crate::reader::page_space::UserSpaceBox;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::reader) enum SegmentKind {
    Move,
    Line,
    /// A point of a curve: one of its two control points or its end.
    Curve,
}

/// One step of a path: a move, a line or a part of a curve to a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::reader) struct Segment {
    pub kind: SegmentKind,
    /// Whether the step ends a closed outline.
    pub closes_outline: bool,
    /// The point that the step goes to, in the user space of the page.
    pub point: (f64, f64),
}

/// The rule that says what is inside a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::reader) enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::reader) struct Fill {
    pub rule: FillRule,
    /// `None` for a pattern.
    pub color: Option<Rgb>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::reader) struct Stroke {
    /// `None` for a pattern.
    pub color: Option<Rgb>,
    /// The width of the line as the content sets it, in the units of the
    /// space that it is drawn in.
    pub width: f32,
    /// The same width in the user space of the page.
    pub width_on_page: f64,
}

/// A path that a page paints.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::reader) struct Path {
    pub segments: Vec<Segment>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

impl Path {
    /// The points of the outline.
    pub fn points(&self) -> impl Iterator<Item = (f64, f64)> {
        self.segments.iter().map(|segment| segment.point)
    }

    /// The smallest upright box around what the path paints: its outline,
    /// and the line along it if it is stroked.
    /// `None` for a path without points.
    pub fn bounds(&self) -> Option<UserSpaceBox> {
        let outline = UserSpaceBox::bounding(self.points())?;
        let reach = self.stroke.map_or(0.0, |stroke| stroke.width_on_page / 2.0);
        Some(outline.grown_by(reach))
    }
}

/// A path under construction, in the user space of the page.
#[derive(Default)]
pub(super) struct PathBuilder {
    segments: Vec<Segment>,
    /// Where the outline that is being drawn starts.
    outline_start: Option<(f64, f64)>,
    current_point: Option<(f64, f64)>,
}

impl PathBuilder {
    pub fn current_point(&self) -> Option<(f64, f64)> {
        self.current_point
    }

    pub fn move_to(&mut self, point: (f64, f64)) {
        self.outline_start = Some(point);
        self.push(SegmentKind::Move, point);
    }

    /// A line without a point to start from is a move.
    pub fn line_to(&mut self, point: (f64, f64)) {
        match self.current_point {
            Some(_) => self.push(SegmentKind::Line, point),
            None => self.move_to(point),
        }
    }

    pub fn curve_to(&mut self, control_points: [(f64, f64); 2], end: (f64, f64)) {
        if self.current_point.is_none() {
            self.move_to(control_points[0]);
        }
        let [first, second] = control_points;
        self.push(SegmentKind::Curve, first);
        self.push(SegmentKind::Curve, second);
        self.push(SegmentKind::Curve, end);
    }

    /// A rectangle is an outline of its own: a move, three lines and the
    /// way back to where it starts.
    pub fn rectangle(&mut self, corners: [(f64, f64); 4]) {
        let [first, rest @ ..] = corners;
        self.move_to(first);
        for corner in rest {
            self.line_to(corner);
        }
        self.close();
    }

    /// Closes the outline that is being drawn, by going back to its start.
    pub fn close(&mut self) {
        if let Some(last) = self.segments.last_mut() {
            last.closes_outline = true;
            self.current_point = self.outline_start;
        }
    }

    /// The smallest upright box around the points so far.
    pub fn bounds(&self) -> Option<UserSpaceBox> {
        UserSpaceBox::bounding(self.segments.iter().map(|segment| segment.point))
    }

    /// The steps so far, with none of which the next path starts.
    pub fn take(&mut self) -> Vec<Segment> {
        std::mem::take(self).segments
    }

    fn push(&mut self, kind: SegmentKind, point: (f64, f64)) {
        self.current_point = Some(point);
        self.segments.push(Segment {
            kind,
            closes_outline: false,
            point,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(segments: &[Segment]) -> Vec<SegmentKind> {
        segments.iter().map(|segment| segment.kind).collect()
    }

    #[test]
    fn rectangle_is_a_closed_outline_of_four_points() {
        let mut builder = PathBuilder::default();
        builder.rectangle([(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)]);
        let segments = builder.take();
        assert_eq!(
            kinds(&segments),
            [
                SegmentKind::Move,
                SegmentKind::Line,
                SegmentKind::Line,
                SegmentKind::Line
            ]
        );
        let closing: Vec<bool> = segments
            .iter()
            .map(|segment| segment.closes_outline)
            .collect();
        assert_eq!(closing, [false, false, false, true]);
    }

    #[test]
    fn closing_goes_back_to_the_start_of_the_outline() {
        let mut builder = PathBuilder::default();
        builder.move_to((1.0, 1.0));
        builder.line_to((5.0, 1.0));
        builder.close();
        assert_eq!(builder.current_point(), Some((1.0, 1.0)));
    }

    #[test]
    fn curve_is_three_points() {
        let mut builder = PathBuilder::default();
        builder.move_to((0.0, 0.0));
        builder.curve_to([(1.0, 2.0), (3.0, 2.0)], (4.0, 0.0));
        assert_eq!(builder.current_point(), Some((4.0, 0.0)));
        assert_eq!(kinds(&builder.take())[1..], [SegmentKind::Curve; 3]);
    }

    #[test]
    fn line_without_a_start_is_a_move() {
        let mut builder = PathBuilder::default();
        builder.line_to((2.0, 2.0));
        assert_eq!(kinds(&builder.take()), [SegmentKind::Move]);
    }

    #[test]
    fn taking_the_steps_starts_a_new_path() {
        let mut builder = PathBuilder::default();
        builder.move_to((2.0, 2.0));
        assert_eq!(builder.take().len(), 1);
        assert_eq!(builder.current_point(), None);
        assert!(builder.take().is_empty());
    }

    #[test]
    fn bounds_of_a_stroked_path_reach_half_the_line_width_further() {
        let mut builder = PathBuilder::default();
        builder.move_to((10.0, 10.0));
        builder.line_to((20.0, 10.0));
        let mut path = Path {
            segments: builder.take(),
            fill: None,
            stroke: None,
        };
        assert_eq!(path.bounds().unwrap().bottom, 10.0);
        path.stroke = Some(Stroke {
            color: None,
            width: 1.0,
            width_on_page: 2.0,
        });
        let bounds = path.bounds().unwrap();
        assert_eq!((bounds.bottom, bounds.top), (9.0, 11.0));
        assert_eq!((bounds.left, bounds.right), (9.0, 21.0));
    }
}
