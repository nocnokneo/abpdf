//! The text that a page shows.

use std::rc::Rc;

use super::state::TextParameters;
use crate::reader::Color;
use crate::reader::fonts::{Code, Font, FontDescription, GLYPH_UNITS_PER_EM};
use crate::reader::matrix::Matrix;
use crate::reader::page_space::UserSpaceBox;

/// The character code of a space in a font with codes of one byte, which
/// is the only one that word spacing applies to.
const SPACE_CODE: u32 = 32;

/// The glyphs that one operator shows.
#[derive(Clone, Debug)]
pub(in crate::reader) struct TextRun {
    pub glyphs: Vec<Glyph>,
    pub font: Rc<FontDescription>,
    /// The size of the font as the text is shown, in units of the user
    /// space of the page.
    pub font_size: f32,
    pub fill_color: Color,
    /// The direction that the text is written in, as a step of length one
    /// in the user space of the page.
    pub direction: (f64, f64),
}

/// A glyph as it is shown, in the user space of the page.
#[derive(Clone, Debug)]
pub(in crate::reader) struct Glyph {
    /// The text that the glyph maps to.
    pub text: Rc<str>,
    /// Where on its baseline the glyph starts.
    pub start: (f64, f64),
    /// Where the next glyph starts if nothing but the width of this one
    /// moves it.
    pub end: (f64, f64),
    /// The smallest upright box around the area that the font reserves for
    /// the glyph: as long as the glyph is wide and as high as a line.
    pub bounds: UserSpaceBox,
}

/// Where a content stream shows its next glyph.
pub(super) struct TextPosition {
    /// Takes text space to the space that the content is drawn in.
    matrix: Matrix,
    /// The same at the start of the line.
    line_matrix: Matrix,
}

impl Default for TextPosition {
    fn default() -> Self {
        Self {
            matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
        }
    }
}

impl TextPosition {
    pub fn set(&mut self, matrix: Matrix) {
        self.matrix = matrix;
        self.line_matrix = matrix;
    }

    /// Starts a line that is offset by (`x`, `y`) from the start of the
    /// current one, in text space.
    pub fn start_line(&mut self, x: f64, y: f64) {
        self.set(translation(x, y).then(&self.line_matrix));
    }

    fn move_by(&mut self, (x, y): (f64, f64)) {
        self.matrix = translation(x, y).then(&self.matrix);
    }
}

fn translation(x: f64, y: f64) -> Matrix {
    Matrix {
        e: x,
        f: y,
        ..Matrix::IDENTITY
    }
}

/// Shows the text of one operator.
pub(super) struct TextShow<'s, 'a> {
    position: &'s mut TextPosition,
    parameters: &'s TextParameters<'a>,
    font: &'s Font<'a>,
    /// Takes the space that the content is drawn in to the user space of
    /// the page.
    to_page: Matrix,
    /// The run so far, which starts with the first glyph.
    run: Option<TextRun>,
}

impl<'s, 'a> TextShow<'s, 'a> {
    /// `None` if no font is set to show text in.
    pub fn new(
        position: &'s mut TextPosition,
        parameters: &'s TextParameters<'a>,
        to_page: Matrix,
    ) -> Option<Self> {
        Some(Self {
            position,
            font: parameters.font.as_deref()?,
            parameters,
            to_page,
            run: None,
        })
    }

    /// Shows a string.
    /// Its glyphs take `fill_color` unless the run has begun.
    pub fn string(&mut self, bytes: &[u8], fill_color: Color) {
        let mut offset = 0;
        while offset < bytes.len() {
            let (code_number, byte_count) = self.font.read_code(bytes, offset);
            offset += byte_count.max(1);
            let code = self.font.code(code_number);
            let glyph = self.glyph(&code);
            if let Some(run) = self.run_at_text_position(fill_color) {
                run.glyphs.push(glyph);
            }
            let is_space = code_number == SPACE_CODE && byte_count == 1;
            let word_spacing = match is_space {
                true => self.parameters.word_spacing,
                false => 0.0,
            };
            let spacing = self.parameters.character_spacing + word_spacing;
            self.position
                .move_by(self.along_the_line(self.length_of(&code) + f64::from(spacing)));
        }
    }

