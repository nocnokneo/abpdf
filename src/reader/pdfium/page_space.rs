//! Conversion from PDF user space to [page space](crate::geometry).

use crate::geometry::{Rect, Size};

/// A rectangle in PDF user space, where y grows upward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct UserSpaceBox {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

/// Maps PDF user space positions on one page to page space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageSpace {
    /// The part of the page that a viewer shows.
    visible: UserSpaceBox,
    clockwise_quarter_turns: u8,
}

impl PageSpace {
    /// `clockwise_quarter_turns` is the page's `/Rotate` entry divided by 90.
    pub fn new(visible: UserSpaceBox, clockwise_quarter_turns: i32) -> Self {
        Self {
            visible,
            clockwise_quarter_turns: clockwise_quarter_turns.rem_euclid(4) as u8,
        }
    }

    pub fn size(&self) -> Size {
        let width = (self.visible.right - self.visible.left) as f32;
        let height = (self.visible.top - self.visible.bottom) as f32;
        if self.clockwise_quarter_turns.is_multiple_of(2) {
            Size { width, height }
        } else {
            Size {
                width: height,
                height: width,
            }
        }
    }

    pub fn point(&self, x: f64, y: f64) -> (f32, f32) {
        let UserSpaceBox {
            left,
            bottom,
            right,
            top,
        } = self.visible;
        let (page_x, page_y) = match self.clockwise_quarter_turns {
            0 => (x - left, top - y),
            1 => (y - bottom, x - left),
            2 => (right - x, y - bottom),
            _ => (top - y, right - x),
        };
        (page_x as f32, page_y as f32)
    }

    pub fn rect(&self, user_space: UserSpaceBox) -> Rect {
        let corners = [
            self.point(user_space.left, user_space.bottom),
            self.point(user_space.right, user_space.top),
        ];
        Rect::bounding(corners).expect("two points have bounds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page whose visible box is 200 wide and 300 high, away from the origin.
    const VISIBLE: UserSpaceBox = UserSpaceBox {
        left: 10.0,
        bottom: 20.0,
        right: 210.0,
        top: 320.0,
    };

    /// A point 30 right of and 40 below the top left corner of the unrotated page.
    const POINT: (f64, f64) = (40.0, 280.0);

    fn convert(clockwise_quarter_turns: i32) -> (Size, (f32, f32)) {
        let page_space = PageSpace::new(VISIBLE, clockwise_quarter_turns);
        (page_space.size(), page_space.point(POINT.0, POINT.1))
    }

    const PORTRAIT: Size = Size {
        width: 200.0,
        height: 300.0,
    };
    const LANDSCAPE: Size = Size {
        width: 300.0,
        height: 200.0,
    };

    #[test]
    fn unrotated_page_measures_from_the_top_left_of_the_visible_box() {
        assert_eq!(convert(0), (PORTRAIT, (30.0, 40.0)));
    }

    #[test]
    fn quarter_turn_moves_the_top_left_corner_to_the_top_right() {
        assert_eq!(convert(1), (LANDSCAPE, (260.0, 30.0)));
    }

    #[test]
    fn half_turn_moves_the_top_left_corner_to_the_bottom_right() {
        assert_eq!(convert(2), (PORTRAIT, (170.0, 260.0)));
    }

    #[test]
    fn three_quarter_turn_moves_the_top_left_corner_to_the_bottom_left() {
        assert_eq!(convert(3), (LANDSCAPE, (40.0, 170.0)));
    }

    #[test]
    fn rotation_is_taken_modulo_a_full_turn() {
        assert_eq!(convert(-1), convert(3));
        assert_eq!(convert(5), convert(1));
    }

    #[test]
    fn rect_stays_normalized_under_rotation() {
        let rect = PageSpace::new(VISIBLE, 1).rect(UserSpaceBox {
            left: 40.0,
            bottom: 270.0,
            right: 60.0,
            top: 280.0,
        });
        assert_eq!(
            rect,
            Rect {
                left: 250.0,
                top: 30.0,
                right: 260.0,
                bottom: 50.0
            }
        );
    }
}
