//! What the interpreter reports of content streams.

use hayro_syntax::Pdf;

use super::*;
use crate::reader::testing::{PAGE, objects_drawn_by, page_pdf, stream};

/// The advance widths of glyphs of Helvetica at a font size of 10.
const WIDTH_OF_A: f64 = 5.56;
const WIDTH_OF_SPACE: f64 = 2.78;

/// Shows text in Helvetica at a size of 10.
fn text(operators: &str) -> String {
    format!("BT /F1 10 Tf {operators} ET")
}

fn draws(content: &str) -> Vec<Object> {
    objects_drawn_by(content, "", &[])
}

/// The only object that a content stream draws.
fn only(objects: Vec<Object>) -> Object {
    let [object] = <[Object; 1]>::try_from(objects).expect("one object is drawn");
    object
}

fn run_of(object: &Object) -> &TextRun {
    match &object.content {
        Content::Text(run) => run,
        other => panic!("{other:?} is no text"),
    }
}

fn path_of(object: &Object) -> &Path {
    match &object.content {
        Content::Path(path) => path,
        other => panic!("{other:?} is no path"),
    }
}

fn image_of(object: &Object) -> &Image {
    match &object.content {
        Content::Image(image) => image,
        other => panic!("{other:?} is no image"),
    }
}

fn parts_of(object: &Object) -> &[Object] {
    match &object.content {
        Content::Form(parts) => parts,
        other => panic!("{other:?} is no form"),
    }
}

/// Where the glyphs of the only run of a content stream start.
fn starts(content: &str) -> Vec<(f64, f64)> {
    let object = only(draws(content));
    let glyphs = run_of(&object).glyphs.iter();
    glyphs.map(|glyph| rounded(glyph.start)).collect()
}

/// A point to hundredths, which the widths of the test font are in.
fn rounded((x, y): (f64, f64)) -> (f64, f64) {
    ((x * 100.0).round() / 100.0, (y * 100.0).round() / 100.0)
}

fn points(path: &Path) -> Vec<(f64, f64)> {
    path.points().collect()
}

#[test]
fn text_is_shown_glyph_by_glyph_from_where_its_matrix_puts_it() {
    let object = only(draws(&text("1 0 0 1 20 100 Tm (ab) Tj")));
    let run = run_of(&object);
    let texts: Vec<&str> = run.glyphs.iter().map(|glyph| &*glyph.text).collect();
    assert_eq!(texts, ["a", "b"]);
    assert_eq!(rounded(run.glyphs[0].start), (20.0, 100.0));
    assert_eq!(rounded(run.glyphs[0].end), (20.0 + WIDTH_OF_A, 100.0));
    assert_eq!(rounded(run.glyphs[1].start), (20.0 + WIDTH_OF_A, 100.0));
    assert_eq!(run.font_size, 10.0);
    assert_eq!(run.direction, (1.0, 0.0));
    assert_eq!(run.font.name, "Helvetica");
}

#[test]
fn each_operator_that_shows_text_is_a_run() {
    let objects = draws(&text("(a) Tj (b) Tj [(c) (d)] TJ"));
    let glyph_counts: Vec<usize> = objects
        .iter()
        .map(|object| run_of(object).glyphs.len())
        .collect();
    assert_eq!(glyph_counts, [1, 1, 2]);
    // An operator that shows nothing draws nothing.
    assert!(draws(&text("() Tj")).is_empty());
}

#[test]
fn glyph_box_is_as_wide_as_the_glyph_and_as_high_as_the_font_reaches() {
    let object = only(draws(&text("1 0 0 1 20 100 Tm (a) Tj")));
    let bounds = run_of(&object).glyphs[0].bounds;
    assert!((bounds.left - 20.0).abs() < 0.001 && (bounds.right - 25.56).abs() < 0.001);
    // Helvetica is not embedded and comes without metrics, so it reaches
    // three quarters of its size up and a quarter down.
    assert!((bounds.bottom - 97.5).abs() < 0.001 && (bounds.top - 107.5).abs() < 0.001);
}

