//! Pages made by hand, for tests that need no PDF.

use std::ops::Range;

use crate::change::Comparison;
use crate::filter::Filters;
use crate::flow::{Flow, FlowBuilder};
use crate::geometry::{Rect, Size};
use crate::reader::{Char, Color, Page, Span, Style, TextItem};

pub(crate) const CELL_WIDTH: f32 = 10.0;
pub(crate) const CELL_HEIGHT: f32 = 12.0;
pub(crate) const LINE_PITCH: f32 = 20.0;

/// Builds a page on which every character takes one cell of a grid.
pub(crate) struct PageBuilder {
    page: Page,
    column: usize,
    line: usize,
}

impl PageBuilder {
    pub(crate) fn new() -> Self {
        Self {
            page: Page {
                size: Size {
                    width: 600.0,
                    height: 800.0,
                },
                spans: vec![body_span()],
                text: Vec::new(),
            },
            column: 0,
            line: 0,
        }
    }

    pub(crate) fn height(mut self, height: f32) -> Self {
        self.page.size.height = height;
        self
    }

    /// Adds text to the current span: a space is white space and a newline
    /// is a line break.
    pub(crate) fn text(mut self, text: &str) -> Self {
        for character in text.chars() {
            match character {
                ' ' => {
                    self.page.text.push(TextItem::Space);
                    self.column += 1;
                }
                '\n' => {
                    self.page.text.push(TextItem::LineBreak);
                    self.next_line();
                }
                _ => self.push_char(character, false),
            }
        }
        self
    }

    /// Starts a span that differs from body text as `change` says.
    pub(crate) fn span(mut self, change: impl FnOnce(&mut Span)) -> Self {
        let mut span = body_span();
        change(&mut span);
        self.page.spans.push(span);
        self
    }

    pub(crate) fn body(self) -> Self {
        self.span(|_| {})
    }

    pub(crate) fn artifact(self) -> Self {
        self.span(|span| span.is_artifact = true)
    }

    pub(crate) fn actual_text(self, actual_text: &str) -> Self {
        self.span(|span| span.actual_text = Some(actual_text.to_owned()))
    }

    /// Adds a hyphen that ends the line inside a word, the way a reader
    /// reports it: with no line break after it.
    pub(crate) fn line_end_hyphen(mut self) -> Self {
        self.push_char('-', true);
        self.next_line();
        self
    }

    fn push_char(&mut self, text: char, is_line_end_hyphen: bool) {
        let left = self.column as f32 * CELL_WIDTH;
        let top = self.line as f32 * LINE_PITCH;
        self.page.text.push(TextItem::Char(Char {
            text,
            bounds: Rect {
                left,
                top,
                right: left + CELL_WIDTH,
                bottom: top + CELL_HEIGHT,
            },
            span: self.page.spans.len() - 1,
            is_line_end_hyphen,
        }));
        self.column += 1;
    }

    fn next_line(&mut self) {
        self.column = 0;
        self.line += 1;
    }

    pub(crate) fn build(self) -> Page {
        self.page
    }
}

fn body_span() -> Span {
    Span {
        style: Style {
            font_name: "Body".to_owned(),
            font_size: 10.0,
            font_weight: Some(400),
            is_italic: false,
            fill_color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            rotation_degrees: 0.0,
        },
        is_artifact: false,
        structure_path: None,
        actual_text: None,
    }
}

/// The flow of a document with these pages, before any token is excluded or
/// joined.
pub(crate) fn raw_flow_of(pages: impl IntoIterator<Item = PageBuilder>) -> Flow {
    let mut builder = FlowBuilder::default();
    for page in pages {
        builder.add_page(&page.build());
    }
    builder.finish()
}

/// The comparison of two documents of one page each, with nothing left out.
pub(crate) fn comparison_of(old_text: &str, new_text: &str) -> Comparison {
    comparison_of_pages(&[old_text], &[new_text])
}

/// The comparison of two documents, given as the text of each of their
/// pages, with nothing left out.
pub(crate) fn comparison_of_pages(
    old_pages: &[impl AsRef<str>],
    new_pages: &[impl AsRef<str>],
) -> Comparison {
    fn flow(pages: &[impl AsRef<str>]) -> Flow {
        raw_flow_of(
            pages
                .iter()
                .map(|text| PageBuilder::new().text(text.as_ref())),
        )
    }
    Comparison::of(flow(old_pages), flow(new_pages), &Filters::default())
}

/// The bounds of the cells from `columns` on `line` of a page.
pub(crate) fn cell_bounds(line: usize, columns: Range<usize>) -> Rect {
    let top = line as f32 * LINE_PITCH;
    Rect {
        left: columns.start as f32 * CELL_WIDTH,
        top,
        right: columns.end as f32 * CELL_WIDTH,
        bottom: top + CELL_HEIGHT,
    }
}

pub(crate) fn texts(flow: &Flow) -> Vec<&str> {
    flow.tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect()
}

pub(crate) fn compared_texts(flow: &Flow) -> Vec<&str> {
    let compared = flow.tokens.iter().filter(|token| !token.is_excluded());
    compared.map(|token| token.text.as_str()).collect()
}
