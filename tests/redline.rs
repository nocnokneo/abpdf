//! What the redline writer does with real PDFs.

mod common;

use std::sync::Arc;

use abpdf::geometry::{Rect, Size};
use abpdf::reader::Reader;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{
    self, Mark, MarkStyle, Redline, Rgb, Sheet, SheetLayout, SheetPage, WriteError,
};
use abpdf::{BySide, Side};
use pdf_writer::{Content, Finish, Name, Pdf, Ref, Str};

use common::{both, corpus, flow_of, raw_flow_of, texts, token};

/// How far a position may be from where it is expected, in points.
const TOLERANCE: f32 = 0.05;

const MEDIA_BOX: pdf_writer::Rect = pdf_writer::Rect {
    x1: 0.0,
    y1: 0.0,
    x2: 300.0,
    y2: 400.0,
};
/// A crop box whose corner is not the origin and whose sides differ from the
/// media box's.
const CROP_BOX: pdf_writer::Rect = pdf_writer::Rect {
    x1: 20.0,
    y1: 30.0,
    x2: 260.0,
    y2: 380.0,
};
const FONT_SIZE: f32 = 12.0;
/// Two words at their baseline origins in PDF user space, far from each other
/// and from any symmetry of the page.
const WORDS: [(&[u8], f32, f32); 2] = [(b"Alpha", 40.0, 340.0), (b"Omega", 180.0, 60.0)];

#[derive(Clone, Copy, Debug, Default)]
struct PageSetup {
    rotation_degrees: i32,
    is_cropped: bool,
    user_unit: Option<f32>,
    pdf_version: Option<(u8, u8)>,
}

/// A PDF with one page that shows [`WORDS`].
fn single_page_pdf(setup: PageSetup) -> Arc<Vec<u8>> {
    let [catalog_id, page_tree_id, page_id, font_id, content_id] = [1, 2, 3, 4, 5].map(Ref::new);
    let font_name = Name(b"F1");
    let mut pdf = Pdf::new();
    if let Some((major, minor)) = setup.pdf_version {
        pdf.set_version(major, minor);
    }
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id).kids([page_id]).count(1);

    let mut page = pdf.page(page_id);
    page.parent(page_tree_id)
        .media_box(MEDIA_BOX)
        .contents(content_id)
        .pair(Name(b"Rotate"), setup.rotation_degrees);
    if setup.is_cropped {
        page.crop_box(CROP_BOX);
    }
    if let Some(user_unit) = setup.user_unit {
        page.pair(Name(b"UserUnit"), user_unit);
    }
    page.resources().fonts().pair(font_name, font_id);
    page.finish();
    pdf.type1_font(font_id).base_font(Name(b"Helvetica"));

    let mut content = Content::new();
    for (text, x, y) in WORDS {
        content
            .begin_text()
            .set_font(font_name, FONT_SIZE)
            .next_line(x, y)
            .show(Str(text))
            .end_text();
    }
    pdf.stream(content_id, &content.finish());
    Arc::new(pdf.finish())
}

fn unmarked_page(page_index: usize) -> SheetPage {
    SheetPage {
        page_index,
        label: String::new(),
        marks: Vec::new(),
    }
}

/// A redline of one sheet that shows the first page of the new document.
fn first_new_page() -> Redline {
    Redline {
        sheets: vec![Sheet {
            pages: BySide {
                old: None,
                new: Some(unmarked_page(0)),
            },
            ..Sheet::default()
        }],
        ..Redline::default()
    }
}

/// The layout of a sheet that shows a new page of this size and no old one.
fn layout_of_new_page(size: Size) -> SheetLayout {
    SheetLayout::new(BySide {
        old: None,
        new: Some(size),
    })
}

fn assert_close(actual: Rect, expected: Rect, context: &str) {
    let edges = |rect: Rect| [rect.left, rect.top, rect.right, rect.bottom];
    for (actual_edge, expected_edge) in edges(actual).into_iter().zip(edges(expected)) {
        assert!(
            (actual_edge - expected_edge).abs() <= TOLERANCE,
            "{context}: {actual:?} is not {expected:?}"
        );
    }
}

