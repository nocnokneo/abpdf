//! The graphics state that a content stream draws with.

use std::rc::Rc;

use hayro_syntax::object::Number;

use super::color::{self, BLACK, ColorSpace, Rgb};
use crate::reader::fonts::Font;
use crate::reader::matrix::Matrix;
use crate::reader::page_space::UserSpaceBox;

/// What a content stream has set of what its operators draw with.
/// `q` saves it and `Q` brings it back.
#[derive(Clone)]
pub(super) struct GraphicsState<'a> {
    /// Takes the space that the content is drawn in to the user space of
    /// the page.
    pub to_page: Matrix,
    pub fill: Paint,
    pub stroke: Paint,
    /// How much of what is under a fill the fill covers, from 0 to 1.
    pub fill_opacity: f32,
    pub line_width: f32,
    /// The upright box that nothing is drawn out of, in the user space of
    /// the page.
    pub clip: UserSpaceBox,
    pub text: TextParameters<'a>,
}

impl GraphicsState<'_> {
    /// The state that a page starts with.
    /// `visible` is the part of the page that a viewer shows.
    pub fn initial(visible: UserSpaceBox) -> Self {
        Self {
            to_page: Matrix::IDENTITY,
            fill: Paint::default(),
            stroke: Paint::default(),
            fill_opacity: 1.0,
            line_width: 1.0,
            clip: visible,
            text: TextParameters::default(),
        }
    }

    /// How long a step of length one of the space that the content is
    /// drawn in is on the page, as a mean over all directions.
    pub fn scale_to_page(&self) -> f64 {
        let Matrix { a, b, c, d, .. } = self.to_page;
        (a * d - b * c).abs().sqrt()
    }
}

/// The color space and the color of fills or of strokes.
#[derive(Clone)]
pub(super) struct Paint {
    pub space: Rc<ColorSpace>,
    /// `None` for a pattern.
    pub color: Option<Rgb>,
}

impl Paint {
    /// The paint that a content stream has when it has chosen a color
    /// space and no color yet.
    pub fn of_space(space: ColorSpace) -> Self {
        Self {
            color: space.initial_color(),
            space: Rc::new(space),
        }
    }

    pub fn of_color(space: ColorSpace, numbers: &[f32]) -> Self {
        Self {
            color: space.color(numbers),
            space: Rc::new(space),
        }
    }

    /// Sets the color from the operands of an operator that sets one in
    /// the color space of the paint.
    /// With the name of a pattern among them, the paint is that pattern.
    pub fn set(&mut self, operands: &[Number], has_pattern_name: bool) {
        self.color = match has_pattern_name {
            true => None,
            false => self.space.color(&color::numbers(operands)),
        };
    }
}

impl Default for Paint {
    fn default() -> Self {
        Self {
            space: Rc::new(ColorSpace::Gray),
            color: Some(BLACK),
        }
    }
}

/// What a content stream has set of how text is shown.
#[derive(Clone)]
pub(super) struct TextParameters<'a> {
    pub font: Option<Rc<Font<'a>>>,
    pub font_size: f32,
    pub character_spacing: f32,
    pub word_spacing: f32,
    /// The factor that text is stretched by along its line, where 1 is
    /// no stretch.
    pub horizontal_scale: f32,
    pub leading: f32,
    pub rise: f32,
}

impl Default for TextParameters<'_> {
    fn default() -> Self {
        Self {
            font: None,
            font_size: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
        }
    }
}
