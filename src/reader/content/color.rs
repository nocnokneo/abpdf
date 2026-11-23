//! The colors that a page paints in.
//!
//! A color is read to tell one from another and to name it, not to show
//! it, so the conversions are the plain ones: no color profile is applied,
//! and of the inks of a `Separation` or `DeviceN` space only the amount is
//! read, as a gray.

use hayro_syntax::object::{self, Array, Name, Object, Stream};
use hayro_syntax::page::Resources;

/// A color without its opacity: red, green and blue.
pub(super) type Rgb = [u8; 3];

pub(super) const BLACK: Rgb = [0, 0, 0];

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    /// Of its three numbers, only the lightness is read.
    Lab,
    /// A table of colors of another space, each as one byte for each of
    /// that space's numbers.
    Indexed {
        base: Box<ColorSpace>,
        table: Vec<u8>,
    },
    /// Amounts of inks.
    Inks,
    /// What is painted is a pattern and no color.
    Pattern,
}

impl ColorSpace {
    /// The color space that a content stream names: one that every PDF
    /// has, or one of `resources`.
    pub fn named(name: &Name, resources: &Resources) -> ColorSpace {
        Self::built_in(name).unwrap_or_else(|| match resources.get_color_space(name) {
            Some(object) => Self::of(&object),
            None => ColorSpace::Gray,
        })
    }

    fn built_in(name: &[u8]) -> Option<ColorSpace> {
        match name {
            b"DeviceGray" | b"G" | b"CalGray" => Some(ColorSpace::Gray),
            b"DeviceRGB" | b"RGB" | b"CalRGB" => Some(ColorSpace::Rgb),
            b"DeviceCMYK" | b"CMYK" => Some(ColorSpace::Cmyk),
            b"Lab" => Some(ColorSpace::Lab),
            b"Separation" | b"DeviceN" => Some(ColorSpace::Inks),
            b"Pattern" => Some(ColorSpace::Pattern),
            _ => None,
        }
    }

    /// The color space that an object of a PDF describes.
    /// One that cannot be told is gray.
    fn of(object: &Object) -> ColorSpace {
        let described = match object {
            Object::Name(name) => Self::built_in(name),
            Object::Array(array) => Self::of_array(array),
            _ => None,
        };
        described.unwrap_or(ColorSpace::Gray)
    }

    fn of_array(array: &Array) -> Option<ColorSpace> {
        let mut parts = array.iter::<Object>();
        let family = parts.next()?.into_name()?;
        match &*family {
            b"ICCBased" => {
                let profile = parts.next()?.into_stream()?;
                Some(match profile.dict().get::<i32>("N") {
                    Some(3) => ColorSpace::Rgb,
                    Some(4) => ColorSpace::Cmyk,
                    _ => ColorSpace::Gray,
                })
            }
            b"Indexed" | b"I" => {
                let base = Self::of(&parts.next()?);
                let _highest_index = parts.next()?;
                let table = match parts.next()? {
                    Object::String(table) => table.as_bytes().to_vec(),
                    Object::Stream(table) => decoded(&table),
                    _ => return None,
                };
                Some(ColorSpace::Indexed {
                    base: Box::new(base),
                    table,
                })
            }
            family => Self::built_in(family),
        }
    }

    /// How many numbers a color of the space has, where the space says.
    fn number_count(&self) -> Option<usize> {
        match self {
            ColorSpace::Gray | ColorSpace::Indexed { .. } => Some(1),
            ColorSpace::Rgb | ColorSpace::Lab => Some(3),
            ColorSpace::Cmyk => Some(4),
            ColorSpace::Inks | ColorSpace::Pattern => None,
        }
    }

    /// The color that a content stream paints in when it has chosen the
    /// space and no color yet.
    pub fn initial_color(&self) -> Option<Rgb> {
        match self {
            ColorSpace::Cmyk => self.color(&[0.0, 0.0, 0.0, 1.0]),
            // All of every ink.
            ColorSpace::Inks => self.color(&[1.0]),
            _ => self.color(&[0.0; 3]),
        }
    }

