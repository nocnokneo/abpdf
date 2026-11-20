//! Splitting the text of pages into tokens.

use std::mem;

use unicode_segmentation::UnicodeSegmentation;

use super::hyphen_marks::{self, HyphenMarks};
use super::{Cell, Fragment, Gap, HyphenKind, LineEndHyphen, RawFlow, RawToken};
use crate::geometry::{Rect, Size};
use crate::reader::{Char, Page, Style, TextItem};

const SOFT_HYPHEN: char = '\u{AD}';

/// Characters that a viewer does not show.
/// Zero width joiners are not among them, because they change how their
/// neighbors are shown.
const INVISIBLE_CHARACTERS: [char; 3] = ['\u{200B}', '\u{2060}', '\u{FEFF}'];

#[derive(Default)]
pub struct FlowBuilder {
    tokens: Vec<RawToken>,
    page_sizes: Vec<Size>,
    styles: Vec<Style>,
    hyphen_marks: HyphenMarks,
}

impl FlowBuilder {
    /// Adds the next page of the document.
    pub fn add_page(&mut self, page: &Page) {
        let page_index = self.page_sizes.len();
        self.page_sizes.push(page.size);
        let span_styles = page.spans.iter().map(|span| self.style_index(&span.style));
        PageTokenizer {
            span_styles: span_styles.collect(),
            builder: self,
            page,
            page_index,
            line_index: 0,
            gap: Gap::LineBreak,
            run: Vec::new(),
            trailing_soft_hyphen: None,
        }
        .tokenize();
    }

    pub fn finish(mut self) -> RawFlow {
        self.hyphen_marks.resolve_unmarked(&mut self.tokens);
        RawFlow {
            tokens: self.tokens,
            page_sizes: self.page_sizes,
            styles: self.styles,
        }
    }

    /// The index of a style among the styles of the document.
    fn style_index(&mut self, style: &Style) -> usize {
        let known = self.styles.iter().position(|known| known == style);
        known.unwrap_or_else(|| {
            self.styles.push(style.clone());
            self.styles.len() - 1
        })
    }
}

/// A hyphen that ends the current run and the line.
struct RunEndHyphen {
    character: Char,
    /// `None` if the producer did not mark the hyphen.
    marked_kind: Option<HyphenKind>,
}

struct PageTokenizer<'a> {
    builder: &'a mut FlowBuilder,
    page: &'a Page,
    /// The index among the styles of the document of the style of each span
    /// of the page.
    span_styles: Vec<usize>,
    page_index: usize,
    line_index: usize,
    /// The white space since the last token.
    gap: Gap,
    /// The characters since the last white space.
    run: Vec<Char>,
    /// A soft hyphen after `run`, which is only shown if the line ends there.
    trailing_soft_hyphen: Option<Char>,
}

impl PageTokenizer<'_> {
    fn tokenize(mut self) {
        for item in &self.page.text {
            match item {
                TextItem::Char(character) => self.add_char(character),
                TextItem::Space => {
                    self.end_run(None);
                    self.gap = self.gap.max(Gap::Space);
                }
                TextItem::LineBreak => {
                    self.end_run(None);
                    self.gap = Gap::LineBreak;
                    self.line_index += 1;
                }
            }
        }
        self.end_run(None);
    }

    fn add_char(&mut self, character: &Char) {
        let page = self.page;
        let span = &page.spans[character.span];
        self.builder.hyphen_marks.note(span);

        if character.is_line_end_hyphen && !self.run.is_empty() {
            self.end_run(Some(RunEndHyphen {
                character: *character,
                marked_kind: hyphen_marks::marked_kind(span),
            }));
            self.line_index += 1;
            return;
        }
        if character.text == SOFT_HYPHEN && !self.run.is_empty() {
            self.trailing_soft_hyphen = Some(*character);
            return;
        }
        if character.text == SOFT_HYPHEN || INVISIBLE_CHARACTERS.contains(&character.text) {
            return;
        }

        let changes_artifact_state = self
            .run
            .first()
            .is_some_and(|first| page.spans[first.span].is_artifact != span.is_artifact);
        if changes_artifact_state {
            self.end_run(None);
        }
        self.trailing_soft_hyphen = None;
        self.run.push(*character);
    }

    /// Turns the run into tokens.
    fn end_run(&mut self, hyphen: Option<RunEndHyphen>) {
        let hyphen = hyphen.or_else(|| {
            let character = self.trailing_soft_hyphen.take()?;
            Some(RunEndHyphen {
                character,
                marked_kind: Some(HyphenKind::Soft),
            })
        });
        let run = mem::take(&mut self.run);
        if run.is_empty() {
            return;
        }

        let text: String = run.iter().map(|character| character.text).collect();
        let mut remaining = run.as_slice();
        for segment in text.split_word_bounds() {
            let (characters, rest) = remaining.split_at(segment.chars().count());
            remaining = rest;
            self.push_token(segment, characters);
        }
        if let Some(hyphen) = hyphen {
            self.attach_hyphen(hyphen);
        }
    }

    fn push_token(&mut self, text: &str, characters: &[Char]) {
        let first_span = &self.page.spans[characters[0].span];
        let bounds = characters
            .iter()
            .map(|character| character.bounds)
            .reduce(|bounds, next| bounds.union(&next))
            .expect("a token has characters");
        let mut styles: Vec<usize> = characters
            .iter()
            .map(|character| self.span_styles[character.span])
            .collect();
        styles.dedup();
        self.builder.tokens.push(RawToken {
            text: text.to_owned(),
            fragment: self.fragment(bounds),
            gap: mem::replace(&mut self.gap, Gap::None),
            styles,
            is_artifact: first_span.is_artifact,
            cell: first_span.cell.map(|index| Cell {
                page_index: self.page_index,
                index,
            }),
            line_end_hyphen: None,
        });
    }

    fn attach_hyphen(&mut self, hyphen: RunEndHyphen) {
        let fragment = self.fragment(hyphen.character.bounds);
        let token_index = self.builder.tokens.len() - 1;
        self.builder.tokens[token_index].line_end_hyphen = Some(LineEndHyphen {
            kind: hyphen.marked_kind.unwrap_or(HyphenKind::Unknown),
            fragment,
        });
        if hyphen.marked_kind.is_none() {
            let span = &self.page.spans[hyphen.character.span];
            self.builder.hyphen_marks.note_unmarked(token_index, span);
        }
    }

    fn fragment(&self, bounds: Rect) -> Fragment {
        Fragment {
            page_index: self.page_index,
            line_index: self.line_index,
            bounds,
        }
    }
}
