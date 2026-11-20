//! Rejoining words that a line break splits, among the tokens that are
//! compared.

use std::collections::HashSet;

use super::{ExcludedToken, Exclusion, Flow, Gap, HyphenKind, LineEndHyphen, RawToken, Screened};
use super::{Token, is_word};
use crate::{BySide, Side};

/// The words of the compared documents, which tell what a hyphen of unknown
/// kind is.
///
/// Such a hyphen is taken to be hard if the two words around it are joined
/// by a hyphen elsewhere and never written as one word, and soft otherwise.
#[derive(Clone, Debug, Default)]
pub(super) struct Vocabulary {
    words: HashSet<String>,
    /// Pairs of words that a hyphen joins within a line, as `first-second`.
    hyphenated_pairs: HashSet<String>,
}

impl Vocabulary {
    /// Collects the words of the tokens of both documents that take part in
    /// the comparison.
    pub(super) fn of(documents: &BySide<Screened>) -> Self {
        let mut vocabulary = Self::default();
        for side in Side::BOTH {
            let compared: Vec<&RawToken> =
                documents[side].compared().map(|(_, token)| token).collect();
            for token in &compared {
                if is_word(&token.text) {
                    vocabulary.words.insert(token.text.clone());
                }
            }
            for window in compared.windows(3) {
                let [first, hyphen, second] = window else {
                    continue;
                };
                let is_hyphenated_pair = is_word(&first.text)
                    && first.line_end_hyphen.is_none()
                    && hyphen.text == "-"
                    && hyphen.gap == Gap::None
                    && hyphen.line_end_hyphen.is_none()
                    && is_word(&second.text)
                    && second.gap == Gap::None;
                if is_hyphenated_pair {
                    vocabulary
                        .hyphenated_pairs
                        .insert(hyphenated(&first.text, &second.text));
                }
            }
        }
        vocabulary
    }

    fn says_hyphen_is_hard(&self, before: &str, after: &str) -> bool {
        self.hyphenated_pairs.contains(&hyphenated(before, after))
            && !self.words.contains(&format!("{before}{after}"))
    }
}

fn hyphenated(first: &str, second: &str) -> String {
    format!("{first}-{second}")
}

/// Sets the tokens that have an exclusion aside and resolves the line-end
/// hyphens of the others.
///
/// A soft hyphen joins its word to the next token that takes part in the
/// comparison, which may be on the next page with a footer and a header in
/// between.
/// A hard hyphen becomes a token of its own.
/// `vocabulary` decides about a hyphen of unknown kind.
pub(super) fn join(screened: Screened, vocabulary: &Vocabulary) -> Flow {
    let Screened { flow, exclusions } = screened;
    let mut tokens: Vec<Token> = Vec::with_capacity(flow.tokens.len());
    let mut excluded = Vec::new();
    // The hyphen after the last of `tokens`, until the token after it tells
    // what it means.
    let mut open_hyphen: Option<LineEndHyphen> = None;

    for (raw, exclusion) in flow.tokens.into_iter().zip(exclusions) {
        if let Some(exclusion) = exclusion {
            excluded.push(excluded_token(raw, exclusion));
            continue;
        }
        let mut gap = raw.gap;
        if let Some(hyphen) = open_hyphen.take() {
            let word = tokens.last_mut().expect("a hyphen follows a token");
            match meaning(&hyphen, &word.text, Some(&raw.text), vocabulary) {
                Meaning::LineBreakOnly => {
                    extend_to_hyphen(word, &hyphen);
                    word.text.push_str(&raw.text);
                    word.fragments.push(raw.fragment);
                    open_hyphen = raw.line_end_hyphen;
                    continue;
                }
                Meaning::Punctuation => {
                    tokens.push(hyphen_token(&hyphen));
                    gap = Gap::None;
                }
                Meaning::Nothing => extend_to_hyphen(word, &hyphen),
            }
        }
        open_hyphen = raw.line_end_hyphen;
        tokens.push(Token {
            gap,
            ..token_of(raw)
        });
    }

    if let Some(hyphen) = open_hyphen {
        let word = tokens.last_mut().expect("a hyphen follows a token");
        match meaning(&hyphen, &word.text, None, vocabulary) {
            Meaning::Punctuation => tokens.push(hyphen_token(&hyphen)),
            _ => extend_to_hyphen(word, &hyphen),
        }
    }
    Flow {
        tokens,
        excluded,
        page_sizes: flow.page_sizes,
    }
}

/// What a line-end hyphen contributes to the text.
enum Meaning {
    /// The word continues in the next token, without the hyphen.
    LineBreakOnly,
    /// The hyphen is a character of the text.
    Punctuation,
    /// A soft hyphen with no word to continue.
    Nothing,
}

fn meaning(
    hyphen: &LineEndHyphen,
    before: &str,
    after: Option<&str>,
    vocabulary: &Vocabulary,
) -> Meaning {
    let Some(after) = after.filter(|after| is_word(before) && is_word(after)) else {
        return match hyphen.kind {
            HyphenKind::Soft => Meaning::Nothing,
            HyphenKind::Hard | HyphenKind::Unknown => Meaning::Punctuation,
        };
    };
    let is_hard = match hyphen.kind {
        HyphenKind::Soft => false,
        HyphenKind::Hard => true,
        HyphenKind::Unknown => vocabulary.says_hyphen_is_hard(before, after),
    };
    if is_hard {
        Meaning::Punctuation
    } else {
        Meaning::LineBreakOnly
    }
}

/// Makes the hyphen part of where the word before it is drawn.
fn extend_to_hyphen(word: &mut Token, hyphen: &LineEndHyphen) {
    let last = word.fragments.last_mut().expect("a token has a fragment");
    last.bounds = last.bounds.union(&hyphen.fragment.bounds);
}

/// The hyphen as a token that follows its word directly.
fn hyphen_token(hyphen: &LineEndHyphen) -> Token {
    Token {
        text: "-".to_owned(),
        fragments: vec![hyphen.fragment],
        gap: Gap::None,
    }
}

/// A token that is left out as it is read, with the hyphen that may end its
/// line as part of where it is drawn.
fn excluded_token(raw: RawToken, exclusion: Exclusion) -> ExcludedToken {
    let hyphen = raw.line_end_hyphen;
    let mut token = token_of(raw);
    if let Some(hyphen) = hyphen {
        extend_to_hyphen(&mut token, &hyphen);
    }
    ExcludedToken { token, exclusion }
}

/// The token as it is read: on one line, and without the hyphen that may
/// end the line.
fn token_of(raw: RawToken) -> Token {
    Token {
        text: raw.text,
        fragments: vec![raw.fragment],
        gap: raw.gap,
    }
}