#[test]
fn spacing_moves_the_glyphs_that_follow() {
    let between_characters = 1.0;
    let between_words = 2.0;
    let space_start = WIDTH_OF_A + between_characters;
    let word_start = space_start + WIDTH_OF_SPACE + between_characters + between_words;
    assert_eq!(
        starts(&text("1 Tc 2 Tw (a a) Tj")),
        [
            (0.0, 0.0),
            rounded((space_start, 0.0)),
            rounded((word_start, 0.0))
        ]
    );
}

#[test]
fn number_between_strings_moves_back_by_thousandths_of_the_font_size() {
    assert_eq!(
        starts(&text("[(a) -500 (a) 100 (a)] TJ")),
        [
            (0.0, 0.0),
            rounded((WIDTH_OF_A + 5.0, 0.0)),
            rounded((2.0 * WIDTH_OF_A + 5.0 - 1.0, 0.0))
        ]
    );
}

#[test]
fn horizontal_scaling_stretches_glyphs_and_what_is_between_them() {
    let object = only(draws(&text("50 Tz 2 Tc (aa) Tj")));
    let glyphs = &run_of(&object).glyphs;
    assert_eq!(rounded(glyphs[0].end), (WIDTH_OF_A / 2.0, 0.0));
    assert_eq!(rounded(glyphs[1].start), (WIDTH_OF_A / 2.0 + 1.0, 0.0));
}

#[test]
fn rise_lifts_text_off_its_line() {
    assert_eq!(starts(&text("3 Ts (a) Tj")), [(0.0, 3.0)]);
}

#[test]
fn lines_start_where_the_operators_between_them_say() {
    let line_starts = |operators: &str| -> Vec<(f64, f64)> {
        let objects = draws(&text(operators));
        let runs = objects.iter().map(run_of);
        runs.map(|run| rounded(run.glyphs[0].start)).collect()
    };
    // An offset from the start of the line, also where it sets the leading.
    assert_eq!(
        line_starts("5 50 Td (a) Tj 0 -12 Td (a) Tj 2 -14 TD (a) Tj T* (a) Tj"),
        [(5.0, 50.0), (5.0, 38.0), (7.0, 24.0), (7.0, 10.0)]
    );
    // The leading, with the operators that show text on the next line.
    assert_eq!(
        line_starts("12 TL (a) Tj T* (a) Tj (a) ' 0 0 (a) \""),
        [(0.0, 0.0), (0.0, -12.0), (0.0, -24.0), (0.0, -36.0)]
    );
    // A text object starts over.
    let again = format!("{} {}", text("5 50 Td (a) Tj"), text("(a) Tj"));
    let objects = draws(&again);
    assert_eq!(rounded(run_of(&objects[1]).glyphs[0].start), (0.0, 0.0));
}

#[test]
fn transformation_applies_to_what_is_drawn_after_it() {
    let content = format!("2 0 0 2 10 20 cm {}", text("(a) Tj"));
    let object = only(draws(&content));
    let run = run_of(&object);
    assert_eq!(rounded(run.glyphs[0].start), (10.0, 20.0));
    assert_eq!(
        rounded(run.glyphs[0].end),
        rounded((10.0 + 2.0 * WIDTH_OF_A, 20.0))
    );
    assert_eq!(run.font_size, 20.0);
}

#[test]
fn turned_text_knows_its_direction() {
    // A quarter turn counterclockwise.
    let object = only(draws(&text("0 1 -1 0 50 50 Tm (a) Tj")));
    let run = run_of(&object);
    assert_eq!(rounded(run.direction), (0.0, 1.0));
    assert_eq!(rounded(run.glyphs[0].end), (50.0, 50.0 + WIDTH_OF_A));
    assert_eq!(run.font_size, 10.0);
}

#[test]
fn saved_state_comes_back_and_no_more_than_was_saved() {
    let content = format!("q 2 0 0 2 0 0 cm Q Q Q {}", text("(a) Tj"));
    assert_eq!(run_of(&only(draws(&content))).font_size, 10.0);
}

