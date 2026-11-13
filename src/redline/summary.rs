//! The pages at the front of the redline that list the changes.

use std::ops::Range;

use krilla::Document;
use krilla::annotation::{LinkAnnotation, Target as LinkTarget};
use krilla::destination::XyzDestination;
use krilla::page::PageSettings;

use super::text::TextFont;
use super::{Rgb, Summary, SummaryEntry, Target};
use crate::geometry::Size;
use crate::listing::shortened;

/// The page size of a summary without sheets to take it from: A4 on its side.
const DEFAULT_PAGE_SIZE: Size = Size {
    width: 841.89,
    height: 595.28,
};

const MARGIN: f32 = 36.0;
const TITLE_FONT_SIZE: f32 = 14.0;
const FONT_SIZE: f32 = 9.0;
const LINE_HEIGHT: f32 = 12.5;
/// The space that the title and the line under it take on the first page.
const HEADING_HEIGHT: f32 = 58.0;
const ENTRY_GAP: f32 = 5.0;

/// The right edge of the column of change numbers.
const NUMBER_RIGHT: f32 = MARGIN + 24.0;
const LOCATION_LEFT: f32 = MARGIN + 34.0;
const SIGN_LEFT: f32 = MARGIN + 170.0;
const TEXT_LEFT: f32 = SIGN_LEFT + 12.0;

const TEXT_COLOR: Rgb = Rgb::new(51, 51, 51);

/// How many characters of each text a bookmark shows.
const BOOKMARK_TEXT_CHARS: usize = 40;

/// How the entries of a summary are divided over pages.
pub(super) struct SummaryPages {
    page_size: Size,
    /// The entries on each page. There is always a first page.
    entries: Vec<Range<usize>>,
}

