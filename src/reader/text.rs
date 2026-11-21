//! The text of a page.
//!
//! A page shows glyphs at places, and nothing in it says where a word or a
//! line ends: a producer may show a space as a glyph or as a gap, and
//! starts a line by showing text somewhere else.
//! Both are told here from where the glyphs stand.

use std::rc::Rc;

use super::content::{Content, Glyph, Mark, Marks, Object, TextRun};
use super::page_space::PageSpace;
use super::structure::PageStructure;
use super::{Char, Span, Style, TextItem};

/// How far apart glyphs on a line are that a space is between, as a share
/// of the font size.
/// It is more than fonts and producers move glyphs within a word, and less
/// than the narrowest space between words that justified text has.
const MIN_WORD_GAP_PER_FONT_SIZE: f64 = 0.125;

/// How far from the baseline of the text before it text is on another
/// line, as a share of the font size.
/// What is raised or lowered on a line stays under it, and the next line
/// of the most closely set text does not.
const MIN_LINE_SHIFT_PER_FONT_SIZE: f64 = 0.5;

/// How far from the baseline of the text before it text is raised or
/// lowered, as a share of the font size.
/// Glyphs of one word stand on one baseline, and the smallest step to an
/// index, an exponent or the mark of a footnote is more than this.
const MIN_RAISE_PER_FONT_SIZE: f64 = 0.1;

/// Directions that differ by more than about eight degrees are those of
/// different lines: this is the cosine of the angle between them.
const MIN_COSINE_OF_ONE_LINE: f64 = 0.99;

const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

/// The spans and the text of a page, with its figures where they stand in
/// the text.
///
/// `objects` are those that the page draws, in the order in which it draws
/// them.
/// `figure_objects` has, for each figure of the page in that order, the
/// index among `objects` of the first object that draws it.
pub(super) fn read(
    objects: &[Object],
    page_space: &PageSpace,
    structure: &PageStructure,
    figure_objects: &[usize],
) -> (Vec<Span>, Vec<TextItem>) {
    let mut runs = Vec::new();
    for (object_index, object) in objects.iter().enumerate() {
        gather_runs(object, object_index, &mut runs);
    }
    let mut reader = TextReader {
        page_space,
        figure_objects,
        spans: Vec::new(),
        items: Vec::new(),
        placed_figure_count: 0,
        last_glyph: None,
    };
    for runs_of_one_text in runs.chunk_by(|run, next| run.stands_for_the_same_text_as(next)) {
        let first_span = reader.spans.len();
        let spans = runs_of_one_text.iter().map(|run| run.span(structure));
        reader.spans.extend(spans);
        let glyphs: Vec<ShownGlyph> = runs_of_one_text
            .iter()
            .enumerate()
            .flat_map(|(index, shown)| {
                let glyphs = shown.run.glyphs.iter();
                glyphs.map(move |glyph| ShownGlyph {
                    glyph,
                    run: shown.run,
                    object_index: shown.object_index,
                    span: first_span + index,
                })
            })
            .collect();
        match runs_of_one_text[0].actual_text() {
            Some(text) => reader.read_as(text, &glyphs),
            None => reader.read_glyphs(&glyphs),
        }
    }
    reader.place_figures(figure_objects.len());
    (reader.spans, reader.items)
}

/// A run of text and where it is drawn.
struct ShownRun<'a> {
    run: &'a TextRun,
    /// The marks around the object that the run is.
    marks: &'a Marks,
    /// The index of the object of the page that the run is, or is inside
    /// of.
    object_index: usize,
}

/// Gathers the runs of text that an object is or has in it, which a form
/// does.
fn gather_runs<'a>(object: &'a Object, object_index: usize, runs: &mut Vec<ShownRun<'a>>) {
    match &object.content {
        Content::Text(run) => runs.push(ShownRun {
            run,
            marks: &object.marks,
            object_index,
        }),
        Content::Form(parts) => {
            for part in parts {
                gather_runs(part, object_index, runs);
            }
        }
        Content::Path(_) | Content::Image(_) | Content::Shading { .. } => {}
    }
}

