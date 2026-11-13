//! Drawing a [`Redline`] with krilla.

use krilla::blend::BlendMode;
use krilla::color::rgb;
use krilla::configure::{ConfigurationBuilder, PdfVersion};
use krilla::destination::XyzDestination;
use krilla::error::KrillaError;
use krilla::geom::{PathBuilder, Point, Transform};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::{Document, SerializeSettings};

use super::Target;
use super::summary::{self, SummaryPages};
use super::text::TextFont;
use super::{Connector, Mark, MarkStyle, Redline, Rgb, Sheet, SheetLayout, SheetPage, Source};
use crate::geometry::Rect;
use crate::{BySide, Side};

const LABEL_FONT_SIZE: f32 = 9.0;
const LABEL_COLOR: Rgb = Rgb::new(51, 51, 51);
const NOTE_FONT_SIZE: f32 = 8.0;
const NOTE_COLOR: Rgb = Rgb::new(115, 115, 115);
const FRAME_COLOR: Rgb = Rgb::new(153, 153, 153);
const FRAME_LINE_WIDTH: f32 = 0.5;
const OUTLINE_LINE_WIDTH: f32 = 0.4;
const CARET_LINE_WIDTH: f32 = 1.2;

/// How much of a mark's color a highlight keeps: the rest is white, so that
/// the text under it stays readable.
const HIGHLIGHT_STRENGTH: f32 = 0.3;

const CONNECTOR_COLOR: Rgb = Rgb::new(140, 89, 204);
const CONNECTOR_OPACITY: f32 = 0.22;
const CONNECTOR_LINE_WIDTH: f32 = 0.6;
/// The space between a connector and the frames it joins.
const CONNECTOR_INSET: f32 = 3.0;
const CONNECTOR_LABEL_FONT_SIZE: f32 = 7.0;
/// How far the band of a connector reaches into the gutter from a side whose
/// other side the sheet does not show.
const CONNECTOR_STUB_LENGTH: f32 = 12.0;

/// How far above its target a link shows the sheet from.
const TARGET_HEADROOM: f32 = 40.0;

pub(super) fn render(
    redline: &Redline,
    sources: &BySide<Source>,
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
    let font = TextFont::load();
    let layouts: Vec<SheetLayout> = redline
        .sheets
        .iter()
        .map(|sheet| layout_of(sheet, sources))
        .collect();

    if let Some(summary) = &redline.summary {
        let pages = SummaryPages::of(summary, layouts.first().map(|layout| layout.size));
        let destinations: Vec<XyzDestination> = summary
            .entries
            .iter()
            .map(|entry| destination_of(&entry.target, &layouts, pages.count()))
            .collect();
        let sheet_count = redline.sheets.len();
        summary::draw(
            &mut document,
            &font,
            summary,
            &pages,
            sheet_count,
            &destinations,
        );

        let mut outline = Outline::new();
        for (index, (entry, destination)) in summary.entries.iter().zip(destinations).enumerate() {
            let title = summary::bookmark_title(index, entry);
            outline.push_child(OutlineNode::new(title, destination));
        }
        document.set_outline(outline);
        document.set_metadata(Metadata::new().title(summary.title.clone()));
    }
    for (sheet, layout) in redline.sheets.iter().zip(&layouts) {
        draw_sheet(&mut document, sources, &font, sheet, layout);
    }
    document.finish()
}

fn layout_of(sheet: &Sheet, sources: &BySide<Source>) -> SheetLayout {
    SheetLayout::new(BySide::from_fn(|side| {
        let page = sheet.pages[side].as_ref()?;
        sources[side].page_size(page.page_index)
    }))
}

/// The view of the redline that shows `target`, on a sheet that is
/// `sheet_page_offset` pages into the document.
fn destination_of(
    target: &Target,
    layouts: &[SheetLayout],
    sheet_page_offset: usize,
) -> XyzDestination {
    let frame = layouts[target.sheet_index].frames.new;
    let top = (frame.top + target.top - TARGET_HEADROOM).max(0.0);
    XyzDestination::new(
        sheet_page_offset + target.sheet_index,
        Point::from_xy(0.0, top),
    )
}

fn draw_sheet(
    document: &mut Document,
    sources: &BySide<Source>,
    font: &TextFont,
    sheet: &Sheet,
    layout: &SheetLayout,
) {
    let settings = PageSettings::from_wh(layout.size.width, layout.size.height)
        .expect("a sheet is larger than its margins");
    let mut page = document.start_page_with(settings);
    let mut surface = page.surface();

    for side in Side::BOTH {
        let frame = layout.frames[side];
        if let Some(sheet_page) = &sheet.pages[side] {
            embed_page(&mut surface, &sources[side], sheet_page, frame);
            draw_marks(&mut surface, layout, side, &sheet_page.marks);
            let label_position = (frame.left, layout.label_baseline());
            font.draw(
                &mut surface,
                label_position,
                LABEL_FONT_SIZE,
                LABEL_COLOR,
                &sheet_page.label,
            );
        }
        stroke_rects(&mut surface, [frame], FRAME_COLOR, FRAME_LINE_WIDTH);
    }
    for connector in &sheet.connectors {
        draw_connector(&mut surface, font, layout, connector);
    }
    let left = layout.frames.old.left;
    let notes = [
        (&sheet.note_above, layout.note_above_baseline()),
        (&sheet.note_below, layout.note_below_baseline()),
    ];
    for (note, baseline) in notes {
        if let Some(note) = note {
            font.draw(
                &mut surface,
                (left, baseline),
                NOTE_FONT_SIZE,
                NOTE_COLOR,
                note,
            );
        }
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
            MarkStyle::Caret => stroke_left_edges(surface, rects, color),
        }
    }
}

