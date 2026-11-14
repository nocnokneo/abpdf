//! The text of a document as one flow of tokens, across page boundaries.
//!
//! A [`RawFlow`] is the text as it is read.
//! [`prepare`] makes of it the [`Flow`] that is compared.

mod builder;
mod hyphen_marks;
mod join;
mod repeats;
#[cfg(test)]
mod tests;

use std::borrow::Cow;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::BySide;
use crate::filter::Filters;
use crate::geometry::{Rect, Size};
use crate::reader::{ReadError, Reader};

pub use self::builder::FlowBuilder;
use self::join::Vocabulary;

/// Gets two documents ready to be compared with each other.
///
/// First it is decided which tokens take part in the comparison: the filters
/// leave tokens out, and then what the pages repeat at their tops is left
/// out, which the filters have to uncover where a header stands above it.
/// Then the words that a line break splits are rejoined, which has to come
/// last: a word joins the next token that takes part, and the words of both
/// documents together tell what a hyphen is where the producer does not.
pub fn prepare(flows: BySide<RawFlow>, filters: &Filters) -> BySide<Flow> {
    let mut exclusions = flows.as_ref().map(|flow| filters.exclusions(flow));
    repeats::exclude(&flows, &mut exclusions);
    let vocabulary = Vocabulary::of([(&flows.old, &exclusions.old), (&flows.new, &exclusions.new)]);
    BySide {
        old: join::join(flows.old, exclusions.old, &vocabulary),
        new: join::join(flows.new, exclusions.new, &vocabulary),
    }
}

/// The tokens of a document in reading order, as they are read.
#[derive(Clone, Debug, PartialEq)]
pub struct RawFlow {
    pub tokens: Vec<RawToken>,
    pub page_sizes: Vec<Size>,
}

impl RawFlow {
    /// Reads every page of a document.
    pub fn read(reader: &dyn Reader) -> Result<RawFlow, ReadError> {
        let mut builder = FlowBuilder::default();
        for page_index in 0..reader.page_count() {
            builder.add_page(&reader.read_page(page_index)?);
        }
        Ok(builder.finish())
    }
}

/// A word or a punctuation mark on one line.
/// A word that a line break splits is two of these.
#[derive(Clone, Debug, PartialEq)]
pub struct RawToken {
    pub text: String,
    /// Where the token is drawn.
    pub fragment: Fragment,
    /// The white space before the token.
    pub gap: Gap,
    /// Whether the producer marked the token as an artifact.
    pub is_artifact: bool,
    /// The hyphen that ends a line right after the token.
    pub line_end_hyphen: Option<LineEndHyphen>,
}

/// The tokens of a document that are compared, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub tokens: Vec<Token>,
    /// The tokens that are left out of the comparison.
    pub excluded: Vec<ExcludedToken>,
    pub page_sizes: Vec<Size>,
}

/// A word or a punctuation mark.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// The text, without a hyphen that only marks a line break.
    pub text: String,
    /// Where the token is drawn: one fragment per line it is on.
    pub fragments: Vec<Fragment>,
    /// The white space before the token.
    pub gap: Gap,
}

impl Token {
    /// The form of the text that is compared: compatibility characters such as
    /// ligatures are folded into their plain equivalents.
    pub fn key(&self) -> Cow<'_, str> {
        key_of(&self.text)
    }
}

fn key_of(text: &str) -> Cow<'_, str> {
    match is_nfkc_quick(text.chars()) {
        IsNormalized::Yes => Cow::Borrowed(text),
        _ => Cow::Owned(text.nfkc().collect()),
    }
}

/// Whether the text of a token is a word, and not a punctuation mark.
fn is_word(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_alphanumeric)
}

/// A token that is left out of the comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct ExcludedToken {
    pub token: Token,
    /// The rule that leaves the token out.
    pub exclusion: Exclusion,
}

/// The part of a token on one line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fragment {
    pub page_index: usize,
    /// Counts the lines of the page in reading order.
    pub line_index: usize,
    pub bounds: Rect,
}

/// The white space between two tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gap {
    None,
    Space,
    LineBreak,
}

/// The rule that leaves a token out of the comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exclusion {
    /// The producer marked the token as an artifact.
    Artifact,
    /// The token is in the band at the top of its page.
    Header,
    /// The token is in the band at the bottom of its page.
    Footer,
    /// The ignore pattern with this index matched the token.
    Pattern(usize),
    /// The token repeats, at the top of its page, text of the page before,
    /// and the other document has no text for it.
    Repeat,
}

/// A hyphen that ends a line in the middle of a word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEndHyphen {
    pub kind: HyphenKind,
    pub fragment: Fragment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyphenKind {
    /// Added by the producer's line breaking: not part of the word.
    Soft,
    /// Part of the word, which happens to break after it.
    Hard,
    /// The producer does not say.
    Unknown,
}
