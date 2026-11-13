//! A [`Reader`] backed by PDFium.

mod library;
mod page_space;
mod strings;
mod structure;
mod text;

use std::collections::HashMap;
use std::ffi::{c_int, c_ulong};
use std::sync::Arc;

use pdfium_render::prelude::{FPDF_DOCUMENT, FPDF_PAGE, FS_RECTF};

pub use self::library::LIBRARY_PATH_VARIABLE;
use self::library::Library;
use self::page_space::{PageSpace, VisibleBox};
use super::{Page, ReadError, Reader};

/// `FPDF_GetLastError`'s value for a document that needs a password.
const PASSWORD_ERROR: c_ulong = 4;

pub struct PdfiumReader {
    document: FPDF_DOCUMENT,
    /// PDFium reads from this buffer for as long as the document is open.
    _pdf: Arc<Vec<u8>>,
    is_tagged: bool,
    page_count: usize,
}

impl PdfiumReader {
    pub fn open(pdf: Arc<Vec<u8>>) -> Result<Self, ReadError> {
        let pdfium = library::lock()?;
        // SAFETY: `pdf` is kept alive until the document is closed.
        unsafe {
            let document = pdfium.FPDF_LoadMemDocument64(&pdf, None);
            if document.is_null() {
                return Err(match pdfium.FPDF_GetLastError() {
                    PASSWORD_ERROR => ReadError::PasswordProtected,
                    _ => ReadError::InvalidDocument,
                });
            }
            Ok(Self {
                document,
                is_tagged: pdfium.FPDFCatalog_IsTagged(document) != 0,
                page_count: pdfium.FPDF_GetPageCount(document).max(0) as usize,
                _pdf: pdf,
            })
        }
    }
}

impl Reader for PdfiumReader {
    fn is_tagged(&self) -> bool {
        self.is_tagged
    }

    fn page_count(&self) -> usize {
        self.page_count
    }

    fn read_page(&self, page_index: usize) -> Result<Page, ReadError> {
        let invalid_page = ReadError::InvalidPage {
            page_number: page_index + 1,
        };
        if page_index >= self.page_count {
            return Err(invalid_page);
        }
        let pdfium = library::lock()?;
        // SAFETY: the document is open, and the page is only used before it is closed.
        unsafe {
            let page = pdfium.FPDF_LoadPage(self.document, page_index as c_int);
            if page.is_null() {
                return Err(invalid_page);
            }
            let page_space = page_space_of(&pdfium, page);
            let structure_paths = if self.is_tagged {
                structure::paths_by_marked_content_id(&pdfium, page)
            } else {
                HashMap::new()
            };
            let (spans, text) = text::read(&pdfium, page, &page_space, &structure_paths);
            pdfium.FPDF_ClosePage(page);
            Ok(Page {
                size: page_space.size(),
                spans,
                text,
            })
        }
    }
}

impl Drop for PdfiumReader {
    fn drop(&mut self) {
        if let Ok(pdfium) = library::lock() {
            // SAFETY: the document is open and is not used again.
            unsafe { pdfium.FPDF_CloseDocument(self.document) };
        }
    }
}

/// # Safety
///
/// `page` must be a loaded page.
unsafe fn page_space_of(pdfium: &Library, page: FPDF_PAGE) -> PageSpace {
    let mut visible = FS_RECTF {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    // SAFETY: `page` is loaded and `visible` outlives the call.
    unsafe {
        pdfium.FPDF_GetPageBoundingBox(page, &mut visible);
        PageSpace::new(
            VisibleBox {
                left: visible.left.into(),
                bottom: visible.bottom.into(),
                right: visible.right.into(),
                top: visible.top.into(),
            },
            pdfium.FPDFPage_GetRotation(page),
        )
    }
}