fn highlight_rects(surface: &mut Surface, rects: impl IntoIterator<Item = Rect>, color: Rgb) {
    let Some(path) = path_of_rects(rects) else {
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
    let Some(path) = path_of_rects(rects) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(stroke(color, line_width)));
    surface.draw_path(&path);
}

fn stroke_left_edges(surface: &mut Surface, rects: impl IntoIterator<Item = Rect>, color: Rgb) {
    let mut builder = PathBuilder::new();
    for rect in rects {
        builder.move_to(rect.left, rect.top);
        builder.line_to(rect.left, rect.bottom);
    }
    let Some(path) = builder.finish() else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(stroke(color, CARET_LINE_WIDTH)));
    surface.draw_path(&path);
}

/// One end of the band of a connector: a vertical extent at a position
/// across the sheet.
#[derive(Clone, Copy)]
struct ConnectorEnd {
    x: f32,
    top: f32,
    bottom: f32,
}

impl ConnectorEnd {
    /// An end of no height, level with the middle of `other`.
    fn point_level_with(other: ConnectorEnd, x: f32) -> Self {
        let center = (other.top + other.bottom) / 2.0;
        Self {
            x,
            top: center,
            bottom: center,
        }
    }
}

/// Draws a band that runs from the old side's extent at the left of the
/// gutter to the new side's extent at its right, with the label in between.
/// Where a side has no extent, the band is a stub that comes to a point,
/// with the label after it: the labels of stubs from the left and from the
/// right then keep out of each other's way.
fn draw_connector(
    surface: &mut Surface,
    font: &TextFont,
    layout: &SheetLayout,
    connector: &Connector,
) {
    let (gutter_left, gutter_right) = layout.gutter();
    let (left_x, right_x) = (
        gutter_left + CONNECTOR_INSET,
        gutter_right - CONNECTOR_INSET,
    );
    let middle = (left_x + right_x) / 2.0;
    let label_width = font.width(&connector.label, CONNECTOR_LABEL_FONT_SIZE);

    let end_at = |side: Side, x: f32| {
        let span = connector.spans[side]?;
        let frame_top = layout.frames[side].top;
        Some(ConnectorEnd {
            x,
            top: frame_top + span.top,
            bottom: frame_top + span.bottom,
        })
    };
    let (left, right, label_left) = match (end_at(Side::Old, left_x), end_at(Side::New, right_x)) {
        (Some(left), Some(right)) => (left, right, middle - label_width / 2.0),
        (Some(left), None) => {
            let point = ConnectorEnd::point_level_with(left, left_x + CONNECTOR_STUB_LENGTH);
            (left, point, point.x + CONNECTOR_INSET)
        }
        (None, Some(right)) => {
            let point = ConnectorEnd::point_level_with(right, right_x - CONNECTOR_STUB_LENGTH);
            (point, right, point.x - CONNECTOR_INSET - label_width)
        }
        (None, None) => return,
    };

    let bend_x = (left.x + right.x) / 2.0;
    let mut builder = PathBuilder::new();
    builder.move_to(left.x, left.top);
    builder.cubic_to(bend_x, left.top, bend_x, right.top, right.x, right.top);
    builder.line_to(right.x, right.bottom);
    builder.cubic_to(
        bend_x,
        right.bottom,
        bend_x,
        left.bottom,
        left.x,
        left.bottom,
    );
    builder.close();
    if let Some(path) = builder.finish() {
        surface.set_fill(Some(Fill {
            opacity: NormalizedF32::new(CONNECTOR_OPACITY).expect("an opacity between 0 and 1"),
            ..fill(CONNECTOR_COLOR)
        }));
        surface.set_stroke(Some(stroke(CONNECTOR_COLOR, CONNECTOR_LINE_WIDTH)));
        surface.draw_path(&path);
    }

    let center = (left.top + left.bottom + right.top + right.bottom) / 4.0;
    let position = (label_left, center + CONNECTOR_LABEL_FONT_SIZE / 3.0);
    font.draw(
        surface,
        position,
        CONNECTOR_LABEL_FONT_SIZE,
        LABEL_COLOR,
        &connector.label,
    );
}

fn path_of_rects(rects: impl IntoIterator<Item = Rect>) -> Option<krilla::geom::Path> {
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

fn stroke(color: Rgb, line_width: f32) -> Stroke {
    Stroke {
        paint: krilla_color(color).into(),
        width: line_width,
        ..Stroke::default()
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
