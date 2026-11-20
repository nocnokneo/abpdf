//! The structure tree of a tagged page.

use std::collections::HashMap;
use std::ffi::c_int;

use super::handles::{LoadedPage, StructureElement};

/// A structure tree that nests deeper than this is cut off, so that a
/// malformed tree cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// The structure path of each piece of marked content on `page`, by its
/// marked content identifier.
pub(super) fn paths_by_marked_content_id(page: &LoadedPage) -> HashMap<c_int, String> {
    let mut paths = HashMap::new();
    if let Some(tree) = page.structure_tree() {
        for element in tree.children() {
            collect(&element, "", 0, &mut paths);
        }
    }
    paths
}

fn collect(
    element: &StructureElement,
    parent_path: &str,
    depth: usize,
    paths: &mut HashMap<c_int, String>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let type_name = element.type_name();
    let path = if parent_path.is_empty() {
        type_name
    } else {
        format!("{parent_path}/{type_name}")
    };
    for id in element.marked_content_ids() {
        paths.entry(id).or_insert_with(|| path.clone());
    }
    for child in element.children() {
        collect(&child, &path, depth + 1, paths);
    }
}

/// PROTOTYPE: the number of the table cell that each piece of marked content
/// on `page` is in, by its marked content identifier.
pub(super) fn cells_by_marked_content_id(page: &LoadedPage) -> HashMap<c_int, usize> {
    let mut cells = HashMap::new();
    let mut cell_count = 0;
    if let Some(tree) = page.structure_tree() {
        for element in tree.children() {
            collect_cells(&element, None, 0, &mut cell_count, &mut cells);
        }
    }
    cells
}

fn collect_cells(
    element: &StructureElement,
    mut cell: Option<usize>,
    depth: usize,
    cell_count: &mut usize,
    cells: &mut HashMap<c_int, usize>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    if matches!(element.type_name().as_str(), "TD" | "TH") {
        cell = Some(*cell_count);
        *cell_count += 1;
    }
    if let Some(cell) = cell {
        for id in element.marked_content_ids() {
            cells.entry(id).or_insert(cell);
        }
    }
    for child in element.children() {
        collect_cells(&child, cell, depth + 1, cell_count, cells);
    }
}
