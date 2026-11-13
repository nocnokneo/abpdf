//! A [`Reader`] backed by PDFium.

mod handles;
mod library;
mod page_space;
mod strings;
mod structure;
mod text;

use std::collections::HashMap;
use std::ffi::c_ulong;
use std::sync::Arc;

use pdfium_render::prelude::FPDF_DOCUMENT;

use self::handles::LoadedPage;
pub use self::library::LIBRARY_PATH_VARIABLE;
use self::library::LoadedLibrary;
use self::page_space::PageSpace;
use super::{Page, ReadError, Reader};

/// `FPDF_GetLastError`'s value for a document that needs a password.
const PASSWORD_ERROR: c_ulong = 4;

pub struct PdfiumReader {
    library: LoadedLibrary,
    document: FPDF_DOCUMENT,
    /// PDFium reads from this buffer for as long as the document is open.
    _pdf: Arc<Vec<u8>>,
    is_tagged: bool,
    page_count: usize,
}

impl PdfiumReader {
    pub fn open(pdf: Arc<Vec<u8>>) -> Result<Self, ReadError> {
        let library = LoadedLibrary::get()?;
        let pdfium = library.lock();
        // SAFETY: `pdf` is kept alive until the document is closed, and the
        // document is only used if it was opened.
        unsafe {
            let document = pdfium.FPDF_LoadMemDocument64(&pdf, None);
            if document.is_null() {
                return Err(match pdfium.FPDF_GetLastError() {
                    PASSWORD_ERROR => ReadError::PasswordProtected,
                    _ => ReadError::InvalidDocument,
                });
            }
            Ok(Self {
                library,
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
        let pdfium = self.library.lock();
        // SAFETY: the document is open until `self` is dropped.
        let page = unsafe { LoadedPage::load(&pdfium, self.document, page_index) };
        let page = page.ok_or(ReadError::InvalidPage {
            page_number: page_index + 1,
        })?;
        let page_space = PageSpace::new(page.visible_box(), page.clockwise_quarter_turns());
        let structure_paths = if self.is_tagged {
            structure::paths_by_marked_content_id(&page)
        } else {
            HashMap::new()
        };
        let (spans, text) = text::read(&page, &page_space, &structure_paths);
        Ok(Page {
            size: page_space.size(),
            spans,
            text,
        })
    }
}

impl Drop for PdfiumReader {
    fn drop(&mut self) {
        let pdfium = self.library.lock();
        // SAFETY: the document is open and is not used again.
        unsafe { pdfium.FPDF_CloseDocument(self.document) };
    }
}
