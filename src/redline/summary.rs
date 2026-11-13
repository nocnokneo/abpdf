//! The pages at the front of the redline that list the changes.

use krilla::Document;
use krilla::annotation::{LinkAnnotation, Target as LinkTarget};
use krilla::destination::XyzDestination;
use krilla::page::PageSettings;
use krilla::surface::Surface;

use super::text::TextFont;
use super::{Rgb, Summary, SummaryEntry};
use crate::excerpt::shortened;
use crate::geometry::Size;

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

/// Where the entries of a summary go.
pub(super) struct SummaryPages {
    page_size: Size,
    /// The entries on each page. There is always a first page.
    pages: Vec<Vec<PlacedEntry>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacedEntry {
    /// Index into [`Summary::entries`].
    index: usize,
    /// The distance of the entry from the top edge of its page.
    top: f32,
}

impl SummaryPages {
    /// Pages of the size of the sheets, so that the redline is uniform.
    pub fn of(summary: &Summary, sheet_size: Option<Size>) -> Self {
        let page_size = sheet_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let bottom = page_size.height - MARGIN;
        let mut pages = vec![Vec::new()];
        let mut top = MARGIN + HEADING_HEIGHT;
        for (index, entry) in summary.entries.iter().enumerate() {
            let is_first_on_page = pages.last().is_some_and(Vec::is_empty);
            if top + height_of(entry) > bottom && !is_first_on_page {
                pages.push(Vec::new());
                top = MARGIN;
            }
            let page = pages.last_mut().expect("there is a first page");
            page.push(PlacedEntry { index, top });
            top += height_of(entry) + ENTRY_GAP;
        }
        Self { page_size, pages }
    }

    pub fn count(&self) -> usize {
        self.pages.len()
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

/// Draws the pages. `destinations` has, for each entry, the view of the
/// redline that it links to.
pub(super) fn draw(
    document: &mut Document,
    font: &TextFont,
    summary: &Summary,
    pages: &SummaryPages,
    sheet_count: usize,
    destinations: &[XyzDestination],
) {
    let Size { width, height } = pages.page_size;
    for (page_index, placed_entries) in pages.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(width, height).expect("a page has a positive size");
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        if page_index == 0 {
            draw_heading(&mut surface, font, summary, sheet_count, width);
        }
        for placed in placed_entries {
            draw_entry(&mut surface, font, summary, placed, width);
        }
        surface.finish();

        for placed in placed_entries {
            let bottom = placed.top + height_of(&summary.entries[placed.index]);
            let area = krilla::geom::Rect::from_ltrb(MARGIN, placed.top, width - MARGIN, bottom);
            if let Some(area) = area {
                let target = LinkTarget::Destination(destinations[placed.index].clone().into());
                page.add_annotation(LinkAnnotation::new(area, target).into());
            }
        }
        page.finish();
    }
}

fn draw_heading(
    surface: &mut Surface,
    font: &TextFont,
    summary: &Summary,
    sheet_count: usize,
    page_width: f32,
) {
    let title = font.fitted(&summary.title, TITLE_FONT_SIZE, page_width - 2.0 * MARGIN);
    let title_position = (MARGIN, MARGIN + 14.0);
    font.draw(surface, title_position, TITLE_FONT_SIZE, TEXT_COLOR, &title);
    let counts = counts_line(summary.entries.len(), sheet_count);
    let counts_position = (MARGIN, MARGIN + 34.0);
    font.draw(surface, counts_position, FONT_SIZE, TEXT_COLOR, &counts);
}

fn draw_entry(
    surface: &mut Surface,
    font: &TextFont,
    summary: &Summary,
    placed: &PlacedEntry,
    page_width: f32,
) {
    let entry = &summary.entries[placed.index];
    let baseline = placed.top + FONT_SIZE;
    let number = (placed.index + 1).to_string();
    let number_left = NUMBER_RIGHT - font.width(&number, FONT_SIZE);
    font.draw(
        surface,
        (number_left, baseline),
        FONT_SIZE,
        TEXT_COLOR,
        &number,
    );
    let location_position = (LOCATION_LEFT, baseline);
    font.draw(
        surface,
        location_position,
        FONT_SIZE,
        TEXT_COLOR,
        &entry.location,
    );
    for (line, (sign, color, text)) in lines_of(entry).enumerate() {
        let baseline = baseline + line as f32 * LINE_HEIGHT;
        let text = font.fitted(text, FONT_SIZE, page_width - MARGIN - TEXT_LEFT);
        font.draw(surface, (SIGN_LEFT, baseline), FONT_SIZE, color, sign);
        font.draw(surface, (TEXT_LEFT, baseline), FONT_SIZE, color, &text);
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
    use crate::redline::Target;

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

    /// The indices of the entries on each page.
    fn entries_by_page(pages: &SummaryPages) -> Vec<Vec<usize>> {
        let indices = |page: &Vec<PlacedEntry>| page.iter().map(|placed| placed.index).collect();
        pages.pages.iter().map(indices).collect()
    }

    #[test]
    fn summary_without_entries_has_one_page() {
        let pages = SummaryPages::of(&summary_of(Vec::new()), None);
        assert_eq!(entries_by_page(&pages), [Vec::<usize>::new()]);
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
        assert_eq!(
            entries_by_page(&pages),
            [vec![0, 1, 2], vec![3, 4, 5, 6, 7, 8], vec![9]]
        );
    }

    #[test]
    fn entries_start_under_the_heading_on_the_first_page_only() {
        let page_size = Size {
            width: 600.0,
            height: 2.0 * MARGIN + HEADING_HEIGHT + LINE_HEIGHT,
        };
        let entries = vec![entry("old", ""), entry("old", ""), entry("old", "")];
        let pages = SummaryPages::of(&summary_of(entries), Some(page_size));
        let tops: Vec<Vec<f32>> = pages
            .pages
            .iter()
            .map(|page| page.iter().map(|placed| placed.top).collect())
            .collect();
        assert_eq!(
            tops,
            [
                vec![MARGIN + HEADING_HEIGHT],
                vec![MARGIN, MARGIN + LINE_HEIGHT + ENTRY_GAP]
            ]
        );
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
        assert_eq!(entries_by_page(&pages), [vec![0], vec![1]]);
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
