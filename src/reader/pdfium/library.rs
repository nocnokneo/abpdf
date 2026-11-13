//! The PDFium shared library.
//!
//! PDFium keeps global state and is not thread safe, so the library is loaded
//! once per process and every use holds the guard that [`lock`] returns.

use std::env;
use std::ops::Deref;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use pdfium_render::prelude::{Pdfium, PdfiumError, PdfiumLibraryBindings};

use crate::reader::ReadError;

/// Names the PDFium shared library to load, instead of searching for it.
pub const LIBRARY_PATH_VARIABLE: &str = "ABPDF_PDFIUM_LIBRARY";

pub(super) struct Library(Box<dyn PdfiumLibraryBindings>);

// SAFETY: the bindings are only reachable through the mutex in `LIBRARY`, so
// PDFium is never entered by two threads at once.
unsafe impl Send for Library {}

impl Deref for Library {
    type Target = dyn PdfiumLibraryBindings;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

static LIBRARY: OnceLock<Result<Mutex<Library>, String>> = OnceLock::new();

/// Exclusive use of PDFium, loading it on first use.
///
/// The guard must not be held across another call to `lock`.
pub(super) fn lock() -> Result<MutexGuard<'static, Library>, ReadError> {
    match LIBRARY.get_or_init(|| load().map(Mutex::new)) {
        Ok(library) => Ok(library.lock().unwrap_or_else(PoisonError::into_inner)),
        Err(reason) => Err(ReadError::EngineUnavailable(reason.clone())),
    }
}

fn load() -> Result<Library, String> {
    let bindings = bind()?;
    // SAFETY: called once, before any other PDFium function.
    unsafe { bindings.FPDF_InitLibrary() };
    Ok(Library(bindings))
}

/// Binds to the library that [`LIBRARY_PATH_VARIABLE`] names, else to the one
/// next to the executable, else to the one the system's loader finds.
fn bind() -> Result<Box<dyn PdfiumLibraryBindings>, String> {
    if let Some(path) = env::var_os(LIBRARY_PATH_VARIABLE) {
        return bind_to(Path::new(&path));
    }
    let beside_executable = env::current_exe().ok().and_then(|executable| {
        let directory = executable.parent()?;
        Some(Pdfium::pdfium_platform_library_name_at_path(directory))
    });
    if let Some(path) = beside_executable.filter(|path| path.exists()) {
        return bind_to(&path);
    }
    Pdfium::bind_to_system_library().map_err(|error| {
        format!(
            "{}; put the library next to the abpdf executable or set {LIBRARY_PATH_VARIABLE} to its path",
            describe(&error)
        )
    })
}

fn bind_to(path: &Path) -> Result<Box<dyn PdfiumLibraryBindings>, String> {
    Pdfium::bind_to_library(path)
        .map_err(|error| format!("{}: {}", path.display(), describe(&error)))
}

fn describe(error: &PdfiumError) -> String {
    match error {
        PdfiumError::LoadLibraryError(error) => error.to_string(),
        other => format!("{other:?}"),
    }
}
