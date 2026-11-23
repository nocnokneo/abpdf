//! What the text of a page reads as.

use std::collections::HashMap;
use std::rc::Rc;

use super::*;
use crate::reader::Color;
use crate::reader::content::Path;
use crate::reader::fonts::FontDescription;
use crate::reader::page_space::UserSpaceBox;
use crate::reader::structure::Structure;
use crate::reader::testing::PAGE;

/// How wide each glyph of the test runs is.
const GLYPH_WIDTH: f64 = 5.0;

const BLACK: Color = Color {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
};

/// A run in a font of the given size, whose glyphs are [`GLYPH_WIDTH`]
/// wide and stand one after the other from `start` in `direction`.
fn run_in(text: &str, start: (f64, f64), direction: (f64, f64), font_size: f32) -> TextRun {
    let glyphs = text.chars().enumerate().map(|(index, character)| {
        let at = |distance: f64| {
            (
                start.0 + distance * direction.0,
                start.1 + distance * direction.1,
            )
        };
        let (start, end) = (
            at(index as f64 * GLYPH_WIDTH),
            at((index + 1) as f64 * GLYPH_WIDTH),
        );
        let corners = [start, end, (end.0, end.1 + f64::from(font_size))];
        Glyph {
            text: character.to_string().into(),
            start,
            end,
            bounds: UserSpaceBox::bounding(corners).expect("three corners"),
        }
    });
    TextRun {
        glyphs: glyphs.collect(),
        font: Rc::new(FontDescription {
            name: "Test".to_owned(),
            weight: Some(400),
            is_italic: false,
            ascent: 0.75,
            descent: -0.25,
        }),
        font_size,
        fill_color: BLACK,
        direction,
    }
}

/// A run of text in a font of size 10 that is written to the right.
fn run(text: &str, x: f64, y: f64) -> TextRun {
    run_in(text, (x, y), (1.0, 0.0), 10.0)
}

fn unmarked(run: TextRun) -> Object {
    marked(run, &Marks::default())
}

fn marked(run: TextRun, marks: &Marks) -> Object {
    Object {
        marks: marks.clone(),
        content: Content::Text(run),
    }
}

fn mark(tag: &str) -> Mark {
    Mark {
        tag: tag.to_owned(),
        marked_content_id: None,
        actual_text: None,
        artifact_type: None,
    }
}

fn standing_for(text: &str) -> Marks {
    Marks::default().with(Mark {
        actual_text: Some(text.to_owned()),
        ..mark("Span")
    })
}

/// Something that is drawn and is no text.
fn shape() -> Object {
    Object {
        marks: Marks::default(),
        content: Content::Path(Path {
            segments: Vec::new(),
            fill: None,
            stroke: None,
        }),
    }
}

fn read_page(objects: &[Object], figure_objects: &[usize]) -> (Vec<Span>, Vec<TextItem>) {
    let page_space = PageSpace::new(PAGE, 0);
    read(objects, &page_space, &HashMap::new(), figure_objects)
}

/// The text of a page with ` ` for a space, `/` for a line break and `#`
/// for a figure.
fn written(items: &[TextItem]) -> String {
    let characters = items.iter().map(|item| match item {
        TextItem::Char(character) => character.text,
        TextItem::Space => ' ',
        TextItem::LineBreak => '/',
        TextItem::Figure(_) => '#',
    });
    characters.collect()
}

/// What runs of text that are drawn one after the other read as.
fn text_of(runs: impl IntoIterator<Item = TextRun>) -> String {
    let objects: Vec<Object> = runs.into_iter().map(unmarked).collect();
    written(&read_page(&objects, &[]).1)
}

#[test]
fn glyphs_that_follow_each_other_are_one_word() {
    assert_eq!(text_of([run("ab", 0.0, 0.0)]), "ab");
    // Also over the end of a run, and where they are moved a little.
    assert_eq!(text_of([run("ab", 0.0, 0.0), run("cd", 10.0, 0.0)]), "abcd");
    assert_eq!(text_of([run("ab", 0.0, 0.0), run("cd", 11.0, 0.0)]), "abcd");
    assert_eq!(text_of([run("ab", 0.0, 0.0), run("cd", 9.0, 0.0)]), "abcd");
}

#[test]
fn gap_on_a_line_is_a_space() {
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), run("cd", 11.25, 0.0)]),
        "ab cd"
    );
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), run("cd", 80.0, 0.0)]),
        "ab cd"
    );
}

#[test]
fn gap_is_measured_by_the_bigger_of_the_fonts_next_to_it() {
    let small = |x: f64| run_in("cd", (x, 0.0), (1.0, 0.0), 4.0);
    assert_eq!(text_of([run("ab", 0.0, 0.0), small(11.0)]), "abcd");
    assert_eq!(text_of([small(0.0), small(11.0)]), "cd cd");
}

