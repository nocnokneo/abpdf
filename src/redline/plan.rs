//! Deciding what the redline of a comparison shows.

use std::collections::BTreeSet;

use super::audit;
use super::pairing::{self, PagePair};
use super::{Connector, Mark, MarkStyle, Redline, Rgb, Sheet, SheetPage, Span, Summary};
use super::{SummaryEntry, SummaryLine, Target};
use crate::change::{Area, Change, ChangeSide, Comparison, TextRole};
use crate::{BySide, Side};

impl Redline {
    /// The redline of a comparison: a summary, and a sheet for every pair of
    /// pages that shows a change.
    ///
    /// The names are what the documents are called on the sheets.
    pub fn of(comparison: &Comparison, names: BySide<&str>) -> Self {
        let pages = BySide::from_fn(|side| pairing::pages_with_changes(comparison, side));
        Planner { comparison, names }.redline(&pages)
    }

    /// The redline for checking what a comparison leaves out.
    ///
    /// Besides what [`Redline::of`] shows, it has every other page of the
    /// two documents on a sheet, and the text that was left out of the
    /// comparison has an outline in a color that tells why.
    pub fn audit_of(comparison: &Comparison, names: BySide<&str>) -> Self {
        let every_page =
            BySide::from_fn(|side| (0..comparison.flows[side].page_sizes.len()).collect());
        let mut redline = Planner { comparison, names }.redline(&every_page);
        audit::add_to(&mut redline, comparison);
        redline
    }
}

struct Planner<'a> {
    comparison: &'a Comparison,
    names: BySide<&'a str>,
}

impl Planner<'_> {
    /// The redline whose sheets show every change and each of `pages`.
    fn redline(&self, pages: &BySide<BTreeSet<usize>>) -> Redline {
        let comparison = self.comparison;
        let page_pairs = pairing::page_pairs(comparison, pages);
        let mut sheets: Vec<Sheet> = page_pairs.iter().map(|pair| self.sheet(pair)).collect();
        self.note_unchanged_pages(&mut sheets);

        let entries = comparison
            .changes
            .iter()
            .map(|change| SummaryEntry {
                location: change.location(),
                lines: summary_lines_of(change),
                target: target_of(change, &page_pairs),
            })
            .collect();
        Redline {
            summary: Some(Summary {
                title: format!("{} → {}", self.names.old, self.names.new),
                legend: Vec::new(),
                entries,
            }),
            sheets,
        }
    }

    fn sheet(&self, pair: &PagePair) -> Sheet {
        let connectors = self
            .comparison
            .changes
            .iter()
            .enumerate()
            .filter_map(|(index, change)| {
                let span = |side: Side| span_on(&change.sides[side], pair[side]?);
                let spans = BySide::from_fn(span);
                let is_on_sheet = spans.old.is_some() || spans.new.is_some();
                is_on_sheet.then(|| Connector {
                    label: (index + 1).to_string(),
                    spans,
                })
            })
            .collect();
        Sheet {
            pages: BySide::from_fn(|side| Some(self.sheet_page(side, pair[side]?))),
            connectors,
            note_above: None,
            note_below: None,
        }
    }

    /// The page with a mark for every area that a change has on it.
    fn sheet_page(&self, side: Side, page_index: usize) -> SheetPage {
        let name = self.names[side];
        let page_count = self.comparison.flows[side].page_sizes.len();
        let mut marks = Vec::new();
        for change in &self.comparison.changes {
            let change_side = &change.sides[side];
            let (color, style) = look_of(change, side);
            let on_page = |area: &&Area| area.page_index == page_index;
            let to_mark = |area: &Area| Mark {
                bounds: area.bounds,
                color,
                style,
            };
            marks.extend(change_side.areas().iter().filter(on_page).map(to_mark));
        }
        SheetPage {
            page_index,
            label: format!("{name} — page {} of {page_count}", page_index + 1),
            marks,
        }
    }

    /// Says on the sheets which pages of the new document come between the
    /// ones that they show, and so have no changes.
    /// Sheets that show no new page say nothing about the others.
    fn note_unchanged_pages(&self, sheets: &mut [Sheet]) {
        let mut next_page_index = 0;
        let mut last_with_new_page = None;
        for sheet in sheets.iter_mut() {
            let Some(page_index) = sheet.pages.new.as_ref().map(|page| page.page_index) else {
                continue;
            };
            sheet.note_above = self.unchanged_note(next_page_index, page_index);
            next_page_index = page_index + 1;
            last_with_new_page = Some(sheet);
        }
        if let Some(last) = last_with_new_page {
            let page_count = self.comparison.flows.new.page_sizes.len();
            last.note_below = self.unchanged_note(next_page_index, page_count);
        }
    }

    /// The note for the pages from `first_page_index` up to `end_page_index`.
    fn unchanged_note(&self, first_page_index: usize, end_page_index: usize) -> Option<String> {
        let name = self.names.new;
        let (first, last) = (first_page_index + 1, end_page_index);
        match last.checked_sub(first)? {
            0 => Some(format!("Page {first} of {name} has no differences")),
            _ => Some(format!(
                "Pages {first}–{last} of {name} have no differences"
            )),
        }
    }
}