impl SummaryPages {
    /// Pages of the size of the sheets, so that the redline is uniform.
    pub fn of(summary: &Summary, sheet_size: Option<Size>) -> Self {
        let page_size = sheet_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let bottom = page_size.height - MARGIN;
        let mut entries: Vec<Range<usize>> = Vec::new();
        entries.push(0..0);
        let mut top = MARGIN + HEADING_HEIGHT;
        for (index, entry) in summary.entries.iter().enumerate() {
            let is_first_on_page = entries.last().is_some_and(Range::is_empty);
            if top + height_of(entry) > bottom && !is_first_on_page {
                entries.push(index..index);
                top = MARGIN;
            }
            top += height_of(entry) + ENTRY_GAP;
            entries.last_mut().expect("there is a first page").end = index + 1;
        }
        Self { page_size, entries }
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

fn height_of(entry: &SummaryEntry) -> f32 {
    lines_of(entry).count() as f32 * LINE_HEIGHT
}

/// The lines of an entry: the sign, the color and the text of each side
/// that has text.
fn lines_of(entry: &SummaryEntry) -> impl Iterator<Item = (&'static str, Rgb, &str)> {
    let lines = [
        ("−", Rgb::REMOVED, entry.old_text.as_str()),
        ("+", Rgb::ADDED, entry.new_text.as_str()),
    ];
    lines.into_iter().filter(|(_, _, text)| !text.is_empty())
}

pub(super) fn draw(
    document: &mut Document,
    font: &TextFont,
    summary: &Summary,
    pages: &SummaryPages,
    sheet_count: usize,
    destination_of: &dyn Fn(&Target) -> XyzDestination,
) {
    let Size { width, height } = pages.page_size;
    for (page_index, entries) in pages.entries.iter().enumerate() {
        let settings = PageSettings::from_wh(width, height).expect("a page has a positive size");
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        let mut links = Vec::new();

        let mut top = MARGIN;
        if page_index == 0 {
            let title = font.fitted(&summary.title, TITLE_FONT_SIZE, width - 2.0 * MARGIN);
            font.draw(
                &mut surface,
                (MARGIN, top + 14.0),
                TITLE_FONT_SIZE,
                TEXT_COLOR,
                &title,
            );
            let counts = counts_line(summary.entries.len(), sheet_count);
            font.draw(
                &mut surface,
                (MARGIN, top + 34.0),
                FONT_SIZE,
                TEXT_COLOR,
                &counts,
            );
            top += HEADING_HEIGHT;
        }
        for index in entries.clone() {
            let entry = &summary.entries[index];
            let baseline = top + FONT_SIZE;
            let number = (index + 1).to_string();
            let number_left = NUMBER_RIGHT - font.width(&number, FONT_SIZE);
            font.draw(
                &mut surface,
                (number_left, baseline),
                FONT_SIZE,
                TEXT_COLOR,
                &number,
            );
            font.draw(
                &mut surface,
                (LOCATION_LEFT, baseline),
                FONT_SIZE,
                TEXT_COLOR,
                &entry.location,
            );
            for (line, (sign, color, text)) in lines_of(entry).enumerate() {
                let baseline = baseline + line as f32 * LINE_HEIGHT;
                let text = font.fitted(text, FONT_SIZE, width - MARGIN - TEXT_LEFT);
                font.draw(&mut surface, (SIGN_LEFT, baseline), FONT_SIZE, color, sign);
                font.draw(&mut surface, (TEXT_LEFT, baseline), FONT_SIZE, color, &text);
            }
            let bottom = top + height_of(entry);
            let area = krilla::geom::Rect::from_ltrb(MARGIN, top, width - MARGIN, bottom);
            links.extend(area.map(|area| (area, destination_of(&entry.target))));
            top = bottom + ENTRY_GAP;
        }

        surface.finish();
        for (area, destination) in links {
            let target = LinkTarget::Destination(destination.into());
            page.add_annotation(LinkAnnotation::new(area, target).into());
        }
        page.finish();
    }
}

fn counts_line(change_count: usize, sheet_count: usize) -> String {
    let plural = |count: usize| if count == 1 { "" } else { "s" };
    match change_count {
        0 => "No differences".to_owned(),
        _ => format!(
            "{change_count} change{} on {sheet_count} sheet{}. \
             Removed text is red on the left, added text is green on the right.",
            plural(change_count),
            plural(sheet_count),
        ),
    }
}

/// The title of the bookmark of the entry at `index`.
pub(super) fn bookmark_title(index: usize, entry: &SummaryEntry) -> String {
    let old = shortened(&entry.old_text, BOOKMARK_TEXT_CHARS);
    let new = shortened(&entry.new_text, BOOKMARK_TEXT_CHARS);
    let texts = match (old.is_empty(), new.is_empty()) {
        (true, _) => format!("+ {new}"),
        (_, true) => format!("− {old}"),
        _ => format!("{old} → {new}"),
    };
    format!("{}. {texts}", index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(old_text: &str, new_text: &str) -> SummaryEntry {
        SummaryEntry {
            location: "old p. 1 / new p. 1".to_owned(),
            old_text: old_text.to_owned(),
            new_text: new_text.to_owned(),
            target: Target {
                sheet_index: 0,
                top: 0.0,
            },
        }
    }

    fn summary_of(entries: Vec<SummaryEntry>) -> Summary {
        Summary {
            title: "old → new".to_owned(),
            entries,
        }
    }

    #[test]
    fn summary_without_entries_has_one_page() {
        let pages = SummaryPages::of(&summary_of(Vec::new()), None);
        assert_eq!(pages.count(), 1);
        assert!(pages.entries[0].is_empty());
        assert_eq!(pages.page_size, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn entries_that_do_not_fit_go_to_the_next_page() {
        let entry_pitch = LINE_HEIGHT + ENTRY_GAP;
        let page_size = Size {
            width: 600.0,
            // Room for three entries under the heading, which is as high as
            // three more and a bit.
            height: 2.0 * MARGIN + HEADING_HEIGHT + 3.0 * entry_pitch,
        };
        assert_eq!((HEADING_HEIGHT / entry_pitch).floor(), 3.0);
        let entries = (0..10).map(|_| entry("old", "")).collect();
        let pages = SummaryPages::of(&summary_of(entries), Some(page_size));
        assert_eq!(pages.entries, [0..3, 3..9, 9..10]);
    }

    #[test]
    fn entry_with_both_texts_takes_two_lines() {
        assert_eq!(
            height_of(&entry("old", "new")),
            2.0 * height_of(&entry("", "new"))
        );
    }

    #[test]
    fn entry_taller_than_a_page_still_gets_one() {
        let page_size = Size {
            width: 600.0,
            height: 2.0 * MARGIN,
        };
        let entries = vec![entry("old", "new"), entry("old", "new")];
        let pages = SummaryPages::of(&summary_of(entries), Some(page_size));
        assert_eq!(pages.entries, [0..1, 1..2]);
    }

    #[test]
    fn bookmark_names_the_change() {
        assert_eq!(bookmark_title(0, &entry("old", "new")), "1. old → new");
        assert_eq!(bookmark_title(1, &entry("", "new")), "2. + new");
        assert_eq!(bookmark_title(2, &entry("old", "")), "3. − old");
    }

    #[test]
    fn counts_agree_in_number() {
        assert_eq!(counts_line(0, 0), "No differences");
        assert!(counts_line(1, 1).starts_with("1 change on 1 sheet."));
        assert!(counts_line(2, 3).starts_with("2 changes on 3 sheets."));
    }
}