#[test]
fn glyph_of_white_space_is_a_space() {
    assert_eq!(text_of([run("a b", 0.0, 0.0)]), "a b");
    assert_eq!(text_of([run("a\u{A0}b", 0.0, 0.0)]), "a b");
}

#[test]
fn text_that_starts_off_the_line_is_on_another_line() {
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), run("cd", 0.0, -12.0)]),
        "ab/cd"
    );
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), run("cd", 10.0, 5.0)]),
        "ab/cd"
    );
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), run("cd", 30.0, 40.0)]),
        "ab/cd"
    );
}

#[test]
fn raised_and_lowered_text_is_apart_on_its_line() {
    let exponent = run_in("2", (10.0, 3.5), (1.0, 0.0), 6.0);
    let index = run_in("i", (10.0, -1.5), (1.0, 0.0), 6.0);
    assert_eq!(
        text_of([run("ab", 0.0, 0.0), exponent, run("cd", 15.0, 0.0)]),
        "ab 2 cd"
    );
    assert_eq!(text_of([run("ab", 0.0, 0.0), index]), "ab i");
    // A line is not as straight as its numbers: what is off by a hair is
    // on it.
    assert_eq!(text_of([run("ab", 0.0, 0.0), run("cd", 10.0, 0.5)]), "abcd");
}

#[test]
fn text_in_another_direction_is_on_another_line() {
    let upward = run_in("cd", (10.0, 0.0), (0.0, 1.0), 10.0);
    assert_eq!(text_of([run("ab", 0.0, 0.0), upward]), "ab/cd");
    // A line may wander a few degrees.
    let (sine, cosine) = 3.0_f64.to_radians().sin_cos();
    let tilted = run_in("cd", (10.0, 0.0), (cosine, sine), 10.0);
    assert_eq!(text_of([run("ab", 0.0, 0.0), tilted]), "abcd");
}

#[test]
fn text_that_starts_over_on_its_line_is_apart() {
    assert_eq!(
        text_of([run("ab", 50.0, 0.0), run("cd", 0.0, 0.0)]),
        "ab cd"
    );
    // Text that is drawn twice to look bold starts where it started.
    assert_eq!(text_of([run("a", 50.0, 0.0), run("a", 50.2, 0.0)]), "aa");
}

#[test]
fn line_is_measured_along_the_direction_of_the_text() {
    // Text that is written downward, in lines from right to left.
    let down = |text: &str, x: f64, y: f64| run_in(text, (x, y), (0.0, -1.0), 10.0);
    assert_eq!(
        text_of([down("ab", 100.0, 200.0), down("cd", 100.0, 190.0)]),
        "abcd"
    );
    assert_eq!(
        text_of([down("ab", 100.0, 200.0), down("cd", 100.0, 170.0)]),
        "ab cd"
    );
    assert_eq!(
        text_of([down("ab", 100.0, 200.0), down("cd", 88.0, 200.0)]),
        "ab/cd"
    );
}

#[test]
fn ligature_reads_as_its_letters() {
    assert_eq!(text_of([run("o\u{FB03}ce", 0.0, 0.0)]), "office");
}

#[test]
fn character_that_shows_nothing_is_replaced() {
    assert_eq!(text_of([run("a\u{2}b", 0.0, 0.0)]), "a\u{FFFD}b");
}

#[test]
fn glyph_that_maps_to_several_characters_is_all_of_them_in_its_place() {
    let mut letters = run("x", 20.0, 0.0);
    letters.glyphs[0].text = "ft".into();
    let (_, items) = read_page(&[unmarked(letters)], &[]);
    let [TextItem::Char(first), TextItem::Char(second)] = items.as_slice() else {
        panic!("{items:?}");
    };
    assert_eq!((first.text, second.text), ('f', 't'));
    assert_eq!(first.bounds, second.bounds);
    assert_eq!((first.bounds.left, first.bounds.right), (20.0, 25.0));
}

#[test]
fn characters_stand_in_page_space() {
    let (_, items) = read_page(&[unmarked(run("a", 20.0, 100.0))], &[]);
    let [TextItem::Char(character)] = items.as_slice() else {
        panic!("{items:?}");
    };
    // The page is 300 high, and the box of the glyph reaches from its
    // baseline 10 up.
    assert_eq!(
        (character.bounds.top, character.bounds.bottom),
        (190.0, 200.0)
    );
}

#[test]
fn text_that_the_producer_names_replaces_the_glyphs_it_stands_for() {
    let hyphen = marked(run("-", 10.0, 0.0), &standing_for("\u{AD}"));
    let objects = [unmarked(run("ab", 0.0, 0.0)), hyphen];
    assert_eq!(written(&read_page(&objects, &[]).1), "ab\u{AD}");
}

