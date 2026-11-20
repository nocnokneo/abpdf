//! The bands across the gutter that join the two sides of a change.

use krilla::geom::{Path, PathBuilder};
use krilla::num::NormalizedF32;
use krilla::paint::Fill;
use krilla::surface::Surface;

use super::paint::{fill, stroke};
use super::text::TextFont;
use super::{Connector, Rgb, SheetLayout};
use crate::Side;
use crate::geometry::Rect;

const COLOR: Rgb = Rgb::new(140, 89, 204);
const OPACITY: f32 = 0.22;
const LINE_WIDTH: f32 = 0.6;
/// The space between a band and the frame that it starts at.
const INSET: f32 = 3.0;
/// How far a band reaches into the gutter from a side whose other side the
/// sheet does not show.
const STUB_LENGTH: f32 = 12.0;

const LABEL_FONT_SIZE: f32 = 7.0;
const LABEL_COLOR: Rgb = Rgb::new(51, 51, 51);
/// How far along a band its label can be, as shares of the way from its old
/// end to its new end, in the order of preference.
const LABEL_SHARES: [f32; 5] = [0.5, 0.35, 0.65, 0.2, 0.8];
/// How many places the label of a stub can be at, one after the other from
/// its point into the gutter.
const STUB_LABEL_PLACES: usize = 3;

/// Draws the connectors of a sheet: for each a band with its label.
///
/// A label goes where it covers no label of a connector before it, if one
/// of the places that a label can be at is free.
pub(super) fn draw_all(
    surface: &mut Surface,
    font: &TextFont,
    layout: &SheetLayout,
    connectors: &[Connector],
) {
    let mut placed_labels: Vec<Rect> = Vec::new();
    for connector in connectors {
        let Some(band) = Band::of(connector, layout) else {
            continue;
        };
        band.draw(surface);

        let label_width = font.width(&connector.label, LABEL_FONT_SIZE);
        let places = band.label_places(label_width);
        let is_free = |place: &&Rect| !placed_labels.iter().any(|label| label.intersects(place));
        let place = *places.iter().find(is_free).unwrap_or(&places[0]);
        let baseline = (place.top + place.bottom) / 2.0 + LABEL_FONT_SIZE / 3.0;
        font.draw(
            surface,
            (place.left, baseline),
            LABEL_FONT_SIZE,
            LABEL_COLOR,
            &connector.label,
        );
        placed_labels.push(place);
    }
}

/// One end of a band: a vertical extent at a position across the sheet.
#[derive(Clone, Copy)]
struct End {
    x: f32,
    top: f32,
    bottom: f32,
}

impl End {
    fn center(&self) -> f32 {
        (self.top + self.bottom) / 2.0
    }

    /// An end of no height, level with the middle of this one.
    fn point_at(&self, x: f32) -> Self {
        Self {
            x,
            top: self.center(),
            bottom: self.center(),
        }
    }
}

/// The shape of a connector.
///
/// It runs from the old side's extent at the left of the gutter to the new
/// side's extent at its right.
/// Where the sheet shows one side only, it is a stub that comes to a point
/// in the gutter.
struct Band {
    old_end: End,
    new_end: End,
    /// The side that the band is a stub of, if it is one.
    stub_of: Option<Side>,
}

impl Band {
    fn of(connector: &Connector, layout: &SheetLayout) -> Option<Self> {
        let (gutter_left, gutter_right) = layout.gutter();
        let end_at = |side: Side, x: f32| {
            let span = connector.spans[side]?;
            let frame_top = layout.frames[side].top;
            Some(End {
                x,
                top: frame_top + span.top,
                bottom: frame_top + span.bottom,
            })
        };
        let old_end = end_at(Side::Old, gutter_left + INSET);
        let new_end = end_at(Side::New, gutter_right - INSET);
        match (old_end, new_end) {
            (Some(old_end), Some(new_end)) => Some(Self {
                old_end,
                new_end,
                stub_of: None,
            }),
            (Some(old_end), None) => Some(Self {
                old_end,
                new_end: old_end.point_at(old_end.x + STUB_LENGTH),
                stub_of: Some(Side::Old),
            }),
            (None, Some(new_end)) => Some(Self {
                old_end: new_end.point_at(new_end.x - STUB_LENGTH),
                new_end,
                stub_of: Some(Side::New),
            }),
            (None, None) => None,
        }
    }

    /// The position across the sheet at which the band bends from the height
    /// of one end to that of the other.
    fn bend_x(&self) -> f32 {
        (self.old_end.x + self.new_end.x) / 2.0
    }

    fn path(&self) -> Option<Path> {
        let (old, new, bend_x) = (self.old_end, self.new_end, self.bend_x());
        let mut builder = PathBuilder::new();
        builder.move_to(old.x, old.top);
        builder.cubic_to(bend_x, old.top, bend_x, new.top, new.x, new.top);
        builder.line_to(new.x, new.bottom);
        builder.cubic_to(bend_x, new.bottom, bend_x, old.bottom, old.x, old.bottom);
        builder.close();
        builder.finish()
    }

    fn draw(&self, surface: &mut Surface) {
        let Some(path) = self.path() else {
            return;
        };
        surface.set_fill(Some(Fill {
            opacity: NormalizedF32::new(OPACITY).expect("an opacity between 0 and 1"),
            ..fill(COLOR)
        }));
        surface.set_stroke(Some(stroke(COLOR, LINE_WIDTH)));
        surface.draw_path(&path);
    }

