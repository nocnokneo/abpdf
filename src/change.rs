//! The change model: what differs between two documents, and where.
//!
//! Every output is rendered from a [`Comparison`].

use std::fmt;

use crate::flow::Flow;
use crate::geometry::{Rect, hundredths};
use crate::reader::Color;
use crate::{BySide, Side};

/// Two documents and the changes from the old one to the new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    /// The documents as they were compared.
    pub flows: BySide<Flow>,
    /// In the order of the new document: a change is where its new text is,
    /// or where the text would be that only the old document has.
    pub changes: Vec<Change>,
    /// For each page of a document, the page of the other document that it
    /// shares most of its unchanged text with, if it shares any.
    pub page_partners: BySide<Vec<Option<usize>>>,
}

/// A place where the documents differ, or text that both have and that
/// stands at another place or looks different in the new document.
/// At least one of the documents has text there.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub sides: BySide<ChangeSide>,
    pub difference: Difference,
}

/// What differs between the two sides of a change.
#[derive(Clone, Debug, PartialEq)]
pub enum Difference {
    /// The text: the sides are two texts at one place, or one of them has
    /// none.
    Text,
    /// The place: the sides are one text at two places.
    /// The text can differ a little between the places, and what differs is
    /// then a change of its own.
    Place,
    /// The style: the sides are one text at one place.
    Style(Vec<StyleDifference>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Text that only the new document has.
    Inserted,
    /// Text that only the old document has.
    Deleted,
    /// Text of the old document that the new document has other text for.
    Replaced,
    /// Text that both documents have, at different places.
    Moved,
    /// Text that both documents have at one place, in different styles.
    Restyled,
}

/// The part that a text has in a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// Only the old document has the text.
    Removed,
    /// Only the new document has the text.
    Added,
    /// Both documents have the text, at different places.
    Moved,
    /// Both documents have the text at one place, in different styles.
    Restyled,
}

impl TextRole {
    /// The part of text that only one side has.
    pub fn of_text_only_on(side: Side) -> Self {
        match side {
            Side::Old => Self::Removed,
            Side::New => Self::Added,
        }
    }
}

/// A property of the style of text, with its value in each document.
#[derive(Clone, Debug, PartialEq)]
pub enum StyleDifference {
    Font(BySide<String>),
    /// The font size in points.
    Size(BySide<f32>),
    /// `None` where the font does not say.
    Weight(BySide<Option<u16>>),
    Italic(BySide<bool>),
    Color(BySide<Color>),
    /// The clockwise rotation in degrees.
    Rotation(BySide<f32>),
}

/// What one document has at a change.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeSide {
    /// Text that the other document does not have, or has at another place.
    Text {
        /// The words of the text, with what each figure in it is in their
        /// place, such as `[image, 640 × 480 pixels]`.
        text: String,
        /// Where the text is, in reading order: one area per line and one
        /// per figure.
        areas: Vec<Area>,
    },
    /// Nothing, where the other document has text.
    Absent {
        /// The place where the text of the other document would be, as an
        /// area of no width.
        /// `None` if nothing of the document is compared, so that there is
        /// no text for the place to be next to.
        insertion_point: Option<Area>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub page_index: usize,
    pub bounds: Rect,
    /// Whether the area is that of a figure, and not of text on a line.
    pub is_figure: bool,
}

impl Change {
    pub fn kind(&self) -> ChangeKind {
        match (&self.difference, &self.sides.old, &self.sides.new) {
            (Difference::Place, ..) => ChangeKind::Moved,
            (Difference::Style(_), ..) => ChangeKind::Restyled,
            (Difference::Text, ChangeSide::Absent { .. }, _) => ChangeKind::Inserted,
            (Difference::Text, _, ChangeSide::Absent { .. }) => ChangeKind::Deleted,
            (Difference::Text, ..) => ChangeKind::Replaced,
        }
    }

    /// The part that the text of a side has in the change, if the side has
    /// text.
    pub fn role_of(&self, side: Side) -> Option<TextRole> {
        self.sides[side].text()?;
        Some(match &self.difference {
            Difference::Text => TextRole::of_text_only_on(side),
            Difference::Place => TextRole::Moved,
            Difference::Style(_) => TextRole::Restyled,
        })
    }

