//! The PDFium objects of a page, as values that close themselves.
//!
//! Every type here borrows the [`Library`], which can only be had while
//! PDFium is locked, and whatever PDFium object its own was made from, so a
//! handle cannot outlive what it points into.
//! Their methods are safe to call with any argument: PDFium checks handles
//! for null and indices for their range, and reports a failure instead.

use std::ffi::{c_int, c_uint, c_ulong, c_void};

use pdfium_render::prelude::{
    FPDF_DOCUMENT, FPDF_PAGE, FPDF_PAGEOBJECT, FPDF_PAGEOBJECTMARK, FPDF_STRUCTELEMENT,
    FPDF_STRUCTTREE, FPDF_TEXTPAGE, FS_MATRIX, FS_RECTF,
};

use super::library::Library;
use super::page_space::UserSpaceBox;
use super::strings;
use crate::reader::Color;

pub(super) struct LoadedPage<'a> {
    pdfium: &'a Library,
    handle: FPDF_PAGE,
}

impl<'a> LoadedPage<'a> {
    /// `None` if the document has no such page.
    ///
    /// # Safety
    ///
    /// `document` must be an open document, and must stay open for as long
    /// as the page is used.
    pub unsafe fn load(
        pdfium: &'a Library,
        document: FPDF_DOCUMENT,
        page_index: usize,
    ) -> Option<Self> {
        let page_index = c_int::try_from(page_index).ok()?;
        // SAFETY: the caller vouches for `document`.
        let handle = unsafe { pdfium.FPDF_LoadPage(document, page_index) };
        (!handle.is_null()).then_some(Self { pdfium, handle })
    }

    /// The part of the page that a viewer shows: its media box cut down to
    /// its crop box.
    pub fn visible_box(&self) -> UserSpaceBox {
        let mut visible = EMPTY_RECT;
        // SAFETY: the page is loaded and `visible` outlives the call.
        unsafe {
            self.pdfium
                .FPDF_GetPageBoundingBox(self.handle, &mut visible);
        }
        user_space_box(visible)
    }

    /// The page's `/Rotate` entry divided by 90.
    pub fn clockwise_quarter_turns(&self) -> i32 {
        // SAFETY: the page is loaded.
        unsafe { self.pdfium.FPDFPage_GetRotation(self.handle) }
    }

    pub fn text(&self) -> Option<TextPage<'_>> {
        // SAFETY: the page is loaded.
        let handle = unsafe { self.pdfium.FPDFText_LoadPage(self.handle) };
        (!handle.is_null()).then_some(TextPage {
            pdfium: self.pdfium,
            handle,
        })
    }

    /// `None` if the page has no part in a structure tree.
    pub fn structure_tree(&self) -> Option<StructureTree<'_>> {
        // SAFETY: the page is loaded.
        let handle = unsafe { self.pdfium.FPDF_StructTree_GetForPage(self.handle) };
        (!handle.is_null()).then_some(StructureTree {
            pdfium: self.pdfium,
            handle,
        })
    }
}

impl Drop for LoadedPage<'_> {
    fn drop(&mut self) {
        // SAFETY: the page is loaded, and everything made from it borrows it.
        unsafe { self.pdfium.FPDF_ClosePage(self.handle) };
    }
}

/// The characters of a page as PDFium extracts them.
pub(super) struct TextPage<'a> {
    pdfium: &'a Library,
    handle: FPDF_TEXTPAGE,
}