    /// Where a label of this width can be, in the order of preference: on
    /// the band, or after the point of a stub.
    /// The labels of stubs from the left and from the right so keep out of
    /// each other's way.
    fn label_places(&self, label_width: f32) -> Vec<Rect> {
        let label_at = |left: f32, center: f32| Rect {
            left,
            top: center - LABEL_FONT_SIZE / 2.0,
            right: left + label_width,
            bottom: center + LABEL_FONT_SIZE / 2.0,
        };
        // How far the places after the point of a stub are from each other.
        let stub_steps = (0..STUB_LABEL_PLACES).map(|place| place as f32 * (label_width + INSET));
        match self.stub_of {
            Some(Side::Old) => {
                let point = self.new_end;
                let first_left = point.x + INSET;
                stub_steps
                    .map(|step| label_at(first_left + step, point.center()))
                    .collect()
            }
            Some(Side::New) => {
                let point = self.old_end;
                let first_left = point.x - INSET - label_width;
                stub_steps
                    .map(|step| label_at(first_left - step, point.center()))
                    .collect()
            }
            None => LABEL_SHARES
                .iter()
                .map(|&share| {
                    let (x, y) = self.middle_at(share);
                    label_at(x - label_width / 2.0, y)
                })
                .collect(),
        }
    }

    /// The point in the middle of the band at a share of the way from its
    /// old end to its new end.
    fn middle_at(&self, share: f32) -> (f32, f32) {
        let (old, new, bend_x) = (self.old_end, self.new_end, self.bend_x());
        (
            cubic_bezier([old.x, bend_x, bend_x, new.x], share),
            cubic_bezier(
                [old.center(), old.center(), new.center(), new.center()],
                share,
            ),
        )
    }
}

/// The value at `share` of the way along a cubic Bézier curve through one
/// coordinate of its four control points.
fn cubic_bezier(points: [f32; 4], share: f32) -> f32 {
    let rest = 1.0 - share;
    let weights = [
        rest * rest * rest,
        3.0 * rest * rest * share,
        3.0 * rest * share * share,
        share * share * share,
    ];
    points
        .iter()
        .zip(weights)
        .map(|(point, weight)| point * weight)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BySide;
    use crate::geometry::Size;
    use crate::redline::Span;

    fn layout() -> SheetLayout {
        let page = Some(Size {
            width: 200.0,
            height: 300.0,
        });
        SheetLayout::new(BySide {
            old: page,
            new: page,
        })
    }

    fn band(old: Option<Span>, new: Option<Span>) -> Band {
        let connector = Connector {
            label: "1".to_owned(),
            spans: BySide { old, new },
        };
        Band::of(&connector, &layout()).expect("a side has an extent")
    }

    const HIGH: Span = Span {
        top: 10.0,
        bottom: 20.0,
    };
    const LOW: Span = Span {
        top: 200.0,
        bottom: 220.0,
    };

    #[test]
    fn label_is_in_the_middle_of_a_band_first() {
        let band = band(Some(HIGH), Some(LOW));
        let (gutter_left, gutter_right) = layout().gutter();
        let frame_top = layout().frames.old.top;
        let place = band.label_places(8.0)[0];
        assert_eq!(
            (place.left + place.right) / 2.0,
            (gutter_left + gutter_right) / 2.0
        );
        // Halfway between the middles of the two extents.
        assert_eq!((place.top + place.bottom) / 2.0, frame_top + 112.5);
    }

    #[test]
    fn other_places_for_a_label_are_along_the_band() {
        let band = band(Some(HIGH), Some(LOW));
        let places = band.label_places(8.0);
        assert_eq!(places.len(), LABEL_SHARES.len());
        // Nearer to the old end the band is nearer to the old extent.
        assert!(places[1].left < places[0].left && places[1].top < places[0].top);
        assert!(places[2].left > places[0].left && places[2].top > places[0].top);
        assert!(!places[0].intersects(&places[3]));
    }

    #[test]
    fn label_of_a_stub_is_after_its_point_on_its_own_half_of_the_gutter() {
        let (gutter_left, gutter_right) = layout().gutter();
        let middle = (gutter_left + gutter_right) / 2.0;
        let from_old = band(Some(HIGH), None).label_places(8.0);
        let from_new = band(None, Some(HIGH)).label_places(8.0);
        assert!(gutter_left < from_old[0].left && from_old[0].right < middle);
        assert!(middle < from_new[0].left && from_new[0].right < gutter_right);
        assert_eq!(from_old[0].top, from_new[0].top);
    }

    #[test]
    fn other_places_for_the_label_of_a_stub_are_further_into_the_gutter() {
        let from_old = band(Some(HIGH), None).label_places(8.0);
        let from_new = band(None, Some(HIGH)).label_places(8.0);
        assert_eq!(from_old.len(), STUB_LABEL_PLACES);
        assert!(from_old[0].right < from_old[1].left);
        assert!(from_new[1].right < from_new[0].left);
    }

    #[test]
    fn connector_without_extents_has_no_band() {
        let connector = Connector {
            label: "1".to_owned(),
            spans: BySide {
                old: None,
                new: None,
            },
        };
        assert!(Band::of(&connector, &layout()).is_none());
    }
}
