//! The text of a page.

use std::collections::HashMap;
use std::ffi::{c_int, c_uint, c_ulong, c_void};

use pdfium_render::prelude::{FPDF_PAGE, FPDF_PAGEOBJECT, FPDF_TEXTPAGE, FS_MATRIX, FS_RECTF};

use super::library::Library;
use super::page_space::PageSpace;
use super::strings;
use crate::geometry::Rect;
use crate::reader::{Char, Color, Span, Style, TextItem};

/// What PDFium reports in place of a hyphen that ends a line inside a word.
const LINE_END_HYPHEN_MARKER: u32 = 0x2;

/// The font descriptor flag of an italic font.
const ITALIC_FONT_FLAG: c_int = 1 << 6;

const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

/// # Safety
///
/// `page` must be a loaded page.
pub(super) unsafe fn read(
    pdfium: &Library,
    page: FPDF_PAGE,
    page_space: &PageSpace,
    structure_paths: &HashMap<c_int, String>,
) -> (Vec<Span>, Vec<TextItem>) {
    // SAFETY: the text page is only used before it is closed.
    unsafe {
        let text_page = pdfium.FPDFText_LoadPage(page);
        if text_page.is_null() {
            return (Vec::new(), Vec::new());
        }
        let mut reader = TextReader {
            pdfium,
            text_page,
            page_space,
            structure_paths,
            spans: Vec::new(),
            span_by_object: HashMap::new(),
        };
        let text = reader.read_items();
        pdfium.FPDFText_ClosePage(text_page);
        (reader.spans, text)
    }
}

struct TextReader<'a> {
    pdfium: &'a Library,
    text_page: FPDF_TEXTPAGE,
    page_space: &'a PageSpace,
    structure_paths: &'a HashMap<c_int, String>,
    spans: Vec<Span>,
    span_by_object: HashMap<FPDF_PAGEOBJECT, usize>,
}

