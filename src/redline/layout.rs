//! Where things go on a sheet.

use crate::geometry::{Rect, Size};
use crate::{BySide, Side};

const MARGIN: f32 = 18.0;
/// The space between the two pages, which connectors cross.
const GUTTER: f32 = 70.0;
/// The space above the pages, for their labels.
const BANNER: f32 = 34.0;

/// The geometry of one sheet, in the sheet's own page space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetLayout {
    pub size: Size,
    /// The part of the sheet that shows the page of each side.
    pub frames: BySide<Rect>,
}

impl SheetLayout {
    /// Lays out a sheet for pages of these sizes, at their original scale.
    /// A side without a page gets a frame as large as the other side's.
    pub fn new(page_sizes: BySide<Option<Size>>) -> Self {
        let fallback = page_sizes.old.or(page_sizes.new).unwrap_or(Size {
            width: 0.0,
            height: 0.0,
        });
        let page_sizes = page_sizes.map(|size| size.unwrap_or(fallback));
        let old = frame(MARGIN, page_sizes.old);
        let new = frame(old.right + GUTTER, page_sizes.new);
        Self {
            size: Size {
                width: new.right + MARGIN,
                height: old.bottom.max(new.bottom) + MARGIN,
            },
            frames: BySide { old, new },
        }
    }

    /// Where a rectangle on the page of one side is on the sheet.
    pub fn place(&self, side: Side, on_page: Rect) -> Rect {
        let frame = self.frames[side];
        Rect {
            left: frame.left + on_page.left,
            top: frame.top + on_page.top,
            right: frame.left + on_page.right,
            bottom: frame.top + on_page.bottom,
        }
    }

    /// The baseline of the label above a frame.
    pub(super) fn label_baseline(&self) -> f32 {
        BANNER - 20.0
    }

    /// The baseline of the note above the frames.
    pub(super) fn note_above_baseline(&self) -> f32 {
        BANNER - 7.0
    }

    /// The baseline of the note below the frames.
    pub(super) fn note_below_baseline(&self) -> f32 {
        self.size.height - 6.0
    }

    /// The horizontal extent of the gutter between the frames.
    pub(super) fn gutter(&self) -> (f32, f32) {
        (self.frames.old.right, self.frames.new.left)
    }
}

fn frame(left: f32, page: Size) -> Rect {
    Rect {
        left,
        top: BANNER,
        right: left + page.width,
        bottom: BANNER + page.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTRAIT: Size = Size {
        width: 200.0,
        height: 300.0,
    };
    const LANDSCAPE: Size = Size {
        width: 300.0,
        height: 200.0,
    };

    fn layout_of(old: Option<Size>, new: Option<Size>) -> SheetLayout {
        SheetLayout::new(BySide { old, new })
    }

    #[test]
    fn frames_are_side_by_side_at_original_size() {
        let BySide { old, new } = layout_of(Some(PORTRAIT), Some(LANDSCAPE)).frames;
        assert_eq!((old.width(), old.height()), (200.0, 300.0));
        assert_eq!((new.width(), new.height()), (300.0, 200.0));
        assert_eq!(new.left - old.right, GUTTER);
        assert_eq!(old.top, new.top);
    }

    #[test]
    fn sheet_fits_the_taller_page() {
        let layout = layout_of(Some(PORTRAIT), Some(LANDSCAPE));
        assert_eq!(layout.size.height, BANNER + 300.0 + MARGIN);
        assert_eq!(layout.size.width, MARGIN + 200.0 + GUTTER + 300.0 + MARGIN);
    }

    #[test]
    fn missing_side_gets_a_frame_like_the_other() {
        let layout = layout_of(None, Some(LANDSCAPE));
        assert_eq!(layout.frames.old.width(), 300.0);
        assert_eq!(layout, layout_of(Some(LANDSCAPE), None));
    }

    #[test]
    fn place_offsets_by_the_frame() {
        let layout = layout_of(Some(PORTRAIT), Some(PORTRAIT));
        let on_page = Rect {
            left: 10.0,
            top: 20.0,
            right: 30.0,
            bottom: 40.0,
        };
        let on_sheet = layout.place(Side::New, on_page);
        let frame = layout.frames.new;
        assert_eq!(on_sheet.left, frame.left + 10.0);
        assert_eq!(on_sheet.bottom, frame.top + 40.0);
    }
}