// SAFETY, for every method: the text page is open until it is dropped, and
// PDFium checks `index` itself.
impl TextPage<'_> {
    pub fn char_count(&self) -> c_int {
        unsafe { self.pdfium.FPDFText_CountChars(self.handle) }.max(0)
    }

    /// The UTF-16 code unit or code point of a character, or 0.
    pub fn unicode(&self, index: c_int) -> u32 {
        unsafe { self.pdfium.FPDFText_GetUnicode(self.handle, index) }
    }

    /// Whether PDFium took the character for a hyphen that ends a line
    /// inside a word.
    pub fn is_hyphen(&self, index: c_int) -> bool {
        unsafe { self.pdfium.FPDFText_IsHyphen(self.handle, index) > 0 }
    }

    /// The box that the font reserves for the character's glyph.
    pub fn loose_char_box(&self, index: c_int) -> UserSpaceBox {
        let mut bounds = EMPTY_RECT;
        unsafe {
            self.pdfium
                .FPDFText_GetLooseCharBox(self.handle, index, &mut bounds);
        }
        user_space_box(bounds)
    }

    /// The text object that draws the character, if one does.
    pub fn text_object(&self, index: c_int) -> Option<TextObject<'_>> {
        let handle = unsafe { self.pdfium.FPDFText_GetTextObject(self.handle, index) };
        (!handle.is_null()).then_some(TextObject {
            pdfium: self.pdfium,
            handle,
        })
    }

    /// The name of the character's font and the flags of its font descriptor.
    pub fn font(&self, index: c_int) -> (String, c_int) {
        let mut flags: c_int = 0;
        let name = strings::utf8(|buffer, byte_capacity| unsafe {
            self.pdfium.FPDFText_GetFontInfo(
                self.handle,
                index,
                buffer as *mut c_void,
                byte_capacity,
                &mut flags,
            )
        });
        (name, flags)
    }

    /// The font size that the character's text object asks for.
    pub fn nominal_font_size(&self, index: c_int) -> f32 {
        unsafe { self.pdfium.FPDFText_GetFontSize(self.handle, index) as f32 }
    }

    pub fn font_weight(&self, index: c_int) -> Option<u16> {
        let weight = unsafe { self.pdfium.FPDFText_GetFontWeight(self.handle, index) };
        u16::try_from(weight).ok()
    }

    /// Opaque black if PDFium cannot tell.
    pub fn fill_color(&self, index: c_int) -> Color {
        let (mut red, mut green, mut blue, mut alpha): (c_uint, c_uint, c_uint, c_uint) =
            (0, 0, 0, u8::MAX.into());
        unsafe {
            self.pdfium.FPDFText_GetFillColor(
                self.handle,
                index,
                &mut red,
                &mut green,
                &mut blue,
                &mut alpha,
            );
        }
        Color {
            red: red as u8,
            green: green as u8,
            blue: blue as u8,
            alpha: alpha as u8,
        }
    }

    /// The clockwise rotation of the character, in radians from 0.
    pub fn rotation_radians(&self, index: c_int) -> f32 {
        unsafe { self.pdfium.FPDFText_GetCharAngle(self.handle, index) }.max(0.0)
    }

    /// The matrix that takes the character's text space to user space.
    pub fn matrix(&self, index: c_int) -> FS_MATRIX {
        let mut matrix = FS_MATRIX {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        };
        unsafe {
            self.pdfium
                .FPDFText_GetMatrix(self.handle, index, &mut matrix);
        }
        matrix
    }
}

impl Drop for TextPage<'_> {
    fn drop(&mut self) {
        // SAFETY: the text page is open, and everything made from it borrows it.
        unsafe { self.pdfium.FPDFText_ClosePage(self.handle) };
    }
}

/// A text object of a page, which the page owns.
#[derive(Clone, Copy)]
pub(super) struct TextObject<'a> {
    pdfium: &'a Library,
    handle: FPDF_PAGEOBJECT,
}

// SAFETY, for every method: the object lives as long as its page, which the
// text page it was taken from borrows.
impl<'a> TextObject<'a> {
    /// What tells this object from the other objects of its page.
    pub fn identity(&self) -> usize {
        self.handle as usize
    }

    /// The identifier that ties the object to the structure tree, if it has
    /// one.
    pub fn marked_content_id(&self) -> Option<c_int> {
        let id = unsafe { self.pdfium.FPDFPageObj_GetMarkedContentID(self.handle) };
        (id >= 0).then_some(id)
    }

    /// The marked content sequences that the object is in, outermost first.
    pub fn marks(&self) -> impl Iterator<Item = Mark<'a>> {
        let (pdfium, object) = (self.pdfium, self.handle);
        let count = unsafe { pdfium.FPDFPageObj_CountMarks(object) }.max(0);
        (0..count as c_ulong).filter_map(move |index| {
            let handle = unsafe { pdfium.FPDFPageObj_GetMark(object, index) };
            (!handle.is_null()).then_some(Mark { pdfium, handle })
        })
    }
}

/// The tag and the properties of a marked content sequence.
pub(super) struct Mark<'a> {
    pdfium: &'a Library,
    handle: FPDF_PAGEOBJECTMARK,
}

