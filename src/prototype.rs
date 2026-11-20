//! PROTOTYPE, not for merging: switches for the options of step 4 that are
//! shown side by side before one is chosen.
//! Each is on when its environment variable is set.

use std::env;

/// Areas of a change and outlines of the audit end where text that is not
/// part of them stands between their tokens.
pub fn splits_around_left_out_text() -> bool {
    env::var_os("ABPDF_PROTOTYPE_SPLIT_AREAS").is_some()
}

/// A replacement that covers several table cells is one change per cell.
pub fn splits_changes_by_cell() -> bool {
    env::var_os("ABPDF_PROTOTYPE_CELLS").is_some()
}

/// Which graphics are compared.
pub fn figures() -> Figures {
    match env::var("ABPDF_PROTOTYPE_FIGURES").as_deref() {
        Ok("grouped") => Figures::Grouped,
        Ok("all") => Figures::All,
        _ => Figures::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Figures {
    None,
    /// Images, and vector graphics that the producer keeps together.
    Grouped,
    /// Also shapes that are drawn on the page one by one.
    All,
}
