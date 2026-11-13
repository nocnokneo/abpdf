//! The text of a page.

use std::collections::HashMap;
use std::ffi::c_int;

use super::handles::{LoadedPage, TextObject, TextPage};
use super::page_space::PageSpace;
use crate::reader::{Char, Span, Style, TextItem};

/// What PDFium reports in place of a hyphen that ends a line inside a word.
const LINE_END_HYPHEN_MARKER: u32 = 0x2;

/// The font descriptor flag of an italic font.
const ITALIC_FONT_FLAG: c_int = 1 << 6;

const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

/// The spans and the text of a page.
/// `structure_paths` has the structure path of marked content by its
/// identifier.
pub(super) fn read(
    page: &LoadedPage,
    page_space: &PageSpace,
    structure_paths: &HashMap<c_int, String>,
) -> (Vec<Span>, Vec<TextItem>) {
    let Some(text_page) = page.text() else {
        return (Vec::new(), Vec::new());
    };
    let mut reader = TextReader {
        text_page: &text_page,
        page_space,
        structure_paths,
        spans: Vec::new(),
        span_by_object: HashMap::new(),
    };
    let text = reader.read_items();
    (reader.spans, text)
}

struct TextReader<'a> {
    text_page: &'a TextPage<'a>,
    page_space: &'a PageSpace,
    structure_paths: &'a HashMap<c_int, String>,
    spans: Vec<Span>,
    /// The span of each text object by its identity, and of the characters
    /// that no object draws under `None`.
    span_by_object: HashMap<Option<usize>, usize>,
}

impl TextReader<'_> {
    fn read_items(&mut self) -> Vec<TextItem> {
        let count = self.text_page.char_count();
        let mut items = Vec::with_capacity(count as usize);
        let mut index = 0;
        while index < count {
            let (code, code_unit_count) = self.read_code_point(index);
            items.extend(self.read_item(index, code));
            index += code_unit_count;
        }
        items
    }

    /// The code point at `index`, and how many of PDFium's characters it takes.
    ///
    /// PDFium reports a character outside the Basic Multilingual Plane as two
    /// surrogates where its `wchar_t` is 16 bits wide.
    fn read_code_point(&self, index: c_int) -> (u32, c_int) {
        let code = self.text_page.unicode(index);
        if (0xD800..0xDC00).contains(&code) {
            let low = self.text_page.unicode(index + 1);
            if (0xDC00..0xE000).contains(&low) {
                return (0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00), 2);
            }
        }
        (code, 1)
    }

    fn read_item(&mut self, index: c_int, code: u32) -> Option<TextItem> {
        let is_line_end_hyphen = code == LINE_END_HYPHEN_MARKER && self.text_page.is_hyphen(index);
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
            bounds: self.page_space.rect(self.text_page.loose_char_box(index)),
            span: self.span_of(index),
            is_line_end_hyphen,
        }))
    }

    /// The index of the span of the text object that draws the character.
    fn span_of(&mut self, index: c_int) -> usize {
        let object = self.text_page.text_object(index);
        let identity = object.map(|object| object.identity());
        if let Some(&span) = self.span_by_object.get(&identity) {
            return span;
        }
        self.spans.push(self.read_span(index, object));
        self.span_by_object.insert(identity, self.spans.len() - 1);
        self.spans.len() - 1
    }

    fn read_span(&self, index: c_int, object: Option<TextObject>) -> Span {
        let mut span = Span {
            style: self.read_style(index),
            is_artifact: false,
            structure_path: None,
            actual_text: None,
        };
        let Some(object) = object else {
            return span;
        };
        for mark in object.marks() {
            span.is_artifact |= mark.name().as_deref() == Some("Artifact");
            // An inner mark, which comes later, overrides an outer one.
            span.actual_text = mark.text_property("ActualText").or(span.actual_text);
        }
        span.structure_path = object
            .marked_content_id()
            .and_then(|id| self.structure_paths.get(&id).cloned());
        span
    }

    fn read_style(&self, index: c_int) -> Style {
        let (font_name, font_flags) = self.text_page.font(index);
        // The size as shown: the one the text object asks for, scaled by
        // its matrix.
        let matrix = self.text_page.matrix(index);
        let font_size = self.text_page.nominal_font_size(index) * matrix.c.hypot(matrix.d);
        Style {
            font_name: without_subset_tag(&font_name).to_owned(),
            font_size,
            font_weight: self.text_page.font_weight(index),
            is_italic: font_flags & ITALIC_FONT_FLAG != 0,
            fill_color: self.text_page.fill_color(index),
            rotation_degrees: self.text_page.rotation_radians(index).to_degrees(),
        }
    }
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