impl ShownRun<'_> {
    /// The mark that says which text the run stands for, if the producer
    /// says that it is not what the glyphs map to.
    /// An empty text says nothing: a producer that wants glyphs left out
    /// marks them as an artifact.
    fn mark_with_actual_text(&self) -> Option<&Rc<Mark>> {
        let mark = self.marks.with_actual_text()?;
        let is_empty = mark.actual_text.as_deref().is_none_or(str::is_empty);
        (!is_empty).then_some(mark)
    }

    fn actual_text(&self) -> Option<&str> {
        self.mark_with_actual_text()?.actual_text.as_deref()
    }

    /// Whether the same mark says of both runs which text they stand for,
    /// which they then stand for together.
    fn stands_for_the_same_text_as(&self, other: &ShownRun) -> bool {
        match (self.mark_with_actual_text(), other.mark_with_actual_text()) {
            (Some(mark), Some(other_mark)) => Rc::ptr_eq(mark, other_mark),
            _ => false,
        }
    }

    fn span(&self, structure: &PageStructure) -> Span {
        let id = self.marks.marked_content_id();
        let structure = id.and_then(|id| structure.get(&id));
        let innermost_text = self.marks.with_actual_text();
        Span {
            style: style_of(self.run),
            is_artifact: self.marks.is_artifact(),
            structure_path: structure.map(|structure| structure.path.clone()),
            cell: structure.and_then(|structure| structure.cell),
            actual_text: innermost_text.and_then(|mark| mark.actual_text.clone()),
        }
    }
}

fn style_of(run: &TextRun) -> Style {
    let (along_x, along_y) = run.direction;
    // User space has its y axis up, so a line that goes down in it is
    // turned clockwise.
    let clockwise_degrees = (0.0 - along_y).atan2(along_x).to_degrees();
    Style {
        font_name: run.font.name.clone(),
        font_size: run.font_size,
        font_weight: run.font.weight,
        is_italic: run.font.is_italic,
        fill_color: run.fill_color,
        rotation_degrees: clockwise_degrees.rem_euclid(360.0) as f32,
    }
}

/// A glyph with what its run says about it.
#[derive(Clone, Copy)]
struct ShownGlyph<'a> {
    glyph: &'a Glyph,
    run: &'a TextRun,
    object_index: usize,
    /// The index of the span of the run among those of the page.
    span: usize,
}

struct TextReader<'a> {
    page_space: &'a PageSpace,
    figure_objects: &'a [usize],
    spans: Vec<Span>,
    items: Vec<TextItem>,
    /// How many of the figures stand in the text so far.
    placed_figure_count: usize,
    /// The glyph that was shown last.
    last_glyph: Option<ShownGlyph<'a>>,
}