/// How one side of a change is marked: its text is highlighted in the color
/// of the part it has in the change, and the place where it has none gets a
/// caret in the color of the text that the other side has there.
fn look_of(change: &Change, side: Side) -> (Rgb, MarkStyle) {
    match change.role_of(side) {
        Some(role) => (color_of(role), MarkStyle::Highlight),
        None => {
            let missing = TextRole::of_text_only_on(side.other());
            (color_of(missing), MarkStyle::Caret)
        }
    }
}

fn summary_lines_of(change: &Change) -> Vec<SummaryLine> {
    let lines = change.lines().into_iter();
    lines
        .map(|(role, text)| SummaryLine {
            sign: sign_of(role),
            color: color_of(role),
            text,
        })
        .collect()
}

fn sign_of(role: TextRole) -> char {
    match role {
        TextRole::Removed => '−',
        TextRole::Added => '+',
        TextRole::Moved => '⇄',
        TextRole::Restyled => '≈',
    }
}

fn color_of(role: TextRole) -> Rgb {
    match role {
        TextRole::Removed => Rgb::REMOVED,
        TextRole::Added => Rgb::ADDED,
        TextRole::Moved => Rgb::MOVED,
        TextRole::Restyled => Rgb::RESTYLED,
    }
}

/// The extent of what one side of a change has on a page, if anything.
fn span_on(change_side: &ChangeSide, page_index: usize) -> Option<Span> {
    let areas = change_side.areas().iter();
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

/// Where to look for a change: on the first sheet that shows the pages it
/// starts on, at its new side or else at its old one.
fn target_of(change: &Change, page_pairs: &[PagePair]) -> Target {
    let first_pages = pairing::first_pages_of(change);
    let sheet_index = page_pairs
        .iter()
        .position(|pair| pairing::shows(pair, &first_pages))
        .expect("every change has a sheet for the pages it starts on");
    let top_on = |side: Side| Some(span_on(&change.sides[side], first_pages[side]?)?.top);
    let top = top_on(Side::New).or_else(|| top_on(Side::Old));
    Target {
        sheet_index,
        top: top.expect("a change has text on a page"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filters;
    use crate::geometry::Rect;
    use crate::testing::{
        CELL_HEIGHT, LINE_PITCH, PARAGRAPH, PageBuilder, REST, cell_bounds,
        comparison_of_built_pages, comparison_of_pages, page_with_bold, raw_flow_of,
    };

    /// The redline of two documents, given as the text of each of their pages.
    fn redline(old_pages: &[impl AsRef<str>], new_pages: &[impl AsRef<str>]) -> Redline {
        let comparison = comparison_of_pages(old_pages, new_pages);
        let names = BySide {
            old: "old.pdf",
            new: "new.pdf",
        };
        Redline::of(&comparison, names)
    }

    fn shown_pages(redline: &Redline) -> Vec<(Option<usize>, Option<usize>)> {
        let page_index = |page: &Option<SheetPage>| page.as_ref().map(|page| page.page_index);
        let sheets = redline.sheets.iter();
        sheets
            .map(|sheet| (page_index(&sheet.pages.old), page_index(&sheet.pages.new)))
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
            sheet.pages.old.as_ref().unwrap().marks,
            [highlight(cells(5..8), Rgb::REMOVED)]
        );
        assert_eq!(
            sheet.pages.new.as_ref().unwrap().marks,
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
                spans: BySide {
                    old: Some(line),
                    new: Some(line),
                },
            }]
        );
    }

    #[test]
    fn side_without_text_gets_a_caret_in_the_color_of_the_other_side() {
        let inserted = redline(&["keep"], &["keep more"]);
        let old_marks = &inserted.sheets[0].pages.old.as_ref().unwrap().marks;
        assert_eq!(
            old_marks.as_slice(),
            [Mark {
                bounds: cells(4..4),
                color: Rgb::ADDED,
                style: MarkStyle::Caret,
            }]
        );

        let deleted = redline(&["keep more"], &["keep"]);
        let new_marks = &deleted.sheets[0].pages.new.as_ref().unwrap().marks;
        assert_eq!(new_marks[0].color, Rgb::REMOVED);
        assert_eq!(new_marks[0].style, MarkStyle::Caret);
    }

    #[test]
    fn pages_are_labeled_with_their_document_and_number() {
        let old = ["unchanged opening", "this was old"];
        let new = ["unchanged opening", "an added page of text", "this was new"];
        let redline = redline(&old, &new);
        let sheet = redline.sheets.last().unwrap();
        assert_eq!(
            sheet.pages.old.as_ref().unwrap().label,
            "old.pdf — page 2 of 2"
        );
        assert_eq!(
            sheet.pages.new.as_ref().unwrap().label,
            "new.pdf — page 3 of 3"
        );
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
        // The first change runs on to a new page that shares no text with
        // an old page, which is shown alone.
        assert_eq!(
            shown_pages(&redline),
            [(Some(2), Some(2)), (None, Some(3)), (Some(4), Some(5))]
        );
    }

    #[test]
    fn page_that_a_change_continues_on_is_shown_next_to_its_partner() {
        let old = ["alpha beta", "gamma delta"];
        let new = ["alpha added words", "more beta", "gamma delta"];
        let redline = redline(&old, &new);
        // The second new page has `beta` of the first old page.
        assert_eq!(
            shown_pages(&redline),
            [(Some(0), Some(0)), (Some(0), Some(1))]
        );
        let has_both_ends =
            |connector: &Connector| connector.spans.old.is_some() && connector.spans.new.is_some();
        let mut connectors = redline.sheets.iter().flat_map(|sheet| &sheet.connectors);
        assert!(connectors.all(has_both_ends));
    }

    #[test]
    fn page_that_a_change_continues_on_is_shown_alone_without_a_partner() {
        let redline = redline(&["keep old", "words", "end"], &["keep end"]);
        assert_eq!(shown_pages(&redline), [(Some(0), Some(0)), (Some(1), None)]);
        // The band of the change ends in the gutter of the second sheet,
        // which does not show the place that the text was removed from.
        let line = Span {
            top: 0.0,
            bottom: CELL_HEIGHT,
        };
        assert_eq!(
            redline.sheets[1].connectors,
            [Connector {
                label: "1".to_owned(),
                spans: BySide {
                    old: Some(line),
                    new: None,
                },
            }]
        );
    }

    #[test]
    fn sheet_without_a_new_page_comes_where_its_old_page_belongs() {
        let old = [
            "first stays",
            "second stays",
            "third keep old",
            "words",
            "end",
        ];
        let new = ["first stays", "second changed", "third keep end"];
        let redline = redline(&old, &new);
        assert_eq!(
            shown_pages(&redline),
            [(Some(1), Some(1)), (Some(2), Some(2)), (Some(3), None)]
        );
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
                lines: vec![
                    SummaryLine {
                        sign: '−',
                        color: Rgb::REMOVED,
                        text: "old".to_owned(),
                    },
                    SummaryLine {
                        sign: '+',
                        color: Rgb::ADDED,
                        text: "new".to_owned(),
                    },
                ],
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
    fn document_without_text_has_no_page_on_the_sheets() {
        let no_pages: [&str; 0] = [];
        let redline = redline(&no_pages, &["\nall new"]);
        assert_eq!(shown_pages(&redline), [(None, Some(0))]);

        let [entry] = entries(&redline) else {
            panic!("{:?}", entries(&redline));
        };
        assert_eq!(entry.location, "new p. 1");
        assert_eq!(entry.lines.len(), 1);
        assert_eq!(entry.lines[0].sign, '+');
        assert_eq!(entry.target.top, LINE_PITCH);
    }

    #[test]
    fn summary_entry_links_to_the_old_side_of_a_change_that_is_nowhere_in_the_new_document() {
        // The new document has a page, but no text for a place to be next to.
        let redline = redline(&["\nall old", "more old"], &[""]);
        assert_eq!(shown_pages(&redline), [(Some(0), None), (Some(1), None)]);
        assert_eq!(entries(&redline)[0].location, "old p. 1");
        let first_sheet_second_line = Target {
            sheet_index: 0,
            top: LINE_PITCH,
        };
        assert_eq!(entries(&redline)[0].target, first_sheet_second_line);
    }

    #[test]
    fn sheets_without_a_new_page_say_nothing_about_the_new_pages() {
        let redline = redline(&["all old"], &["", ""]);
        assert_eq!(shown_pages(&redline), [(Some(0), None)]);
        assert_eq!(redline.sheets[0].note_above, None);
        assert_eq!(redline.sheets[0].note_below, None);
    }

    #[test]
    fn text_that_moved_is_marked_as_such_where_it_was_and_where_it_is() {
        let redline = redline(&[PARAGRAPH, REST], &[REST, PARAGRAPH]);
        // One sheet shows the page that the text was on next to the page
        // that it is on.
        assert_eq!(shown_pages(&redline), [(Some(0), Some(1))]);
        let sheet = &redline.sheets[0];
        let mark = Mark {
            bounds: cells(0..PARAGRAPH.len()),
            color: Rgb::MOVED,
            style: MarkStyle::Highlight,
        };
        assert_eq!(sheet.pages.old.as_ref().unwrap().marks, [mark]);
        assert_eq!(sheet.pages.new.as_ref().unwrap().marks, [mark]);
        assert_eq!(sheet.connectors.len(), 1);

        let [entry] = entries(&redline) else {
            panic!("{:?}", entries(&redline));
        };
        assert_eq!(entry.location, "old p. 1 / new p. 2");
        assert_eq!(
            entry.lines,
            [SummaryLine {
                sign: '⇄',
                color: Rgb::MOVED,
                text: PARAGRAPH.to_owned(),
            }]
        );
    }

    #[test]
    fn text_in_another_style_is_marked_as_such_on_both_pages() {
        let comparison =
            comparison_of_built_pages([page_with_bold("one two")], [page_with_bold("one *two*")]);
        let names = BySide {
            old: "old.pdf",
            new: "new.pdf",
        };
        let redline = Redline::of(&comparison, names);
        let mark = Mark {
            bounds: cells(4..7),
            color: Rgb::RESTYLED,
            style: MarkStyle::Highlight,
        };
        let sheet = &redline.sheets[0];
        assert_eq!(sheet.pages.old.as_ref().unwrap().marks, [mark]);
        assert_eq!(sheet.pages.new.as_ref().unwrap().marks, [mark]);
        assert_eq!(
            entries(&redline)[0].lines,
            [SummaryLine {
                sign: '≈',
                color: Rgb::RESTYLED,
                text: "weight 400 → 700: two".to_owned(),
            }]
        );
    }

    #[test]
    fn audit_shows_every_page_and_outlines_what_was_left_out() {
        let pages = |revision: &str| {
            let page = |text: &str| {
                PageBuilder::new()
                    .artifact()
                    .text("Title\n")
                    .body()
                    .text(text)
            };
            [
                page("first page stays"),
                page(&format!("second page is {revision}")),
                page("third page stays"),
            ]
        };
        let flows = BySide {
            old: raw_flow_of(pages("old")),
            new: raw_flow_of(pages("new")),
        };
        let comparison = Comparison::of(flows, &Filters::default());
        let names = BySide {
            old: "old.pdf",
            new: "new.pdf",
        };

        let redline = Redline::of(&comparison, names);
        assert_eq!(shown_pages(&redline), [(Some(1), Some(1))]);
        assert_eq!(redline.summary.as_ref().unwrap().legend, []);
        assert_eq!(redline.sheets[0].pages.new.as_ref().unwrap().marks.len(), 1);

        let audit = Redline::audit_of(&comparison, names);
        assert_eq!(
            shown_pages(&audit),
            [(Some(0), Some(0)), (Some(1), Some(1)), (Some(2), Some(2))]
        );
        assert_eq!(audit.summary.as_ref().unwrap().legend.len(), 1);
        let styles_on = |sheet: &Sheet| -> Vec<MarkStyle> {
            let marks = &sheet.pages.new.as_ref().unwrap().marks;
            marks.iter().map(|mark| mark.style).collect()
        };
        assert_eq!(styles_on(&audit.sheets[0]), [MarkStyle::Outline]);
        assert_eq!(
            styles_on(&audit.sheets[1]),
            [MarkStyle::Highlight, MarkStyle::Outline]
        );
        // Every page is shown, so there are no pages to say of that they
        // have no differences.
        let has_note = |sheet: &Sheet| sheet.note_above.is_some() || sheet.note_below.is_some();
        assert!(!audit.sheets.iter().any(has_note));
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
