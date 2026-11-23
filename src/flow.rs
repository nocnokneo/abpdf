//! The text and the figures of a document as one flow of tokens, across page
//! boundaries.
//!
//! A [`RawFlow`] is what is read.
//! [`prepare`] makes of it the [`Flow`] that is compared.

mod builder;
mod figure;
mod furniture;
mod hyphen_marks;
mod join;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::ops::Range;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::BySide;
use crate::filter::Filters;
use crate::geometry::{Rect, Size};
use crate::reader::{Document, Style};

pub use self::builder::FlowBuilder;
pub use self::figure::{FIGURE_TEXT, Figure};
use self::join::Vocabulary;

/// Gets two documents ready to be compared with each other.
///
/// First it is decided which tokens take part in the comparison: the filters
/// leave tokens out, and then what stands where page furniture does is left
/// out, which the filters have to uncover where they leave out a header
/// above it.
/// Then the words that a line break splits are rejoined, which has to come
/// last: a word joins the next token that takes part, and the words of both
/// documents together tell what a hyphen is where the producer does not.
pub fn prepare(flows: BySide<RawFlow>, filters: &Filters) -> BySide<Flow> {
    let mut screened = flows.map(|flow| Screened {
        exclusions: filters.exclusions(&flow),
        flow,
    });
    furniture::exclude(&mut screened);
    let vocabulary = Vocabulary::of(&screened);
    screened.map(|screened| join::join(screened, &vocabulary))
}

/// A flow as it is read, with the rule that leaves each of its tokens out of
/// the comparison, if one does.
struct Screened {
    flow: RawFlow,
    exclusions: Vec<Option<Exclusion>>,
}

impl Screened {
    /// The tokens that take part in the comparison, each with its index in
    /// the flow.
    fn compared(&self) -> impl Iterator<Item = (usize, &RawToken)> {
        let tokens = self.flow.tokens.iter().enumerate();
        tokens.filter(|(index, _)| self.exclusions[*index].is_none())
    }
}

/// The tokens of a document in reading order, as they are read.
#[derive(Clone, Debug, PartialEq)]
pub struct RawFlow {
    pub tokens: Vec<RawToken>,
    pub page_sizes: Vec<Size>,
    /// The styles that the tokens are drawn in, without repeats.
    pub styles: Vec<Style>,
}

impl RawFlow {
    /// Reads every page of a document.
    pub fn read(document: &Document) -> RawFlow {
        let mut builder = FlowBuilder::default();
        for page in document.pages() {
            builder.add_page(&page);
        }
        builder.finish()
    }
}

/// A word or a punctuation mark on one line, or a figure.
/// A word that a line break splits is two of these.
#[derive(Clone, Debug, PartialEq)]
pub struct RawToken {
    /// The characters of text, or [`FIGURE_TEXT`] for a figure.
    pub text: String,
    /// What the token shows if it is a figure.
    pub figure: Option<Figure>,
    /// Where the token is drawn.
    pub fragment: Fragment,
    /// The white space before the token.
    pub gap: Gap,
    /// The styles of the characters, as indices into [`RawFlow::styles`]: one
    /// for each run of characters in one style.
    pub styles: Vec<usize>,
    /// Whether the producer marked the token as an artifact.
    pub is_artifact: bool,
    /// The table cell that the token is in, if the producer says that it is
    /// in one.
    pub cell: Option<Cell>,
    /// The hyphen that ends a line right after the token.
    pub line_end_hyphen: Option<LineEndHyphen>,
}

impl RawToken {
    /// What [`Token::key`] is.
    pub fn key(&self) -> Cow<'_, str> {
        key_of(&self.text, self.figure.as_ref())
    }
}

/// The tokens of a document that are compared, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub tokens: Vec<Token>,
    /// The tokens that are left out of the comparison.
    pub excluded: Vec<ExcludedToken>,
    pub page_sizes: Vec<Size>,
    /// The styles that the tokens are drawn in, without repeats.
    pub styles: Vec<Style>,
}

/// A word or a punctuation mark, or a figure.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// The text, without a hyphen that only marks a line break, or
    /// [`FIGURE_TEXT`] for a figure.
    pub text: String,
    /// What the token shows if it is a figure.
    pub figure: Option<Figure>,
    /// Where the token is drawn: one fragment per line it is on.
    pub fragments: Vec<Fragment>,
    /// The white space before the token.
    pub gap: Gap,
    /// The styles of the characters, as indices into [`Flow::styles`]: one
    /// for each run of characters in one style.
    pub styles: Vec<usize>,
    /// The table cell that the token is in, if the producer says that it is
    /// in one.
    pub cell: Option<Cell>,
    /// Where the token stands in the [`RawFlow`]: the indices of its parts
    /// there, from the first one to after the last one.
    /// A hyphen that became a token of its own has no part there, and its
    /// range is empty, at the place where it stands.
    pub raw_indices: Range<usize>,
}

impl Token {
    /// Whether the token comes right after `previous` in the document as it
    /// is read: no token stands between the two, compared or left out.
    pub fn follows(&self, previous: &Token) -> bool {
        self.raw_indices.start == previous.raw_indices.end
    }

    /// What is compared of the token.
    ///
    /// For text it is the text, with compatibility characters such as
    /// ligatures folded into their plain equivalents.
    /// For a figure it says what the figure is drawn with, so that two
    /// figures with one key can still be drawn through other points.
    pub fn key(&self) -> Cow<'_, str> {
        key_of(&self.text, self.figure.as_ref())
    }
}

/// The key of a token with this text, which is a figure if `figure` is one.
fn key_of<'a>(text: &'a str, figure: Option<&Figure>) -> Cow<'a, str> {
    if let Some(figure) = figure {
        return Cow::Owned(format!("{text}{:016x}", figure.content_hash));
    }
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

/// A table cell of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub page_index: usize,
    /// Counts the cells of the page.
    pub index: usize,
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
    /// The token is in a line at the top of its page that a page nearby has
    /// too, and the other document has no text for it.
    RunningHeader,
    /// The token is in a line at the bottom of its page that a page nearby
    /// has too, and the other document has no text for it.
    RunningFooter,
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
