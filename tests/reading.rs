//! What the PDFium reader and the flow get out of real PDFs.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use abpdf::flow::{Exclusion, Flow, Token};
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::reader::{Color, Page, ReadError, Reader, Span, TextItem};

use common::{corpus, flow_of, raw_flow_of, texts, token};

const POINTS_PER_CENTIMETER: f32 = 72.0 / 2.54;

fn tagged() -> Flow {
    flow_of(corpus("reading-tagged.pdf"))
}

fn untagged() -> Flow {
    flow_of(corpus("reading-untagged.pdf"))
}

/// The pages of a PDF of the corpus as the reader reports them.
fn pages_of(file_name: &str) -> Vec<Page> {
    let reader = PdfiumReader::open(corpus(file_name)).expect("the PDF opens");
    (0..reader.page_count())
        .map(|page_index| reader.read_page(page_index).expect("the page reads"))
        .collect()
}

/// The span of the character with which a page first reads `text`.
fn span_of<'a>(pages: &'a [Page], text: &str) -> &'a Span {
    let span_on = |page: &'a Page| {
        let page_text: String = page
            .text
            .iter()
            .map(|item| match item {
                TextItem::Char(character) => character.text,
                TextItem::Space | TextItem::LineBreak => ' ',
            })
            .collect();
        let item_index = page_text[..page_text.find(text)?].chars().count();
        match &page.text[item_index] {
            TextItem::Char(character) => Some(&page.spans[character.span]),
            TextItem::Space | TextItem::LineBreak => None,
        }
    };
    pages
        .iter()
        .find_map(span_on)
        .unwrap_or_else(|| panic!("no page reads {text:?}"))
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
    let raw_texts = |file_name| -> Vec<String> {
        let tokens = raw_flow_of(corpus(file_name)).tokens;
        tokens.into_iter().map(|token| token.text).collect()
    };
    assert_eq!(
        raw_texts("reading-tagged.pdf"),
        raw_texts("reading-untagged.pdf")
    );
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
    let flow = raw_flow_of(corpus("reading-untagged.pdf"));
    let page_height = flow.page_sizes[0].height;
    let bounds_of = |text: &str| {
        let token = flow.tokens.iter().find(|token| token.text == text);
        token.expect("the page has the text").fragment.bounds
    };
    let header = bounds_of("Reading");
    let footer = bounds_of("Page");

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
    let compared_texts = texts(&flow.tokens);
    let excluded_texts = texts(flow.excluded.iter().map(|excluded| &excluded.token));

    for furniture in ["Reading", "corpus", "Draft", "Page"] {
        assert!(
            !compared_texts.contains(&furniture),
            "{furniture} is compared"
        );
        assert!(
            excluded_texts.contains(&furniture),
            "{furniture} is not excluded"
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
    fn pages_of_header<'a>(tokens: impl IntoIterator<Item = &'a Token>) -> Vec<usize> {
        let headers = tokens.into_iter().filter(|token| token.text == "Key");
        headers.map(|token| token.fragments[0].page_index).collect()
    }
    // The table starts on the second page and continues on two more.
    // With tags the producer says that its header repeats there, and
    // without them its place at the top of the pages does.
    for flow in [tagged(), untagged()] {
        assert_eq!(pages_of_header(&flow.tokens), [1]);
        let excluded = flow.excluded.iter().map(|excluded| &excluded.token);
        assert_eq!(pages_of_header(excluded), [2, 3]);
    }
}

#[test]
fn page_furniture_is_excluded_by_its_place_when_nothing_marks_it() {
    let flow = untagged();
    let texts_excluded_as = |exclusion: Exclusion| -> BTreeSet<&str> {
        let excluded = flow.excluded.iter();
        excluded
            .filter(|excluded| excluded.exclusion == exclusion)
            .map(|excluded| excluded.token.text.as_str())
            .collect()
    };
    // The running header, and the header of the table on the two pages that
    // start with it.
    assert_eq!(
        texts_excluded_as(Exclusion::RunningHeader),
        BTreeSet::from(["Draft", "Key", "Reading", "Value", "corpus"])
    );
    assert_eq!(
        texts_excluded_as(Exclusion::RunningFooter),
        BTreeSet::from(["1", "2", "3", "4", "Page", "of"])
    );
    let by_place = [Exclusion::RunningHeader, Exclusion::RunningFooter];
    let mut excluded = flow.excluded.iter();
    assert!(excluded.all(|excluded| by_place.contains(&excluded.exclusion)));
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
fn structure_path_names_the_elements_around_a_run() {
    let pages = pages_of("reading-tagged.pdf");
    let path_of = |text| span_of(&pages, text).structure_path.as_deref();
    assert_eq!(path_of("Contents"), Some("Document/H1"));
    assert_eq!(path_of("quotes"), Some("Document/P"));
    assert_eq!(path_of("Row"), Some("Document/Table/TBody/TR/TD"));
    assert_eq!(path_of("Key"), Some("Document/Table/THead/TR/TH"));
    assert_eq!(path_of("label"), Some("Document/Div/Caption/Span"));

    let untagged = pages_of("reading-untagged.pdf");
    let mut spans = untagged.iter().flat_map(|page| &page.spans);
    assert!(spans.all(|span| span.structure_path.is_none()));
}

#[test]
fn style_is_read() {
    let pages = pages_of("reading-untagged.pdf");
    let style_of = |text| &span_of(&pages, text).style;

    let plain = style_of("dashes");
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
