//! The objects that a page draws, and those inside its forms.

use std::collections::HashMap;

use super::handles::{LoadedPage, ObjectKind, PageObject};
use crate::reader::matrix::Matrix;

/// Forms inside forms deeper than this are not looked into, so that a
/// malformed PDF cannot exhaust the stack.
const MAX_FORM_DEPTH: usize = 16;

/// The objects of a page in the order in which it draws them.
///
/// A form is one of them, and stands for the objects that it draws in turn.
pub(super) struct PageObjects<'a> {
    objects: Vec<PageObject<'a>>,
    /// For each of the objects and for each object inside a form, by its
    /// identity: the index of the object of the page that it is, or that it
    /// is inside of.
    index_by_identity: HashMap<usize, usize>,
}

impl<'a> PageObjects<'a> {
    pub fn of(page: &LoadedPage<'a>) -> Self {
        let objects: Vec<PageObject> = page.objects().collect();
        let mut index_by_identity = HashMap::new();
        for (index, object) in objects.iter().enumerate() {
            index_by_identity.insert(object.identity(), index);
            visit_parts(object, &Matrix::IDENTITY, 0, &mut |part, _| {
                index_by_identity.insert(part.identity(), index);
            });
        }
        Self {
            objects,
            index_by_identity,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &PageObject<'a>> {
        self.objects.iter()
    }

    pub fn get(&self, index: usize) -> &PageObject<'a> {
        &self.objects[index]
    }

    /// The index of the object of the page that `object` is, or is inside
    /// of.
    pub fn index_of(&self, object: &PageObject) -> Option<usize> {
        self.index_by_identity.get(&object.identity()).copied()
    }

    /// The form of the page that `object` is inside of, if it is inside of
    /// one.
    pub fn form_around(&self, object: &PageObject) -> Option<&PageObject<'a>> {
        let around = &self.objects[self.index_of(object)?];
        (around.identity() != object.identity()).then_some(around)
    }

    /// Calls `visit` for each object that the object at `index` draws: for
    /// the object itself, or for the objects inside it if it is a form.
    /// With each object comes the matrix that takes the space it is drawn
    /// in to the space of the page.
    pub fn visit_parts_of(&self, index: usize, visit: &mut impl FnMut(&PageObject<'a>, &Matrix)) {
        visit_parts(&self.objects[index], &Matrix::IDENTITY, 0, visit);
    }
}

/// Calls `visit` for an object that is no form, and for the objects inside
/// one that is.
/// `to_page` takes the space that `object` is drawn in to that of the page.
fn visit_parts<'a>(
    object: &PageObject<'a>,
    to_page: &Matrix,
    depth: usize,
    visit: &mut impl FnMut(&PageObject<'a>, &Matrix),
) {
    if object.kind() != ObjectKind::Form {
        visit(object, to_page);
        return;
    }
    if depth >= MAX_FORM_DEPTH {
        return;
    }
    let form_to_page = object.matrix().then(to_page);
    for part in object.form_objects() {
        visit_parts(&part, &form_to_page, depth + 1, visit);
    }
}
