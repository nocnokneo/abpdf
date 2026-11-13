//! Drawing a [`Redline`] with krilla.

use krilla::blend::BlendMode;
use krilla::color::rgb;
use krilla::configure::{ConfigurationBuilder, PdfVersion};
use krilla::error::KrillaError;
use krilla::geom::{PathBuilder, Point, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use krilla::{Document, SerializeSettings};

use super::{Mark, MarkStyle, Redline, Rgb, Sheet, SheetLayout, SheetPage, Source, Sources};
use crate::Side;
use crate::geometry::Rect;

static LABEL_FONT: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");

const LABEL_FONT_SIZE: f32 = 9.0;
const LABEL_COLOR: Rgb = Rgb::new(51, 51, 51);
const FRAME_COLOR: Rgb = Rgb::new(153, 153, 153);
const FRAME_LINE_WIDTH: f32 = 0.5;
const OUTLINE_LINE_WIDTH: f32 = 0.4;

/// How much of a mark's color a highlight keeps: the rest is white, so that
/// the text under it stays readable.
const HIGHLIGHT_STRENGTH: f32 = 0.3;

pub(super) fn render(
    redline: &Redline,
    sources: &Sources,
    version: PdfVersion,
) -> Result<Vec<u8>, KrillaError> {
    let configuration = ConfigurationBuilder::new()
        .with_version(version)
        .finish()
        .expect("every version is valid without a validator");
    let mut document = Document::new_with(SerializeSettings {
        configuration,
        ..SerializeSettings::default()
    });
    let label_font = Font::new(LABEL_FONT.into(), 0).expect("the bundled font is valid");
    for sheet in &redline.sheets {
        draw_sheet(&mut document, sources, &label_font, sheet);
    }
    document.finish()
}

fn draw_sheet(document: &mut Document, sources: &Sources, label_font: &Font, sheet: &Sheet) {
    let page_size = |side| {
        let page = sheet.page(side)?;
        sources.get(side).page_size(page.page_index)
    };
    let layout = SheetLayout::new(page_size(Side::Old), page_size(Side::New));
    let settings = PageSettings::from_wh(layout.size.width, layout.size.height)
        .expect("a sheet is larger than its margins");
    let mut page = document.start_page_with(settings);
    let mut surface = page.surface();

    for side in Side::BOTH {
        let frame = layout.frame(side);
        if let Some(sheet_page) = sheet.page(side) {
            embed_page(&mut surface, sources.get(side), sheet_page, frame);
            draw_marks(&mut surface, &layout, side, &sheet_page.marks);
            draw_label(
                &mut surface,
                label_font,
                &sheet_page.label,
                frame.left,
                &layout,
            );
        }
        stroke_rects(&mut surface, [frame], FRAME_COLOR, FRAME_LINE_WIDTH);
    }

    surface.finish();
    page.finish();
}

fn embed_page(surface: &mut Surface, source: &Source, sheet_page: &SheetPage, frame: Rect) {
    let Some(size) = krilla::geom::Size::from_wh(frame.width(), frame.height()) else {
        return;
    };
    surface.push_transform(&Transform::from_translate(frame.left, frame.top));
    surface.draw_pdf_page(&source.embeddable, size, sheet_page.page_index);
    surface.pop();
}

/// Draws the marks of one page, those that look alike in one path each.
fn draw_marks(surface: &mut Surface, layout: &SheetLayout, side: Side, marks: &[Mark]) {
    let mut looks: Vec<(Rgb, MarkStyle)> = Vec::new();
    for mark in marks {
        if !looks.contains(&(mark.color, mark.style)) {
            looks.push((mark.color, mark.style));
        }
    }
    for (color, style) in looks {
        let rects = marks
            .iter()
            .filter(|mark| (mark.color, mark.style) == (color, style))
            .map(|mark| layout.place(side, mark.bounds));
        match style {
            MarkStyle::Highlight => highlight_rects(surface, rects, color),
            MarkStyle::Outline => stroke_rects(surface, rects, color, OUTLINE_LINE_WIDTH),
        }
    }
}

fn highlight_rects(surface: &mut Surface, rects: impl IntoIterator<Item = Rect>, color: Rgb) {
    let Some(path) = path_of(rects) else {
        return;
    };
    surface.push_blend_mode(BlendMode::Multiply);
    surface.set_fill(Some(fill(toward_white(color, HIGHLIGHT_STRENGTH))));
    surface.set_stroke(None);
    surface.draw_path(&path);
    surface.pop();
}

fn stroke_rects(
    surface: &mut Surface,
    rects: impl IntoIterator<Item = Rect>,
    color: Rgb,
    line_width: f32,
) {
    let Some(path) = path_of(rects) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(Stroke {
        paint: krilla_color(color).into(),
        width: line_width,
        ..Stroke::default()
    }));
    surface.draw_path(&path);
}

fn draw_label(surface: &mut Surface, font: &Font, text: &str, left: f32, layout: &SheetLayout) {
    surface.set_fill(Some(fill(LABEL_COLOR)));
    surface.set_stroke(None);
    surface.draw_text(
        Point::from_xy(left, layout.label_baseline()),
        font.clone(),
        LABEL_FONT_SIZE,
        text,
        false,
        TextDirection::Auto,
    );
}

fn path_of(rects: impl IntoIterator<Item = Rect>) -> Option<krilla::geom::Path> {
    let mut builder = PathBuilder::new();
    for rect in rects {
        let rect = krilla::geom::Rect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom);
        if let Some(rect) = rect {
            builder.push_rect(rect);
        }
    }
    builder.finish()
}

fn fill(color: Rgb) -> Fill {
    Fill {
        paint: krilla_color(color).into(),
        opacity: NormalizedF32::ONE,
        rule: Default::default(),
    }
}

fn krilla_color(color: Rgb) -> rgb::Color {
    rgb::Color::new(color.red, color.green, color.blue)
}

/// Mixes `strength` of the color with white.
fn toward_white(color: Rgb, strength: f32) -> Rgb {
    let mix = |channel: u8| {
        let white = f32::from(u8::MAX);
        (white - (white - f32::from(channel)) * strength).round() as u8
    };
    Rgb::new(mix(color.red), mix(color.green), mix(color.blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toward_white_keeps_white_and_lightens_black() {
        assert_eq!(
            toward_white(Rgb::new(255, 255, 255), 0.3),
            Rgb::new(255, 255, 255)
        );
        assert_eq!(
            toward_white(Rgb::new(0, 0, 0), 0.5),
            Rgb::new(128, 128, 128)
        );
    }
}
