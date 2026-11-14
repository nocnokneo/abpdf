//! The changes as JSON, for other programs.
//! The README describes the format.

use std::io::{self, Write};

use serde::Serialize;

use crate::change::{Area, Change, ChangeKind, ChangeSide, Comparison};

#[derive(Serialize)]
struct Report<'a> {
    changes: Vec<ChangeRecord<'a>>,
}

#[derive(Serialize)]
struct ChangeRecord<'a> {
    kind: &'static str,
    old: SideRecord<'a>,
    new: SideRecord<'a>,
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
        },
        old: side_record(&change.sides.old),
        new: side_record(&change.sides.new),
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
    }
}

/// Rounds to hundredths of a point, which is finer than anything on a page
/// and keeps the numbers short.
fn hundredths(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::testing::comparison_of;

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
        let paragraph = "this paragraph stands somewhere else now";
        let rest = "all the other text of the document stays in place";
        let old = format!("{paragraph}\n{rest}");
        let new = format!("{rest}\n{paragraph}");
        let report = report(&old, &new);
        let change = &report["changes"][0];
        assert_eq!(change["kind"], "moved");
        assert_eq!(change["old"]["text"], paragraph);
        assert_eq!(change["new"]["text"], paragraph);
        assert_eq!(change["old"]["areas"][0]["top"], 0.0);
        assert_eq!(change["new"]["areas"][0]["top"], 20.0);
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
