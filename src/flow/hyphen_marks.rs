//! What a producer says about the hyphens that end its lines.
//!
//! A producer that draws both kinds of hyphen with one glyph can map the
//! glyph to only one character, and has to name the other kind with
//! `ActualText` wherever it occurs.
//! So in a font with marked hard hyphens an unmarked hyphen is soft, and the
//! other way around.

use std::collections::HashMap;

use super::{HyphenKind, Token};
use crate::reader::Span;

/// The kind that the producer names for the hyphen that a span draws, if it
/// names one.
pub(super) fn marked_kind(span: &Span) -> Option<HyphenKind> {
    match span.actual_text.as_deref()? {
        "\u{AD}" => Some(HyphenKind::Soft),
        "-" | "\u{2010}" => Some(HyphenKind::Hard),
        _ => None,
    }
}

/// The marked hyphens of a document by font, and its line-end hyphens that
/// have no mark.
#[derive(Default)]
pub(super) struct HyphenMarks {
    by_font: HashMap<String, FontMarks>,
    unmarked: Vec<UnmarkedHyphen>,
}

#[derive(Default)]
struct FontMarks {
    has_marked_soft: bool,
    has_marked_hard: bool,
}

struct UnmarkedHyphen {
    token_index: usize,
    font_name: String,
}

impl HyphenMarks {
    /// Records the mark of the hyphen that a span draws, if it has one.
    pub fn note(&mut self, span: &Span) {
        let Some(kind) = marked_kind(span) else {
            return;
        };
        let font_name = &span.style.font_name;
        let marks = match self.by_font.get_mut(font_name) {
            Some(marks) => marks,
            None => self.by_font.entry(font_name.clone()).or_default(),
        };
        match kind {
            HyphenKind::Soft => marks.has_marked_soft = true,
            HyphenKind::Hard => marks.has_marked_hard = true,
            HyphenKind::Unknown => {}
        }
    }

    /// Records that the line-end hyphen after a token, which `span` draws,
    /// has no mark.
    pub fn note_unmarked(&mut self, token_index: usize, span: &Span) {
        self.unmarked.push(UnmarkedHyphen {
            token_index,
            font_name: span.style.font_name.clone(),
        });
    }

    /// Gives each unmarked hyphen the kind that follows from the marked
    /// hyphens of its font, where one does.
    pub fn resolve_unmarked(&self, tokens: &mut [Token]) {
        for hyphen in &self.unmarked {
            let marks = self.by_font.get(&hyphen.font_name);
            let kind = marks.map_or(HyphenKind::Unknown, FontMarks::kind_of_unmarked);
            if let Some(line_end_hyphen) = &mut tokens[hyphen.token_index].line_end_hyphen {
                line_end_hyphen.kind = kind;
            }
        }
    }
}

impl FontMarks {
    fn kind_of_unmarked(&self) -> HyphenKind {
        match (self.has_marked_soft, self.has_marked_hard) {
            (false, true) => HyphenKind::Soft,
            (true, false) => HyphenKind::Hard,
            _ => HyphenKind::Unknown,
        }
    }
}
