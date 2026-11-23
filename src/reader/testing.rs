//! PDFs for the tests of the reader, written out object by object.

use std::sync::Arc;

use hayro_syntax::Pdf;

use super::content::{self, Object, Shared};
use super::page_space::UserSpaceBox;

/// The resources of [`page_pdf`] that every test may use: Helvetica as
/// `F1`, which no PDF has to embed.
const FONT: &str = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >>";

/// The page of [`page_pdf`].
pub(super) const PAGE: UserSpaceBox = UserSpaceBox {
    left: 0.0,
    bottom: 0.0,
    right: 200.0,
    top: 300.0,
};

/// A PDF with the given objects, which are numbered from 1.
/// The first one is the catalog.
pub(super) fn pdf_of(objects: &[String]) -> Arc<Vec<u8>> {
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).bytes());
    }
    let cross_reference_offset = pdf.len();
    let entry_count = objects.len() + 1;
    pdf.extend(format!("xref\n0 {entry_count}\n0000000000 65535 f \n").bytes());
    for offset in offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {entry_count} /Root 1 0 R >>\n\
             startxref\n{cross_reference_offset}\n%%EOF\n"
        )
        .bytes(),
    );
    Arc::new(pdf)
}

/// A stream with the given entries of its dictionary and the given data.
pub(super) fn stream(entries: &str, data: &str) -> String {
    let length = data.len();
    format!("<< {entries} /Length {length} >>\nstream\n{data}\nendstream")
}

/// A PDF of one page that is 200 wide and 300 high.
///
/// The page has the given content, and `resources` next to the font `F1`.
/// `more_objects` are numbered from 5.
pub(super) fn page_pdf(content: &str, resources: &str, more_objects: &[String]) -> Arc<Vec<u8>> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R \
             /Resources << {FONT} {resources} >> >>"
        ),
        stream("", content),
    ];
    objects.extend_from_slice(more_objects);
    pdf_of(&objects)
}

/// What the page of [`page_pdf`] draws.
pub(super) fn objects_drawn_by(
    content: &str,
    resources: &str,
    more_objects: &[String],
) -> Vec<Object> {
    let pdf = Pdf::new(page_pdf(content, resources, more_objects)).expect("the PDF opens");
    let objects = content::objects_of(&pdf.pages()[0], &Shared::new(), PAGE);
    objects.expect("the page is within the limits")
}
