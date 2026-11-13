//! The change model: what differs between two documents, and where.
//!
//! Every output is rendered from a [`Comparison`].

use std::borrow::Cow;
use std::ops::Range;

use crate::diff::{self, Hunk};
use crate::filter::Filters;
use crate::flow::{self, Flow, Gap, Token};
use crate::geometry::Rect;
use crate::{BySide, Side};

/// Two documents and the changes from the old one to the new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    /// The documents as they were compared.
    pub flows: BySide<Flow>,
    /// In the order of the text.
    pub changes: Vec<Change>,
}

/// A place where the documents differ.
/// At least one of them has text there.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub sides: BySide<ChangeSide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Text that only the new document has.
    Inserted,
    /// Text that only the old document has.
    Deleted,
    /// Text of the old document that the new document has other text for.
    Replaced,
}

/// What one document has at a change.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeSide {
    /// Text that the other document does not have.
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

impl Comparison {
    /// Compares two flows as [`Flow::read`] returns them.
    pub fn of(flows: BySide<Flow>, filters: &Filters) -> Self {
        let [old, new] = flow::prepare([flows.old, flows.new], filters);
        let flows = BySide { old, new };
        let compared = flows.as_ref().map(Compared::of);
        let hunks = diff::diff(&compared.as_ref().map(Compared::keys));
        let changes = hunks
            .iter()
            .map(|hunk| change_at(hunk, &compared))
            .collect();
        Self { flows, changes }
    }
}

