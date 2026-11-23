//! The settings of a comparison, as a TOML file that can be kept next to the
//! documents it is for.
//! The README describes the format.

use std::path::Path;
use std::{fs, io};

use regex::Regex;
use serde::Deserialize;

use crate::filter::Filters;

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub filters: Filters,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error(transparent)]
    Unreadable(#[from] io::Error),
    #[error("{0}")]
    Syntax(#[from] toml::de::Error),
    #[error("{name} is {value}, but a height is a number from 0 up")]
    ImpossibleHeight { name: &'static str, value: f32 },
    #[error("ignore pattern {number} is not a regular expression: {source}")]
    Pattern { number: usize, source: regex::Error },
}

/// The profile file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    #[serde(default)]
    header_height_points: f32,
    #[serde(default)]
    footer_height_points: f32,
    #[serde(default)]
    ignore_patterns: Vec<String>,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        Self::parse(&fs::read_to_string(path)?)
    }

    pub fn parse(text: &str) -> Result<Self, ProfileError> {
        let file: ProfileFile = toml::from_str(text)?;
        let heights = [
            ("header_height_points", file.header_height_points),
            ("footer_height_points", file.footer_height_points),
        ];
        let is_impossible = |value: f32| !value.is_finite() || value < 0.0;
        if let Some((name, value)) = heights.into_iter().find(|(_, value)| is_impossible(*value)) {
            return Err(ProfileError::ImpossibleHeight { name, value });
        }
        let ignore_patterns = file
            .ignore_patterns
            .iter()
            .enumerate()
            .map(|(index, pattern)| {
                Regex::new(pattern).map_err(|source| ProfileError::Pattern {
                    number: index + 1,
                    source,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            filters: Filters {
                header_height: file.header_height_points,
                footer_height: file.footer_height_points,
                ignore_patterns,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_profile_filters_nothing() {
        let filters = Profile::parse("").unwrap().filters;
        assert_eq!((filters.header_height, filters.footer_height), (0.0, 0.0));
        assert!(filters.ignore_patterns.is_empty());
    }

    #[test]
    fn settings_are_read() {
        let profile = Profile::parse(
            r"
            header_height_points = 95
            footer_height_points = 50.5
            ignore_patterns = ['\(p\. (\d+)\)', 'Figure (\d+)']
            ",
        )
        .unwrap();
        let filters = profile.filters;
        assert_eq!((filters.header_height, filters.footer_height), (95.0, 50.5));
        let patterns: Vec<&str> = filters.ignore_patterns.iter().map(Regex::as_str).collect();
        assert_eq!(patterns, [r"\(p\. (\d+)\)", r"Figure (\d+)"]);
    }

    #[test]
    fn unknown_setting_is_an_error() {
        let error = Profile::parse("header_height = 95").unwrap_err();
        assert!(matches!(error, ProfileError::Syntax(_)), "{error}");
        assert!(error.to_string().contains("header_height"), "{error}");
    }

    #[test]
    fn height_that_is_negative_or_no_number_is_an_error() {
        for value in ["-1", "nan", "inf"] {
            let error = Profile::parse(&format!("footer_height_points = {value}")).unwrap_err();
            assert!(
                matches!(
                    error,
                    ProfileError::ImpossibleHeight {
                        name: "footer_height_points",
                        ..
                    }
                ),
                "{value}: {error}"
            );
        }
    }

    #[test]
    fn pattern_that_does_not_compile_is_named_by_its_position() {
        let error = Profile::parse("ignore_patterns = ['fine', '(unclosed']").unwrap_err();
        assert!(
            matches!(error, ProfileError::Pattern { number: 2, .. }),
            "{error}"
        );
    }

    #[test]
    fn file_that_is_missing_is_an_error() {
        let error = Profile::load(Path::new("no/such/profile.toml")).unwrap_err();
        assert!(matches!(error, ProfileError::Unreadable(_)), "{error}");
    }
}
