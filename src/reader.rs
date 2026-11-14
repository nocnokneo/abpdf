//! Reading the content of a PDF.
//!
//! [`Reader`] is the boundary between a PDF engine and the rest of abpdf.
//! An engine reports each page as a [`Page`]: plain data in
//! [page space](crate::geometry), with nothing engine-specific in it, so that
//! two engines can be compared by comparing what they report.

pub mod pdfium;

use std::fmt;

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

/// The text of one page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub size: Size,
    /// The runs that [`Char::span`] refers to.
    pub spans: Vec<Span>,
    /// The text in reading order.
    pub text: Vec<TextItem>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextItem {
    Char(Char),
    /// White space within a line.
    Space,
    LineBreak,
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
    /// Whether this is a hyphen that ends a line in the middle of a word.
    ///
    /// The word continues with the next character: no line break is reported
    /// after such a hyphen.
    /// The engine may not know which character the hyphen is, in which case
    /// `text` is `'-'`; [`Span::actual_text`] can tell.
    pub is_line_end_hyphen: bool,
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