    /// What says the change in words: the text that was removed and the
    /// text that was added, or the text that both documents have, as the new
    /// one has it and after what differs in its style if the style does.
    pub fn lines(&self) -> Vec<(TextRole, String)> {
        let line = |side: Side| {
            let text = self.sides[side].text()?;
            Some((self.role_of(side)?, text.to_owned()))
        };
        let lines = match &self.difference {
            Difference::Text => [line(Side::Old), line(Side::New)],
            Difference::Place => [line(Side::New), None],
            Difference::Style(differences) => {
                let differences = StyleDifference::list(differences);
                let restyled = line(Side::New);
                [
                    restyled.map(|(role, text)| (role, format!("{differences}: {text}"))),
                    None,
                ]
            }
        };
        lines.into_iter().flatten().collect()
    }

    /// The pages on which the two sides start, in words and as a reader
    /// counts them.
    /// A side that is nowhere in its document is not mentioned.
    pub fn location(&self) -> String {
        let pages = Side::BOTH.into_iter().filter_map(|side| {
            let page_number = self.sides[side].first_page_index()? + 1;
            Some(format!("{side} p. {page_number}"))
        });
        pages.collect::<Vec<_>>().join(" / ")
    }
}

impl ChangeSide {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } => Some(text),
            Self::Absent { .. } => None,
        }
    }

    /// Where the side is in its document: the areas of its text, or the
    /// place where it has none.
    pub fn areas(&self) -> &[Area] {
        match self {
            Self::Text { areas, .. } => areas,
            Self::Absent { insertion_point } => insertion_point.as_slice(),
        }
    }

    /// The page on which the side starts, if it is anywhere in its document.
    pub fn first_page_index(&self) -> Option<usize> {
        Some(self.areas().first()?.page_index)
    }
}

impl StyleDifference {
    /// The differences one after the other, such as
    /// `size 10 pt → 12 pt, upright → italic`.
    pub fn list(differences: &[Self]) -> String {
        let differences: Vec<String> = differences.iter().map(Self::to_string).collect();
        differences.join(", ")
    }
}

/// The property and its two values, such as `size 10 pt → 12 pt`.
impl fmt::Display for StyleDifference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slant = |is_italic: bool| if is_italic { "italic" } else { "upright" };
        let weight = |weight: Option<u16>| match weight {
            Some(weight) => weight.to_string(),
            None => "unknown".to_owned(),
        };
        match self {
            Self::Font(font) => write!(formatter, "font {} → {}", font.old, font.new),
            Self::Size(size) => {
                let size = size.map(hundredths);
                write!(formatter, "size {} pt → {} pt", size.old, size.new)
            }
            Self::Weight(values) => {
                let values = values.map(weight);
                write!(formatter, "weight {} → {}", values.old, values.new)
            }
            Self::Italic(is_italic) => {
                let slants = is_italic.map(slant);
                write!(formatter, "{} → {}", slants.old, slants.new)
            }
            Self::Color(color) => write!(formatter, "color {} → {}", color.old, color.new),
            Self::Rotation(rotation) => {
                let rotation = rotation.map(hundredths);
                write!(formatter, "rotation {}° → {}°", rotation.old, rotation.new)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn both<T>(old: T, new: T) -> BySide<T> {
        BySide { old, new }
    }

    #[test]
    fn style_difference_reads_as_its_property_and_values() {
        let black = Color {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        let red = Color { red: 255, ..black };
        let differences = [
            StyleDifference::Font(both("Body".to_owned(), "Body-Bold".to_owned())),
            StyleDifference::Size(both(10.0, 11.499)),
            StyleDifference::Weight(both(None, Some(700))),
            StyleDifference::Italic(both(true, false)),
            StyleDifference::Color(both(black, red)),
            StyleDifference::Rotation(both(0.0, 90.0)),
        ];
        assert_eq!(
            differences.map(|difference| difference.to_string()),
            [
                "font Body → Body-Bold",
                "size 10 pt → 11.5 pt",
                "weight unknown → 700",
                "italic → upright",
                "color #000000 → #ff0000",
                "rotation 0° → 90°",
            ]
        );
    }

    #[test]
    fn style_differences_are_listed_with_commas() {
        let differences = [
            StyleDifference::Size(both(10.0, 12.0)),
            StyleDifference::Italic(both(false, true)),
        ];
        assert_eq!(
            StyleDifference::list(&differences),
            "size 10 pt → 12 pt, upright → italic"
        );
    }
}
