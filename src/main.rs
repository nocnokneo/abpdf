use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use abpdf::change::Comparison;
use abpdf::flow::RawFlow;
use abpdf::profile::Profile;
use abpdf::reader::Document;
use abpdf::redline::{self, Redline};
use abpdf::{BySide, json, listing};
use clap::Parser;

/// Compare two revisions of a PDF.
///
/// The changes are listed on standard output.
/// The exit status is 0 if there are none, 1 if there are some and 2 if
/// the comparison failed.
#[derive(Parser)]
#[command(version)]
struct Arguments {
    /// The old revision
    old: PathBuf,
    /// The new revision
    new: PathBuf,
    /// Write the redline, a PDF that shows the changed pages side by side,
    /// to this file
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Write the redline only if there are changes, and remove the file if
    /// there are none
    #[arg(long, requires = "output")]
    only_if_different: bool,
    /// Show in the redline what was left out of the comparison: outline it,
    /// and add a sheet for every page without a change
    #[arg(long, requires = "output")]
    audit: bool,
    /// Write the changes as JSON to this file
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Read the settings of the comparison from this file
    #[arg(long, value_name = "FILE")]
    profile: Option<PathBuf>,
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    // A panic is a failed comparison as well, so that the exit status is
    // always one of the three.
    let outcome = panic::catch_unwind(|| run(&arguments)).unwrap_or_else(|_| {
        Err("the comparison broke off with the error above, which abpdf does not expect".into())
    });
    match outcome {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(message) => {
            eprintln!("abpdf: {message}");
            ExitCode::from(2)
        }
    }
}

/// Compares the documents and writes what was asked for.
/// Returns whether the documents differ.
fn run(arguments: &Arguments) -> Result<bool, String> {
    check_files_to_write(arguments)?;
    let profile = match &arguments.profile {
        Some(path) => Profile::load(path).map_err(|error| in_file(path, error))?,
        None => Profile::default(),
    };
    let (old_pdf, old_flow) = read(&arguments.old)?;
    let (new_pdf, new_flow) = read(&arguments.new)?;
    let pdfs = BySide {
        old: old_pdf,
        new: new_pdf,
    };
    let flows = BySide {
        old: old_flow,
        new: new_flow,
    };
    let comparison = Comparison::of(flows, &profile.filters);

    if let Some(path) = &arguments.output {
        if arguments.only_if_different && comparison.changes.is_empty() {
            remove_if_present(path)?;
        } else {
            let names = BySide {
                old: file_name(&arguments.old),
                new: file_name(&arguments.new),
            };
            let names = names.as_ref().map(String::as_str);
            let redline = match arguments.audit {
                true => Redline::audit_of(&comparison, names),
                false => Redline::of(&comparison, names),
            };
            let pdf = redline::write(&redline, pdfs).map_err(|error| error.to_string())?;
            fs::write(path, pdf).map_err(|error| in_file(path, error))?;
        }
    }
    if let Some(path) = &arguments.json {
        let file = File::create(path).map_err(|error| in_file(path, error))?;
        json::write(&comparison, BufWriter::new(file)).map_err(|error| in_file(path, error))?;
    }
    match listing::write(&comparison, io::stdout().lock()) {
        // Whoever reads the listing has seen enough of it.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {}
        result => result.map_err(|error| error.to_string())?,
    }
    Ok(!comparison.changes.is_empty())
}

/// Fails if a file to write is one that the comparison would destroy by
/// writing it: a document to compare, or the other file to write.
fn check_files_to_write(arguments: &Arguments) -> Result<(), String> {
    let documents = [&arguments.old, &arguments.new];
    let files_to_write = [&arguments.output, &arguments.json];
    let files_to_write: Vec<&PathBuf> = files_to_write.into_iter().flatten().collect();
    for file in &files_to_write {
        let is_document = |document: &&PathBuf| lead_to_one_file(document, file);
        if documents.iter().any(is_document) {
            return Err(in_file(
                file,
                "is a document to compare, and would be replaced",
            ));
        }
    }
    match files_to_write[..] {
        [redline, json] if lead_to_one_file(redline, json) => {
            Err(in_file(json, "cannot hold both the redline and the JSON"))
        }
        _ => Ok(()),
    }
}

/// Whether two paths lead to one file, be it there yet or not.
fn lead_to_one_file(first: &Path, second: &Path) -> bool {
    match (destination(first), destination(second)) {
        (Some(first), Some(second)) => first == second,
        // A path that leads nowhere fails where it is read or written.
        _ => false,
    }
}

/// Where a path leads once its links and its steps up are followed.
/// `None` if not even its directory is there.
fn destination(path: &Path) -> Option<PathBuf> {
    if let Ok(file) = fs::canonicalize(path) {
        return Some(file);
    }
    let directory = match path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    };
    Some(fs::canonicalize(directory).ok()?.join(path.file_name()?))
}

/// The bytes of a PDF and the flow of its text.
fn read(path: &Path) -> Result<(Arc<Vec<u8>>, RawFlow), String> {
    let pdf = Arc::new(fs::read(path).map_err(|error| in_file(path, error))?);
    let document = Document::open(pdf.clone()).map_err(|error| in_file(path, error))?;
    let flow = RawFlow::read(&document).map_err(|error| in_file(path, error))?;
    Ok((pdf, flow))
}

/// Removes a redline that an earlier comparison left, so that the file is
/// only there if this comparison wrote it.
fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(in_file(path, error)),
        _ => Ok(()),
    }
}

/// What a document is called in the redline.
fn file_name(path: &Path) -> String {
    let name = path.file_name().unwrap_or(path.as_os_str());
    name.to_string_lossy().into_owned()
}

fn in_file(path: &Path, error: impl std::fmt::Display) -> String {
    format!("{}: {error}", path.display())
}
