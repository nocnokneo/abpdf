//! Positions on a page.
//!
//! Every position in abpdf is in *page space*: the page as a viewer shows it,
//! after the crop box and the page rotation have been applied.
//! The origin is the top left corner, x grows to the right and y grows
//! downward.
//! The unit is the PDF user space unit, 1/72 inch unless the page sets a
//! `UserUnit`, which abpdf does not apply.
//!
//! A [reader](crate::reader) converts into page space, so nothing downstream
//! needs to know about page boxes or rotation.

/// The width and height of a page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// An axis-aligned rectangle, with `left <= right` and `top <= bottom`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    /// The smallest rectangle that contains all `points`, or `None` without points.
    pub fn bounding(points: impl IntoIterator<Item = (f32, f32)>) -> Option<Rect> {
        points
            .into_iter()
            .map(|(x, y)| Rect {
                left: x,
                top: y,
                right: x,
                bottom: y,
            })
            .reduce(|bounds, point| bounds.union(&point))
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// The smallest rectangle that contains both `self` and `other`.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_covers_every_point() {
        let bounds = Rect::bounding([(3.0, 8.0), (1.0, 9.0), (2.0, 7.0)]).unwrap();
        assert_eq!(
            bounds,
            Rect {
                left: 1.0,
                top: 7.0,
                right: 3.0,
                bottom: 9.0
            }
        );
        assert_eq!((bounds.width(), bounds.height()), (2.0, 2.0));
    }

    #[test]
    fn bounding_of_nothing_is_none() {
        assert_eq!(Rect::bounding([]), None);
    }
}
