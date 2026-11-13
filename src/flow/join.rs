//! Rejoining words that a line break splits.

use std::mem;

use super::{Flow, Gap, HyphenKind, LineEndHyphen, Token};

impl Flow {
    /// Resolves every [`Token::line_end_hyphen`].
    ///
    /// A soft hyphen joins its word to the next token that takes part in the
    /// comparison, which may be on the next page with a footer and a header
    /// in between.
    /// A hard hyphen becomes a token of its own.
    /// A hyphen of unknown kind is taken to be soft.
    pub fn join_broken_words(&mut self) {
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
                match meaning(&hyphen, &joined[index], Some(&token)) {
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
            match meaning(&hyphen, &joined[index], None) {
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

fn meaning(hyphen: &LineEndHyphen, before: &Token, after: Option<&Token>) -> Meaning {
    let splits_a_word = before.is_word() && after.is_some_and(Token::is_word);
    match hyphen.kind {
        HyphenKind::Hard => Meaning::Punctuation,
        HyphenKind::Soft | HyphenKind::Unknown if splits_a_word => Meaning::LineBreakOnly,
        HyphenKind::Soft => Meaning::Nothing,
        HyphenKind::Unknown => Meaning::Punctuation,
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
