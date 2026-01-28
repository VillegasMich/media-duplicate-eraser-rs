//! Shared test utilities for integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Returns the path to the test fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Returns the path to the text fixtures directory.
pub fn text_fixtures_dir() -> PathBuf {
    fixtures_dir().join("text")
}

/// Returns the path to the images fixtures directory.
pub fn images_fixtures_dir() -> PathBuf {
    fixtures_dir().join("images")
}

/// Returns the path to a specific fixture file.
pub fn fixture_path(relative_path: &str) -> PathBuf {
    fixtures_dir().join(relative_path)
}

/// Creates a temporary directory for test output.
/// Returns a TempDir that will be cleaned up when dropped.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp directory")
}

/// Asserts that a path exists.
pub fn assert_path_exists(path: &Path) {
    assert!(path.exists(), "Path does not exist: {:?}", path);
}

/// Asserts that a path does not exist.
pub fn assert_path_not_exists(path: &Path) {
    assert!(!path.exists(), "Path should not exist: {:?}", path);
}
