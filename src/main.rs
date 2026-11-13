use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use abpdf::change::Comparison;
use abpdf::flow::Flow;
use abpdf::profile::Profile;
use abpdf::reader::ReadError;
use abpdf::reader::pdfium::PdfiumReader;
use abpdf::redline::{self, Redline};
use abpdf::{json, listing};
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
    /// Write the changes as JSON to this file
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Read the settings of the comparison from this file
    #[arg(long, value_name = "FILE")]
    profile: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run(&Arguments::parse()) {
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
    let profile = match &arguments.profile {
        Some(path) => Profile::load(path).map_err(|error| in_file(path, error))?,
        None => Profile::default(),
    };
    let (old_pdf, old_flow) = read(&arguments.old)?;
    let (new_pdf, new_flow) = read(&arguments.new)?;
    let comparison = Comparison::of_flows(old_flow, new_flow, &profile.filters);

    if let Some(path) = &arguments.output {
        let redline = Redline::of(
            &comparison,
            &file_name(&arguments.old),
            &file_name(&arguments.new),
        );
        let pdf = redline::write(&redline, old_pdf, new_pdf).map_err(|error| error.to_string())?;
        fs::write(path, pdf).map_err(|error| in_file(path, error))?;
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

/// The bytes of a PDF and the flow of its text.
fn read(path: &Path) -> Result<(Arc<Vec<u8>>, Flow), String> {
    let pdf = Arc::new(fs::read(path).map_err(|error| in_file(path, error))?);
    let reader = PdfiumReader::open(pdf.clone()).map_err(|error| match error {
        // Not a fault of this file.
        ReadError::EngineUnavailable(_) => error.to_string(),
        _ => in_file(path, error),
    })?;
    let flow = Flow::read(&reader).map_err(|error| in_file(path, error))?;
    Ok((pdf, flow))
}

/// What a document is called in the redline.
fn file_name(path: &Path) -> String {
    let name = path.file_name().unwrap_or(path.as_os_str());
    name.to_string_lossy().into_owned()
}

fn in_file(path: &Path, error: impl std::fmt::Display) -> String {
    format!("{}: {error}", path.display())
}
