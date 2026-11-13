//! What the PDFium reader and the flow get out of real PDFs.

mod common;

use std::sync::Arc;

use abpdf::flow::{Flow, Token};
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::reader::{Color, ReadError, Reader};

use common::{corpus, flow_of, texts, token};

const POINTS_PER_CENTIMETER: f32 = 72.0 / 2.54;

fn tagged() -> Flow {
    flow_of(corpus("reading-tagged.pdf"))
}

fn untagged() -> Flow {
    flow_of(corpus("reading-untagged.pdf"))
}

fn on_page(flow: &Flow, page_index: usize) -> impl Iterator<Item = &Token> {
    flow.tokens
        .iter()
        .filter(move |token| token.fragments[0].page_index == page_index)
}

/// Whether `sequence` occurs in `texts` without gaps.
fn contains_sequence(texts: &[&str], sequence: &[&str]) -> bool {
    texts
        .windows(sequence.len())
        .any(|window| window == sequence)
}

#[test]
fn tagging_is_detected() {
    let is_tagged = |file_name| {
        PdfiumReader::open(corpus(file_name))
            .expect("the PDF opens")
            .is_tagged()
    };
    assert!(is_tagged("reading-tagged.pdf"));
    assert!(!is_tagged("reading-untagged.pdf"));
}

#[test]
fn tagged_and_untagged_documents_read_as_the_same_text() {
    let (tagged, untagged) = (tagged(), untagged());
    assert_eq!(texts(&tagged.tokens), texts(&untagged.tokens));
}

#[test]
fn pages_are_measured_in_points() {
    let flow = tagged();
    assert_eq!(flow.page_sizes.len(), 4);
    let a6 = flow.page_sizes[0];
    assert!((a6.width - 10.5 * POINTS_PER_CENTIMETER).abs() < 0.01);
    assert!((a6.height - 14.8 * POINTS_PER_CENTIMETER).abs() < 0.01);
}

#[test]
fn positions_are_measured_from_the_top_left_corner() {
    let flow = tagged();
    let page_height = flow.page_sizes[0].height;
    let header = token(&flow, "Reading").fragments[0].bounds;
    let footer = token(&flow, "Page").fragments[0].bounds;

    let side_margin = 1.5 * POINTS_PER_CENTIMETER;
    let top_and_bottom_margin = 2.0 * POINTS_PER_CENTIMETER;
    assert!((header.left - side_margin).abs() < 0.5);
    assert!(header.bottom < top_and_bottom_margin);
    assert!(footer.top > page_height - top_and_bottom_margin);
    assert!(header.top < header.bottom && header.left < header.right);
}

#[test]
fn page_furniture_is_excluded_when_the_producer_marks_it() {
    let flow = tagged();
    let compared: Vec<&Token> = flow
        .tokens
        .iter()
        .filter(|token| !token.is_excluded())
        .collect();
    let compared_texts = texts(compared.iter().copied());

    for furniture in ["Reading", "corpus", "Draft", "Page"] {
        assert!(
            !compared_texts.contains(&furniture),
            "{furniture} is compared"
        );
    }
    // The page reference that the source marks as an artifact, and the
    // leaders of the table of contents.
    assert!(contains_sequence(
        &compared_texts,
        &["See", "Section", "4", "."]
    ));
    assert!(contains_sequence(
        &compared_texts,
        &["Broken", "words", "1", "2", "Characters"]
    ));
}

#[test]
fn table_header_is_excluded_where_it_repeats() {
    let flow = tagged();
    let headers_by_page: Vec<Vec<bool>> = (0..flow.page_sizes.len())
        .map(|page_index| {
            on_page(&flow, page_index)
                .filter(|token| token.text == "Key")
                .map(Token::is_excluded)
                .collect()
        })
        .collect();
    assert_eq!(
        headers_by_page,
        [vec![], vec![false], vec![true], vec![true]]
    );
}

