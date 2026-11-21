//! The marked content that the objects of a page are drawn in.

use std::rc::Rc;

use hayro_syntax::object::{self, Dict, Name};

use crate::reader::text_string;

/// What a piece of marked content says about the objects in it.
#[derive(Debug, PartialEq)]
pub(in crate::reader) struct Mark {
    /// What the content is marked as, such as `P` or `Artifact`.
    pub tag: String,
    /// The number that ties the content to the structure tree of its page.
    /// `None` in a form, whose marked content is numbered apart from that
    /// of the page.
    pub marked_content_id: Option<i32>,
    /// The text that the content stands for, if the producer says that it
    /// is not what its glyphs map to.
    pub actual_text: Option<String>,
    /// What kind of artifact the content is, such as `Pagination` or
    /// `Layout`, if it says so.
    pub artifact_type: Option<String>,
}

impl Mark {
    /// `properties` is the property list of the content, if it has one.
    /// `is_in_form` says whether a form draws the content, and not the
    /// page itself.
    pub fn new(tag: &Name, properties: Option<&Dict>, is_in_form: bool) -> Self {
        let id = properties.and_then(|properties| properties.get::<i32>("MCID"));
        let actual_text = properties
            .and_then(|properties| properties.get::<object::String>("ActualText"))
            .map(|text| text_string::decode(text.as_bytes()));
        let artifact_type = properties
            .and_then(|properties| properties.get::<Name>("Type"))
            .map(|name| name.as_str().to_owned());
        Self {
            tag: tag.as_str().to_owned(),
            marked_content_id: id.filter(|_| !is_in_form),
            actual_text,
            artifact_type,
        }
    }

    pub fn is_artifact(&self) -> bool {
        self.tag == "Artifact"
    }
}

/// The marks around an object, outermost first.
///
/// Those of a form are around every object in it, so an object in a form
/// has the marks around the form before its own.
#[derive(Clone, Debug, Default)]
pub(in crate::reader) struct Marks(Rc<Vec<Rc<Mark>>>);

impl Marks {
    /// These marks and one more inside them.
    pub fn with(&self, mark: Mark) -> Marks {
        let mut marks = Vec::clone(&self.0);
        marks.push(Rc::new(mark));
        Marks(Rc::new(marks))
    }

    /// These marks without the innermost one.
    pub fn without_innermost(&self) -> Marks {
        let mut marks = Vec::clone(&self.0);
        marks.pop();
        Marks(Rc::new(marks))
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    /// The marks that say that the object is an artifact.
    pub fn artifacts(&self) -> impl Iterator<Item = &Mark> {
        let marks = self.0.iter().map(Rc::as_ref);
        marks.filter(|mark| mark.is_artifact())
    }

    pub fn is_artifact(&self) -> bool {
        self.artifacts().next().is_some()
    }

    /// The number that ties the object to the structure tree of its page,
    /// which is that of the innermost mark that has one.
    pub fn marked_content_id(&self) -> Option<i32> {
        let mut inside_out = self.0.iter().rev();
        inside_out.find_map(|mark| mark.marked_content_id)
    }

    /// The innermost mark that says which text its content stands for.
    pub fn with_actual_text(&self) -> Option<&Rc<Mark>> {
        let mut inside_out = self.0.iter().rev();
        inside_out.find(|mark| mark.actual_text.is_some())
    }
}
