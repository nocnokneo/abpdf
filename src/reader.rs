//! Reading the content of a PDF.
//!
//! [`Reader`] is the boundary between a PDF engine and the rest of abpdf.
//! An engine reports each page as a [`Page`]: plain data in
//! [page space](crate::geometry), with nothing engine-specific in it, so that
//! two engines can be compared by comparing what they report.

mod content;
mod document;
mod figures;
mod fonts;
mod matrix;
mod page_space;
pub mod pdfium;
mod structure;
#[cfg(test)]
mod testing;
mod text;
mod text_string;

use std::fmt;

pub use self::document::{Document, Pages};

use crate::geometry::{Rect, Size};

/// One opened PDF document.
pub trait Reader {
    /// Whether the document declares that it is a tagged PDF.
    fn is_tagged(&self) -> bool;

    fn page_count(&self) -> usize;

    fn read_page(&self, page_index: usize) -> Result<Page, ReadError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("cannot load the PDFium library: {0}")]
    EngineUnavailable(String),
    #[error("the file is not a PDF or is damaged")]
    InvalidDocument,
    #[error("the PDF is password protected")]
    PasswordProtected,
    #[error("cannot read page {page_number}")]
    InvalidPage { page_number: usize },
}

/// The text and the figures of one page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub size: Size,
    /// The runs that [`Char::span`] refers to.
    pub spans: Vec<Span>,
    /// The figures that [`TextItem::Figure`] refers to.
    pub figures: Vec<Figure>,
    /// The text in reading order, with the figures where they stand in it.
    ///
    /// The text is as it is printed: a hyphen that ends a line is a
    /// character like any other, with a line break after it.
    /// Whether it breaks a word is not for an engine to say.
    pub text: Vec<TextItem>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextItem {
    Char(Char),
    /// White space within a line.
    Space,
    LineBreak,
    /// A figure, as its index in [`Page::figures`].
    /// It stands before the first character that is drawn after it or in
    /// it.
    Figure(usize),
}

/// A picture on a page: an image, or a drawing of shapes, images or both.
///
/// One figure is what the producer keeps together, or what the page draws
/// in one go of shapes that nothing keeps together, unless those only rule
/// and box text.
/// Text in a figure is not part of it: it is among the text of its page.
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    /// The smallest upright rectangle around the figure.
    pub bounds: Rect,
    pub kind: FigureKind,
    /// Whether the producer marked the figure as an artifact.
    pub is_artifact: bool,
    /// A hash of everything about the figure that is not a position: what
    /// its parts are, in which colors and with which line widths they are
    /// drawn, and the data of its images.
    /// Two figures that an engine reads the same of have the same hash.
    pub content_hash: u64,
    /// The points that the figure is drawn through, in the order in which
    /// it is drawn: those of its paths and the corners of its images.
    pub points: Vec<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FigureKind {
    /// One image.
    Image { pixel_width: u32, pixel_height: u32 },
    /// Shapes, images or both that are drawn as one figure.
    Drawing { part_count: usize },
}

/// As `image, 640 × 480 pixels` or `drawing, 12 parts`.
impl fmt::Display for FigureKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Image {
                pixel_width,
                pixel_height,
            } => write!(formatter, "image, {pixel_width} × {pixel_height} pixels"),
            Self::Drawing { part_count: 1 } => write!(formatter, "drawing, 1 part"),
            Self::Drawing { part_count } => write!(formatter, "drawing, {part_count} parts"),
        }
    }
}

/// What a run of consecutive characters has in common.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub style: Style,
    /// Whether the producer marked the run as an artifact: page furniture or
    /// layout, not content.
    pub is_artifact: bool,
    /// The types of the structure elements that contain the run, outermost
    /// first and separated by `/`, such as `Document/Table/TBody/TR/TD`.
    /// `None` when the run is not part of the structure tree.
    pub structure_path: Option<String>,
    /// The table cell that contains the run, as its number among the cells
    /// of the page.
    /// `None` when the run is in no cell, or when no structure tree says
    /// which cell it is in.
    pub cell: Option<usize>,
    /// The text the producer says the run stands for, when that is not what
    /// its glyphs map to.
    pub actual_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// The font name, without the tag that marks a subset.
    pub font_name: String,
    /// The size the text is shown at, in page space units.
    pub font_size: f32,
    pub font_weight: Option<u16>,
    pub is_italic: bool,
    pub fill_color: Color,
    /// The clockwise rotation of the text on the unrotated page.
    pub rotation_degrees: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

/// As `#rrggbb`, with the opacity as two more digits unless it is full.
impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            red,
            green,
            blue,
            alpha,
        } = self;
        write!(formatter, "#{red:02x}{green:02x}{blue:02x}")?;
        match *alpha {
            u8::MAX => Ok(()),
            alpha => write!(formatter, "{alpha:02x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Char {
    /// The character, which is not white space.
    /// A character the PDF gives no Unicode value for is U+FFFD.
    pub text: char,
    /// The box that the font reserves for the glyph: as high as a line and as
    /// wide as the glyph's advance.
    pub bounds: Rect,
    /// Index into [`Page::spans`].
    pub span: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_reads_as_hexadecimal_with_its_opacity_unless_that_is_full() {
        let mut color = Color {
            red: 255,
            green: 65,
            blue: 0,
            alpha: 255,
        };
        assert_eq!(color.to_string(), "#ff4100");
        color.alpha = 128;
        assert_eq!(color.to_string(), "#ff410080");
    }
}