impl Change {
    pub fn kind(&self) -> ChangeKind {
        match (self.sides.old.text(), self.sides.new.text()) {
            (None, _) => ChangeKind::Inserted,
            (_, None) => ChangeKind::Deleted,
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

/// The tokens of a flow that take part in the comparison.
struct Compared<'a> {
    tokens: Vec<&'a Token>,
    keys: Vec<Cow<'a, str>>,
}

impl<'a> Compared<'a> {
    fn of(flow: &'a Flow) -> Self {
        let tokens: Vec<&Token> = flow
            .tokens
            .iter()
            .filter(|token| !token.is_excluded())
            .collect();
        let keys = tokens.iter().map(|token| token.key()).collect();
        Self { tokens, keys }
    }

    fn keys(&self) -> Vec<&str> {
        self.keys.iter().map(|key| key.as_ref()).collect()
    }
}

fn change_at(hunk: &Hunk, compared: &BySide<Compared>) -> Change {
    Change {
        sides: BySide::from_fn(|side| change_side(&compared[side], &hunk[side])),
    }
}

fn change_side(compared: &Compared, range: &Range<usize>) -> ChangeSide {
    let tokens = &compared.tokens[range.clone()];
    if tokens.is_empty() {
        return ChangeSide::Absent {
            insertion_point: insertion_point(compared, range.start),
        };
    }
    ChangeSide::Text {
        text: text_of(tokens),
        areas: areas_of(tokens),
    }
}

fn text_of(tokens: &[&Token]) -> String {
    let mut text = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 && token.gap != Gap::None {
            text.push(' ');
        }
        text.push_str(&token.text);
    }
    text
}

/// The bounds of the tokens, merged per line.
fn areas_of(tokens: &[&Token]) -> Vec<Area> {
    let mut areas: Vec<Area> = Vec::new();
    let mut last_line = None;
    for fragment in tokens.iter().flat_map(|token| &token.fragments) {
        let line = (fragment.page_index, fragment.line_index);
        match areas.last_mut() {
            Some(area) if last_line == Some(line) => {
                area.bounds = area.bounds.union(&fragment.bounds);
            }
            _ => areas.push(Area {
                page_index: fragment.page_index,
                bounds: fragment.bounds,
            }),
        }
        last_line = Some(line);
    }
    areas
}

/// Where text would be that came before the compared token at `position`:
/// at the end of the token before it, or else at the start of the first
/// token, if there is one.
fn insertion_point(compared: &Compared, position: usize) -> Option<Area> {
    let before = position.checked_sub(1).map(|index| compared.tokens[index]);
    if let Some(fragment) = before.and_then(|token| token.fragments.last()) {
        return Some(Area {
            page_index: fragment.page_index,
            bounds: Rect {
                left: fragment.bounds.right,
                ..fragment.bounds
            },
        });
    }
    let fragment = compared.tokens.first()?.fragments[0];
    Some(Area {
        page_index: fragment.page_index,
        bounds: Rect {
            right: fragment.bounds.left,
            ..fragment.bounds
        },
    })
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;
    use crate::testing::{PageBuilder, cell_bounds, comparison_of, raw_flow_of};

    fn compare_with(
        old: impl IntoIterator<Item = PageBuilder>,
        new: impl IntoIterator<Item = PageBuilder>,
        filters: &Filters,
    ) -> Vec<Change> {
        let flows = BySide {
            old: raw_flow_of(old),
            new: raw_flow_of(new),
        };
        Comparison::of(flows, filters).changes
    }

    fn compare(old: &str, new: &str) -> Vec<Change> {
        comparison_of(old, new).changes
    }

    /// The kind and the two texts of each change.
    fn summaries(changes: &[Change]) -> Vec<(ChangeKind, &str, &str)> {
        changes
            .iter()
            .map(|change| {
                let texts = change.sides.as_ref().map(ChangeSide::text);
                (
                    change.kind(),
                    texts.old.unwrap_or_default(),
                    texts.new.unwrap_or_default(),
                )
            })
            .collect()
    }

    /// The area of the cells from `columns` on `line` of page `page_index`.
    fn cells(page_index: usize, line: usize, columns: Range<usize>) -> Area {
        Area {
            page_index,
            bounds: cell_bounds(line, columns),
        }
    }

    #[test]
    fn equal_documents_have_no_changes() {
        assert_eq!(compare("one two three", "one two three"), []);
    }

    #[test]
    fn inserted_text_is_only_on_the_new_side() {
        let changes = compare("one three", "one two three");
        assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", "two")]);
        assert_eq!(changes[0].sides.new.areas(), [cells(0, 0, 4..7)]);
    }

    #[test]
    fn side_without_text_is_located_after_the_token_before_it() {
        let changes = compare("one three", "one two three");
        let insertion_point = Some(cells(0, 0, 3..3));
        assert_eq!(changes[0].sides.old, ChangeSide::Absent { insertion_point });
    }

    #[test]
    fn insertion_before_all_text_is_located_at_the_first_token() {
        let changes = compare("one", "zero one");
        assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 0..0)]);
    }

    #[test]
    fn side_of_a_document_without_text_is_nowhere() {
        let new = || [PageBuilder::new().text("one")];
        let nowhere = ChangeSide::Absent {
            insertion_point: None,
        };
        let empty_page = [PageBuilder::new()];
        let no_pages: [PageBuilder; 0] = [];
        let from_empty_page = compare_with(empty_page, new(), &Filters::default());
        let from_no_pages = compare_with(no_pages, new(), &Filters::default());
        for changes in [from_empty_page, from_no_pages] {
            assert_eq!(changes[0].sides.old, nowhere);
            assert_eq!(changes[0].location(), "new p. 1");
        }
    }

    #[test]
    fn location_names_the_page_that_each_side_starts_on() {
        let old = [
            PageBuilder::new().text("keep"),
            PageBuilder::new().text("old words"),
        ];
        let new = [PageBuilder::new().text("keep new words")];
        let changes = compare_with(old, new, &Filters::default());
        assert_eq!(changes[0].location(), "old p. 2 / new p. 1");
    }

    #[test]
    fn deleted_text_is_only_on_the_old_side() {
        let changes = compare("one two three", "one three");
        assert_eq!(summaries(&changes), [(ChangeKind::Deleted, "two", "")]);
    }

    #[test]
    fn replaced_text_is_on_both_sides() {
        let changes = compare("calibrated annually by staff", "verified monthly by staff");
        assert_eq!(
            summaries(&changes),
            [(
                ChangeKind::Replaced,
                "calibrated annually",
                "verified monthly"
            )]
        );
    }

    #[test]
    fn text_keeps_its_spacing() {
        let changes = compare("see p. 3, then", "see pp. 4; then");
        assert_eq!(
            summaries(&changes),
            [(ChangeKind::Replaced, "p. 3,", "pp. 4;")]
        );
    }

    #[test]
    fn punctuation_changes_apart_from_its_word() {
        let changes = compare("press Export now", "press Export. now");
        assert_eq!(summaries(&changes), [(ChangeKind::Inserted, "", ".")]);
    }

    #[test]
    fn change_has_one_area_per_line() {
        let changes = compare("keep old\nwords here", "keep here");
        assert_eq!(changes[0].sides.old.text(), Some("old words"));
        assert_eq!(
            changes[0].sides.old.areas(),
            [cells(0, 0, 5..8), cells(0, 1, 0..5)]
        );
    }

    #[test]
    fn change_can_span_pages() {
        let old = [
            PageBuilder::new().text("keep old"),
            PageBuilder::new().text("words here"),
        ];
        let new = [PageBuilder::new().text("keep here")];
        let changes = compare_with(old, new, &Filters::default());
        assert_eq!(changes[0].sides.old.page_indices(), [0, 1]);
        assert_eq!(changes[0].sides.new.page_indices(), [0]);
    }

    #[test]
    fn moving_to_other_lines_and_pages_is_not_a_change() {
        let old = [PageBuilder::new().text("one two three four")];
        let new = [
            PageBuilder::new().text("one\ntwo"),
            PageBuilder::new().text("three four"),
        ];
        assert_eq!(compare_with(old, new, &Filters::default()), []);
    }

    #[test]
    fn word_that_breaks_at_another_place_is_not_a_change() {
        let old = [PageBuilder::new()
            .text("an inter")
            .line_end_hyphen()
            .text("national one")];
        let new = [PageBuilder::new().text("an international one")];
        assert_eq!(compare_with(old, new, &Filters::default()), []);
    }

    #[test]
    fn compatibility_characters_compare_as_their_plain_form() {
        assert_eq!(compare("the o\u{FB03}ce", "the office"), []);
    }

    #[test]
    fn what_the_filters_leave_out_is_not_compared() {
        let page = |revision: &str, page_number: &str| {
            PageBuilder::new()
                .artifact()
                .text(&format!("Revision {revision}\n"))
                .body()
                .text(&format!("same text (p. {page_number})"))
        };
        let filters = Filters {
            ignore_patterns: vec![Regex::new(r"\(p\. (\d+)\)").unwrap()],
            ..Filters::default()
        };
        assert_eq!(
            compare_with([page("A", "3")], [page("B", "4")], &filters),
            []
        );
        assert_eq!(
            compare_with([page("A", "3")], [page("B", "4")], &Filters::default()).len(),
            1
        );
    }

    #[test]
    fn change_next_to_text_that_is_left_out_does_not_cover_it() {
        let page = |word: &str| {
            PageBuilder::new()
                .text(&format!("{word} "))
                .artifact()
                .text("(p. 3)")
        };
        let changes = compare_with([page("old")], [page("new")], &Filters::default());
        assert_eq!(summaries(&changes), [(ChangeKind::Replaced, "old", "new")]);
        assert_eq!(changes[0].sides.old.areas(), [cells(0, 0, 0..3)]);
    }
}
