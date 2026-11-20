//! The font of the text that the redline adds, and how wide text is in it.

use std::borrow::Cow;

use krilla::geom::Point;
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use ttf_parser::Face;

use super::Rgb;
use super::paint::fill;
use crate::excerpt::ELLIPSIS;

static FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");

pub(super) struct TextFont {
    font: Font,
    face: Face<'static>,
}

impl TextFont {
    pub fn load() -> Self {
        Self {
            font: Font::new(FONT_DATA.into(), 0).expect("the bundled font is valid"),
            face: Face::parse(FONT_DATA, 0).expect("the bundled font is valid"),
        }
    }

    /// Draws a line of text with its left end and baseline at `(left, baseline)`.
    pub fn draw(
        &self,
        surface: &mut Surface,
        (left, baseline): (f32, f32),
        font_size: f32,
        color: Rgb,
        text: &str,
    ) {
        surface.set_fill(Some(fill(color)));
        surface.set_stroke(None);
        surface.draw_text(
            Point::from_xy(left, baseline),
            self.font.clone(),
            font_size,
            text,
            false,
            TextDirection::Auto,
        );
    }

    /// The width of a line of text, without kerning.
    pub fn width(&self, text: &str, font_size: f32) -> f32 {
        text.chars()
            .map(|character| self.advance(character, font_size))
            .sum()
    }

    /// `text` if it is at most `max_width` wide, and otherwise as much of its
    /// start as fits together with an ellipsis.
    pub fn fitted<'a>(&self, text: &'a str, font_size: f32, max_width: f32) -> Cow<'a, str> {
        if self.width(text, font_size) <= max_width {
            return Cow::Borrowed(text);
        }
        let mut width = self.advance(ELLIPSIS, font_size);
        let mut end = 0;
        for (index, character) in text.char_indices() {
            width += self.advance(character, font_size);
            if width > max_width {
                break;
            }
            end = index + character.len_utf8();
        }
        Cow::Owned(format!("{}{ELLIPSIS}", text[..end].trim_end()))
    }

    fn advance(&self, character: char, font_size: f32) -> f32 {
        // A character that the font lacks is drawn as its first glyph.
        let glyph = self.face.glyph_index(character).unwrap_or_default();
        let advance = self.face.glyph_hor_advance(glyph).unwrap_or_default();
        f32::from(advance) / f32::from(self.face.units_per_em()) * font_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_grows_with_the_text_and_the_font_size() {
        let font = TextFont::load();
        assert_eq!(font.width("", 10.0), 0.0);
        assert!(font.width("mm", 10.0) > font.width("m", 10.0));
        assert_eq!(font.width("m", 20.0), 2.0 * font.width("m", 10.0));
        assert!(font.width("m", 10.0) > font.width("i", 10.0));
    }

    #[test]
    fn text_that_fits_is_kept() {
        let font = TextFont::load();
        let width = font.width("fits", 10.0);
        assert_eq!(font.fitted("fits", 10.0, width), "fits");
    }

    #[test]
    fn text_that_is_too_wide_ends_in_an_ellipsis_and_fits() {
        let font = TextFont::load();
        let max_width = font.width("too wide for", 10.0);
        let fitted = font.fitted("too wide for this width", 10.0, max_width);
        assert!(fitted.ends_with(ELLIPSIS), "{fitted}");
        assert!(fitted.starts_with("too wide"), "{fitted}");
        assert!(font.width(&fitted, 10.0) <= max_width);
    }

    #[test]
    fn nothing_but_the_ellipsis_is_left_when_nothing_fits() {
        let font = TextFont::load();
        assert_eq!(font.fitted("wide", 10.0, 1.0), "…");
    }
}