/// Every method requires that `text_page` is still open.
impl TextReader<'_> {
    unsafe fn read_items(&mut self) -> Vec<TextItem> {
        // SAFETY: indices stay below the character count.
        unsafe {
            let count = self.pdfium.FPDFText_CountChars(self.text_page).max(0);
            let mut items = Vec::with_capacity(count as usize);
            let mut index = 0;
            while index < count {
                let (code, code_unit_count) = self.read_code_point(index, count);
                if let Some(item) = self.read_item(index, code) {
                    items.push(item);
                }
                index += code_unit_count;
            }
            items
        }
    }

    /// The code point at `index`, and how many of PDFium's characters it takes.
    ///
    /// PDFium reports a character outside the Basic Multilingual Plane as two
    /// surrogates where its `wchar_t` is 16 bits wide.
    unsafe fn read_code_point(&self, index: c_int, count: c_int) -> (u32, c_int) {
        // SAFETY: `index` and `index + 1` are checked against `count`.
        unsafe {
            let code = self.pdfium.FPDFText_GetUnicode(self.text_page, index);
            if (0xD800..0xDC00).contains(&code) && index + 1 < count {
                let low = self.pdfium.FPDFText_GetUnicode(self.text_page, index + 1);
                if (0xDC00..0xE000).contains(&low) {
                    return (0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00), 2);
                }
            }
            (code, 1)
        }
    }

    unsafe fn read_item(&mut self, index: c_int, code: u32) -> Option<TextItem> {
        // SAFETY: `index` is a valid character index.
        unsafe {
            let is_line_end_hyphen = code == LINE_END_HYPHEN_MARKER
                && self.pdfium.FPDFText_IsHyphen(self.text_page, index) != 0;
            let text = if is_line_end_hyphen {
                '-'
            } else {
                match char::from_u32(code) {
                    Some('\r') => return None,
                    Some('\n') => return Some(TextItem::LineBreak),
                    Some(character) if character.is_whitespace() => return Some(TextItem::Space),
                    Some(character) if is_displayable(character) => character,
                    _ => REPLACEMENT_CHARACTER,
                }
            };
            Some(TextItem::Char(Char {
                text,
                bounds: self.read_bounds(index),
                span: self.span_of(index),
                is_line_end_hyphen,
            }))
        }
    }

    unsafe fn read_bounds(&self, index: c_int) -> Rect {
        let mut bounds = FS_RECTF {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        // SAFETY: `index` is a valid character index and `bounds` outlives the call.
        unsafe {
            self.pdfium
                .FPDFText_GetLooseCharBox(self.text_page, index, &mut bounds);
        }
        self.page_space.rect(
            bounds.left.into(),
            bounds.bottom.into(),
            bounds.right.into(),
            bounds.top.into(),
        )
    }

    /// The index of the span of the text object that the character belongs to.
    unsafe fn span_of(&mut self, index: c_int) -> usize {
        // SAFETY: `index` is a valid character index.
        unsafe {
            let object = self.pdfium.FPDFText_GetTextObject(self.text_page, index);
            if let Some(&span) = self.span_by_object.get(&object) {
                return span;
            }
            let span = self.read_span(index, object);
            self.spans.push(span);
            self.span_by_object.insert(object, self.spans.len() - 1);
            self.spans.len() - 1
        }
    }

    unsafe fn read_span(&self, index: c_int, object: FPDF_PAGEOBJECT) -> Span {
        // SAFETY: `index` is a valid character index, and `object` is the
        // text object of that character or null.
        unsafe {
            let style = self.read_style(index);
            if object.is_null() {
                return Span {
                    style,
                    is_artifact: false,
                    structure_path: None,
                    actual_text: None,
                };
            }
            let marks = self.read_marks(object);
            let marked_content_id = self.pdfium.FPDFPageObj_GetMarkedContentID(object);
            Span {
                style,
                is_artifact: marks.is_artifact,
                structure_path: self.structure_paths.get(&marked_content_id).cloned(),
                actual_text: marks.actual_text,
            }
        }
    }

    unsafe fn read_marks(&self, object: FPDF_PAGEOBJECT) -> Marks {
        let mut marks = Marks::default();
        // SAFETY: `object` is a live page object and mark indices stay below the count.
        unsafe {
            for index in 0..self.pdfium.FPDFPageObj_CountMarks(object).max(0) {
                let mark = self.pdfium.FPDFPageObj_GetMark(object, index as c_ulong);
                let name = strings::utf16(|buffer, byte_capacity| {
                    let mut byte_length = 0;
                    let found = self.pdfium.FPDFPageObjMark_GetName(
                        mark,
                        buffer,
                        byte_capacity,
                        &mut byte_length,
                    );
                    (found != 0).then_some(byte_length)
                });
                marks.is_artifact |= name.as_deref() == Some("Artifact");
                // PDFium's string getter misreads UTF-16 strings, so decode the bytes here.
                let actual_text = strings::pdf_text(|buffer, byte_capacity| {
                    let mut byte_length = 0;
                    let found = self.pdfium.FPDFPageObjMark_GetParamBlobValue(
                        mark,
                        "ActualText",
                        buffer,
                        byte_capacity,
                        &mut byte_length,
                    );
                    (found != 0).then_some(byte_length)
                });
                // An inner mark, which comes later, overrides an outer one.
                marks.actual_text = actual_text.or(marks.actual_text);
            }
        }
        marks
    }

    unsafe fn read_style(&self, index: c_int) -> Style {
        // SAFETY: `index` is a valid character index and every out parameter
        // outlives its call.
        unsafe {
            let mut font_flags: c_int = 0;
            let font_name = strings::utf8(|buffer, byte_capacity| {
                self.pdfium.FPDFText_GetFontInfo(
                    self.text_page,
                    index,
                    buffer as *mut c_void,
                    byte_capacity,
                    &mut font_flags,
                )
            });
            let font_weight = self.pdfium.FPDFText_GetFontWeight(self.text_page, index);
            let (mut red, mut green, mut blue, mut alpha): (c_uint, c_uint, c_uint, c_uint) =
                (0, 0, 0, u8::MAX.into());
            self.pdfium.FPDFText_GetFillColor(
                self.text_page,
                index,
                &mut red,
                &mut green,
                &mut blue,
                &mut alpha,
            );
            let rotation_radians = self.pdfium.FPDFText_GetCharAngle(self.text_page, index);
            Style {
                font_name: without_subset_tag(&font_name).to_owned(),
                font_size: self.read_font_size(index),
                font_weight: u16::try_from(font_weight).ok(),
                is_italic: font_flags & ITALIC_FONT_FLAG != 0,
                fill_color: Color {
                    red: red as u8,
                    green: green as u8,
                    blue: blue as u8,
                    alpha: alpha as u8,
                },
                rotation_degrees: rotation_radians.max(0.0).to_degrees(),
            }
        }
    }

    /// The font size as shown: the size the text object asks for, scaled by
    /// the object's matrix.
    unsafe fn read_font_size(&self, index: c_int) -> f32 {
        let mut matrix = FS_MATRIX {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        };
        // SAFETY: `index` is a valid character index and `matrix` outlives the call.
        unsafe {
            self.pdfium
                .FPDFText_GetMatrix(self.text_page, index, &mut matrix);
            let nominal_size = self.pdfium.FPDFText_GetFontSize(self.text_page, index) as f32;
            nominal_size * matrix.c.hypot(matrix.d)
        }
    }
}

#[derive(Default)]
struct Marks {
    is_artifact: bool,
    actual_text: Option<String>,
}

/// Whether a viewer shows something for the character.
fn is_displayable(character: char) -> bool {
    let is_noncharacter = (character as u32) & 0xFFFE == 0xFFFE;
    !character.is_control() && !is_noncharacter
}

/// Strips the `ABCDEF+` tag that marks a font as a subset.
fn without_subset_tag(font_name: &str) -> &str {
    match font_name.split_once('+') {
        Some((tag, name))
            if tag.len() == 6 && tag.bytes().all(|byte| byte.is_ascii_uppercase()) =>
        {
            name
        }
        _ => font_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subset_tag_is_stripped() {
        assert_eq!(
            without_subset_tag("YLHLOX+LibertinusSerif-Regular"),
            "LibertinusSerif-Regular"
        );
    }

    #[test]
    fn other_plus_signs_are_kept() {
        assert_eq!(without_subset_tag("Noto+Sans"), "Noto+Sans");
        assert_eq!(without_subset_tag("abcdef+Font"), "abcdef+Font");
    }

    #[test]
    fn control_and_noncharacters_are_not_displayable() {
        assert!(is_displayable('a'));
        assert!(!is_displayable('\u{2}'));
        assert!(!is_displayable('\u{FFFE}'));
    }
}
