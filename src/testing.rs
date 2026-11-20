//! Pages made by hand, for tests that need no PDF.

use std::ops::Range;

use crate::BySide;
use crate::change::{Comparison, StyleDifference};
use crate::filter::Filters;
use crate::flow::{Flow, FlowBuilder, RawFlow};
use crate::geometry::{Rect, Size};
use crate::reader::{Char, Color, Figure, FigureKind, Page, Span, Style, TextItem};

pub(crate) const CELL_WIDTH: f32 = 10.0;
pub(crate) const CELL_HEIGHT: f32 = 12.0;
pub(crate) const LINE_PITCH: f32 = 20.0;

/// A paragraph of six tokens, for tests in which text moves.
pub(crate) const PARAGRAPH: &str = "this paragraph stands somewhere else now";
/// Ten tokens, so more than the paragraph, which a diff therefore takes for
/// the text that stays in place where the two change places.
pub(crate) const REST: &str = "all the other text of the document stays in place";

/// How many cells of a line a figure of a test page takes.
pub(crate) const FIGURE_COLUMN_COUNT: usize = 5;
/// What a change says in the place of a figure of a test page.
pub(crate) const FIGURE_WORDS: &str = "[image, 4 × 3 pixels]";

pub(crate) const BODY_WEIGHT: u16 = 400;
pub(crate) const BOLD_WEIGHT: u16 = 700;

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
                figures: Vec::new(),
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

    /// Starts a span in the table cell with this number.
    pub(crate) fn cell(self, cell: usize) -> Self {
        self.span(|span| span.cell = Some(cell))
    }

    /// Starts a span in a style that differs from that of body text as
    /// `change` says.
    pub(crate) fn styled(self, change: impl FnOnce(&mut Style)) -> Self {
        self.span(|span| change(&mut span.style))
    }

    pub(crate) fn bold(self) -> Self {
        self.styled(|style| style.font_weight = Some(BOLD_WEIGHT))
    }

    pub(crate) fn actual_text(self, actual_text: &str) -> Self {
        self.span(|span| span.actual_text = Some(actual_text.to_owned()))
    }

    /// Adds a figure where the next character would be: an image that is
    /// [`FIGURE_COLUMN_COUNT`] cells wide, and differs from the image that
    /// this adds elsewhere as `change` says.
    pub(crate) fn figure(mut self, change: impl FnOnce(&mut Figure)) -> Self {
        let columns = self.column..self.column + FIGURE_COLUMN_COUNT;
        let bounds = cell_bounds(self.line, columns);
        let mut figure = Figure {
            bounds,
            kind: FigureKind::Image {
                pixel_width: 4,
                pixel_height: 3,
            },
            is_artifact: false,
            content_hash: 0,
            points: vec![
                (bounds.left, bounds.top),
                (bounds.right, bounds.top),
                (bounds.right, bounds.bottom),
                (bounds.left, bounds.bottom),
            ],
        };
        change(&mut figure);
        self.page.figures.push(figure);
        let index = self.page.figures.len() - 1;
        self.page.text.push(TextItem::Figure(index));
        self.column += FIGURE_COLUMN_COUNT;
        self
    }

    /// Adds the figure that [`Self::figure`] adds unchanged.
    pub(crate) fn image(self) -> Self {
        self.figure(|_| {})
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
            font_weight: Some(BODY_WEIGHT),
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
        cell: None,
        actual_text: None,
    }
}

/// The flow of a document with these pages, before any token is excluded or
/// joined.
pub(crate) fn raw_flow_of(pages: impl IntoIterator<Item = PageBuilder>) -> RawFlow {
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
    fn built(pages: &[impl AsRef<str>]) -> impl Iterator<Item = PageBuilder> {
        let texts = pages.iter();
        texts.map(|text| PageBuilder::new().text(text.as_ref()))
    }
    comparison_of_built_pages(built(old_pages), built(new_pages))
}

/// The comparison of two documents with these pages, with nothing left out.
pub(crate) fn comparison_of_built_pages(
    old_pages: impl IntoIterator<Item = PageBuilder>,
    new_pages: impl IntoIterator<Item = PageBuilder>,
) -> Comparison {
    let flows = BySide {
        old: raw_flow_of(old_pages),
        new: raw_flow_of(new_pages),
    };
    Comparison::of(flows, &Filters::default())
}

/// A page with this text, in which what stands between two `*` is bold.
pub(crate) fn page_with_bold(text: &str) -> PageBuilder {
    let mut page = PageBuilder::new();
    for (index, part) in text.split('*').enumerate() {
        page = match index % 2 {
            0 => page.body(),
            _ => page.bold(),
        };
        page = page.text(part);
    }
    page
}

/// A page with a line of text and under it a figure, which differs from
/// that of [`PageBuilder::image`] as `change` says.
pub(crate) fn page_with_figure(change: impl FnOnce(&mut Figure)) -> PageBuilder {
    PageBuilder::new().text("one\n").figure(change)
}

/// What differs in the style of text that [`page_with_bold`] makes bold.
pub(crate) fn bold_difference() -> StyleDifference {
    StyleDifference::Weight(BySide {
        old: Some(BODY_WEIGHT),
        new: Some(BOLD_WEIGHT),
    })
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

pub(crate) fn excluded_texts(flow: &Flow) -> Vec<&str> {
    let tokens = flow.excluded.iter();
    tokens
        .map(|excluded| excluded.token.text.as_str())
        .collect()
}