/// Checks that a mark at a token's bounds lands on the token: the reader's
/// page space is where the writer shows the page.
///
/// The redline is read back, and each word of the embedded page has to be
/// where the layout puts the bounds that the reader reported for it.
fn assert_marks_land_on_tokens(setup: PageSetup) {
    let context = format!("{setup:?}");
    let source = single_page_pdf(setup);
    let source_flow = flow_of(source.clone());
    let page_size = source_flow.page_sizes[0];

    let output = redline::write(&first_new_page(), both(source)).expect(&context);
    let sheet_flow = flow_of(Arc::new(output));

    let layout = layout_of_new_page(page_size);
    assert_eq!(sheet_flow.page_sizes, [layout.size], "{context}");
    assert_eq!(
        texts(&sheet_flow.tokens),
        texts(&source_flow.tokens),
        "{context}"
    );
    for (on_sheet, on_page) in sheet_flow.tokens.iter().zip(&source_flow.tokens) {
        let expected = layout.place(Side::New, on_page.fragments[0].bounds);
        assert_close(on_sheet.fragments[0].bounds, expected, &context);
    }
}

#[test]
fn marks_land_on_tokens_of_a_rotated_page() {
    for rotation_degrees in [0, 90, 180, 270] {
        assert_marks_land_on_tokens(PageSetup {
            rotation_degrees,
            ..PageSetup::default()
        });
    }
}

#[test]
fn marks_land_on_tokens_of_a_cropped_page() {
    for rotation_degrees in [0, 90, 180, 270] {
        assert_marks_land_on_tokens(PageSetup {
            rotation_degrees,
            is_cropped: true,
            ..PageSetup::default()
        });
    }
}

#[test]
fn marks_land_on_tokens_of_a_page_with_a_user_unit() {
    assert_marks_land_on_tokens(PageSetup {
        user_unit: Some(2.0),
        ..PageSetup::default()
    });
}

#[test]
fn page_space_has_its_origin_at_the_top_left_of_the_crop_box() {
    let flow = flow_of(single_page_pdf(PageSetup {
        is_cropped: true,
        ..PageSetup::default()
    }));
    assert_eq!(
        flow.page_sizes,
        [Size {
            width: 240.0,
            height: 350.0
        }]
    );

    let (_, x, y) = WORDS[0];
    let alpha = token(&flow, "Alpha").fragments[0].bounds;
    let baseline = CROP_BOX.y2 - y;
    assert!(
        (alpha.left - (x - CROP_BOX.x1)).abs() <= TOLERANCE,
        "{alpha:?}"
    );
    assert!(alpha.top < baseline && baseline < alpha.bottom, "{alpha:?}");
    assert!(alpha.height() < 1.5 * FONT_SIZE, "{alpha:?}");
}

#[test]
fn quarter_turn_shows_the_page_on_its_side() {
    let flow = flow_of(single_page_pdf(PageSetup {
        rotation_degrees: 90,
        ..PageSetup::default()
    }));
    assert_eq!(
        flow.page_sizes,
        [Size {
            width: 400.0,
            height: 300.0
        }]
    );

    // The line of text now runs down the page, from the point that the
    // rotation moves its origin to.
    let (_, x, y) = WORDS[0];
    let alpha = token(&flow, "Alpha").fragments[0].bounds;
    assert!((alpha.top - x).abs() <= TOLERANCE, "{alpha:?}");
    assert!(alpha.left < y && y < alpha.right, "{alpha:?}");
    assert!(alpha.height() > alpha.width(), "{alpha:?}");
}

