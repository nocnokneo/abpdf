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

#[test]
fn pdfium_that_cannot_be_loaded_exits_with_2_without_blaming_a_document() {
    let pdf = corpus("revisions-old-tagged.pdf");
    let output = Command::new(env!("CARGO_BIN_EXE_abpdf"))
        .args([&pdf, &pdf])
        .env("ABPDF_PDFIUM_LIBRARY", corpus("no-such-library"))
        .output()
        .expect("abpdf runs");
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(
        message.starts_with("abpdf: cannot load the PDFium library: "),
        "{message}"
    );
    assert!(message.contains("no-such-library"), "{message}");
    assert!(!message.contains("revisions-old-tagged.pdf"), "{message}");
}