#[test]
fn text_is_filled_in_the_color_and_with_the_opacity_of_the_state() {
    let color_of = |operators: &str| {
        let objects = objects_drawn_by(
            &text(operators),
            "/ExtGState << /Half << /ca 0.5 >> >> \
             /ColorSpace << /Palette [/Indexed /DeviceRGB 1 <0000FF00FF00>] >>",
            &[],
        );
        run_of(&only(objects)).fill_color.to_string()
    };
    assert_eq!(color_of("(a) Tj"), "#000000");
    assert_eq!(color_of("1 0 0 rg (a) Tj"), "#ff0000");
    assert_eq!(color_of("0.5 g (a) Tj"), "#808080");
    assert_eq!(color_of("0 1 1 0 k (a) Tj"), "#ff0000");
    assert_eq!(color_of("/DeviceRGB cs 0 1 0 sc (a) Tj"), "#00ff00");
    assert_eq!(color_of("/Palette cs 1 scn (a) Tj"), "#00ff00");
    // A color space starts out with its darkest color.
    assert_eq!(color_of("1 0 0 rg /DeviceCMYK cs (a) Tj"), "#000000");
    assert_eq!(color_of("1 0 0 rg /Half gs (a) Tj"), "#ff000080");
    // What is painted with a pattern has no color to tell.
    assert_eq!(color_of("/Pattern cs /Dots scn (a) Tj"), "#000000");
}

#[test]
fn text_without_a_font_of_its_page_is_read_as_helvetica() {
    let object = only(draws("BT /Missing 10 Tf (a) Tj ET"));
    let run = run_of(&object);
    assert_eq!(
        (&*run.glyphs[0].text, run.font.name.as_str()),
        ("a", "Helvetica")
    );
    // Before a font is set, nothing says how big text is.
    assert!(draws("BT (a) Tj ET").is_empty());
}

