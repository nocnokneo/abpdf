//! The change model: what differs between two documents, and where.
//!
//! Every output is rendered from a [`Comparison`].

use crate::flow::Flow;
use crate::geometry::Rect;
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

/// A place where the documents differ, or text that stands at another place
/// in the new document.
/// At least one of the documents has text there.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub sides: BySide<ChangeSide>,
    /// Whether the sides are one text at two places, and not two texts at
    /// one place.
    /// The text can differ a little between the places, and what differs is
    /// then a change of its own.
    pub is_move: bool,
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
}

/// What one document has at a change.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeSide {
    /// Text that the other document does not have, or has at another place.
    Text {
        text: String,
        /// Where the text is: one area per line, in reading order.
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
}

impl Change {
    pub fn kind(&self) -> ChangeKind {
        match (self.sides.old.text(), self.sides.new.text()) {
            (None, _) => ChangeKind::Inserted,
            (_, None) => ChangeKind::Deleted,
            _ if self.is_move => ChangeKind::Moved,
            _ => ChangeKind::Replaced,
        }
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

    /// The pages that the side is on, in reading order and without repeats.
    pub fn page_indices(&self) -> Vec<usize> {
        let areas = self.areas().iter();
        let mut page_indices: Vec<usize> = areas.map(|area| area.page_index).collect();
        page_indices.dedup();
        page_indices
    }

    /// The page on which the side starts, if it is anywhere in its document.
    pub fn first_page_index(&self) -> Option<usize> {
        Some(self.areas().first()?.page_index)
    }
}
