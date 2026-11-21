//! The PDFium objects of a page, as values that close themselves.
//!
//! Every type here borrows the [`Library`], which can only be had while
//! PDFium is locked, and whatever PDFium object its own was made from, so a
//! handle cannot outlive what it points into.
//! Their methods are safe to call with any argument: PDFium checks handles
//! for null and indices for their range, and reports a failure instead.

use std::ffi::{c_int, c_uint, c_ulong, c_void};
use std::ptr;

use pdfium_render::prelude::{
    FPDF_DOCUMENT, FPDF_PAGE, FPDF_PAGEOBJECT, FPDF_PAGEOBJECTMARK, FPDF_STRUCTELEMENT,
    FPDF_STRUCTTREE, FPDF_TEXTPAGE, FS_MATRIX, FS_RECTF,
};

use super::library::Library;
use super::strings;
use crate::reader::Color;
use crate::reader::matrix::Matrix;
use crate::reader::page_space::UserSpaceBox;

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

    /// The objects that the page draws, in the order in which it draws them.
    pub fn objects(&self) -> impl Iterator<Item = PageObject<'a>> {
        let (pdfium, page) = (self.pdfium, self.handle);
        // SAFETY: the page is loaded, and PDFium checks `index`.
        let count = unsafe { pdfium.FPDFPage_CountObjects(page) }.max(0);
        (0..count).filter_map(move |index| {
            let handle = unsafe { pdfium.FPDFPage_GetObject(page, index) };
            (!handle.is_null()).then_some(PageObject { pdfium, handle })
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
    pub fn text_object(&self, index: c_int) -> Option<PageObject<'_>> {
        let handle = unsafe { self.pdfium.FPDFText_GetTextObject(self.handle, index) };
        (!handle.is_null()).then_some(PageObject {
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
        let mut channels = OPAQUE_BLACK;
        let [red, green, blue, alpha] = &mut channels;
        unsafe {
            self.pdfium
                .FPDFText_GetFillColor(self.handle, index, red, green, blue, alpha);
        }
        color(channels)
    }

    /// The clockwise rotation of the character, in radians from 0.
    pub fn rotation_radians(&self, index: c_int) -> f32 {
        unsafe { self.pdfium.FPDFText_GetCharAngle(self.handle, index) }.max(0.0)
    }

    /// The matrix that takes the character's text space to user space.
    pub fn matrix(&self, index: c_int) -> FS_MATRIX {
        let mut matrix = IDENTITY_MATRIX;
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

/// Something that a page draws, which the page owns: a piece of text, a
/// path, an image, a shading, or a form, which is a group of such objects.
#[derive(Clone, Copy)]
pub(super) struct PageObject<'a> {
    pdfium: &'a Library,
    handle: FPDF_PAGEOBJECT,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ObjectKind {
    Text,
    Path,
    Image,
    Shading,
    Form,
    Other,
}

/// One step of a path: a move, a line or a curve to a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PathSegment {
    /// PDFium's number for the kind of step, one of `FPDF_SEGMENT_*`.
    pub kind: c_int,
    /// Whether the step ends a closed outline.
    pub closes_outline: bool,
    /// The point that the step goes to, in the space of its path.
    pub point: (f32, f32),
}

/// How a path is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PathPaint {
    /// PDFium's number for the rule that says what is inside the path, one
    /// of `FPDF_FILLMODE_*`, of which 0 is no fill.
    pub fill_mode: c_int,
    pub is_stroked: bool,
}

// SAFETY, for every method: the object lives as long as its page, which
// whatever it was taken from borrows, and PDFium checks that the object is
// of the kind that a function is for, and indices for their range.
impl<'a> PageObject<'a> {
    /// What tells this object from the other objects of its page.
    pub fn identity(&self) -> usize {
        self.handle as usize
    }

    pub fn kind(&self) -> ObjectKind {
        // The values of PDFium's `FPDF_PAGEOBJ_*`.
        match unsafe { self.pdfium.FPDFPageObj_GetType(self.handle) } {
            1 => ObjectKind::Text,
            2 => ObjectKind::Path,
            3 => ObjectKind::Image,
            4 => ObjectKind::Shading,
            5 => ObjectKind::Form,
            _ => ObjectKind::Other,
        }
    }

    /// The smallest upright rectangle around the object, in the space that
    /// it is drawn in: that of its page, or that of the form it is part of.
    /// `None` if PDFium cannot tell.
    pub fn bounds(&self) -> Option<UserSpaceBox> {
        let (mut left, mut bottom, mut right, mut top) = (0.0, 0.0, 0.0, 0.0);
        let found = unsafe {
            self.pdfium.FPDFPageObj_GetBounds(
                self.handle,
                &mut left,
                &mut bottom,
                &mut right,
                &mut top,
            )
        };
        (found != 0).then_some(UserSpaceBox {
            left: left.into(),
            bottom: bottom.into(),
            right: right.into(),
            top: top.into(),
        })
    }

    /// The matrix that takes the object's own space to the space that it is
    /// drawn in.
    pub fn matrix(&self) -> Matrix {
        let mut matrix = IDENTITY_MATRIX;
        unsafe { self.pdfium.FPDFPageObj_GetMatrix(self.handle, &mut matrix) };
        Matrix {
            a: matrix.a.into(),
            b: matrix.b.into(),
            c: matrix.c.into(),
            d: matrix.d.into(),
            e: matrix.e.into(),
            f: matrix.f.into(),
        }
    }

    /// The objects of a form, in the order in which it draws them.
    /// Any other object has none.
    pub fn form_objects(&self) -> impl Iterator<Item = PageObject<'a>> {
        let (pdfium, form) = (self.pdfium, self.handle);
        let count = unsafe { pdfium.FPDFFormObj_CountObjects(form) }.max(0);
        (0..count as c_ulong).filter_map(move |index| {
            let handle = unsafe { pdfium.FPDFFormObj_GetObject(form, index) };
            (!handle.is_null()).then_some(PageObject { pdfium, handle })
        })
    }

    /// The steps of a path.
    /// Any other object has none.
    pub fn path_segments(&self) -> impl Iterator<Item = PathSegment> + use<'a> {
        let (pdfium, path) = (self.pdfium, self.handle);
        let count = unsafe { pdfium.FPDFPath_CountSegments(path) }.max(0);
        (0..count).filter_map(move |index| {
            let segment = unsafe { pdfium.FPDFPath_GetPathSegment(path, index) };
            if segment.is_null() {
                return None;
            }
            let (mut x, mut y) = (0.0, 0.0);
            // SAFETY: the segment is not null and lives as long as its path.
            unsafe {
                pdfium.FPDFPathSegment_GetPoint(segment, &mut x, &mut y);
                Some(PathSegment {
                    kind: pdfium.FPDFPathSegment_GetType(segment),
                    closes_outline: pdfium.FPDFPathSegment_GetClose(segment) != 0,
                    point: (x, y),
                })
            }
        })
    }

    /// How a path is painted.
    pub fn path_paint(&self) -> PathPaint {
        let (mut fill_mode, mut is_stroked) = (0, 0);
        unsafe {
            self.pdfium
                .FPDFPath_GetDrawMode(self.handle, &mut fill_mode, &mut is_stroked);
        }
        PathPaint {
            fill_mode,
            is_stroked: is_stroked != 0,
        }
    }

    /// The color that the object is filled with, which is opaque black if
    /// PDFium cannot tell.
    pub fn fill_color(&self) -> Color {
        let mut channels = OPAQUE_BLACK;
        let [red, green, blue, alpha] = &mut channels;
        unsafe {
            self.pdfium
                .FPDFPageObj_GetFillColor(self.handle, red, green, blue, alpha);
        }
        color(channels)
    }

    /// The color that the outline of the object is drawn in, which is
    /// opaque black if PDFium cannot tell.
    pub fn stroke_color(&self) -> Color {
        let mut channels = OPAQUE_BLACK;
        let [red, green, blue, alpha] = &mut channels;
        unsafe {
            self.pdfium
                .FPDFPageObj_GetStrokeColor(self.handle, red, green, blue, alpha);
        }
        color(channels)
    }

    /// The width of the line that the outline of the object is drawn with,
    /// in the object's own space.
    pub fn stroke_width(&self) -> f32 {
        let mut width = 0.0;
        unsafe {
            self.pdfium
                .FPDFPageObj_GetStrokeWidth(self.handle, &mut width);
        }
        width
    }

    /// The numbers of columns and of rows of pixels of an image.
    pub fn image_pixel_size(&self) -> (u32, u32) {
        let (mut width, mut height): (c_uint, c_uint) = (0, 0);
        unsafe {
            self.pdfium
                .FPDFImageObj_GetImagePixelSize(self.handle, &mut width, &mut height);
        }
        (width, height)
    }

    /// The data of an image as the PDF stores it, with the filters of its
    /// stream undone.
    pub fn image_data(&self) -> Vec<u8> {
        let (pdfium, image) = (self.pdfium, self.handle);
        // PDFium is asked for the length first, because it aborts if it is
        // given a buffer that the data does not fit into.
        let length = unsafe { pdfium.FPDFImageObj_GetImageDataDecoded(image, ptr::null_mut(), 0) };
        let mut data = vec![0u8; length as usize];
        if !data.is_empty() {
            let buffer = data.as_mut_ptr() as *mut c_void;
            // SAFETY: the buffer is as long as PDFium is told.
            unsafe { pdfium.FPDFImageObj_GetImageDataDecoded(image, buffer, length) };
        }
        data
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

    /// Whether the sequence has a property with this key.
    pub fn has_property(&self, key: &str) -> bool {
        // 0 is PDFium's `FPDF_OBJECT_UNKNOWN`, for a property that is not
        // there.
        unsafe {
            self.pdfium
                .FPDFPageObjMark_GetParamValueType(self.handle, key)
                != 0
        }
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

const IDENTITY_MATRIX: FS_MATRIX = FS_MATRIX {
    a: 1.0,
    b: 0.0,
    c: 0.0,
    d: 1.0,
    e: 0.0,
    f: 0.0,
};

/// Red, green, blue and opacity.
const OPAQUE_BLACK: [c_uint; 4] = [0, 0, 0, u8::MAX as c_uint];

fn color([red, green, blue, alpha]: [c_uint; 4]) -> Color {
    Color {
        red: red as u8,
        green: green as u8,
        blue: blue as u8,
        alpha: alpha as u8,
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
