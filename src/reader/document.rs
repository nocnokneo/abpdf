//! A PDF document and its pages.

use std::iter::Enumerate;
use std::slice;
use std::sync::Arc;

use hayro_syntax::object::Dict;
use hayro_syntax::page::{Page as PdfPage, Rotation};
use hayro_syntax::{DecryptionError, LoadPdfError, Pdf};

use super::page_space::{PageSpace, UserSpaceBox};
use super::structure::StructureTree;
use super::{Page, PageLimit, ReadError, content, figures, text};

/// One opened PDF document.
pub struct Document {
    pdf: Pdf,
    is_tagged: bool,
}

impl Document {
    pub fn open(pdf: Arc<Vec<u8>>) -> Result<Self, ReadError> {
        let pdf = Pdf::new(pdf).map_err(|error| match error {
            LoadPdfError::Decryption(DecryptionError::PasswordProtected) => {
                ReadError::PasswordProtected
            }
            LoadPdfError::Decryption(_) | LoadPdfError::Invalid => ReadError::InvalidDocument,
        })?;
        let mark_information = catalog_of(&pdf).and_then(|catalog| catalog.get::<Dict>("MarkInfo"));
        let is_marked = mark_information.and_then(|information| information.get::<bool>("Marked"));
        Ok(Self {
            is_tagged: is_marked == Some(true),
            pdf,
        })
    }

    /// Whether the document declares that it is a tagged PDF.
    pub fn is_tagged(&self) -> bool {
        self.is_tagged
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Reads the pages of the document, from the first one on.
    pub fn pages(&self) -> Pages<'_> {
        let catalog = catalog_of(&self.pdf).filter(|_| self.is_tagged);
        Pages {
            pages: self.pdf.pages().iter().enumerate(),
            shared: content::Shared::new(),
            structure: catalog.map_or_else(StructureTree::default, |catalog| {
                StructureTree::read(&catalog)
            }),
        }
    }
}

fn catalog_of(pdf: &Pdf) -> Option<Dict<'_>> {
    let cross_references = pdf.xref();
    cross_references.get(cross_references.root_id())
}

/// The pages of a document as they are read, one after the other.
///
/// What pages share, such as their fonts, is read once.
pub struct Pages<'a> {
    pages: Enumerate<slice::Iter<'a, PdfPage<'a>>>,
    shared: content::Shared<'a>,
    /// What the structure tree says, which is nothing for a document that
    /// is not tagged.
    structure: StructureTree,
}

impl<'a> Pages<'a> {
    fn read(&self, page: &PdfPage<'a>) -> Result<Page, PageLimit> {
        let visible = visible_box_of(page);
        let page_space = PageSpace::new(visible, clockwise_quarter_turns_of(page));
        let structure = self.structure.of_page(page.raw());
        let objects = content::objects_of(page, &self.shared, visible)?;
        let figures = figures::read(&objects, &page_space, structure);
        let figure_objects: Vec<usize> = figures.iter().map(|found| found.first_object).collect();
        let (spans, text) = text::read(&objects, &page_space, structure, &figure_objects);
        Ok(Page {
            size: page_space.size(),
            spans,
            figures: figures.into_iter().map(|found| found.figure).collect(),
            text,
        })
    }
}

impl Iterator for Pages<'_> {
    type Item = Result<Page, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (page_index, page) = self.pages.next()?;
        let page_number = page_index + 1;
        let read = self.read(page);
        Some(read.map_err(|limit| ReadError::PageBeyondLimit { page_number, limit }))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pages.size_hint()
    }
}

impl ExactSizeIterator for Pages<'_> {}

/// The part of a page that a viewer shows: its media box cut down to its
/// crop box.
fn visible_box_of(page: &PdfPage) -> UserSpaceBox {
    let visible = page.intersected_crop_box();
    UserSpaceBox {
        left: visible.x0,
        bottom: visible.y0,
        right: visible.x1,
        top: visible.y1,
    }
}