    /// The color that `numbers` are in the space.
    /// `None` for a pattern, and where numbers are missing.
    pub fn color(&self, numbers: &[f32]) -> Option<Rgb> {
        if self
            .number_count()
            .is_some_and(|count| numbers.len() < count)
        {
            return None;
        }
        let share = |number: f32| (number.clamp(0.0, 1.0) * 255.0).round() as u8;
        match self {
            ColorSpace::Gray => Some([share(numbers[0]); 3]),
            ColorSpace::Rgb => Some([share(numbers[0]), share(numbers[1]), share(numbers[2])]),
            ColorSpace::Cmyk => {
                let black = numbers[3];
                let without_black = |ink: f32| share((1.0 - ink) * (1.0 - black));
                Some([
                    without_black(numbers[0]),
                    without_black(numbers[1]),
                    without_black(numbers[2]),
                ])
            }
            ColorSpace::Lab => Some([share(numbers[0] / 100.0); 3]),
            ColorSpace::Indexed { base, table } => {
                let index = numbers[0].max(0.0).round() as usize;
                let number_count = base.number_count()?;
                let entry = table.chunks_exact(number_count).nth(index)?;
                let numbers: Vec<f32> = entry.iter().map(|&byte| f32::from(byte) / 255.0).collect();
                base.color(&numbers)
            }
            ColorSpace::Inks => {
                let most = numbers.iter().copied().fold(0.0, f32::max);
                Some([share(1.0 - most); 3])
            }
            ColorSpace::Pattern => None,
        }
    }
}

/// The data of a stream without its filters, or nothing if they cannot be
/// undone.
fn decoded(stream: &Stream) -> Vec<u8> {
    stream.decoded_data().map(Vec::from).unwrap_or_default()
}

/// The numbers among the operands of an operator that sets a color.
pub(super) fn numbers(operands: &[object::Number]) -> Vec<f32> {
    operands.iter().map(object::Number::as_f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_and_rgb_are_read_as_they_are() {
        assert_eq!(ColorSpace::Gray.color(&[0.5]), Some([128, 128, 128]));
        assert_eq!(ColorSpace::Rgb.color(&[1.0, 0.5, 0.0]), Some([255, 128, 0]));
    }

    #[test]
    fn cmyk_takes_its_inks_from_white() {
        assert_eq!(ColorSpace::Cmyk.color(&[0.0, 0.0, 0.0, 1.0]), Some(BLACK));
        assert_eq!(
            ColorSpace::Cmyk.color(&[0.0, 1.0, 1.0, 0.0]),
            Some([255, 0, 0])
        );
    }

    #[test]
    fn indexed_color_is_looked_up_in_the_table() {
        let palette = ColorSpace::Indexed {
            base: Box::new(ColorSpace::Rgb),
            table: vec![0, 0, 0, 255, 0, 0, 0, 0, 255],
        };
        assert_eq!(palette.color(&[1.0]), Some([255, 0, 0]));
        assert_eq!(palette.color(&[2.0]), Some([0, 0, 255]));
        assert_eq!(palette.color(&[3.0]), None);
    }

    #[test]
    fn inks_are_as_dark_as_the_most_of_one() {
        assert_eq!(ColorSpace::Inks.color(&[0.25, 1.0]), Some(BLACK));
        assert_eq!(ColorSpace::Inks.color(&[0.0]), Some([255, 255, 255]));
    }

    #[test]
    fn pattern_and_missing_numbers_are_no_color() {
        assert_eq!(ColorSpace::Pattern.color(&[]), None);
        assert_eq!(ColorSpace::Rgb.color(&[1.0]), None);
    }

    #[test]
    fn space_starts_out_black() {
        for space in [ColorSpace::Gray, ColorSpace::Rgb, ColorSpace::Cmyk] {
            assert_eq!(space.initial_color(), Some(BLACK));
        }
        assert_eq!(ColorSpace::Inks.initial_color(), Some(BLACK));
    }
}