impl<'a> TextReader<'a> {
    /// Reads glyphs as the text that they map to.
    fn read_glyphs(&mut self, glyphs: &[ShownGlyph<'a>]) {
        for shown in glyphs {
            self.separate_from_last_glyph(shown);
            for character in shown.glyph.text.chars() {
                self.add(character, shown);
            }
        }
    }

    /// Reads glyphs as a text that the producer says they stand for.
    /// The characters of the text take the places of the glyphs in turn,
    /// evenly.
    fn read_as(&mut self, text: &str, glyphs: &[ShownGlyph<'a>]) {
        let Some(first) = glyphs.first() else {
            return;
        };
        self.separate_from_last_glyph(first);
        let character_count = text.chars().count();
        for (index, character) in text.chars().enumerate() {
            self.add(character, &glyphs[index * glyphs.len() / character_count]);
        }
        self.last_glyph = glyphs.last().copied();
    }

    /// Adds the white space that there is between the glyph that was shown
    /// last and the next one, and makes that one the last.
    fn separate_from_last_glyph(&mut self, next: &ShownGlyph<'a>) {
        let last = self.last_glyph.replace(*next);
        self.items
            .extend(last.and_then(|last| separation(&last, next)));
    }

    /// Adds a character that stands where a glyph is shown.
    /// A ligature that Unicode has as one character is added as its
    /// letters.
    fn add(&mut self, character: char, shown: &ShownGlyph) {
        if let Some(letters) = letters_of_ligature(character) {
            for letter in letters.chars() {
                self.add(letter, shown);
            }
            return;
        }
        if character.is_whitespace() {
            self.items.push(TextItem::Space);
            return;
        }
        self.place_figures_before(shown.object_index);
        let text = match is_displayable(character) {
            true => character,
            false => REPLACEMENT_CHARACTER,
        };
        self.items.push(TextItem::Char(Char {
            text,
            bounds: self.page_space.rect(shown.glyph.bounds),
            span: shown.span,
        }));
    }

    /// Places the figures that stand before a character: those that the
    /// page starts to draw before the object of the character or with it,
    /// which is how a form draws the text inside it.
    fn place_figures_before(&mut self, object_index: usize) {
        let figure_count = self
            .figure_objects
            .partition_point(|first_object| *first_object <= object_index);
        self.place_figures(figure_count);
    }

    /// Places the figures that are not in the text yet, up to the one with
    /// the index `figure_count`.
    fn place_figures(&mut self, figure_count: usize) {
        let figures = self.placed_figure_count..figure_count;
        self.items.extend(figures.map(TextItem::Figure));
        self.placed_figure_count = self.placed_figure_count.max(figure_count);
    }
}

/// What separates two glyphs that a page shows one after the other, if
/// anything does.
///
/// The glyphs are on one line if they are written in one direction and
/// the second starts near the baseline of the first.
/// On a line, they are apart where the second
///
/// - starts well after the end of the first, which is a space,
/// - starts before the start of the first, as text does that starts over,
///   or
/// - is raised or lowered, as an index, an exponent and the mark of a
///   footnote are, which are no part of the word that they stand next to.
fn separation(last: &ShownGlyph, next: &ShownGlyph) -> Option<TextItem> {
    let font_size = f64::from(last.run.font_size.max(next.run.font_size));
    let (along_x, along_y) = last.run.direction;
    let along =
        |from: (f64, f64), to: (f64, f64)| (to.0 - from.0) * along_x + (to.1 - from.1) * along_y;
    let across =
        |from: (f64, f64), to: (f64, f64)| (to.1 - from.1) * along_x - (to.0 - from.0) * along_y;
    let cosine = along_x * next.run.direction.0 + along_y * next.run.direction.1;
    let line_shift = across(last.glyph.start, next.glyph.start).abs();
    if cosine < MIN_COSINE_OF_ONE_LINE || line_shift >= MIN_LINE_SHIFT_PER_FONT_SIZE * font_size {
        return Some(TextItem::LineBreak);
    }
    let min_word_gap = MIN_WORD_GAP_PER_FONT_SIZE * font_size;
    let is_after_gap = along(last.glyph.end, next.glyph.start) >= min_word_gap;
    let starts_over = along(last.glyph.start, next.glyph.start) <= -min_word_gap;
    let is_raised_or_lowered = line_shift >= MIN_RAISE_PER_FONT_SIZE * font_size;
    (is_after_gap || starts_over || is_raised_or_lowered).then_some(TextItem::Space)
}

/// The letters of a ligature of Latin letters that Unicode has as one
/// character, which is how some fonts say what the glyph of a ligature
/// stands for.
fn letters_of_ligature(character: char) -> Option<&'static str> {
    match character {
        '\u{FB00}' => Some("ff"),
        '\u{FB01}' => Some("fi"),
        '\u{FB02}' => Some("fl"),
        '\u{FB03}' => Some("ffi"),
        '\u{FB04}' => Some("ffl"),
        '\u{FB05}' | '\u{FB06}' => Some("st"),
        _ => None,
    }
}

/// Whether a viewer shows something for the character.
fn is_displayable(character: char) -> bool {
    let is_noncharacter = (character as u32) & 0xFFFE == 0xFFFE;
    !character.is_control() && !is_noncharacter
}

#[cfg(test)]
mod tests;