#[test]
fn text_of_embedded_pages_stays_text() {
    let pdf = corpus("reading-tagged.pdf");
    let page_count = PdfiumReader::open(pdf.clone())
        .expect("the PDF opens")
        .page_count();
    let redline = Redline {
        sheets: (0..page_count)
            .map(|page_index| Sheet {
                pages: BySide {
                    old: Some(unmarked_page(page_index)),
                    new: None,
                },
                ..Sheet::default()
            })
            .collect(),
        ..Redline::default()
    };
    let output = redline::write(&redline, both(pdf.clone())).expect("the redline is written");

    let raw_texts = |pdf| -> Vec<String> {
        let tokens = raw_flow_of(pdf).tokens;
        tokens.into_iter().map(|token| token.text).collect()
    };
    assert_eq!(raw_texts(Arc::new(output)), raw_texts(pdf));
}

#[test]
fn label_is_written_above_its_page() {
    let source = single_page_pdf(PageSetup::default());
    let mut redline = first_new_page();
    redline.sheets[0].pages.new.as_mut().unwrap().label = "Label".to_owned();
    let output = redline::write(&redline, both(source)).expect("the redline is written");

    let flow = flow_of(Arc::new(output));
    let frame = layout_of_new_page(Size {
        width: 300.0,
        height: 400.0,
    })
    .frames
    .new;
    let label = token(&flow, "Label").fragments[0].bounds;
    assert!((label.left - frame.left).abs() < 1.0, "{label:?}");
    assert!(label.bottom < frame.top, "{label:?}");
}

#[test]
fn same_input_gives_the_same_bytes() {
    let pdfs = BySide::from_fn(|side| match side {
        Side::Old => corpus("reading-tagged.pdf"),
        Side::New => corpus("reading-untagged.pdf"),
    });
    let mark = Mark {
        bounds: Rect {
            left: 40.0,
            top: 50.0,
            right: 90.0,
            bottom: 62.0,
        },
        color: Rgb::REMOVED,
        style: MarkStyle::Highlight,
    };
    let page = |page_index| SheetPage {
        page_index,
        label: format!("page {}", page_index + 1),
        marks: vec![
            mark,
            Mark {
                style: MarkStyle::Outline,
                ..mark
            },
        ],
    };
    let redline = Redline {
        sheets: (0..4)
            .map(|page_index| Sheet {
                pages: BySide {
                    old: Some(page(page_index)),
                    new: Some(page(3 - page_index)),
                },
                ..Sheet::default()
            })
            .collect(),
        ..Redline::default()
    };
    let write = || redline::write(&redline, pdfs.clone()).expect("the redline is written");
    assert_eq!(write(), write());
}

#[test]
fn redline_is_written_in_the_pdf_version_that_its_sources_need() {
    let version_of = |pdf_version| {
        let source = single_page_pdf(PageSetup {
            pdf_version,
            ..PageSetup::default()
        });
        let output =
            redline::write(&first_new_page(), both(source)).expect("the redline is written");
        String::from_utf8_lossy(&output[..8]).into_owned()
    };
    assert_eq!(version_of(None), "%PDF-1.7");
    assert_eq!(version_of(Some((1, 4))), "%PDF-1.7");
    assert_eq!(version_of(Some((2, 0))), "%PDF-2.0");
}

#[test]
fn page_that_a_source_does_not_have_is_an_error() {
    let source = single_page_pdf(PageSetup::default());
    let redline = Redline {
        sheets: vec![Sheet {
            pages: BySide {
                old: Some(unmarked_page(1)),
                new: None,
            },
            ..Sheet::default()
        }],
        ..Redline::default()
    };
    let result = redline::write(&redline, both(source));
    assert!(matches!(
        result,
        Err(WriteError::NoSuchPage {
            side: Side::Old,
            page_number: 2
        })
    ));
}

#[test]
fn source_that_is_not_a_pdf_is_an_error() {
    let pdfs = BySide {
        old: single_page_pdf(PageSetup::default()),
        new: Arc::new(b"not a PDF".to_vec()),
    };
    let result = redline::write(&Redline::default(), pdfs);
    assert!(matches!(
        result,
        Err(WriteError::UnreadableSource(Side::New))
    ));
}
