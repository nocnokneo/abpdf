//! Cutting text down to what there is room for.

pub(crate) const ELLIPSIS: char = '…';

/// `text`, or its start with an ellipsis if it has more than `max_chars`
/// characters.
pub(crate) fn shortened(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars.saturating_sub(1)) {
        Some((cut, _)) if text[cut..].chars().nth(1).is_some() => {
            format!("{}{ELLIPSIS}", text[..cut].trim_end())
        }
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_kept() {
        assert_eq!(shortened("abc", 3), "abc");
        assert_eq!(shortened("", 3), "");
    }

    #[test]
    fn long_text_is_cut_to_the_limit_with_an_ellipsis() {
        assert_eq!(shortened("abcd", 3), "ab…");
        assert_eq!(shortened("ab cd", 4), "ab…");
    }
}
