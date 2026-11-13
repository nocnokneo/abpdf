//! The text of a document as one flow of tokens, across page boundaries.

mod builder;
mod hyphen_marks;
mod join;
#[cfg(test)]
mod tests;

use std::borrow::Cow;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::filter::Filters;
use crate::geometry::{Rect, Size};
use crate::reader::{ReadError, Reader};

pub use self::builder::FlowBuilder;
use self::join::Vocabulary;

/// Gets flows as [`Flow::read`] returns them ready to be compared with each
/// other.
///
/// First the filters decide which tokens take part in the comparison.
/// Then the words that a line break splits are rejoined, which has to come
/// second: a word joins the next token that takes part, and the words of all
/// the flows together tell what a hyphen is where the producer does not.
pub fn prepare<const N: usize>(mut flows: [Flow; N], filters: &Filters) -> [Flow; N] {
    for flow in &mut flows {
        filters.apply(flow);
    }
    let vocabulary = Vocabulary::of(&flows);
    for flow in &mut flows {
        flow.join_broken_words(&vocabulary);
    }
    flows
}

/// The tokens of a document in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub tokens: Vec<Token>,
    pub page_sizes: Vec<Size>,
}

impl Flow {
    /// Reads every page of a document.
    ///
    /// Words that a line break splits are still in two tokens, and no token
    /// is left out of the comparison yet: [`prepare`] does both.
    pub fn read(reader: &dyn Reader) -> Result<Flow, ReadError> {
        let mut builder = FlowBuilder::default();
        for page_index in 0..reader.page_count() {
            builder.add_page(&reader.read_page(page_index)?);
        }
        Ok(builder.finish())
    }
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
    /// Whether the producer marked the token as an artifact.
    pub is_artifact: bool,
    /// Why the token is left out of the comparison, if it is.
    /// [`prepare`] decides.
    pub exclusion: Option<Exclusion>,
    /// The hyphen that ends a line right after the token, until [`prepare`]
    /// has dealt with it.
    pub line_end_hyphen: Option<LineEndHyphen>,
}

impl Token {
    /// The form of the text that is compared: compatibility characters such as
    /// ligatures are folded into their plain equivalents.
    pub fn key(&self) -> Cow<'_, str> {
        match is_nfkc_quick(self.text.chars()) {
            IsNormalized::Yes => Cow::Borrowed(&self.text),
            _ => Cow::Owned(self.text.nfkc().collect()),
        }
    }

    pub fn is_word(&self) -> bool {
        self.text.chars().next().is_some_and(char::is_alphanumeric)
    }

    pub fn is_excluded(&self) -> bool {
        self.exclusion.is_some()
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    /// The producer marked the token as an artifact.
    Artifact,
    /// The token is in the band at the top of its page.
    Header,
    /// The token is in the band at the bottom of its page.
    Footer,
    /// The ignore pattern with this index matched the token.
    Pattern(usize),
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