#[test]
fn graphics_state_dictionary_can_set_the_font() {
    let objects = objects_drawn_by(
        "BT /Big gs (a) Tj ET",
        "/ExtGState << /Big << /Font [5 0 R 30] >> >>",
        &["<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_owned()],
    );
    let object = only(objects);
    let run = run_of(&object);
    assert_eq!(
        (run.font.name.as_str(), run.font_size),
        ("Times-Roman", 30.0)
    );
}

#[test]
fn filled_rectangle_is_a_closed_path_of_four_points() {
    let object = only(draws("0 0 1 rg 10 20 30 40 re f"));
    let path = path_of(&object);
    assert_eq!(
        points(path),
        [(10.0, 20.0), (40.0, 20.0), (40.0, 60.0), (10.0, 60.0)]
    );
    assert!(path.segments[3].closes_outline);
    let expected_fill = Fill {
        rule: FillRule::NonZero,
        color: Some([0, 0, 255]),
    };
    assert_eq!((path.fill, path.stroke), (Some(expected_fill), None));
}

#[test]
fn operators_paint_a_path_as_they_say() {
    let painted = |operator: &str| {
        let object = only(draws(&format!("0 0 m 10 0 l 10 10 l {operator}")));
        let path = path_of(&object);
        let rule = path.fill.map(|fill| fill.rule);
        let closes = path.segments.last().is_some_and(|last| last.closes_outline);
        (rule, path.stroke.is_some(), closes)
    };
    let (non_zero, even_odd) = (Some(FillRule::NonZero), Some(FillRule::EvenOdd));
    assert_eq!(painted("S"), (None, true, false));
    assert_eq!(painted("s"), (None, true, true));
    assert_eq!(painted("f"), (non_zero, false, false));
    assert_eq!(painted("F"), (non_zero, false, false));
    assert_eq!(painted("f*"), (even_odd, false, false));
    assert_eq!(painted("B"), (non_zero, true, false));
    assert_eq!(painted("B*"), (even_odd, true, false));
    assert_eq!(painted("b"), (non_zero, true, true));
    assert_eq!(painted("b*"), (even_odd, true, true));
    assert_eq!(painted("h S"), (None, true, true));
}

#[test]
fn stroke_knows_its_color_and_its_width_as_set_and_on_the_page() {
    let object = only(draws("2 0 0 2 0 0 cm 1 0 0 RG 3 w 0 0 m 10 0 l S"));
    let path = path_of(&object);
    assert_eq!(points(path), [(0.0, 0.0), (20.0, 0.0)]);
    let expected = Stroke {
        color: Some([255, 0, 0]),
        width: 3.0,
        width_on_page: 6.0,
    };
    assert_eq!(path.stroke, Some(expected));
}

#[test]
fn curve_can_take_a_control_point_from_where_it_starts_or_ends() {
    let curve_points = |operators: &str| {
        let object = only(draws(&format!("1 1 m {operators} S")));
        points(path_of(&object))[1..].to_vec()
    };
    assert_eq!(
        curve_points("2 3 4 5 6 7 c"),
        [(2.0, 3.0), (4.0, 5.0), (6.0, 7.0)]
    );
    assert_eq!(
        curve_points("4 5 6 7 v"),
        [(1.0, 1.0), (4.0, 5.0), (6.0, 7.0)]
    );
    assert_eq!(
        curve_points("2 3 6 7 y"),
        [(2.0, 3.0), (6.0, 7.0), (6.0, 7.0)]
    );
}

#[test]
fn path_that_is_not_painted_is_no_object() {
    assert!(draws("0 0 10 10 re n").is_empty());
    // The next path starts from nothing.
    let object = only(draws("0 0 10 10 re n 5 5 m 9 9 l S"));
    assert_eq!(points(path_of(&object)), [(5.0, 5.0), (9.0, 9.0)]);
}

#[test]
fn shading_fills_what_the_clip_leaves_of_the_page() {
    let bounds_of_shading = |content: &str| match only(draws(content)).content {
        Content::Shading { bounds } => (bounds.left, bounds.bottom, bounds.right, bounds.top),
        other => panic!("{other:?} is no shading"),
    };
    assert_eq!(bounds_of_shading("/Fade sh"), (0.0, 0.0, 200.0, 300.0));
    assert_eq!(
        bounds_of_shading("10 20 30 40 re W n 0 0 25 100 re W* n /Fade sh"),
        (10.0, 20.0, 25.0, 60.0)
    );
    // The clip is part of the state that is saved and brought back.
    assert_eq!(
        bounds_of_shading("q 10 20 30 40 re W n Q /Fade sh"),
        (0.0, 0.0, 200.0, 300.0)
    );
}

/// An image of two gray pixels with the given values, as the object 5.
fn image_object(pixels: &str) -> String {
    stream(
        "/Type /XObject /Subtype /Image /Width 2 /Height 1 \
         /ColorSpace /DeviceGray /BitsPerComponent 8",
        pixels,
    )
}

#[test]
fn image_fills_the_unit_square_of_the_space_it_is_drawn_in() {
    let objects = objects_drawn_by(
        "q 20 0 0 10 5 7 cm /Picture Do Q",
        "/XObject << /Picture 5 0 R >>",
        &[image_object("ab")],
    );
    let object = only(objects);
    let image = image_of(&object);
    assert_eq!((image.pixel_width, image.pixel_height), (2, 1));
    assert_eq!(
        image.corners,
        [(5.0, 7.0), (25.0, 7.0), (25.0, 17.0), (5.0, 17.0)]
    );
}

#[test]
fn image_is_told_from_another_by_its_data_and_not_by_its_place() {
    let objects = objects_drawn_by(
        "/Picture Do q 2 0 0 2 0 0 cm /Picture Do Q /Other Do /Copy Do",
        "/XObject << /Picture 5 0 R /Other 6 0 R /Copy 7 0 R >>",
        &[image_object("ab"), image_object("ac"), image_object("ab")],
    );
    let hashes: Vec<u64> = objects
        .iter()
        .map(|object| image_of(object).data_hash)
        .collect();
    assert_eq!(hashes[0], hashes[1]);
    assert_ne!(hashes[0], hashes[2]);
    assert_eq!(hashes[0], hashes[3]);
}

#[test]
fn mask_of_an_image_is_part_of_its_data() {
    let masked = format!(
        "{} {}",
        "/Type /XObject /Subtype /Image /Width 2 /Height 1",
        "/ColorSpace /DeviceGray /BitsPerComponent 8 /SMask 7 0 R"
    );
    let objects = objects_drawn_by(
        "/Plain Do /Masked Do",
        "/XObject << /Plain 5 0 R /Masked 6 0 R >>",
        &[
            image_object("ab"),
            stream(&masked, "ab"),
            image_object("zz"),
        ],
    );
    assert_ne!(
        image_of(&objects[0]).data_hash,
        image_of(&objects[1]).data_hash
    );
}

#[test]
fn image_in_the_content_stream_is_an_image() {
    let object = only(draws(
        "q 20 0 0 10 0 0 cm BI /W 2 /H 1 /CS /G /BPC 8 ID ab EI Q",
    ));
    let image = image_of(&object);
    assert_eq!((image.pixel_width, image.pixel_height), (2, 1));
    assert_eq!(image.corners[2], (20.0, 10.0));
}

/// A form that draws a square and a letter, as the object 5.
fn form_object(entries: &str) -> String {
    let content = format!("0 0 10 10 re f {}", text("(a) Tj"));
    stream(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 50 50] {entries}"),
        &content,
    )
}

