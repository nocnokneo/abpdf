//! The changes as JSON, for other programs.
//! The README describes the format.

use std::io::{self, Write};

use serde::Serialize;
use serde_json::{Value, json};

use crate::change::{
    Area, Change, ChangeKind, ChangeSide, Comparison, Difference, StyleDifference,
};
use crate::geometry::hundredths;

#[derive(Serialize)]
struct Report<'a> {
    changes: Vec<ChangeRecord<'a>>,
}

#[derive(Serialize)]
struct ChangeRecord<'a> {
    kind: &'static str,
    old: SideRecord<'a>,
    new: SideRecord<'a>,
    /// What differs in the style of text that is restyled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    style: Vec<StyleRecord>,
}

#[derive(Serialize)]
struct StyleRecord {
    property: &'static str,
    old: Value,
    new: Value,
}

#[derive(Serialize)]
struct SideRecord<'a> {
    text: &'a str,
    areas: Vec<AreaRecord>,
}

#[derive(Serialize)]
struct AreaRecord {
    page: usize,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    /// Whether the area is that of a figure.
    #[serde(skip_serializing_if = "is_false")]
    figure: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

pub fn write(comparison: &Comparison, mut output: impl Write) -> io::Result<()> {
    let report = Report {
        changes: comparison.changes.iter().map(change_record).collect(),
    };
    serde_json::to_writer_pretty(&mut output, &report)?;
    writeln!(output)?;
    output.flush()
}

fn change_record(change: &Change) -> ChangeRecord<'_> {
    ChangeRecord {
        kind: match change.kind() {
            ChangeKind::Inserted => "inserted",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Replaced => "replaced",
            ChangeKind::Moved => "moved",
            ChangeKind::Restyled => "restyled",
        },
        old: side_record(&change.sides.old),
        new: side_record(&change.sides.new),
        style: match &change.difference {
            Difference::Style(differences) => differences.iter().map(style_record).collect(),
            Difference::Text | Difference::Place => Vec::new(),
        },
    }
}

fn style_record(difference: &StyleDifference) -> StyleRecord {
    let (property, values) = match difference {
        StyleDifference::Font(font) => ("font", font.as_ref().map(|font| json!(font))),
        StyleDifference::Size(size) => ("size", size.map(|size| json!(hundredths(size)))),
        StyleDifference::Weight(weight) => ("weight", weight.map(|weight| json!(weight))),
        StyleDifference::Italic(is_italic) => {
            ("italic", is_italic.map(|is_italic| json!(is_italic)))
        }
        StyleDifference::Color(color) => ("color", color.map(|color| json!(color.to_string()))),
        StyleDifference::Rotation(rotation) => (
            "rotation",
            rotation.map(|rotation| json!(hundredths(rotation))),
        ),
    };
    StyleRecord {
        property,
        old: values.old,
        new: values.new,
    }
}

fn side_record(side: &ChangeSide) -> SideRecord<'_> {
    SideRecord {
        text: side.text().unwrap_or_default(),
        areas: side.areas().iter().map(area_record).collect(),
    }
}

fn area_record(area: &Area) -> AreaRecord {
    AreaRecord {
        page: area.page_index + 1,
        left: hundredths(area.bounds.left),
        top: hundredths(area.bounds.top),
        right: hundredths(area.bounds.right),
        bottom: hundredths(area.bounds.bottom),
        figure: area.is_figure,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::testing::{
        FIGURE_WORDS, PARAGRAPH, REST, comparison_of, comparison_of_built_pages, page_with_bold,
        page_with_figure,
    };

    fn report(old: &str, new: &str) -> Value {
        let mut output = Vec::new();
        write(&comparison_of(old, new), &mut output).unwrap();
        assert_eq!(output.last(), Some(&b'\n'));
        serde_json::from_slice(&output).unwrap()
    }

    #[test]
    fn documents_without_changes_have_an_empty_list() {
        assert_eq!(report("same", "same"), json!({ "changes": [] }));
    }

    #[test]
    fn change_has_its_kind_texts_and_areas() {
        let expected = json!({
            "changes": [{
                "kind": "replaced",
                "old": {
                    "text": "old",
                    "areas": [{"page": 1, "left": 50.0, "top": 0.0, "right": 80.0, "bottom": 12.0}],
                },
                "new": {
                    "text": "newer",
                    "areas": [{"page": 1, "left": 50.0, "top": 0.0, "right": 100.0, "bottom": 12.0}],
                },
            }]
        });
        assert_eq!(report("keep old", "keep newer"), expected);
    }

    #[test]
    fn text_that_moved_has_its_areas_in_both_documents() {
        let old = format!("{PARAGRAPH}\n{REST}");
        let new = format!("{REST}\n{PARAGRAPH}");
        let report = report(&old, &new);
        let change = &report["changes"][0];
        assert_eq!(change["kind"], "moved");
        assert_eq!(change["old"]["text"], PARAGRAPH);
        assert_eq!(change["new"]["text"], PARAGRAPH);
        assert_eq!(change["old"]["areas"][0]["top"], 0.0);
        assert_eq!(change["new"]["areas"][0]["top"], 20.0);
    }

    #[test]
    fn text_in_another_style_has_what_differs_in_the_style() {
        let comparison =
            comparison_of_built_pages([page_with_bold("one two")], [page_with_bold("one *two*")]);
        let mut output = Vec::new();
        write(&comparison, &mut output).unwrap();
        let report: Value = serde_json::from_slice(&output).unwrap();
        let change = &report["changes"][0];
        assert_eq!(change["kind"], "restyled");
        assert_eq!(change["old"]["text"], "two");
        assert_eq!(
            change["style"],
            json!([{ "property": "weight", "old": 400, "new": 700 }])
        );
    }

    #[test]
    fn area_of_a_figure_says_that_it_is_one() {
        let comparison = comparison_of_built_pages(
            [page_with_figure(|_| {})],
            [page_with_figure(|figure| figure.content_hash = 1)],
        );
        let mut output = Vec::new();
        write(&comparison, &mut output).unwrap();
        let report: Value = serde_json::from_slice(&output).unwrap();
        let old = &report["changes"][0]["old"];
        assert_eq!(old["text"], FIGURE_WORDS);
        assert_eq!(old["areas"][0]["figure"], true);
    }

    #[test]
    fn area_of_text_does_not_say_that_it_is_a_figure() {
        let report = report("keep old", "keep newer");
        assert_eq!(report["changes"][0]["old"]["areas"][0].get("figure"), None);
    }

    #[test]
    fn only_text_in_another_style_has_a_style() {
        let report = report("keep old", "keep newer");
        assert_eq!(report["changes"][0].get("style"), None);
    }

    #[test]
    fn side_without_text_has_an_area_of_no_width() {
        let report = report("keep", "keep more");
        let change = &report["changes"][0];
        assert_eq!(change["kind"], "inserted");
        assert_eq!(change["old"]["text"], "");
        assert_eq!(
            change["old"]["areas"],
            json!([{"page": 1, "left": 40.0, "top": 0.0, "right": 40.0, "bottom": 12.0}])
        );
    }

    #[test]
    fn side_of_a_document_without_text_has_no_area() {
        let report = report("", "all new");
        assert_eq!(
            report["changes"][0]["old"],
            json!({"text": "", "areas": []})
        );
    }

    #[test]
    fn positions_are_rounded_to_hundredths() {
        assert_eq!(hundredths(12.345_678), 12.35);
        assert_eq!(hundredths(0.1 + 0.2), 0.3);
    }
}
