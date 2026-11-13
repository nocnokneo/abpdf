//! Compare two PDFs and get a side-by-side redline PDF.
//!
//! The comparison is a pipeline; `docs/design.md` has the reasons behind it:
//!
//! 1. A [`reader`] reports the text of each page.
//! 2. A [`flow`] is that text as one sequence of tokens across pages.
//! 3. A [`redline`] shows pages of the two documents side by side, with marks
//!    drawn over them.

pub mod flow;
pub mod geometry;
pub mod reader;
pub mod redline;

use std::fmt;

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