#[test]
fn named_text_takes_the_places_of_its_glyphs_in_turn() {
    let objects = [marked(run("xy", 0.0, 0.0), &standing_for("abcd"))];
    let (_, items) = read_page(&objects, &[]);
    let lefts: Vec<f32> = items
        .iter()
        .map(|item| match item {
            TextItem::Char(character) => character.bounds.left,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(written(&items), "abcd");
    assert_eq!(lefts, [0.0, 0.0, 5.0, 5.0]);
}

#[test]
fn text_is_named_once_for_all_runs_that_one_mark_is_around() {
    let named = standing_for("word");
    let objects = [
        unmarked(run("a", 0.0, 0.0)),
        marked(run("wo", 20.0, 0.0), &named),
        marked(run("rd", 30.0, 0.0), &named),
        // The same text under a mark of its own is there again.
        marked(run("xx", 60.0, 0.0), &standing_for("word")),
        unmarked(run("b", 90.0, 0.0)),
    ];
    let (spans, items) = read_page(&objects, &[]);
    assert_eq!(written(&items), "a word word b");
    assert_eq!(spans.len(), 5);
    assert_eq!(spans[1].actual_text.as_deref(), Some("word"));
    assert_eq!(spans[0].actual_text, None);
}

#[test]
fn empty_named_text_leaves_the_glyphs() {
    let objects = [marked(run("ab", 0.0, 0.0), &standing_for(""))];
    assert_eq!(written(&read_page(&objects, &[]).1), "ab");
}

#[test]
fn figure_stands_before_the_text_that_is_drawn_after_it() {
    let objects = [
        unmarked(run("ab", 0.0, 0.0)),
        shape(),
        unmarked(run("cd", 0.0, -12.0)),
        shape(),
    ];
    assert_eq!(written(&read_page(&objects, &[1, 3]).1), "ab/#cd#");
    assert_eq!(written(&read_page(&[shape()], &[0]).1), "#");
}

#[test]
fn figure_stands_before_the_text_that_it_draws_itself() {
    let form = Object {
        marks: Marks::default(),
        content: Content::Form(vec![shape(), unmarked(run("cd", 0.0, -12.0))]),
    };
    let objects = [unmarked(run("ab", 0.0, 0.0)), form];
    let (spans, items) = read_page(&objects, &[1]);
    assert_eq!(written(&items), "ab/#cd");
    assert_eq!(spans.len(), 2);
}

#[test]
fn span_says_how_its_run_looks() {
    let mut red = run_in("a", (0.0, 0.0), (0.0, -1.0), 12.0);
    red.fill_color = Color { red: 255, ..BLACK };
    let (spans, _) = read_page(&[unmarked(red)], &[]);
    let expected = Style {
        font_name: "Test".to_owned(),
        font_size: 12.0,
        font_weight: Some(400),
        is_italic: false,
        fill_color: Color { red: 255, ..BLACK },
        // Downward is a quarter turn clockwise.
        rotation_degrees: 90.0,
    };
    assert_eq!(spans[0].style, expected);
    let (spans, _) = read_page(&[unmarked(run("a", 0.0, 0.0))], &[]);
    assert_eq!(spans[0].style.rotation_degrees, 0.0);
    assert!(spans[0].style.rotation_degrees.is_sign_positive());
}

#[test]
fn span_says_what_marks_and_structure_say_of_its_run() {
    let in_cell = Marks::default().with(Mark {
        marked_content_id: Some(7),
        ..mark("P")
    });
    let artifact = in_cell.with(mark("Artifact"));
    let objects = [
        marked(run("a", 0.0, 0.0), &in_cell),
        marked(run("b", 0.0, -12.0), &artifact),
        unmarked(run("c", 0.0, -24.0)),
    ];
    let structure = HashMap::from([(
        7,
        Structure {
            path: "Document/Table/TR/TD".to_owned(),
            cell: Some(2),
        },
    )]);
    let (spans, _) = read(&objects, &PageSpace::new(PAGE, 0), &structure, &[]);
    let said: Vec<(Option<&str>, Option<usize>, bool)> = spans
        .iter()
        .map(|span| (span.structure_path.as_deref(), span.cell, span.is_artifact))
        .collect();
    let path = Some("Document/Table/TR/TD");
    assert_eq!(
        said,
        [
            (path, Some(2), false),
            (path, Some(2), true),
            (None, None, false)
        ]
    );
}

#[test]
fn control_and_noncharacters_are_not_displayable() {
    assert!(is_displayable('a'));
    assert!(!is_displayable('\u{2}'));
    assert!(!is_displayable('\u{FFFE}'));
}
