//! Integration tests for the clean command.

use std::fs;

use media_duplicate_eraser_rs::commands::clean::Cleaner;
use media_duplicate_eraser_rs::commands::Command;

use crate::common::{assert_path_exists, assert_path_not_exists, temp_dir};

/// Helper to create a minimal duplicates.json file for testing.
fn create_duplicates_json(dir: &std::path::Path) -> std::path::PathBuf {
    let duplicates_path = dir.join("duplicates.json");
    let content = r#"{
        "version": "1.0",
        "scanned_at": "2024-01-01T00:00:00Z",
        "total_files_scanned": 3,
        "duplicate_groups": 1,
        "total_duplicates": 1,
        "entries": []
    }"#;
    fs::write(&duplicates_path, content).expect("Failed to write duplicates.json");
    duplicates_path
}

#[test]
fn test_clean_removes_duplicates_json() {
    // Setup: Create a temp directory with a duplicates.json file
    let tmp = temp_dir();
    let duplicates_path = create_duplicates_json(tmp.path());

    // Verify setup
    assert_path_exists(&duplicates_path);

    // Execute: Run the cleaner
    let cleaner = Cleaner::new(tmp.path().to_path_buf(), true);
    let result = cleaner.execute();

    // Verify: Command succeeded and file was removed
    assert!(result.is_ok(), "Cleaner should execute without error");
    assert_path_not_exists(&duplicates_path);
}

#[test]
fn test_clean_handles_missing_duplicates_json() {
    // Setup: Create an empty temp directory (no duplicates.json)
    let tmp = temp_dir();
    let duplicates_path = tmp.path().join("duplicates.json");

    // Verify setup - file should not exist
    assert_path_not_exists(&duplicates_path);

    // Execute: Run the cleaner on directory without duplicates.json
    let cleaner = Cleaner::new(tmp.path().to_path_buf(), true);
    let result = cleaner.execute();

    // Verify: Command should succeed gracefully (no error)
    assert!(
        result.is_ok(),
        "Cleaner should handle missing file gracefully"
    );
}

#[test]
fn test_clean_does_not_affect_other_files() {
    // Setup: Create a temp directory with duplicates.json and other files
    let tmp = temp_dir();
    let duplicates_path = create_duplicates_json(tmp.path());

    // Create some other files that should not be affected
    let other_file = tmp.path().join("other_file.txt");
    let nested_dir = tmp.path().join("subdir");
    fs::write(&other_file, "test content").expect("Failed to write other file");
    fs::create_dir(&nested_dir).expect("Failed to create nested dir");
    let nested_file = nested_dir.join("nested.txt");
    fs::write(&nested_file, "nested content").expect("Failed to write nested file");

    // Verify setup
    assert_path_exists(&duplicates_path);
    assert_path_exists(&other_file);
    assert_path_exists(&nested_file);

    // Execute: Run the cleaner
    let cleaner = Cleaner::new(tmp.path().to_path_buf(), true);
    let result = cleaner.execute();

    // Verify: Only duplicates.json was removed, other files remain
    assert!(result.is_ok(), "Cleaner should execute without error");
    assert_path_not_exists(&duplicates_path);
    assert_path_exists(&other_file);
    assert_path_exists(&nested_file);
}

#[test]
fn test_clean_is_idempotent() {
    // Setup: Create a temp directory with a duplicates.json file
    let tmp = temp_dir();
    let duplicates_path = create_duplicates_json(tmp.path());

    // Execute: Run the cleaner twice
    let cleaner = Cleaner::new(tmp.path().to_path_buf(), true);

    let result1 = cleaner.execute();
    assert!(result1.is_ok(), "First clean should succeed");
    assert_path_not_exists(&duplicates_path);

    let result2 = cleaner.execute();
    assert!(result2.is_ok(), "Second clean should also succeed");
    assert_path_not_exists(&duplicates_path);
}
