//! Compare two PDFs and get a side-by-side redline PDF.
//!
//! The comparison is a pipeline; `docs/design.md` has the reasons behind it:
//!
//! 1. A [`reader`] reports the text of each page.
//! 2. A [`flow`] is that text as one sequence of tokens across pages.

pub mod flow;
pub mod geometry;
pub mod reader;