fn clockwise_quarter_turns_of(page: &PdfPage) -> i32 {
    match page.rotation() {
        Rotation::None => 0,
        Rotation::Horizontal => 1,
        Rotation::Flipped => 2,
        Rotation::FlippedHorizontal => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::TextItem;
    use crate::reader::testing::{page_pdf, pdf_of, stream};

    /// A document of two pages that show a letter each, with the given
    /// entries of its catalog and of its first page.
    fn two_pages(catalog_entries: &str, first_page_entries: &str) -> Document {
        let font = "/Resources << /Font << /F1 << /Type /Font /Subtype /Type1 \
                    /BaseFont /Helvetica >> >> >>";
        let objects = [
            format!("<< /Type /Catalog /Pages 2 0 R {catalog_entries} >>"),
            format!(
                "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] {font} >>"
            ),
            format!("<< /Type /Page /Parent 2 0 R /Contents 5 0 R {first_page_entries} >>"),
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".to_owned(),
            stream("", "BT /F1 10 Tf 1 0 0 1 20 100 Tm (a) Tj ET"),
            stream("", "BT /F1 10 Tf (b) Tj ET"),
        ];
        Document::open(pdf_of(&objects)).expect("the PDF opens")
    }

    fn first_page(document: &Document) -> Page {
        let page = document.pages().next().expect("the document has a page");
        page.expect("the page is read")
    }

    fn characters(page: &Page) -> Vec<crate::reader::Char> {
        let items = page.text.iter();
        items
            .filter_map(|item| match item {
                TextItem::Char(character) => Some(*character),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn pages_are_read_in_their_order() {
        let document = two_pages("", "");
        assert_eq!(document.page_count(), 2);
        let pages = document.pages();
        assert_eq!(pages.len(), 2);
        let texts: Vec<char> = pages
            .map(|page| characters(&page.expect("the page is read"))[0].text)
            .collect();
        assert_eq!(texts, ['a', 'b']);
    }

    #[test]
    fn document_is_tagged_if_its_catalog_says_so() {
        assert!(!two_pages("", "").is_tagged());
        assert!(!two_pages("/MarkInfo << /Marked false >>", "").is_tagged());
        assert!(two_pages("/MarkInfo << /Marked true >>", "").is_tagged());
    }

    #[test]
    fn page_is_read_as_a_viewer_shows_it() {
        // The visible part is 100 wide and 200 high, and is shown turned
        // by a quarter.
        let document = two_pages("", "/CropBox [10 20 110 220] /Rotate 90");
        let page = first_page(&document);
        assert_eq!((page.size.width, page.size.height), (200.0, 100.0));
        // The letter starts 10 right of and 80 above the bottom left
        // corner of the visible part, which is the top left one once the
        // page is turned.
        let bounds = characters(&page)[0].bounds;
        assert!((bounds.left - 77.5).abs() < 0.001, "{bounds:?}");
        assert!((bounds.right - 87.5).abs() < 0.001, "{bounds:?}");
        assert!((bounds.top - 10.0).abs() < 0.001, "{bounds:?}");
        assert!((bounds.bottom - 15.56).abs() < 0.001, "{bounds:?}");
    }

    #[test]
    fn structure_is_only_read_of_a_tagged_document() {
        let tree = "/StructTreeRoot << /K << /S /P /Pg 3 0 R /K 0 >> >>";
        let path_of_first_span = |catalog_entries: &str| {
            let objects = [
                format!("<< /Type /Catalog /Pages 2 0 R {catalog_entries} >>"),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_owned(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 \
                 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>"
                    .to_owned(),
                stream("", "/P << /MCID 0 >> BDC BT /F1 10 Tf (a) Tj ET EMC"),
            ];
            let document = Document::open(pdf_of(&objects)).expect("the PDF opens");
            first_page(&document).spans[0].structure_path.clone()
        };
        assert_eq!(path_of_first_span(tree), None);
        let tagged = format!("{tree} /MarkInfo << /Marked true >>");
        assert_eq!(path_of_first_span(&tagged).as_deref(), Some("P"));
    }

    #[test]
    fn page_without_content_has_none() {
        let document = Document::open(page_pdf("", "", &[])).expect("the PDF opens");
        let page = first_page(&document);
        assert!(page.text.is_empty() && page.figures.is_empty() && page.spans.is_empty());
    }

    #[test]
    fn page_beyond_a_limit_is_not_read_and_is_named() {
        let too_many_saved_states = "q ".repeat(content::MAX_SAVED_STATE_DEPTH + 1);
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".to_owned(),
            stream("", "q Q"),
            stream("", &too_many_saved_states),
        ];
        let document = Document::open(pdf_of(&objects)).expect("the PDF opens");
        let pages: Vec<Result<Page, ReadError>> = document.pages().collect();
        assert!(pages[0].is_ok());
        let error = pages[1]
            .as_ref()
            .expect_err("the second page is beyond a limit");
        assert!(matches!(
            error,
            ReadError::PageBeyondLimit {
                page_number: 2,
                limit: PageLimit::SavedStateDepth
            }
        ));
    }

    #[test]
    fn what_is_not_a_pdf_does_not_open() {
        let result = Document::open(Arc::new(b"not a PDF".to_vec()));
        assert!(matches!(result, Err(ReadError::InvalidDocument)));
    }
}
