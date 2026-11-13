//! Deciding what the redline of a comparison shows.

use std::collections::BTreeSet;

use super::{Connector, Mark, MarkStyle, Redline, Rgb, Sheet, SheetPage, Span, Summary};
use super::{SummaryEntry, Target};
use crate::Side;
use crate::change::{Change, Comparison};

impl Redline {
    /// The redline of a comparison: a summary, and a sheet for every pair of
    /// pages that shows a change.
    ///
    /// The names are what the documents are called on the sheets.
    pub fn of(comparison: &Comparison, old_name: &str, new_name: &str) -> Self {
        let planner = Planner {
            comparison,
            old_name,
            new_name,
        };
        let page_pairs = planner.page_pairs();
        let mut sheets: Vec<Sheet> = page_pairs.iter().map(|pair| planner.sheet(pair)).collect();
        planner.note_unchanged_pages(&mut sheets);

        let entries = comparison
            .changes
            .iter()
            .map(|change| SummaryEntry {
                location: change.location(),
                old_text: change.old.text.clone(),
                new_text: change.new.text.clone(),
                target: target_of(change, &page_pairs),
            })
            .collect();
        Self {
            summary: Some(Summary {
                title: format!("{old_name} → {new_name}"),
                entries,
            }),
            sheets,
        }
    }
}

/// The pages that a sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PagePair {
    // The new page comes first so that sheets are in the order of the new
    // document.
    new_page_index: usize,
    old_page_index: usize,
}

impl PagePair {
    /// The pages on which the two sides of a change start.
    fn of_first_pages(change: &Change) -> Self {
        Self {
            old_page_index: change.old.areas[0].page_index,
            new_page_index: change.new.areas[0].page_index,
        }
    }
}

struct Planner<'a> {
    comparison: &'a Comparison,
    old_name: &'a str,
    new_name: &'a str,
}

impl Planner<'_> {
    /// The pairs of pages that together show both sides of every change, in
    /// the order of their sheets.
    ///
    /// A change that is on several pages pairs them in order, and repeats the
    /// last page of the side that has fewer.
    fn page_pairs(&self) -> Vec<PagePair> {
        let mut pairs = BTreeSet::new();
        for change in &self.comparison.changes {
            let old_pages = change.old.page_indices();
            let new_pages = change.new.page_indices();
            for position in 0..old_pages.len().max(new_pages.len()) {
                pairs.insert(PagePair {
                    old_page_index: old_pages[position.min(old_pages.len() - 1)],
                    new_page_index: new_pages[position.min(new_pages.len() - 1)],
                });
            }
        }
        pairs.into_iter().collect()
    }

    fn sheet(&self, pair: &PagePair) -> Sheet {
        let connectors = self
            .comparison
            .changes
            .iter()
            .enumerate()
            .filter_map(|(index, change)| {
                Some(Connector {
                    label: (index + 1).to_string(),
                    old: span_on(change, Side::Old, pair.old_page_index)?,
                    new: span_on(change, Side::New, pair.new_page_index)?,
                })
            })
            .collect();
        Sheet {
            old: self.sheet_page(Side::Old, pair.old_page_index),
            new: self.sheet_page(Side::New, pair.new_page_index),
            connectors,
            note_above: None,
            note_below: None,
        }
    }

    /// The page with a mark for every area that a change has on it.
    /// `None` if the document has no such page, which is the case for the
    /// page that a change is located on in a document without pages.
    fn sheet_page(&self, side: Side, page_index: usize) -> Option<SheetPage> {
        let (name, flow) = match side {
            Side::Old => (self.old_name, &self.comparison.old),
            Side::New => (self.new_name, &self.comparison.new),
        };
        let page_count = flow.page_sizes.len();
        if page_index >= page_count {
            return None;
        }
        let mut marks = Vec::new();
        for change in &self.comparison.changes {
            let has_text = !change.side(side).text.is_empty();
            let areas = change.side(side).areas.iter();
            marks.extend(
                areas
                    .filter(|area| area.page_index == page_index)
                    .map(|area| Mark {
                        bounds: area.bounds,
                        color: color_of(change, side),
                        style: if has_text {
                            MarkStyle::Highlight
                        } else {
                            MarkStyle::Caret
                        },
                    }),
            );
        }
        Some(SheetPage {
            page_index,
            label: format!("{name} — page {} of {page_count}", page_index + 1),
            marks,
        })
    }

    /// Says on the sheets which pages of the new document come between them,
    /// and so have no changes.
    fn note_unchanged_pages(&self, sheets: &mut [Sheet]) {
        let mut next_page_index = 0;
        for sheet in sheets.iter_mut() {
            let Some(page) = &sheet.new else {
                continue;
            };
            sheet.note_above = self.unchanged_note(next_page_index, page.page_index);
            next_page_index = page.page_index + 1;
        }
        if let Some(last) = sheets.last_mut() {
            let page_count = self.comparison.new.page_sizes.len();
            last.note_below = self.unchanged_note(next_page_index, page_count);
        }
    }

    /// The note for the pages from `first_page_index` up to `end_page_index`.
    fn unchanged_note(&self, first_page_index: usize, end_page_index: usize) -> Option<String> {
        let name = self.new_name;
        let (first, last) = (first_page_index + 1, end_page_index);
        match last.checked_sub(first)? {
            0 => Some(format!("Page {first} of {name} has no differences")),
            _ => Some(format!(
                "Pages {first}–{last} of {name} have no differences"
            )),
        }
    }
}