#[test]
fn form_stands_for_what_it_draws_in_the_space_of_the_page() {
    let objects = objects_drawn_by(
        "q 2 0 0 2 0 0 cm /Stamp Do Q 0 0 1 1 re f",
        "/XObject << /Stamp 5 0 R >>",
        &[form_object("/Matrix [1 0 0 1 30 40]")],
    );
    let [form, square_after] = objects.as_slice() else {
        panic!("{objects:?}");
    };
    let [square, letter] = parts_of(form) else {
        panic!("{form:?}");
    };
    // The matrix of the form applies before that of the page around it.
    assert_eq!(points(path_of(square))[0], (60.0, 80.0));
    assert_eq!(points(path_of(square))[2], (80.0, 100.0));
    assert_eq!(rounded(run_of(letter).glyphs[0].start), (60.0, 80.0));
    assert_eq!(run_of(letter).font_size, 20.0);
    // What the form sets ends with it.
    assert_eq!(points(path_of(square_after))[2], (1.0, 1.0));
}

#[test]
fn form_uses_the_resources_of_what_draws_it_unless_it_has_its_own() {
    let own_font = "/Resources << /Font << /F1 << /Type /Font /Subtype /Type1 \
                    /BaseFont /Courier >> >> >>";
    let font_in_form = |entries: &str| {
        let objects = objects_drawn_by(
            "/Stamp Do",
            "/XObject << /Stamp 5 0 R >>",
            &[form_object(entries)],
        );
        run_of(&parts_of(&objects[0])[1]).font.name.clone()
    };
    assert_eq!(font_in_form(""), "Helvetica");
    assert_eq!(font_in_form(own_font), "Courier");
}

/// What a page draws, or the limit that it goes beyond, where the page may
/// run as much content as given.
fn objects_drawn_within(
    content: &str,
    resources: &str,
    more_objects: &[String],
    max_content_bytes: usize,
) -> Result<Vec<Object>, PageLimit> {
    let pdf = Pdf::new(page_pdf(content, resources, more_objects)).expect("the PDF opens");
    objects_within(&pdf.pages()[0], &Shared::new(), PAGE, max_content_bytes)
}

/// What a page draws that has the given content and no other resources
/// than its font, or the limit that it goes beyond.
fn drawn_within_the_limits(content: &str) -> Result<Vec<Object>, PageLimit> {
    objects_drawn_within(content, "", &[], MAX_CONTENT_BYTES_PER_PAGE)
}

/// Forms as the objects from 5 on, of which each draws the next one as
/// often as given and the last one draws a square, and the content of the
/// first one.
fn forms_inside_forms(depth: usize, draw_count: usize) -> (Vec<String>, Vec<String>) {
    let contents: Vec<String> = (0..depth)
        .map(|index| match index + 1 < depth {
            true => vec!["/Next Do"; draw_count].join(" "),
            false => "0 0 1 1 re f".to_owned(),
        })
        .collect();
    let forms = contents.iter().enumerate().map(|(index, content)| {
        let next = 5 + index + 1;
        stream(
            &format!(
                "/Type /XObject /Subtype /Form /BBox [0 0 50 50] \
                 /Resources << /XObject << /Next {next} 0 R >> >>"
            ),
            content,
        )
    });
    (forms.collect(), contents)
}

