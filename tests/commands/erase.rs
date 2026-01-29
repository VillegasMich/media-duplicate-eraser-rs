//! Integration tests for the erase command.

use std::fs;

use media_duplicate_eraser_rs::commands::erase::Eraser;
use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::commands::Command;

use crate::common::{assert_path_exists, assert_path_not_exists, images_fixtures_dir, temp_dir};

/// Helper to create a test scenario with duplicate files.
/// Returns (temp_dir, original_path, duplicate_path)
fn setup_duplicates() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = temp_dir();

    // Create an "original" file
    let original = tmp.path().join("original.txt");
    fs::write(&original, "This is the content that will be duplicated").unwrap();

    // Create a "duplicate" file with identical content
    let duplicate = tmp.path().join("duplicate.txt");
    fs::write(&duplicate, "This is the content that will be duplicated").unwrap();

    // Create a unique file that should not be affected
    let unique = tmp.path().join("unique.txt");
    fs::write(&unique, "This is unique content").unwrap();

    (tmp, original, duplicate)
}

/// Helper to run scan and create duplicates.json
fn run_scan(dir: &std::path::Path) {
    let scanner = Scanner::new(dir.to_path_buf(), false, false, None, true);
    scanner.execute().expect("Scan should succeed");
}

#[test]
fn test_erase_deletes_duplicate_files() {
    // Setup: Create duplicate files and scan
    let (tmp, original, duplicate) = setup_duplicates();
    let unique = tmp.path().join("unique.txt");

    // Run scan to create duplicates.json
    run_scan(tmp.path());

    // Verify scan created duplicates.json
    let duplicates_json = tmp.path().join("duplicates.json");
    assert_path_exists(&duplicates_json);

    // Verify all files exist before erase
    assert_path_exists(&original);
    assert_path_exists(&duplicate);
    assert_path_exists(&unique);

    // Execute: Run the eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should execute without error");

    // Verify: duplicates.json was removed
    assert_path_not_exists(&duplicates_json);

    // Verify: One of the duplicate pair should be deleted, one should remain
    // (The first file alphabetically is kept as "original")
    let duplicate_exists = duplicate.exists();
    let original_exists = original.exists();

    assert!(
        original_exists || duplicate_exists,
        "At least one file from duplicate pair should remain"
    );
    assert!(
        !(original_exists && duplicate_exists),
        "One file from duplicate pair should be deleted"
    );

    // Verify: Unique file should not be affected
    assert_path_exists(&unique);
}

#[test]
fn test_erase_handles_missing_duplicates_json() {
    // Setup: Create a temp directory without duplicates.json
    let tmp = temp_dir();

    // Create some files but don't run scan
    let file = tmp.path().join("some_file.txt");
    fs::write(&file, "content").unwrap();

    // Execute: Run eraser without duplicates.json
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command should succeed gracefully
    assert!(
        result.is_ok(),
        "Eraser should handle missing duplicates.json gracefully"
    );

    // Verify: File should still exist
    assert_path_exists(&file);
}

#[test]
fn test_erase_handles_empty_duplicates_list() {
    // Setup: Create duplicates.json with no entries
    let tmp = temp_dir();
    let duplicates_json = tmp.path().join("duplicates.json");

    let content = r#"{
        "version": "1.0",
        "scanned_at": "2024-01-01T00:00:00Z",
        "total_files_scanned": 1,
        "duplicate_groups": 0,
        "total_duplicates": 0,
        "entries": []
    }"#;
    fs::write(&duplicates_json, content).unwrap();

    // Create a file that should not be affected
    let file = tmp.path().join("file.txt");
    fs::write(&file, "content").unwrap();

    // Execute: Run eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command should succeed
    assert!(result.is_ok(), "Eraser should handle empty list gracefully");

    // Verify: File should still exist
    assert_path_exists(&file);
}

#[test]
fn test_erase_skips_already_deleted_files() {
    // Setup: Create three duplicate files so we have some to delete even if one is missing
    let tmp = temp_dir();

    // Create three duplicate files (same content)
    let file_a = tmp.path().join("file_a.txt");
    let file_b = tmp.path().join("file_b.txt");
    let file_c = tmp.path().join("file_c.txt");
    fs::write(&file_a, "duplicate content").unwrap();
    fs::write(&file_b, "duplicate content").unwrap();
    fs::write(&file_c, "duplicate content").unwrap();

    run_scan(tmp.path());

    let duplicates_json = tmp.path().join("duplicates.json");
    assert_path_exists(&duplicates_json);

    // Manually delete one file before running erase
    // This simulates a file being deleted between scan and erase
    fs::remove_file(&file_c).unwrap();
    assert_path_not_exists(&file_c);

    // Execute: Run eraser (it should handle missing files gracefully)
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command should succeed (missing files are skipped with a warning)
    assert!(
        result.is_ok(),
        "Eraser should handle already-deleted files gracefully: {:?}",
        result
    );

    // Verify: duplicates.json should be removed (erase completed successfully)
    assert_path_not_exists(&duplicates_json);

    // Verify: Exactly one file should remain (the "original" that wasn't marked for deletion)
    let remaining_count = [&file_a, &file_b, &file_c]
        .into_iter()
        .filter(|p| p.exists())
        .count();
    assert_eq!(remaining_count, 1, "Exactly one file should remain");
}

