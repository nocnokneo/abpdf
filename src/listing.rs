//! The changes as lines of text, in the manner of `diff`.

use std::io::{self, Write};

use crate::change::{Comparison, Difference, StyleDifference};
use crate::excerpt::shortened;

/// Longer texts are cut off, so that one change stays a few lines.
const MAX_TEXT_CHARS: usize = 200;

/// Writes each change as the pages it is on and what it is, and a count at
/// the end.
///
/// A line with the text of a change starts with a sign:
///
/// - `-` before text that only the old document has
/// - `+` before text that only the new document has
/// - `>` before text that moved
/// - `~` before text in another style, after what differs in the style
pub fn write(comparison: &Comparison, mut output: impl Write) -> io::Result<()> {
    for change in &comparison.changes {
        writeln!(output, "@ {}", change.location())?;
        let texts = change.sides.as_ref().map(|side| {
            let text = side.text()?;
            Some(shortened(text, MAX_TEXT_CHARS))
        });
        let lines = match &change.difference {
            Difference::Text => vec![('-', texts.old), ('+', texts.new)],
            Difference::Place => vec![('>', texts.new)],
            Difference::Style(differences) => {
                let differences = StyleDifference::list(differences);
                let line = texts.new.map(|text| format!("{differences}: {text}"));
                vec![('~', line)]
            }
        };
        for (sign, text) in lines {
            if let Some(text) = text {
                writeln!(output, "{sign} {text}")?;
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
    use crate::testing::{
        PARAGRAPH, REST, comparison_of, comparison_of_built_pages, page_with_bold,
    };

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
    fn text_in_another_style_is_listed_after_what_differs_in_the_style() {
        let comparison =
            comparison_of_built_pages([page_with_bold("one two")], [page_with_bold("one *two*")]);
        let mut output = Vec::new();
        write(&comparison, &mut output).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "@ old p. 1 / new p. 1\n~ weight 400 → 700: two\n1 change\n"
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
