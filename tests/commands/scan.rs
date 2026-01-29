//! Integration tests for the scan command.

use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::services::duplicate::{self, DuplicateType};

use crate::common::{fixture_path, images_fixtures_dir, temp_dir, text_fixtures_dir};

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
    let scanner = Scanner::new(text_fixtures_dir(), true, false, Some(output.clone()), true);
    let result = scanner.execute();

    assert!(result.is_ok(), "Scanner should execute without error");
    assert!(output.exists(), "Duplicates file should be created");
}

// ============================================================================
// Image-specific tests
// ============================================================================

/// Helper to get image files from the images fixtures directory
fn get_image_files() -> Vec<std::path::PathBuf> {
    std::fs::read_dir(images_fixtures_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "png"))
        .collect()
}

#[test]
fn test_scan_detects_image_duplicates() {
    // image_a.png and "image_a copy.png" are exact duplicates
    // The tool may merge them with perceptual duplicates (image_b.png is visually similar)
    let files = get_image_files();

    let report = duplicate::find_duplicates(&files).unwrap();

    // Should find at least one duplicate group
    assert!(
        !report.groups.is_empty(),
        "Should find duplicate images"
    );

    // Should detect duplicates involving image_a files
    let has_image_a_duplicates = report.groups.iter().any(|g| {
        g.files
            .iter()
            .any(|f| f.file_name().unwrap().to_str().unwrap().contains("image_a"))
    });

    assert!(
        has_image_a_duplicates,
        "Should find image_a.png in a duplicate group"
    );

    // Both image_a.png and "image_a copy.png" should be in the same group
    let image_a_group = report.groups.iter().find(|g| {
        g.files
            .iter()
            .any(|f| f.file_name().unwrap().to_str().unwrap() == "image_a.png")
    });

    assert!(image_a_group.is_some(), "Should have a group with image_a.png");

    let group = image_a_group.unwrap();
    let has_copy = group.files.iter().any(|f| {
        f.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("image_a copy")
    });

    assert!(
        has_copy,
        "image_a.png and image_a copy.png should be in the same duplicate group"
    );
}

#[test]
fn test_scan_unique_image_not_in_duplicates() {
    let unique_image = fixture_path("images/unique.png");
    let files = vec![unique_image.clone()];

    let report = duplicate::find_duplicates(&files).unwrap();

    assert!(
        report.groups.is_empty(),
        "Single unique image should not be in any duplicate group"
    );
    assert_eq!(report.total_files, 1, "Should have scanned 1 file");
    assert_eq!(report.errors, 0, "Should have no errors");
}

#[test]
fn test_scan_image_reports_correct_counts() {
    let files = get_image_files();

    let report = duplicate::find_duplicates(&files).unwrap();

    // We have 4 images: image_a.png, "image_a copy.png", image_b.png, unique.png
    assert_eq!(report.total_files, 4, "Should have scanned 4 image files");

    // Should find at least 1 duplicate (image_a copy is a duplicate of image_a)
    // Note: exact duplicates may be merged with perceptual groups
    assert!(
        report.duplicate_count() >= 1,
        "Should have at least 1 duplicate"
    );

    // At least one group should exist
    assert!(
        !report.groups.is_empty(),
        "Should have at least one duplicate group"
    );
}

#[test]
fn test_scanner_executes_on_images_without_error() {
    let tmp = temp_dir();
    let output = tmp.path().join("duplicates.json");
    let scanner = Scanner::new(images_fixtures_dir(), false, false, Some(output.clone()), true);
    let result = scanner.execute();

    assert!(result.is_ok(), "Scanner should execute on images without error");
    assert!(output.exists(), "Duplicates file should be created for images");

    // Verify the output file contains valid JSON with duplicate entries
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains("\"entries\""),
        "Output should contain entries field"
    );
    assert!(
        content.contains("image_a"),
        "Output should reference image_a duplicates"
    );
}

#[test]
fn test_scan_mixed_files_in_directory() {
    // Test scanning a directory with both text and image files
    // Using the parent fixtures directory
    let tmp = temp_dir();
    let output = tmp.path().join("duplicates.json");

    // Copy both text and image fixtures to temp directory
    let text_dir = tmp.path().join("text");
    let images_dir = tmp.path().join("images");
    std::fs::create_dir(&text_dir).unwrap();
    std::fs::create_dir(&images_dir).unwrap();

    // Copy text files
    for entry in std::fs::read_dir(text_fixtures_dir()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            std::fs::copy(entry.path(), text_dir.join(entry.file_name())).unwrap();
        }
    }

    // Copy image files
    for entry in std::fs::read_dir(images_fixtures_dir()).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
            std::fs::copy(&path, images_dir.join(entry.file_name())).unwrap();
        }
    }

    // Scan recursively
    let scanner = Scanner::new(tmp.path().to_path_buf(), true, false, Some(output.clone()), true);
    let result = scanner.execute();

    assert!(result.is_ok(), "Scanner should handle mixed file types");
    assert!(output.exists(), "Duplicates file should be created");

    // Should find duplicates from both text and image files
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains("\"entries\""),
        "Output should contain entries"
    );
}
