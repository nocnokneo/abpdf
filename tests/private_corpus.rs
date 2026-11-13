//! Runs the comparison on documents that are not checked in.
//!
//!     ABPDF_PRIVATE_CORPUS=/path/to/corpus cargo test --test private_corpus -- --nocapture
//!
//! Each directory in the corpus holds `old.pdf` and `new.pdf`, and may hold
//! the `profile.toml` to compare them with.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs};

use abpdf::change::Comparison;
use abpdf::profile::Profile;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{self, Redline};

const CORPUS_VARIABLE: &str = "ABPDF_PRIVATE_CORPUS";

#[test]
fn private_corpus_compares_the_same_every_time() {
    let Some(corpus) = env::var_os(CORPUS_VARIABLE) else {
        return;
    };
    let mut directories: Vec<_> = fs::read_dir(&corpus)
        .unwrap_or_else(|error| panic!("{}: {error}", Path::new(&corpus).display()))
        .map(|entry| entry.expect("the corpus lists").path())
        .filter(|path| path.join("old.pdf").is_file() && path.join("new.pdf").is_file())
        .collect();
    directories.sort();
    assert!(
        !directories.is_empty(),
        "the corpus has no pair of old.pdf and new.pdf"
    );

    for directory in directories {
        let started = Instant::now();
        let (change_count, redline) = compare(&directory);
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "{}: {change_count} changes, {} bytes of redline, {seconds:.2} s",
            directory.display(),
            redline.len()
        );
        assert!(compare(&directory).1 == redline, "{}", directory.display());
    }
}

/// The number of changes between the documents in `directory`, and their
/// redline.
fn compare(directory: &Path) -> (usize, Vec<u8>) {
    let read =
        |file_name: &str| Arc::new(fs::read(directory.join(file_name)).expect("the PDF reads"));
    let (old_pdf, new_pdf) = (read("old.pdf"), read("new.pdf"));
    let profile_path = directory.join("profile.toml");
    let profile = match profile_path.is_file() {
        true => Profile::load(&profile_path).expect("the profile loads"),
        false => Profile::default(),
    };

    let old = PdfiumReader::open(old_pdf.clone()).expect("old.pdf opens");
    let new = PdfiumReader::open(new_pdf.clone()).expect("new.pdf opens");
    let comparison = Comparison::of(&old, &new, &profile.filters).expect("the PDFs read");
    let redline = Redline::of(&comparison, "old.pdf", "new.pdf");
    let output = redline::write(&redline, old_pdf, new_pdf).expect("the redline is written");
    (comparison.changes.len(), output)
}
