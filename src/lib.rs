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
mod side;
#[cfg(test)]
mod testing;

pub use self::side::{BySide, Side};