#[test]
fn forms_inside_forms_are_drawn_down_to_a_depth_and_fail_the_page_below_it() {
    let draw_forms = |depth| {
        let (forms, _) = forms_inside_forms(depth, 1);
        let resources = "/XObject << /First 5 0 R >>";
        objects_drawn_within("/First Do", resources, &forms, MAX_CONTENT_BYTES_PER_PAGE)
    };
    let objects = draw_forms(MAX_FORM_DEPTH).expect("the forms are drawn");
    let mut depth = 0;
    let mut drawn = &objects[0];
    while let Content::Form(parts) = &drawn.content {
        drawn = &parts[0];
        depth += 1;
    }
    assert_eq!(depth, MAX_FORM_DEPTH);
    assert_eq!(
        draw_forms(MAX_FORM_DEPTH + 1).unwrap_err(),
        PageLimit::FormDepth
    );
}

#[test]
fn forms_that_draw_each_other_in_a_circle_fail_the_page() {
    // Each form draws a square and the form after it, and the last one
    // draws the first.
    let form_count = MAX_FORM_DEPTH + 4;
    let content = "0 0 1 1 re f /Next Do";
    let forms: Vec<String> = (0..form_count)
        .map(|index| {
            let next = 5 + (index + 1) % form_count;
            stream(
                &format!(
                    "/Type /XObject /Subtype /Form /BBox [0 0 50 50] \
                     /Resources << /XObject << /Next {next} 0 R >> >>"
                ),
                content,
            )
        })
        .collect();
    // Room for the forms to be drawn twice as deep as they may be, so that
    // the depth is what stops them.
    let max_content_bytes = 2 * MAX_FORM_DEPTH * content.len() + "/First Do".len();
    let resources = "/XObject << /First 5 0 R >>";
    let drawn = objects_drawn_within("/First Do", resources, &forms, max_content_bytes);
    assert_eq!(drawn.unwrap_err(), PageLimit::FormDepth);
}

#[test]
fn content_of_a_form_counts_each_time_that_the_form_is_drawn() {
    // 4 to the power of 5 squares, drawn by forms that each draw the next
    // form four times.
    let (forms, contents) = forms_inside_forms(6, 4);
    let page_content = "/First Do";
    let content_bytes: usize = page_content.len()
        + (contents.iter().enumerate())
            .map(|(depth, content)| 4_usize.pow(depth as u32) * content.len())
            .sum::<usize>();
    let resources = "/XObject << /First 5 0 R >>";
    let draw_within = |max_content_bytes| {
        objects_drawn_within(page_content, resources, &forms, max_content_bytes)
    };
    assert!(draw_within(content_bytes).is_ok());
    assert_eq!(
        draw_within(content_bytes - 1).unwrap_err(),
        PageLimit::ContentBytes
    );
}

#[test]
fn content_of_the_page_counts_too() {
    let content = "0 0 1 1 re f";
    let draw_within = |max_content_bytes| objects_drawn_within(content, "", &[], max_content_bytes);
    assert!(draw_within(content.len()).is_ok());
    assert_eq!(
        draw_within(content.len() - 1).unwrap_err(),
        PageLimit::ContentBytes
    );
}

#[test]
fn graphics_states_saved_at_once_beyond_a_depth_fail_the_page() {
    let saved = |count| "q ".repeat(count);
    assert!(drawn_within_the_limits(&saved(MAX_SAVED_STATE_DEPTH)).is_ok());
    assert_eq!(
        drawn_within_the_limits(&saved(MAX_SAVED_STATE_DEPTH + 1)).unwrap_err(),
        PageLimit::SavedStateDepth
    );
    // A state that is brought back is no longer saved.
    let saved_and_brought_back = "q Q ".repeat(MAX_SAVED_STATE_DEPTH + 1);
    assert!(drawn_within_the_limits(&saved_and_brought_back).is_ok());
}