#[test]
fn test_erase_preserves_directory_structure() {
    // Setup: Create a more complex directory structure
    let tmp = temp_dir();

    // Create subdirectories
    let subdir1 = tmp.path().join("photos");
    let subdir2 = tmp.path().join("backup");
    fs::create_dir(&subdir1).unwrap();
    fs::create_dir(&subdir2).unwrap();

    // Create duplicate files in different directories
    let original = subdir1.join("photo.txt");
    let duplicate = subdir2.join("photo_copy.txt");
    fs::write(&original, "photo content").unwrap();
    fs::write(&duplicate, "photo content").unwrap();

    // Run scan recursively
    let scanner = Scanner::new(tmp.path().to_path_buf(), true, false, None, true);
    scanner.execute().unwrap();

    // Execute: Run eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should execute without error");

    // Verify: Directory structure should be preserved
    assert_path_exists(&subdir1);
    assert_path_exists(&subdir2);

    // Verify: One file should remain
    let original_exists = original.exists();
    let duplicate_exists = duplicate.exists();
    assert!(
        (original_exists && !duplicate_exists) || (!original_exists && duplicate_exists),
        "Exactly one file from duplicate pair should remain"
    );
}

#[test]
fn test_erase_with_multiple_duplicate_groups() {
    // Setup: Create multiple groups of duplicates
    let tmp = temp_dir();

    // Group 1: Two identical files
    let group1_a = tmp.path().join("group1_a.txt");
    let group1_b = tmp.path().join("group1_b.txt");
    fs::write(&group1_a, "content for group 1").unwrap();
    fs::write(&group1_b, "content for group 1").unwrap();

    // Group 2: Three identical files
    let group2_a = tmp.path().join("group2_a.txt");
    let group2_b = tmp.path().join("group2_b.txt");
    let group2_c = tmp.path().join("group2_c.txt");
    fs::write(&group2_a, "content for group 2").unwrap();
    fs::write(&group2_b, "content for group 2").unwrap();
    fs::write(&group2_c, "content for group 2").unwrap();

    // Unique file
    let unique = tmp.path().join("unique.txt");
    fs::write(&unique, "unique content").unwrap();

    // Run scan
    run_scan(tmp.path());

    // Execute: Run eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should execute without error");

    // Verify: Unique file should still exist
    assert_path_exists(&unique);

    // Verify: Exactly one file should remain from group 1
    let group1_count = [&group1_a, &group1_b]
        .into_iter()
        .filter(|p| p.exists())
        .count();
    assert_eq!(group1_count, 1, "Exactly one file should remain from group 1");

    // Verify: Exactly one file should remain from group 2
    let group2_count = [&group2_a, &group2_b, &group2_c]
        .into_iter()
        .filter(|p| p.exists())
        .count();
    assert_eq!(
        group2_count, 1,
        "Exactly one file should remain from group 2"
    );
}

#[test]
fn test_erase_is_idempotent() {
    // Setup: Create duplicate files and scan
    let (tmp, original, duplicate) = setup_duplicates();

    run_scan(tmp.path());

    // Execute: Run eraser twice
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);

    let result1 = eraser.execute();
    assert!(result1.is_ok(), "First erase should succeed");

    // After first erase, duplicates.json should be gone
    assert_path_not_exists(&tmp.path().join("duplicates.json"));

    // Second erase should succeed gracefully (no duplicates.json)
    let result2 = eraser.execute();
    assert!(
        result2.is_ok(),
        "Second erase should succeed (no-op when no duplicates.json)"
    );

    // Verify: Files should be in consistent state
    let original_exists = original.exists();
    let duplicate_exists = duplicate.exists();
    assert!(
        original_exists || duplicate_exists,
        "At least one file should remain"
    );
}

// ============================================================================
// Image-specific tests
// ============================================================================

