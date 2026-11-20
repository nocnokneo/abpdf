//! The changes as lines of text, in the manner of `diff`.

use std::io::{self, Write};

use crate::change::{ChangeSide, Comparison};
use crate::excerpt::shortened;

/// Longer texts are cut off, so that one change stays a few lines.
const MAX_TEXT_CHARS: usize = 200;

/// Writes each change as the pages it is on, its old text after `-` and its
/// new text after `+`, or text that moved after `>`, and a count at the end.
pub fn write(comparison: &Comparison, mut output: impl Write) -> io::Result<()> {
    for change in &comparison.changes {
        writeln!(output, "@ {}", change.location())?;
        let texts = change.sides.as_ref().map(ChangeSide::text);
        let lines = match change.is_move {
            true => vec![('>', texts.new)],
            false => vec![('-', texts.old), ('+', texts.new)],
        };
        for (sign, text) in lines {
            if let Some(text) = text {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{PARAGRAPH, REST, comparison_of};

    fn listing(old: &str, new: &str) -> String {
        let mut output = Vec::new();
        write(&comparison_of(old, new), &mut output).unwrap();
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
    fn side_that_is_nowhere_in_its_document_has_no_page() {
        assert_eq!(listing("", "new"), "@ new p. 1\n+ new\n1 change\n");
    }

    #[test]
    fn text_that_moved_is_listed_once() {
        assert_eq!(
            listing(
                &format!("{PARAGRAPH}\n{REST}"),
                &format!("{REST}\n{PARAGRAPH}")
            ),
            format!("@ old p. 1 / new p. 1\n> {PARAGRAPH}\n1 change\n")
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
}
