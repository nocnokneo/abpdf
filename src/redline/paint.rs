//! The paints and the paths that the parts of a redline are drawn with.

use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;

use super::Rgb;
use crate::geometry::Rect;

pub(super) const OUTLINE_LINE_WIDTH: f32 = 0.4;

pub(super) fn stroke_rects(
    surface: &mut Surface,
    rects: impl IntoIterator<Item = Rect>,
    color: Rgb,
    line_width: f32,
) {
    let Some(path) = path_of_rects(rects) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(stroke(color, line_width)));
    surface.draw_path(&path);
}

pub(super) fn path_of_rects(rects: impl IntoIterator<Item = Rect>) -> Option<Path> {
    let mut builder = PathBuilder::new();
    for rect in rects {
        let rect = krilla::geom::Rect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom);
        if let Some(rect) = rect {
            builder.push_rect(rect);
        }
    }
    builder.finish()
}

pub(super) fn fill(color: Rgb) -> Fill {
    Fill {
        paint: krilla_color(color).into(),
        opacity: NormalizedF32::ONE,
        rule: Default::default(),
    }
}

pub(super) fn stroke(color: Rgb, line_width: f32) -> Stroke {
    Stroke {
        paint: krilla_color(color).into(),
        width: line_width,
        ..Stroke::default()
    }
}

fn krilla_color(color: Rgb) -> rgb::Color {
    rgb::Color::new(color.red, color.green, color.blue)
}
