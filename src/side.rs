//! The two documents of a comparison, and what each of them has.

use std::fmt;
use std::ops::Index;

/// One of the two documents that are compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Old,
    New,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Old, Side::New];
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Side::Old => "old",
            Side::New => "new",
        })
    }
}

/// A value for each of the two documents.
///
/// What belongs to the old and to the new document travels as one value, so
/// that the two cannot change places on the way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BySide<T> {
    pub old: T,
    pub new: T,
}

impl<T> BySide<T> {
    /// What `value_of` gives for each side.
    pub fn from_fn(mut value_of: impl FnMut(Side) -> T) -> Self {
        Self {
            old: value_of(Side::Old),
            new: value_of(Side::New),
        }
    }

    pub fn as_ref(&self) -> BySide<&T> {
        BySide {
            old: &self.old,
            new: &self.new,
        }
    }

    pub fn map<U>(self, mut convert: impl FnMut(T) -> U) -> BySide<U> {
        BySide {
            old: convert(self.old),
            new: convert(self.new),
        }
    }
}

impl<T> Index<Side> for BySide<T> {
    type Output = T;

    fn index(&self, side: Side) -> &T {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_the_value_of_that_side() {
        let names = BySide {
            old: "a.pdf",
            new: "b.pdf",
        };
        assert_eq!(names[Side::Old], "a.pdf");
        assert_eq!(names[Side::New], "b.pdf");
    }

    #[test]
    fn from_fn_asks_for_each_side() {
        let names = BySide::from_fn(|side| format!("{side}.pdf"));
        assert_eq!(names.old, "old.pdf");
        assert_eq!(names.new, "new.pdf");
    }

    #[test]
    fn map_keeps_each_value_on_its_side() {
        let lengths = BySide {
            old: "one",
            new: "three",
        }
        .map(str::len);
        assert_eq!(lengths, BySide { old: 3, new: 5 });
    }
}