/// The side of a change that has no text is marked in the color of the other.
fn color_of(change: &Change, side: Side) -> Rgb {
    let side_with_text = match side {
        Side::Old if change.old.text.is_empty() => Side::New,
        Side::New if change.new.text.is_empty() => Side::Old,
        side => side,
    };
    match side_with_text {
        Side::Old => Rgb::REMOVED,
        Side::New => Rgb::ADDED,
    }
}

/// The extent of what a change has on a page of one side, if anything.
fn span_on(change: &Change, side: Side, page_index: usize) -> Option<Span> {
    let areas = change.side(side).areas.iter();
    areas
        .filter(|area| area.page_index == page_index)
        .map(|area| Span {
            top: area.bounds.top,
            bottom: area.bounds.bottom,
        })
        .reduce(|span, next| Span {
            top: span.top.min(next.top),
            bottom: span.bottom.max(next.bottom),
        })
}

/// Where to look for a change: on the sheet of the pages it starts on, which
/// is the first sheet that shows both of its sides.
fn target_of(change: &Change, page_pairs: &[PagePair]) -> Target {
    let first_pages = PagePair::of_first_pages(change);
    let sheet_index = page_pairs
        .binary_search(&first_pages)
        .expect("every change has a sheet for its first pages");
    let span = span_on(change, Side::New, first_pages.new_page_index)
        .expect("a change has an area on its first page");
    Target {
        sheet_index,
        top: span.top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::testing::{CELL_HEIGHT, LINE_PITCH, cell_bounds, comparison_of_pages};

    /// The redline of two documents, given as the text of each of their pages.
    fn redline(old_pages: &[impl AsRef<str>], new_pages: &[impl AsRef<str>]) -> Redline {
        let comparison = comparison_of_pages(old_pages, new_pages);
        Redline::of(&comparison, "old.pdf", "new.pdf")
    }

    fn shown_pages(redline: &Redline) -> Vec<(Option<usize>, Option<usize>)> {
        let page_index = |page: &Option<SheetPage>| page.as_ref().map(|page| page.page_index);
        let sheets = redline.sheets.iter();
        sheets
            .map(|sheet| (page_index(&sheet.old), page_index(&sheet.new)))
            .collect()
    }

    fn entries(redline: &Redline) -> &[SummaryEntry] {
        &redline.summary.as_ref().unwrap().entries
    }

    /// The bounds of the cells from `columns` on the first line of a page.
    fn cells(columns: std::ops::Range<usize>) -> Rect {
        cell_bounds(0, columns)
    }

    #[test]
    fn equal_documents_have_a_summary_and_no_sheets() {
        let redline = redline(&["same"], &["same"]);
        assert_eq!(redline.sheets, []);
        let summary = redline.summary.unwrap();
        assert_eq!(summary.title, "old.pdf → new.pdf");
        assert_eq!(summary.entries, []);
    }

    #[test]
    fn replacement_is_highlighted_on_both_pages_and_connected() {
        let redline = redline(&["keep old"], &["keep newer"]);
        let [sheet] = redline.sheets.as_slice() else {
            panic!("{:?}", redline.sheets);
        };
        let highlight = |bounds, color| Mark {
            bounds,
            color,
            style: MarkStyle::Highlight,
        };
        assert_eq!(
            sheet.old.as_ref().unwrap().marks,
            [highlight(cells(5..8), Rgb::REMOVED)]
        );
        assert_eq!(
            sheet.new.as_ref().unwrap().marks,
            [highlight(cells(5..10), Rgb::ADDED)]
        );
        let line = Span {
            top: 0.0,
            bottom: CELL_HEIGHT,
        };
        assert_eq!(
            sheet.connectors,
            [Connector {
                label: "1".to_owned(),
                old: line,
                new: line,
            }]
        );
    }

    #[test]
    fn side_without_text_gets_a_caret_in_the_color_of_the_other_side() {
        let inserted = redline(&["keep"], &["keep more"]);
        let old_marks = &inserted.sheets[0].old.as_ref().unwrap().marks;
        assert_eq!(
            old_marks.as_slice(),
            [Mark {
                bounds: cells(4..4),
                color: Rgb::ADDED,
                style: MarkStyle::Caret,
            }]
        );

        let deleted = redline(&["keep more"], &["keep"]);
        let new_marks = &deleted.sheets[0].new.as_ref().unwrap().marks;
        assert_eq!(new_marks[0].color, Rgb::REMOVED);
        assert_eq!(new_marks[0].style, MarkStyle::Caret);
    }

    #[test]
    fn pages_are_labeled_with_their_document_and_number() {
        let old = ["unchanged opening", "this was old"];
        let new = ["unchanged opening", "an added page of text", "this was new"];
        let redline = redline(&old, &new);
        let sheet = redline.sheets.last().unwrap();
        assert_eq!(sheet.old.as_ref().unwrap().label, "old.pdf — page 2 of 2");
        assert_eq!(sheet.new.as_ref().unwrap().label, "new.pdf — page 3 of 3");
    }

    #[test]
    fn only_pages_with_changes_are_shown_in_the_order_of_the_new_document() {
        let old = [
            "page one stays",
            "page two stays",
            "page three old",
            "page four stays",
            "page five old",
        ];
        let new = [
            "page one stays",
            "page two stays",
            "page three new",
            "inserted words here",
            "page four stays",
            "page five new",
        ];
        let redline = redline(&old, &new);
        // The first change runs on to a new page that has no old page of its
        // own, which is shown next to the old page of the change.
        assert_eq!(
            shown_pages(&redline),
            [(Some(2), Some(2)), (Some(2), Some(3)), (Some(4), Some(5))]
        );
    }

    #[test]
    fn change_over_several_pages_pairs_them_in_order() {
        let redline = redline(&["keep old", "words", "end"], &["keep end"]);
        assert_eq!(
            shown_pages(&redline),
            [(Some(0), Some(0)), (Some(1), Some(0))]
        );
        // The second sheet shows the rest of the removed text next to the
        // place it was removed from.
        assert_eq!(redline.sheets[1].connectors.len(), 1);
    }

    #[test]
    fn pages_between_sheets_are_noted_as_unchanged() {
        let pages = |revision: &str| -> Vec<String> {
            (1..=8)
                .map(|number| match number {
                    3 | 6 => format!("page {number} is {revision}"),
                    _ => format!("page {number} stays the same"),
                })
                .collect()
        };
        let redline = redline(&pages("old"), &pages("new"));
        let notes: Vec<_> = redline
            .sheets
            .iter()
            .map(|sheet| (sheet.note_above.as_deref(), sheet.note_below.as_deref()))
            .collect();
        assert_eq!(
            notes,
            [
                (Some("Pages 1–2 of new.pdf have no differences"), None),
                (
                    Some("Pages 4–5 of new.pdf have no differences"),
                    Some("Pages 7–8 of new.pdf have no differences")
                ),
            ]
        );
    }

    #[test]
    fn single_unchanged_page_is_noted_in_the_singular() {
        let redline = redline(
            &["unchanged opening", "this was old"],
            &["unchanged opening", "this was new"],
        );
        assert_eq!(
            redline.sheets[0].note_above.as_deref(),
            Some("Page 1 of new.pdf has no differences")
        );
        assert_eq!(redline.sheets[0].note_below, None);
    }

    #[test]
    fn summary_entry_says_where_the_change_is_and_links_to_it() {
        let redline = redline(
            &["unchanged opening", "keep\nold"],
            &["unchanged opening", "keep\nnew"],
        );
        assert_eq!(
            entries(&redline),
            [SummaryEntry {
                location: "old p. 2 / new p. 2".to_owned(),
                old_text: "old".to_owned(),
                new_text: "new".to_owned(),
                target: Target {
                    sheet_index: 0,
                    top: LINE_PITCH,
                },
            }]
        );
    }

    #[test]
    fn summary_entry_links_to_the_sheet_of_the_pages_a_change_starts_on() {
        // The removed text is on two old pages, each of which is shown next
        // to the new page that the text is gone from.
        let redline = redline(&["start", "old", "more kept"], &["start kept"]);
        assert_eq!(
            shown_pages(&redline),
            [(Some(1), Some(0)), (Some(2), Some(0))]
        );
        assert_eq!(entries(&redline)[0].target.sheet_index, 0);
    }

    #[test]
    fn connectors_are_numbered_like_the_summary() {
        let old = ["first page old text", "second page old text"];
        let new = ["first page new text", "second page new text"];
        let redline = redline(&old, &new);
        let labels: Vec<&str> = redline
            .sheets
            .iter()
            .flat_map(|sheet| &sheet.connectors)
            .map(|connector| connector.label.as_str())
            .collect();
        assert_eq!(labels, ["1", "2"]);
        assert_eq!(entries(&redline).len(), 2);
    }
}
