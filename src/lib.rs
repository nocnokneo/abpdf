//! Compare two PDFs and get a side-by-side redline PDF.
//!
//! The comparison is a pipeline; `docs/design.md` has the reasons behind it:
//!
//! 1. A [`reader`] reports the text of each page.
//! 2. A [`flow`] is that text as one sequence of tokens across pages.
//! 3. [`flow::prepare`] leaves page furniture out of the comparison, as a
//!    [`filter`] says, and rejoins the words that a line break splits.
//! 4. A [`diff`] finds where the tokens of the two documents differ.
//! 5. A [`change::Comparison`] says what each difference is and where.
//!
//! The outputs are rendered from the comparison: a [`redline`] PDF, a
//! [`listing`] as text and [`json`].

pub mod change;
pub mod diff;
mod excerpt;
pub mod filter;
pub mod flow;
pub mod geometry;
pub mod json;
pub mod listing;
pub mod profile;
pub mod reader;
pub mod redline;
#[cfg(test)]
mod testing;

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
