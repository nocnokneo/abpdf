//! Rejoining words that a line break splits.

use std::collections::HashSet;
use std::mem;

use super::{Flow, Gap, HyphenKind, LineEndHyphen, Token};

/// The words of the compared documents, which tell what a hyphen of unknown
/// kind is.
///
/// Such a hyphen is taken to be hard if the two words around it are joined
/// by a hyphen elsewhere and never written as one word, and soft otherwise.
#[derive(Clone, Debug, Default)]
pub struct Vocabulary {
    words: HashSet<String>,
    /// Pairs of words that a hyphen joins within a line, as `first-second`.
    hyphenated_pairs: HashSet<String>,
}

impl Vocabulary {
    /// Collects the words of the tokens that take part in the comparison.
    pub fn of<'a>(flows: impl IntoIterator<Item = &'a Flow>) -> Self {
        let mut vocabulary = Self::default();
        for flow in flows {
            let compared: Vec<&Token> = flow
                .tokens
                .iter()
                .filter(|token| !token.is_excluded())
                .collect();
            for token in &compared {
                if token.is_word() {
                    vocabulary.words.insert(token.text.clone());
                }
            }
            for window in compared.windows(3) {
                let [first, hyphen, second] = window else {
                    continue;
                };
                let is_hyphenated_pair = first.is_word()
                    && first.line_end_hyphen.is_none()
                    && hyphen.text == "-"
                    && hyphen.gap == Gap::None
                    && hyphen.line_end_hyphen.is_none()
                    && second.is_word()
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

impl Flow {
    /// Resolves every [`Token::line_end_hyphen`].
    ///
    /// A soft hyphen joins its word to the next token that takes part in the
    /// comparison, which may be on the next page with a footer and a header
    /// in between.
    /// A hard hyphen becomes a token of its own.
    /// `vocabulary` decides about a hyphen of unknown kind.
    pub fn join_broken_words(&mut self, vocabulary: &Vocabulary) {
        let mut joined: Vec<Token> = Vec::with_capacity(self.tokens.len());
        // The index in `joined` of a token that waits for the rest of its word.
        let mut open: Option<usize> = None;

        for mut token in mem::take(&mut self.tokens) {
            if token.is_excluded() {
                if let Some(hyphen) = token.line_end_hyphen.take() {
                    extend_last_fragment(&mut token, &hyphen);
                }
                joined.push(token);
                continue;
            }
            if let Some(index) = open.take() {
                let hyphen = joined[index].line_end_hyphen.take().expect("open token");
                match meaning(&hyphen, &joined[index], Some(&token), vocabulary) {
                    Meaning::LineBreakOnly => {
                        let word = &mut joined[index];
                        extend_last_fragment(word, &hyphen);
                        word.text.push_str(&token.text);
                        word.fragments.append(&mut token.fragments);
                        word.line_end_hyphen = token.line_end_hyphen;
                        open = word.line_end_hyphen.map(|_| index);
                        continue;
                    }
                    Meaning::Punctuation => {
                        joined.insert(index + 1, hyphen_token(&joined[index], &hyphen));
                        token.gap = Gap::None;
                    }
                    Meaning::Nothing => extend_last_fragment(&mut joined[index], &hyphen),
                }
            }
            if token.line_end_hyphen.is_some() {
                open = Some(joined.len());
            }
            joined.push(token);
        }

        if let Some(index) = open {
            let hyphen = joined[index].line_end_hyphen.take().expect("open token");
            match meaning(&hyphen, &joined[index], None, vocabulary) {
                Meaning::Punctuation => {
                    joined.insert(index + 1, hyphen_token(&joined[index], &hyphen));
                }
                _ => extend_last_fragment(&mut joined[index], &hyphen),
            }
        }
        self.tokens = joined;
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
    before: &Token,
    after: Option<&Token>,
    vocabulary: &Vocabulary,
) -> Meaning {
    let Some(after) = after.filter(|after| before.is_word() && after.is_word()) else {
        return match hyphen.kind {
            HyphenKind::Soft => Meaning::Nothing,
            HyphenKind::Hard | HyphenKind::Unknown => Meaning::Punctuation,
        };
    };
    let is_hard = match hyphen.kind {
        HyphenKind::Soft => false,
        HyphenKind::Hard => true,
        HyphenKind::Unknown => vocabulary.says_hyphen_is_hard(&before.text, &after.text),
    };
    if is_hard {
        Meaning::Punctuation
    } else {
        Meaning::LineBreakOnly
    }
}

fn extend_last_fragment(token: &mut Token, hyphen: &LineEndHyphen) {
    let last = token.fragments.last_mut().expect("a token has a fragment");
    last.bounds = last.bounds.union(&hyphen.fragment.bounds);
}

/// The hyphen as a token that follows `word` directly.
fn hyphen_token(word: &Token, hyphen: &LineEndHyphen) -> Token {
    Token {
        text: "-".to_owned(),
        fragments: vec![hyphen.fragment],
        gap: Gap::None,
        line_end_hyphen: None,
        ..word.clone()
    }
}
