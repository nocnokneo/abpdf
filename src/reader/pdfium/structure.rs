//! The structure tree of a tagged page.

use std::collections::HashMap;
use std::ffi::{c_int, c_ulong, c_void};

use pdfium_render::prelude::{FPDF_PAGE, FPDF_STRUCTELEMENT};

use super::library::Library;
use super::strings;

/// A structure tree that nests deeper than this is cut off, so that a
/// malformed tree cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// The structure path of each piece of marked content on `page`, by its
/// marked content identifier.
///
/// # Safety
///
/// `page` must be a loaded page.
pub(super) unsafe fn paths_by_marked_content_id(
    pdfium: &Library,
    page: FPDF_PAGE,
) -> HashMap<c_int, String> {
    let mut paths = HashMap::new();
    // SAFETY: the tree and its elements are only used before the tree is closed.
    unsafe {
        let tree = pdfium.FPDF_StructTree_GetForPage(page);
        if tree.is_null() {
            return paths;
        }
        for index in 0..pdfium.FPDF_StructTree_CountChildren(tree) {
            let child = pdfium.FPDF_StructTree_GetChildAtIndex(tree, index);
            collect(pdfium, child, "", 0, &mut paths);
        }
        pdfium.FPDF_StructTree_Close(tree);
    }
    paths
}

/// # Safety
///
/// `element` must be null or belong to a structure tree that is still open.
unsafe fn collect(
    pdfium: &Library,
    element: FPDF_STRUCTELEMENT,
    parent_path: &str,
    depth: usize,
    paths: &mut HashMap<c_int, String>,
) {
    if element.is_null() || depth > MAX_DEPTH {
        return;
    }
    // SAFETY: `element` is a live element, per this function's contract.
    unsafe {
        let element_type = strings::utf16(|buffer, byte_capacity| {
            let byte_length: c_ulong =
                pdfium.FPDF_StructElement_GetType(element, buffer as *mut c_void, byte_capacity);
            (byte_length > 0).then_some(byte_length)
        })
        .unwrap_or_default();
        let path = if parent_path.is_empty() {
            element_type
        } else {
            format!("{parent_path}/{element_type}")
        };
        for index in 0..pdfium.FPDF_StructElement_GetMarkedContentIdCount(element) {
            let id = pdfium.FPDF_StructElement_GetMarkedContentIdAtIndex(element, index);
            if id >= 0 {
                paths.entry(id).or_insert_with(|| path.clone());
            }
        }
        for index in 0..pdfium.FPDF_StructElement_CountChildren(element) {
            let child = pdfium.FPDF_StructElement_GetChildAtIndex(element, index);
            collect(pdfium, child, &path, depth + 1, paths);
        }
    }
}