#[test]
fn marked_content_beyond_a_depth_fails_the_page() {
    let marked = |depth| "/Span BMC ".repeat(depth);
    assert!(drawn_within_the_limits(&marked(MAX_MARKED_CONTENT_DEPTH)).is_ok());
    assert_eq!(
        drawn_within_the_limits(&marked(MAX_MARKED_CONTENT_DEPTH + 1)).unwrap_err(),
        PageLimit::MarkedContentDepth
    );
    // Marked content that ends is no longer around what follows it.
    let marked_and_ended = "/Span BMC EMC ".repeat(MAX_MARKED_CONTENT_DEPTH + 1);
    assert!(drawn_within_the_limits(&marked_and_ended).is_ok());
}

/// The marked content identifiers of the objects that a content stream
/// draws.
fn ids(content: &str, resources: &str) -> Vec<Option<i32>> {
    let objects = objects_drawn_by(content, resources, &[]);
    let marks = objects.iter().map(|object| &object.marks);
    marks.map(Marks::marked_content_id).collect()
}

#[test]
fn marked_content_marks_what_is_drawn_in_it() {
    let square = "0 0 1 1 re f";
    let content = format!(
        "{square} /P << /MCID 3 >> BDC {square} /Span BMC {square} EMC \
         /Span << /MCID 4 >> BDC {square} EMC {square} EMC {square}"
    );
    assert_eq!(
        ids(&content, ""),
        [None, Some(3), Some(3), Some(4), Some(3), None]
    );
}

#[test]
fn properties_of_marked_content_can_be_a_resource() {
    let content = "/P /Third BDC 0 0 1 1 re f EMC";
    assert_eq!(
        ids(content, "/Properties << /Third << /MCID 3 >> >>"),
        [Some(3)]
    );
}

#[test]
fn marked_content_that_was_not_begun_cannot_be_ended() {
    assert_eq!(ids("EMC /P << /MCID 3 >> BDC 0 0 1 1 re f", ""), [Some(3)]);
}

#[test]
fn artifact_says_what_kind_it_is() {
    let objects = draws(
        "/Artifact << /Type /Pagination /Subtype /Header >> BDC 0 0 1 1 re f EMC \
         /Artifact BMC 0 0 1 1 re f EMC 0 0 1 1 re f",
    );
    let kinds: Vec<Vec<Option<&str>>> = objects
        .iter()
        .map(|object| {
            let artifacts = object.marks.artifacts();
            artifacts
                .map(|mark| mark.artifact_type.as_deref())
                .collect()
        })
        .collect();
    assert_eq!(kinds, [vec![Some("Pagination")], vec![None], vec![]]);
    assert!(objects[1].marks.is_artifact() && !objects[2].marks.is_artifact());
}

#[test]
fn actual_text_of_the_innermost_mark_that_has_one_counts() {
    // The soft hyphen in UTF-16, and a plain string.
    let objects = draws(
        "/Span << /ActualText <FEFF00AD> >> BDC 0 0 1 1 re f \
         /Span << /ActualText (fi) >> BDC 0 0 1 1 re f EMC \
         /Span BMC 0 0 1 1 re f EMC EMC 0 0 1 1 re f",
    );
    let texts: Vec<Option<&str>> = objects
        .iter()
        .map(|object| {
            let mark = object.marks.with_actual_text()?;
            mark.actual_text.as_deref()
        })
        .collect();
    assert_eq!(texts, [Some("\u{AD}"), Some("fi"), Some("\u{AD}"), None]);
}

#[test]
fn marks_around_a_form_are_around_what_it_draws() {
    let marked_form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 50 50]",
        "/P << /MCID 9 >> BDC 0 0 1 1 re f EMC /Artifact BMC 0 0 1 1 re f EMC EMC",
    );
    let objects = objects_drawn_by(
        "/Figure << /MCID 1 >> BDC /Stamp Do 0 0 1 1 re f EMC",
        "/XObject << /Stamp 5 0 R >>",
        &[marked_form],
    );
    let [form, square_after] = objects.as_slice() else {
        panic!("{objects:?}");
    };
    let [first, second] = parts_of(form) else {
        panic!("{form:?}");
    };
    // The marked content in a form is numbered apart from that of the
    // page, so the number of the page counts.
    assert_eq!(first.marks.marked_content_id(), Some(1));
    assert!(!first.marks.is_artifact() && second.marks.is_artifact());
    // A form cannot end the marked content around it.
    assert_eq!(square_after.marks.marked_content_id(), Some(1));
}