    /// Moves the next glyph back by `thousandths` of the font size, as a
    /// number between the strings of one operator does.
    pub fn adjust(&mut self, thousandths: f32) {
        let amount = -f64::from(thousandths / GLYPH_UNITS_PER_EM * self.parameters.font_size);
        self.position.move_by(self.along_the_line(amount));
    }

    /// The run that was shown, unless it has no glyphs.
    pub fn finish(self) -> Option<TextRun> {
        self.run
    }

    /// How long the glyph of a code is along its line, in text space
    /// before text is stretched.
    fn length_of(&self, code: &Code) -> f64 {
        f64::from(code.advance / GLYPH_UNITS_PER_EM * self.parameters.font_size)
    }

    /// The step in text space that goes `amount` along the line.
    fn along_the_line(&self, amount: f64) -> (f64, f64) {
        match self.font.is_horizontal() {
            true => (amount * f64::from(self.parameters.horizontal_scale), 0.0),
            false => (0.0, amount),
        }
    }

    fn text_to_page(&self) -> Matrix {
        self.position.matrix.then(&self.to_page)
    }

    /// The glyph of a code where text is shown next.
    fn glyph(&self, code: &Code) -> Glyph {
        let description = &self.font.description;
        let size = f64::from(self.parameters.font_size);
        let rise = f64::from(self.parameters.rise);
        let (end_x, end_y) = self.along_the_line(self.length_of(code));
        let corners = if self.font.is_horizontal() {
            let low = rise + f64::from(description.descent) * size;
            let high = rise + f64::from(description.ascent) * size;
            [(0.0, low), (end_x, low), (end_x, high), (0.0, high)]
        } else {
            // A glyph that is written from the top stands in the middle
            // of its line, which is as wide as the font is big.
            let half_width = size / 2.0;
            let (start_y, end_y) = (rise, rise + end_y);
            [
                (-half_width, start_y),
                (half_width, start_y),
                (half_width, end_y),
                (-half_width, end_y),
            ]
        };
        let to_page = self.text_to_page();
        let on_page = corners.map(|(x, y)| to_page.apply(x, y));
        Glyph {
            text: code.text.clone(),
            start: to_page.apply(0.0, rise),
            end: to_page.apply(end_x, rise + end_y),
            bounds: UserSpaceBox::bounding(on_page).expect("a box has corners"),
        }
    }

    /// The run, which begins where text is shown next if it has not begun.
    /// `None` if text has no size or no direction there.
    fn run_at_text_position(&mut self, fill_color: Color) -> Option<&mut TextRun> {
        if self.run.is_none() {
            let to_page = self.text_to_page();
            let size = self.parameters.font_size;
            let (forward_x, forward_y) = match self.font.is_horizontal() {
                true => ((size * self.parameters.horizontal_scale).signum(), 0.0),
                // Lines that are written from the top go down.
                false => (0.0, -size.signum()),
            };
            let forward = to_page.apply_to_step(forward_x.into(), forward_y.into());
            let direction = unit(forward)?;
            let (up_x, up_y) = to_page.apply_to_step(0.0, 1.0);
            let font_size = size.abs() * up_x.hypot(up_y) as f32;
            if font_size == 0.0 {
                return None;
            }
            self.run = Some(TextRun {
                glyphs: Vec::new(),
                font: self.font.description.clone(),
                font_size,
                fill_color,
                direction,
            });
        }
        self.run.as_mut()
    }
}

/// The step of length one in the direction of `step`, unless it has none.
fn unit((x, y): (f64, f64)) -> Option<(f64, f64)> {
    let length = x.hypot(y);
    (length > 0.0 && length.is_finite()).then(|| (x / length, y / length))
}
