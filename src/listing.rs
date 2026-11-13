//! The changes as lines of text, in the manner of `diff`.

use std::io::{self, Write};

use crate::change::{Change, Comparison};

/// Longer texts are cut off, so that one change stays a few lines.
const MAX_TEXT_CHARS: usize = 200;

/// Writes each change as the pages it is on, its old text after `-` and its
/// new text after `+`, and a count at the end.
pub fn write(comparison: &Comparison, mut output: impl Write) -> io::Result<()> {
    for change in &comparison.changes {
        writeln!(output, "@ {}", pages_of(change))?;
        for (sign, text) in [('-', &change.old.text), ('+', &change.new.text)] {
            if !text.is_empty() {
                writeln!(output, "{sign} {}", shortened(text, MAX_TEXT_CHARS))?;
            }
        }
    }
    match comparison.changes.len() {
        0 => writeln!(output, "No differences"),
        1 => writeln!(output, "1 change"),
        count => writeln!(output, "{count} changes"),
    }
}

/// The pages on which a change starts, as a reader counts them.
pub fn pages_of(change: &Change) -> String {
    let first_page = |areas: &[crate::change::Area]| areas[0].page_index + 1;
    format!(
        "old p. {} / new p. {}",
        first_page(&change.old.areas),
        first_page(&change.new.areas)
    )
}

/// `text`, or its start with an ellipsis if it has more than `max_chars`
/// characters.
pub fn shortened(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars.saturating_sub(1)) {
        Some((cut, _)) if text[cut..].chars().nth(1).is_some() => {
            format!("{}…", text[..cut].trim_end())
        }
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filters;
    use crate::testing::{PageBuilder, raw_flow_of};

    fn listing(old: &str, new: &str) -> String {
        let flow = |text| raw_flow_of([PageBuilder::new().text(text)]);
        let comparison = Comparison::of_flows(flow(old), flow(new), &Filters::default());
        let mut output = Vec::new();
        write(&comparison, &mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn each_change_lists_its_pages_and_texts() {
        assert_eq!(
            listing("keep old text and more", "keep new text and more still"),
            "@ old p. 1 / new p. 1\n- old\n+ new\n@ old p. 1 / new p. 1\n+ still\n2 changes\n"
        );
    }

    #[test]
    fn no_changes_are_said_so() {
        assert_eq!(listing("same", "same"), "No differences\n");
    }

    #[test]
    fn one_change_is_singular() {
        assert!(listing("one", "two").ends_with("\n1 change\n"));
    }

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
