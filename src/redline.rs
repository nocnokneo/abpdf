//! The redline: a PDF of sheets, each showing an old page on the left and a
//! new page on the right, with marks drawn over them.
//!
//! A [`Redline`] describes the document as plain data, and [`write`] renders
//! it.
//! The source pages are embedded as they are, so their text stays text and
//! their drawings stay vector drawings.

mod layout;
mod render;

use std::sync::Arc;

use krilla::error::KrillaError;
use krilla::pdf::{Pdf, PdfDocument, PdfError};

pub use self::layout::SheetLayout;
use crate::Side;
use crate::geometry::{Rect, Size};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Redline {
    pub sheets: Vec<Sheet>,
}

/// One page of the redline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheet {
    pub old: Option<SheetPage>,
    pub new: Option<SheetPage>,
}

impl Sheet {
    pub fn page(&self, side: Side) -> Option<&SheetPage> {
        match side {
            Side::Old => self.old.as_ref(),
            Side::New => self.new.as_ref(),
        }
    }
}

/// A source page on a sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetPage {
    pub page_index: usize,
    /// The line of text above the page.
    pub label: String,
    pub marks: Vec<Mark>,
}

/// A rectangle drawn over a source page, in the [page space](crate::geometry)
/// of that page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub bounds: Rect,
    pub color: Rgb,
    pub style: MarkStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkStyle {
    /// Tints what is under it, as a highlighter pen does.
    Highlight,
    Outline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const REMOVED: Rgb = Rgb::new(217, 38, 38);
    pub const ADDED: Rgb = Rgb::new(26, 153, 51);
    pub const MOVED: Rgb = Rgb::new(38, 89, 217);
    pub const EXCLUDED: Rgb = Rgb::new(128, 128, 128);

    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("cannot read the {0} document to embed its pages")]
    UnreadableSource(Side),
    #[error("the {side} document has no page {page_number}")]
    NoSuchPage { side: Side, page_number: usize },
    #[error("cannot write the redline: {0}")]
    Serialization(String),
}

/// Renders `redline` over the pages of the two source documents.
///
/// The output depends on nothing but the arguments: the same input gives the
/// same bytes.
pub fn write(
    redline: &Redline,
    old_pdf: Arc<Vec<u8>>,
    new_pdf: Arc<Vec<u8>>,
) -> Result<Vec<u8>, WriteError> {
    let sources = Sources {
        old: Source::load(Side::Old, old_pdf)?,
        new: Source::load(Side::New, new_pdf)?,
    };
    sources.check_pages_exist(redline)?;

    let default_version = krilla::configure::PdfVersion::default();
    let rendered = match render::render(redline, &sources, default_version) {
        // A source page cannot be embedded in a PDF of an older version than
        // its own, so write the version that the source asks for.
        Err(KrillaError::Pdf(_, PdfError::VersionMismatch(required_version), _)) => {
            render::render(redline, &sources, required_version)
        }
        rendered => rendered,
    };
    rendered.map_err(|error| WriteError::Serialization(format!("{error:?}")))
}

struct Sources {
    old: Source,
    new: Source,
}

impl Sources {
    fn get(&self, side: Side) -> &Source {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }

    fn check_pages_exist(&self, redline: &Redline) -> Result<(), WriteError> {
        for sheet in &redline.sheets {
            for side in Side::BOTH {
                let missing_page = sheet
                    .page(side)
                    .filter(|page| self.get(side).page_size(page.page_index).is_none());
                if let Some(page) = missing_page {
                    return Err(WriteError::NoSuchPage {
                        side,
                        page_number: page.page_index + 1,
                    });
                }
            }
        }
        Ok(())
    }
}

/// A document whose pages are embedded.
struct Source {
    pdf: Arc<Pdf>,
    embeddable: PdfDocument,
}

impl Source {
    fn load(side: Side, pdf: Arc<Vec<u8>>) -> Result<Self, WriteError> {
        let pdf = Arc::new(Pdf::new(pdf).map_err(|_| WriteError::UnreadableSource(side))?);
        Ok(Self {
            embeddable: PdfDocument::new(pdf.clone()),
            pdf,
        })
    }

    /// The size at which a page is embedded, which is its size in page space.
    fn page_size(&self, page_index: usize) -> Option<Size> {
        let (width, height) = self.pdf.pages().get(page_index)?.render_dimensions();
        Some(Size { width, height })
    }
}
