//! The notices that every release carries for what abpdf embeds of others.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The directory of the source of a package that abpdf is built with.
fn source_of(package_name: &str) -> PathBuf {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(output.status.success(), "{output:?}");
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo writes JSON");
    let packages = metadata["packages"].as_array().expect("there are packages");
    let package = packages
        .iter()
        .find(|package| package["name"] == package_name)
        .unwrap_or_else(|| panic!("abpdf is not built with {package_name} any more"));
    let manifest = package["manifest_path"]
        .as_str()
        .expect("a package has a manifest");
    Path::new(manifest).parent().unwrap().to_path_buf()
}

/// A copy can go stale when a crate is updated, and this makes it fail.
#[test]
fn notices_hold_the_licenses_of_the_data_that_crates_embed_as_the_crates_have_them() {
    let notices_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("NOTICES");
    let notices = fs::read_to_string(notices_path).unwrap();
    let licenses = [
        // The standard fonts that `StandardFont::get_font_data` returns.
        source_of("hayro-font").join("assets/LICENSE_FOXIT"),
        // The CMaps that `load_embedded` returns.
        source_of("hayro-cmap").join("assets/LICENSE.txt"),
    ];
    for license in licenses {
        let text = fs::read_to_string(&license)
            .unwrap_or_else(|error| panic!("{}: {error}", license.display()));
        assert!(
            notices.contains(&text),
            "NOTICES does not have {} as it is now",
            license.display()
        );
    }
}
