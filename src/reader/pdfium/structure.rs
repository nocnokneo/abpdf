//! The structure tree of a tagged page.

use std::collections::HashMap;
use std::ffi::c_int;

use super::handles::{LoadedPage, StructureElement};

/// A structure tree that nests deeper than this is cut off, so that a
/// malformed tree cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// What the structure tree says about a piece of marked content.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Structure {
    /// The types of the elements that contain the content, outermost first
    /// and separated by `/`.
    pub path: String,
    /// The table cell that contains the content: its number among the cells
    /// of the page, which are counted in the order of the tree.
    pub cell: Option<usize>,
}

impl Structure {
    /// Whether the element that the content is directly in is a figure.
    pub fn is_figure(&self) -> bool {
        self.path.rsplit('/').next() == Some("Figure")
    }
}

/// The structure of each piece of marked content on `page`, by its marked
/// content identifier.
pub(super) fn by_marked_content_id(page: &LoadedPage) -> HashMap<c_int, Structure> {
    let mut reader = TreeReader::default();
    if let Some(tree) = page.structure_tree() {
        for element in tree.children() {
            reader.read(&element, "", None, 0);
        }
    }
    reader.structures
}

#[derive(Default)]
struct TreeReader {
    structures: HashMap<c_int, Structure>,
    cell_count: usize,
}

impl TreeReader {
    /// Reads an element and the elements inside it.
    /// `parent_cell` is the table cell that the element is in, if it is in
    /// one.
    fn read(
        &mut self,
        element: &StructureElement,
        parent_path: &str,
        parent_cell: Option<usize>,
        depth: usize,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        let type_name = element.type_name();
        let cell = match type_name.as_str() {
            "TD" | "TH" => {
                self.cell_count += 1;
                Some(self.cell_count - 1)
            }
            _ => parent_cell,
        };
        let path = if parent_path.is_empty() {
            type_name
        } else {
            format!("{parent_path}/{type_name}")
        };
        for id in element.marked_content_ids() {
            self.structures.entry(id).or_insert_with(|| Structure {
                path: path.clone(),
                cell,
            });
        }
        for child in element.children() {
            self.read(&child, &path, cell, depth + 1);
        }
    }
}