/// Helper to copy image fixtures to a temp directory for testing
/// Returns (temp_dir, paths to copied images)
fn setup_image_duplicates() -> (tempfile::TempDir, Vec<std::path::PathBuf>) {
    let tmp = temp_dir();
    let mut copied_files = Vec::new();

    for entry in fs::read_dir(images_fixtures_dir()).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        // Only copy PNG files (skip .gitkeep, .DS_Store, etc.)
        if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
            let dest = tmp.path().join(entry.file_name());
            fs::copy(&path, &dest).unwrap();
            copied_files.push(dest);
        }
    }

    (tmp, copied_files)
}

#[test]
fn test_erase_deletes_duplicate_images() {
    // Setup: Copy image fixtures to temp directory
    let (tmp, copied_files) = setup_image_duplicates();

    // Run scan to detect duplicates
    let scanner = Scanner::new(tmp.path().to_path_buf(), false, false, None, true);
    scanner.execute().expect("Scan should succeed");

    // Verify scan created duplicates.json
    let duplicates_json = tmp.path().join("duplicates.json");
    assert_path_exists(&duplicates_json);

    // Count files before erase
    let files_before: Vec<_> = copied_files.iter().filter(|p| p.exists()).collect();
    assert_eq!(files_before.len(), 4, "Should have 4 images before erase");

    // Execute: Run the eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should execute without error on images");

    // Verify: duplicates.json was removed
    assert_path_not_exists(&duplicates_json);

    // Verify: Some files were deleted
    let files_after: Vec<_> = copied_files.iter().filter(|p| p.exists()).collect();

    // We should have fewer files after erase (at least one duplicate removed)
    assert!(
        files_after.len() < files_before.len(),
        "Should have deleted at least one duplicate image"
    );

    // unique.png should still exist (it's not similar to the others)
    let unique_exists = copied_files
        .iter()
        .any(|p| p.file_name().unwrap().to_str().unwrap() == "unique.png" && p.exists());
    assert!(unique_exists, "unique.png should not be deleted");

    // Note: image_b.png may be deleted if it's perceptually similar to image_a files
    // The tool groups perceptually similar images together
}

#[test]
fn test_erase_preserves_unique_images() {
    // Setup: Create temp directory with only unique images (no duplicates)
    let tmp = temp_dir();

    // Copy only unique.png and image_b.png (not duplicates of each other)
    let unique_src = images_fixtures_dir().join("unique.png");
    let image_b_src = images_fixtures_dir().join("image_b.png");

    let unique_dest = tmp.path().join("unique.png");
    let image_b_dest = tmp.path().join("image_b.png");

    fs::copy(&unique_src, &unique_dest).unwrap();
    fs::copy(&image_b_src, &image_b_dest).unwrap();

    // Run scan
    let scanner = Scanner::new(tmp.path().to_path_buf(), false, false, None, true);
    scanner.execute().expect("Scan should succeed");

    // duplicates.json might or might not exist depending on perceptual similarity
    // but if it does, erase should not delete any files

    // Execute: Run eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should succeed");

    // Verify: Both unique images should still exist
    assert_path_exists(&unique_dest);
    assert_path_exists(&image_b_dest);
}

#[test]
fn test_erase_image_duplicates_keeps_one_copy() {
    // Setup: Copy only the duplicate images (image_a.png and image_a copy.png)
    let tmp = temp_dir();

    let image_a_src = images_fixtures_dir().join("image_a.png");
    let image_a_copy_src = images_fixtures_dir().join("image_a copy.png");

    let image_a_dest = tmp.path().join("image_a.png");
    let image_a_copy_dest = tmp.path().join("image_a copy.png");

    fs::copy(&image_a_src, &image_a_dest).unwrap();
    fs::copy(&image_a_copy_src, &image_a_copy_dest).unwrap();

    // Run scan
    let scanner = Scanner::new(tmp.path().to_path_buf(), false, false, None, true);
    scanner.execute().expect("Scan should succeed");

    let duplicates_json = tmp.path().join("duplicates.json");
    assert_path_exists(&duplicates_json);

    // Execute: Run eraser
    let eraser = Eraser::new(tmp.path().to_path_buf(), true);
    let result = eraser.execute();

    // Verify: Command succeeded
    assert!(result.is_ok(), "Eraser should execute without error");

    // Verify: Exactly one image should remain
    let image_a_exists = image_a_dest.exists();
    let image_a_copy_exists = image_a_copy_dest.exists();

    assert!(
        (image_a_exists && !image_a_copy_exists) || (!image_a_exists && image_a_copy_exists),
        "Exactly one copy of image_a should remain after erase"
    );
}
