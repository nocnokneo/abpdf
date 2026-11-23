//! The command line: what it prints, writes and exits with.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use serde_json::Value;

use common::{REVISIONS_CHANGE_COUNT, corpus_path as corpus, page_texts};

/// A path for a file that a test writes.
fn scratch(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(file_name)
}

fn abpdf(arguments: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_abpdf"))
        .args(arguments)
        .output()
        .expect("abpdf runs")
}

/// Compares the revisions of the corpus with their profile.
fn compare_revisions(more_arguments: &[&Path]) -> Output {
    let mut arguments = vec![
        corpus("revisions-old-tagged.pdf"),
        corpus("revisions-new-tagged.pdf"),
        "--profile".into(),
        corpus("revisions.toml"),
    ];
    arguments.extend(more_arguments.iter().map(|argument| argument.to_path_buf()));
    let arguments: Vec<&Path> = arguments.iter().map(PathBuf::as_path).collect();
    abpdf(&arguments)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn equal_documents_exit_with_0() {
    let pdf = corpus("revisions-old-tagged.pdf");
    let output = abpdf(&[&pdf, &pdf]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "No differences\n");
}

#[test]
fn different_documents_exit_with_1_and_list_their_changes() {
    let output = compare_revisions(&[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let listing = stdout(&output);
    let change =
        "@ old p. 1 / new p. 1\n- shall be calibrated annually\n+ must be verified every month\n";
    assert!(listing.contains(change), "{listing}");
    let count = format!("\n{REVISIONS_CHANGE_COUNT} changes\n");
    assert!(listing.ends_with(&count), "{listing}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn redline_and_json_are_written_where_asked() {
    let (redline, json) = (scratch("cli-redline.pdf"), scratch("cli-changes.json"));
    let output = compare_revisions(&["--output".as_ref(), &redline, "--json".as_ref(), &json]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));

    assert!(fs::read(&redline).unwrap().starts_with(b"%PDF-"));
    let report: Value = serde_json::from_slice(&fs::read(&json).unwrap()).unwrap();
    let changes = report["changes"].as_array().unwrap();
    assert_eq!(changes.len(), REVISIONS_CHANGE_COUNT);
    assert_eq!(changes[2]["kind"], "replaced");
    assert_eq!(changes[2]["new"]["text"], "must be verified every month");
    assert_eq!(changes[2]["new"]["areas"][0]["page"], 1);
}

#[test]
fn redline_is_written_for_equal_documents_too() {
    let (pdf, redline) = (corpus("revisions-old-tagged.pdf"), scratch("cli-equal.pdf"));
    let output = abpdf(&[&pdf, &pdf, "-o".as_ref(), &redline]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(fs::read(&redline).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn redline_of_equal_documents_can_be_left_out() {
    let (pdf, redline) = (
        corpus("revisions-old-tagged.pdf"),
        scratch("cli-left-out.pdf"),
    );
    fs::write(&redline, "the redline of an earlier comparison").unwrap();
    let output = abpdf(&[
        &pdf,
        &pdf,
        "-o".as_ref(),
        &redline,
        "--only-if-different".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!redline.exists());

    // And it does not matter that there was no file to remove.
    let output = abpdf(&[
        &pdf,
        &pdf,
        "-o".as_ref(),
        &redline,
        "--only-if-different".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn redline_of_different_documents_is_written_either_way() {
    let redline = scratch("cli-different.pdf");
    let output = compare_revisions(&["-o".as_ref(), &redline, "--only-if-different".as_ref()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(fs::read(&redline).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn leaving_the_redline_out_needs_a_redline_to_leave_out() {
    let pdf = corpus("revisions-old-tagged.pdf");
    let output = abpdf(&[&pdf, &pdf, "--only-if-different".as_ref()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--output"), "{}", stderr(&output));
}

#[test]
fn audit_shows_every_page_and_says_what_its_outlines_are() {
    let (redline, audit) = (scratch("cli-no-audit.pdf"), scratch("cli-audit.pdf"));
    let pdf = corpus("revisions-old-tagged.pdf");
    abpdf(&[&pdf, &pdf, "--output".as_ref(), &redline]);
    let output = abpdf(&[&pdf, &pdf, "--output".as_ref(), &audit, "--audit".as_ref()]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let pages_of = |path: &Path| page_texts(Arc::new(fs::read(path).unwrap()));
    let (redline_pages, audit_pages) = (pages_of(&redline), pages_of(&audit));
    // Without changes a redline is its summary, and the audit shows the
    // three pages of the document after it.
    assert_eq!(redline_pages.len(), 1);
    assert_eq!(audit_pages.len(), 1 + 3);
    assert!(!redline_pages[0].contains("left out of the comparison"));
    assert!(
        audit_pages[0].contains("left out of the comparison"),
        "{}",
        audit_pages[0]
    );
}

#[test]
fn audit_needs_a_redline_to_show_it_in() {
    let output = compare_revisions(&["--audit".as_ref()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--output"), "{}", stderr(&output));
}

#[test]
fn file_that_is_missing_exits_with_2() {
    let (pdf, missing) = (corpus("revisions-old-tagged.pdf"), corpus("no-such.pdf"));
    let output = abpdf(&[&pdf, &missing]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).starts_with("abpdf: "),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("no-such.pdf"),
        "{}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn file_that_is_not_a_pdf_exits_with_2() {
    let (pdf, source) = (corpus("revisions-old-tagged.pdf"), corpus("revisions.typ"));
    let output = abpdf(&[&source, &pdf]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("revisions.typ: the file is not a PDF"),
        "{}",
        stderr(&output)
    );
}

/// A copy of a document of the corpus that a test may lose.
fn scratch_copy(file_name: &str) -> PathBuf {
    let copy = scratch(file_name);
    fs::copy(corpus("revisions-new-tagged.pdf"), &copy).unwrap();
    copy
}

#[test]
fn file_to_write_that_is_a_document_is_refused_and_the_document_kept() {
    let old = corpus("revisions-old-tagged.pdf");
    let document = scratch_copy("cli-document-to-write.pdf");
    let content = fs::read(&document).unwrap();
    for option in ["--output", "--json"] {
        let output = abpdf(&[&old, &document, option.as_ref(), &document]);
        assert_eq!(output.status.code(), Some(2), "{option}");
        assert!(
            stderr(&output).contains("cli-document-to-write.pdf: is a document to compare"),
            "{}",
            stderr(&output)
        );
        assert_eq!(fs::read(&document).unwrap(), content, "{option}");
    }
}

#[test]
fn document_is_not_removed_as_a_redline_to_leave_out() {
    let document = scratch_copy("cli-document-to-leave-out.pdf");
    let output = abpdf(&[
        &document,
        &document,
        "--output".as_ref(),
        &document,
        "--only-if-different".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(document.exists());
}

#[test]
fn document_is_known_by_another_path_to_it() {
    let document = scratch_copy("cli-document-by-another-path.pdf");
    let directory = document.parent().unwrap();
    let another_path = directory
        .join("..")
        .join(directory.file_name().unwrap())
        .join(document.file_name().unwrap());
    let output = abpdf(&[&document, &document, "--output".as_ref(), &another_path]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn redline_and_json_cannot_be_one_file() {
    let file = scratch("cli-redline-and-json");
    let _ = fs::remove_file(&file);
    let output = compare_revisions(&["--output".as_ref(), &file, "--json".as_ref(), &file]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot hold both"),
        "{}",
        stderr(&output)
    );
    assert!(!file.exists());
}

/// A document whose page has, among its resources, a name with an escape
/// that is none.
/// Writing a redline with this page has made a library panic.
const DOCUMENT_WITH_A_BROKEN_NAME: &str = "\
%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R  >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> /ExtGState << /G << /#nX 1 >> >> >>  >>
endobj
4 0 obj
<<  /Length 36 >>
stream
BT /F1 12 Tf 20 250 Td (alpha) Tj ET
endstream
endobj
xref
0 5
0000000000 65535 f\x20
0000000009 00000 n\x20
0000000059 00000 n\x20
0000000116 00000 n\x20
0000000325 00000 n\x20
trailer
<< /Size 5 /Root 1 0 R  >>
startxref
412
%%EOF
";

#[test]
fn document_that_a_library_panics_on_exits_like_any_other() {
    let document = scratch("cli-broken-name.pdf");
    fs::write(&document, DOCUMENT_WITH_A_BROKEN_NAME).unwrap();
    let redline = scratch("cli-broken-name-redline.pdf");
    let output = abpdf(&[
        &document,
        &document,
        "--output".as_ref(),
        &redline,
        "--audit".as_ref(),
    ]);
    assert!(
        matches!(output.status.code(), Some(0 | 2)),
        "{:?}: {}",
        output.status,
        stderr(&output)
    );
}

#[test]
fn profile_with_a_mistake_exits_with_2() {
    let profile = scratch("cli-mistaken.toml");
    fs::write(&profile, "ignore_patterns = ['(unclosed']\n").unwrap();
    let pdf = corpus("revisions-old-tagged.pdf");
    let output = abpdf(&[&pdf, &pdf, "--profile".as_ref(), &profile]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("ignore pattern 1"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn profile_that_is_missing_exits_with_2_and_is_named_once() {
    let (pdf, profile) = (corpus("revisions-old-tagged.pdf"), corpus("no-such.toml"));
    let output = abpdf(&[&pdf, &pdf, "--profile".as_ref(), &profile]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output).matches("no-such.toml").count(),
        1,
        "{}",
        stderr(&output)
    );
}

#[test]
fn missing_arguments_exit_with_2() {
    let output = abpdf(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("Usage: abpdf"),
        "{}",
        stderr(&output)
    );
}