// SAFETY, for every method: the mark lives as long as its page object.
impl Mark<'_> {
    /// The tag, such as `Artifact` or `Span`.
    pub fn name(&self) -> Option<String> {
        strings::utf16(|buffer, byte_capacity| {
            let mut byte_length = 0;
            let found = unsafe {
                self.pdfium.FPDFPageObjMark_GetName(
                    self.handle,
                    buffer,
                    byte_capacity,
                    &mut byte_length,
                )
            };
            (found != 0).then_some(byte_length)
        })
    }

    /// The value of a property that is a text string.
    ///
    /// PDFium's own getter for strings takes the bytes of every string for
    /// UTF-8, so this decodes them itself.
    pub fn text_property(&self, key: &str) -> Option<String> {
        strings::pdf_text(|buffer, byte_capacity| {
            let mut byte_length = 0;
            let found = unsafe {
                self.pdfium.FPDFPageObjMark_GetParamBlobValue(
                    self.handle,
                    key,
                    buffer,
                    byte_capacity,
                    &mut byte_length,
                )
            };
            (found != 0).then_some(byte_length)
        })
    }
}

/// The part of a document's structure tree that is on one page.
pub(super) struct StructureTree<'a> {
    pdfium: &'a Library,
    handle: FPDF_STRUCTTREE,
}

impl StructureTree<'_> {
    pub fn children(&self) -> impl Iterator<Item = StructureElement<'_>> {
        // SAFETY: the tree is open until it is dropped.
        let count = unsafe { self.pdfium.FPDF_StructTree_CountChildren(self.handle) }.max(0);
        (0..count).filter_map(move |index| {
            // SAFETY: as above, and PDFium checks `index`.
            let handle = unsafe {
                self.pdfium
                    .FPDF_StructTree_GetChildAtIndex(self.handle, index)
            };
            (!handle.is_null()).then_some(StructureElement {
                pdfium: self.pdfium,
                handle,
            })
        })
    }
}

impl Drop for StructureTree<'_> {
    fn drop(&mut self) {
        // SAFETY: the tree is open, and its elements borrow it.
        unsafe { self.pdfium.FPDF_StructTree_Close(self.handle) };
    }
}

pub(super) struct StructureElement<'a> {
    pdfium: &'a Library,
    handle: FPDF_STRUCTELEMENT,
}

// SAFETY, for every method: the element lives as long as its tree, and
// PDFium checks indices itself.
impl<'a> StructureElement<'a> {
    /// The structure type, such as `P` or `TD`.
    pub fn type_name(&self) -> String {
        strings::utf16(|buffer, byte_capacity| {
            let byte_length = unsafe {
                self.pdfium.FPDF_StructElement_GetType(
                    self.handle,
                    buffer as *mut c_void,
                    byte_capacity,
                )
            };
            (byte_length > 0).then_some(byte_length)
        })
        .unwrap_or_default()
    }

    /// The identifiers of the marked content that is directly in the
    /// element.
    pub fn marked_content_ids(&self) -> impl Iterator<Item = c_int> {
        let (pdfium, element) = (self.pdfium, self.handle);
        let count = unsafe { pdfium.FPDF_StructElement_GetMarkedContentIdCount(element) }.max(0);
        (0..count)
            .map(move |index| unsafe {
                pdfium.FPDF_StructElement_GetMarkedContentIdAtIndex(element, index)
            })
            .filter(|id| *id >= 0)
    }

    pub fn children(&self) -> impl Iterator<Item = StructureElement<'a>> {
        let (pdfium, element) = (self.pdfium, self.handle);
        let count = unsafe { pdfium.FPDF_StructElement_CountChildren(element) }.max(0);
        (0..count).filter_map(move |index| {
            let handle = unsafe { pdfium.FPDF_StructElement_GetChildAtIndex(element, index) };
            (!handle.is_null()).then_some(StructureElement { pdfium, handle })
        })
    }
}

const EMPTY_RECT: FS_RECTF = FS_RECTF {
    left: 0.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
};

fn user_space_box(rect: FS_RECTF) -> UserSpaceBox {
    UserSpaceBox {
        left: rect.left.into(),
        bottom: rect.bottom.into(),
        right: rect.right.into(),
        top: rect.top.into(),
    }
}
