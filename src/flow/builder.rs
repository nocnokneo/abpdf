//! Splitting the text of pages into tokens.

use std::collections::HashMap;
use std::mem;

use unicode_segmentation::UnicodeSegmentation;

use super::hyphen_marks::{self, HyphenMarks};
use super::{Flow, Fragment, Gap, HyphenKind, LineEndHyphen, Token};
use crate::geometry::{Rect, Size};
use crate::reader::{Char, Color, Page, Style, TextItem};

const SOFT_HYPHEN: char = '\u{AD}';

/// Characters that a viewer does not show.
/// Zero width joiners are not among them, because they change how their
/// neighbors are shown.
const INVISIBLE_CHARACTERS: [char; 3] = ['\u{200B}', '\u{2060}', '\u{FEFF}'];

#[derive(Default)]
pub struct FlowBuilder {
    tokens: Vec<Token>,
    styles: StyleTable,
    structure_paths: StructurePathTable,
    page_sizes: Vec<Size>,
    hyphen_marks: HyphenMarks,
}

impl FlowBuilder {
    /// Adds the next page of the document.
    pub fn add_page(&mut self, page: &Page) {
        let page_index = self.page_sizes.len();
        self.page_sizes.push(page.size);
        PageTokenizer {
            builder: self,
            page,
            page_index,
            line_index: 0,
            gap: Gap::LineBreak,
            run: Vec::new(),
            trailing_soft_hyphen: None,
            span_properties: vec![None; page.spans.len()],
        }
        .tokenize();
    }

    pub fn finish(mut self) -> Flow {
        self.hyphen_marks.resolve_unmarked(&mut self.tokens);
        Flow {
            tokens: self.tokens,
            styles: self.styles.styles,
            structure_paths: self.structure_paths.paths,
            page_sizes: self.page_sizes,
        }
    }
}

/// A hyphen that ends the current run and the line.
struct RunEndHyphen {
    character: Char,
    /// `None` if the producer did not mark the hyphen.
    marked_kind: Option<HyphenKind>,
}

#[derive(Clone, Copy)]
struct SpanProperties {
    style: usize,
    structure_path: Option<usize>,
}

struct PageTokenizer<'a> {
    builder: &'a mut FlowBuilder,
    page: &'a Page,
    page_index: usize,
    line_index: usize,
    /// The white space since the last token.
    gap: Gap,
    /// The characters since the last white space.
    run: Vec<Char>,
    /// A soft hyphen after `run`, which is only shown if the line ends there.
    trailing_soft_hyphen: Option<Char>,
    /// The interned properties of each of the page's spans, once looked up.
    span_properties: Vec<Option<SpanProperties>>,
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
        let first = &characters[0];
        let properties = self.span_properties(first.span);
        let bounds = characters
            .iter()
            .map(|character| character.bounds)
            .reduce(|bounds, next| bounds.union(&next))
            .expect("a token has characters");
        self.builder.tokens.push(Token {
            text: text.to_owned(),
            fragments: vec![self.fragment(bounds)],
            gap: mem::replace(&mut self.gap, Gap::None),
            style: properties.style,
            is_artifact: self.page.spans[first.span].is_artifact,
            structure_path: properties.structure_path,
            exclusion: None,
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

    fn span_properties(&mut self, span_index: usize) -> SpanProperties {
        if let Some(properties) = self.span_properties[span_index] {
            return properties;
        }
        let span = &self.page.spans[span_index];
        let properties = SpanProperties {
            style: self.builder.styles.intern(&span.style),
            structure_path: span
                .structure_path
                .as_deref()
                .map(|path| self.builder.structure_paths.intern(path)),
        };
        self.span_properties[span_index] = Some(properties);
        properties
    }
}

#[derive(Default)]
struct StyleTable {
    styles: Vec<Style>,
    index_by_key: HashMap<StyleKey, usize>,
}

/// The fields of a [`Style`], with floats as their bits so that they can be hashed.
type StyleKey = (String, u32, Option<u16>, bool, Color, u32);

impl StyleTable {
    fn intern(&mut self, style: &Style) -> usize {
        // Naming every field keeps the key in step with `Style`.
        let Style {
            font_name,
            font_size,
            font_weight,
            is_italic,
            fill_color,
            rotation_degrees,
        } = style;
        let key = (
            font_name.clone(),
            font_size.to_bits(),
            *font_weight,
            *is_italic,
            *fill_color,
            rotation_degrees.to_bits(),
        );
        *self.index_by_key.entry(key).or_insert_with(|| {
            self.styles.push(style.clone());
            self.styles.len() - 1
        })
    }
}

#[derive(Default)]
struct StructurePathTable {
    paths: Vec<String>,
    index_by_path: HashMap<String, usize>,
}

impl StructurePathTable {
    fn intern(&mut self, path: &str) -> usize {
        if let Some(&index) = self.index_by_path.get(path) {
            return index;
        }
        self.paths.push(path.to_owned());
        self.index_by_path
            .insert(path.to_owned(), self.paths.len() - 1);
        self.paths.len() - 1
    }
}
