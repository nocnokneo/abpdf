//! The change model: what differs between two documents, and where.
//!
//! Every output is rendered from a [`Comparison`].

use std::borrow::Cow;
use std::ops::Range;

use crate::Side;
use crate::diff::{self, Hunk};
use crate::filter::Filters;
use crate::flow::{self, Flow, Gap, Token};
use crate::geometry::Rect;
use crate::reader::{ReadError, Reader};

/// Two documents and the changes from the old one to the new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub old: Flow,
    pub new: Flow,
    /// In the order of the text.
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub old: ChangeSide,
    pub new: ChangeSide,
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
pub struct ChangeSide {
    /// The changed text, which is empty on the old side of an insertion and
    /// on the new side of a deletion.
    pub text: String,
    /// Where the text is: one area per line, in reading order.
    /// For a side without text, one area of no width where the text of the
    /// other side would be.
    pub areas: Vec<Area>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub page_index: usize,
    pub bounds: Rect,
}

impl Comparison {
    /// Reads and compares two documents.
    pub fn of(old: &dyn Reader, new: &dyn Reader, filters: &Filters) -> Result<Self, ReadError> {
        Ok(Self::of_flows(Flow::read(old)?, Flow::read(new)?, filters))
    }

    /// Compares two flows as [`Flow::read`] returns them.
    pub fn of_flows(old: Flow, new: Flow, filters: &Filters) -> Self {
        let [old, new] = flow::prepare([old, new], filters);
        let (old_compared, new_compared) = (Compared::of(&old), Compared::of(&new));
        let hunks = diff::diff(&old_compared.keys(), &new_compared.keys());
        let changes = hunks
            .iter()
            .map(|hunk| change_at(hunk, &old_compared, &new_compared))
            .collect();
        Self { old, new, changes }
    }
}

impl Change {
    pub fn side(&self, side: Side) -> &ChangeSide {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }
}

impl ChangeSide {
    /// The pages that the side is on, in reading order and without repeats.
    pub fn page_indices(&self) -> Vec<usize> {
        let mut page_indices: Vec<usize> = self.areas.iter().map(|area| area.page_index).collect();
        page_indices.dedup();
        page_indices
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

fn change_at(hunk: &Hunk, old: &Compared, new: &Compared) -> Change {
    let kind = match (hunk.old.is_empty(), hunk.new.is_empty()) {
        (true, _) => ChangeKind::Inserted,
        (_, true) => ChangeKind::Deleted,
        _ => ChangeKind::Replaced,
    };
    Change {
        kind,
        old: change_side(old, &hunk.old),
        new: change_side(new, &hunk.new),
    }
}

fn change_side(compared: &Compared, range: &Range<usize>) -> ChangeSide {
    let tokens = &compared.tokens[range.clone()];
    if tokens.is_empty() {
        return ChangeSide {
            text: String::new(),
            areas: vec![insertion_point(compared, range.start)],
        };
    }
    ChangeSide {
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
/// at the end of the token before it, or else at the start of that token.
fn insertion_point(compared: &Compared, position: usize) -> Area {
    let before = position.checked_sub(1).map(|index| compared.tokens[index]);
    if let Some(fragment) = before.and_then(|token| token.fragments.last()) {
        return Area {
            page_index: fragment.page_index,
            bounds: Rect {
                left: fragment.bounds.right,
                ..fragment.bounds
            },
        };
    }
    match compared.tokens.first().map(|token| token.fragments[0]) {
        Some(fragment) => Area {
            page_index: fragment.page_index,
            bounds: Rect {
                right: fragment.bounds.left,
                ..fragment.bounds
            },
        },
        None => Area {
            page_index: 0,
            bounds: Rect {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;
    use crate::testing::{CELL_HEIGHT, CELL_WIDTH, LINE_PITCH, PageBuilder, raw_flow_of};

    fn compare_with(
        old: impl IntoIterator<Item = PageBuilder>,
        new: impl IntoIterator<Item = PageBuilder>,
        filters: &Filters,
    ) -> Vec<Change> {
        Comparison::of_flows(raw_flow_of(old), raw_flow_of(new), filters).changes
    }

    fn compare(old: &str, new: &str) -> Vec<Change> {
        compare_with(
            [PageBuilder::new().text(old)],
            [PageBuilder::new().text(new)],
            &Filters::default(),
        )
    }

    /// The kind and the two texts of each change.
    fn summaries(changes: &[Change]) -> Vec<(ChangeKind, &str, &str)> {
        changes
            .iter()
            .map(|change| {
                (
                    change.kind,
                    change.old.text.as_str(),
                    change.new.text.as_str(),
                )
            })
            .collect()
    }

    /// The area of the cells from `columns` on `line` of page `page_index`.
    fn cells(page_index: usize, line: usize, columns: Range<usize>) -> Area {
        let top = line as f32 * LINE_PITCH;
        Area {
            page_index,
            bounds: Rect {
                left: columns.start as f32 * CELL_WIDTH,
                top,
                right: columns.end as f32 * CELL_WIDTH,
                bottom: top + CELL_HEIGHT,
            },
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
        assert_eq!(changes[0].new.areas, [cells(0, 0, 4..7)]);
    }

    #[test]
    fn side_without_text_is_located_after_the_token_before_it() {
        let changes = compare("one three", "one two three");
        assert_eq!(changes[0].old.areas, [cells(0, 0, 3..3)]);
    }

    #[test]
    fn insertion_before_all_text_is_located_at_the_first_token() {
        let changes = compare("one", "zero one");
        assert_eq!(changes[0].old.areas, [cells(0, 0, 0..0)]);
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
        assert_eq!(changes[0].old.text, "old words");
        assert_eq!(changes[0].old.areas, [cells(0, 0, 5..8), cells(0, 1, 0..5)]);
    }

    #[test]
    fn change_can_span_pages() {
        let old = [
            PageBuilder::new().text("keep old"),
            PageBuilder::new().text("words here"),
        ];
        let new = [PageBuilder::new().text("keep here")];
        let changes = compare_with(old, new, &Filters::default());
        assert_eq!(changes[0].old.page_indices(), [0, 1]);
        assert_eq!(changes[0].new.page_indices(), [0]);
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
        assert_eq!(changes[0].old.areas, [cells(0, 0, 0..3)]);
    }
}