#[test]
fn nothing_is_excluded_without_tags() {
    assert!(untagged().tokens.iter().all(|token| !token.is_excluded()));
}

#[test]
fn words_that_a_line_break_splits_are_whole() {
    for flow in [tagged(), untagged()] {
        let texts = texts(&flow.tokens);
        // Hard hyphens at line ends, which the regular font leaves unmarked.
        let compound = [
            "a",
            "state",
            "-",
            "of",
            "-",
            "the",
            "-",
            "art",
            "-",
            "comparison",
            "is",
        ];
        assert!(contains_sequence(&texts, &compound), "{texts:?}");
        assert!(contains_sequence(
            &texts,
            &["extraordinarily", "uncharacteristic", "."]
        ));
        // Soft hyphens that the bold font marks.
        let bold = [
            "Well",
            "-",
            "known",
            "misunderstandings",
            "are",
            "uncharacteristically",
            "counterproductive",
            ".",
        ];
        assert!(contains_sequence(&texts, &bold), "{texts:?}");
        // A soft hyphen in a font that marks nothing.
        assert!(contains_sequence(
            &texts,
            &["Some", "incomprehensibilities", "."]
        ));
    }
}

#[test]
fn word_that_a_line_break_splits_is_marked_on_both_lines() {
    let flow = tagged();
    let word = token(&flow, "incomprehensibilities");
    let [first, second] = word.fragments.as_slice() else {
        panic!("{:?}", word.fragments);
    };
    assert_eq!(second.line_index, first.line_index + 1);
    assert!(second.bounds.top >= first.bounds.bottom - 1.0);
    assert!(second.bounds.left < first.bounds.right);
}

#[test]
fn ligatures_read_as_their_letters() {
    let flow = tagged();
    assert!(contains_sequence(
        &texts(&flow.tokens),
        &["The", "first", "office"]
    ));
}

#[test]
fn structure_path_names_the_elements_around_a_token() {
    let flow = tagged();
    let path_of = |text| {
        let index = token(&flow, text).structure_path.expect("a structure path");
        flow.structure_paths[index].as_str()
    };
    assert_eq!(path_of("Contents"), "Document/H1");
    assert_eq!(path_of("quotes"), "Document/P");
    assert_eq!(path_of("Row"), "Document/Table/TBody/TR/TD");
    assert_eq!(path_of("Key"), "Document/Table/THead/TR/TH");
    assert_eq!(path_of("label"), "Document/Div/Caption/Span");

    assert!(
        untagged()
            .tokens
            .iter()
            .all(|token| token.structure_path.is_none())
    );
}

#[test]
fn style_is_read() {
    let flow = untagged();
    let style_of = |text| &flow.styles[token(&flow, text).style];

    let plain = style_of("and");
    assert_eq!(plain.font_name, "LibertinusSerif-Regular");
    assert_eq!(plain.font_size, 10.0);
    assert!(!plain.is_italic);
    assert_eq!(plain.rotation_degrees, 0.0);
    let black = Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    assert_eq!(plain.fill_color, black);

    assert_ne!(style_of("Red").fill_color, black);
    assert!(style_of("bold").font_weight > plain.font_weight);
    assert!(style_of("italic").is_italic);
    assert_eq!(style_of("bigger").font_size, 14.0);
    assert!((style_of("Rotated").rotation_degrees - 90.0).abs() < 0.01);
}

#[test]
fn what_is_not_a_pdf_is_rejected() {
    let result = PdfiumReader::open(Arc::new(b"not a PDF".to_vec()));
    assert!(matches!(result, Err(ReadError::InvalidDocument)));
}

#[test]
fn page_that_does_not_exist_is_an_error() {
    let reader = PdfiumReader::open(corpus("reading-tagged.pdf")).expect("the PDF opens");
    assert_eq!(reader.page_count(), 4);
    let result = reader.read_page(4);
    assert!(matches!(
        result,
        Err(ReadError::InvalidPage { page_number: 5 })
    ));
}
