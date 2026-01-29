//! Integration tests for the scan command.

use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::services::duplicate::{self, DuplicateType};

use crate::common::{fixture_path, temp_dir, text_fixtures_dir};

#[test]
fn test_scan_detects_exact_duplicates() {
    let report = duplicate::find_duplicates(
        &std::fs::read_dir(text_fixtures_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect::<Vec<_>>(),
    )
    .unwrap();

    // Should find one group of exact duplicates (duplicate_a.txt and duplicate_b.txt)
    assert!(
        !report.groups.is_empty(),
        "Should find at least one duplicate group"
    );

    let exact_groups: Vec<_> = report
        .groups
        .iter()
        .filter(|g| g.duplicate_type == DuplicateType::Exact)
        .collect();

    assert_eq!(
        exact_groups.len(),
        1,
        "Should find exactly one exact duplicate group"
    );
    assert_eq!(
        exact_groups[0].files.len(),
        2,
        "Duplicate group should have 2 files"
    );
}

#[test]
fn test_scan_does_not_report_unique_files_as_duplicates() {
    let unique_file = fixture_path("text/unique.txt");
    let files = vec![unique_file];

    let report = duplicate::find_duplicates(&files).unwrap();

    assert!(
        report.groups.is_empty(),
        "Unique file should not be in any duplicate group"
    );
    assert_eq!(report.errors, 0, "Should have no errors");
}

#[test]
fn test_scan_handles_empty_input() {
    let files: Vec<std::path::PathBuf> = vec![];

    let report = duplicate::find_duplicates(&files).unwrap();

    assert!(
        report.groups.is_empty(),
        "Empty input should produce no groups"
    );
    assert_eq!(report.total_files, 0, "Total files should be 0");
    assert_eq!(report.errors, 0, "Should have no errors");
}

#[test]
fn test_scan_reports_correct_counts() {
    let files: Vec<_> = std::fs::read_dir(text_fixtures_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();

    let report = duplicate::find_duplicates(&files).unwrap();

    assert_eq!(report.total_files, 3, "Should have scanned 3 files");
    assert_eq!(
        report.duplicate_count(),
        1,
        "Should have 1 duplicate (excluding original)"
    );
    assert_eq!(
        report.exact_duplicate_count(),
        1,
        "Should have 1 exact duplicate"
    );
    assert_eq!(
        report.perceptual_duplicate_count(),
        0,
        "Should have 0 perceptual duplicates"
    );
}

#[test]
fn test_scanner_executes_without_error() {
    let tmp = temp_dir();
    let output = tmp.path().join("duplicates.json");
    let scanner = Scanner::new(vec![text_fixtures_dir()], true, false, Some(output.clone()));
    let result = scanner.execute();

    assert!(result.is_ok(), "Scanner should execute without error");
    assert!(output.exists(), "Duplicates file should be created");
}
