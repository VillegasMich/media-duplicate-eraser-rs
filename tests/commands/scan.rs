//! Integration tests for the scan command.

use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::services::duplicate::{
    self, DetectionOptions, DuplicateReport, DuplicateType, MediaFilter,
};
use media_duplicate_eraser_rs::services::hasher;
use media_duplicate_eraser_rs::services::manifest::DuplicatesFile;

use crate::common::{fixture_path, images_fixtures_dir, temp_dir, text_fixtures_dir};

/// Runs detection over every file type, including non-media files.
fn find_any_file_duplicates(files: &[std::path::PathBuf]) -> DuplicateReport {
    let options = DetectionOptions {
        filter: MediaFilter::AnyFile,
        ..DetectionOptions::default()
    };
    duplicate::find_duplicates_with_options(files, None, options).unwrap()
}

#[test]
fn test_scan_detects_exact_duplicates() {
    let report = find_any_file_duplicates(
        &std::fs::read_dir(text_fixtures_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect::<Vec<_>>(),
    );

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

    let report = find_any_file_duplicates(&files);

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

    let report = find_any_file_duplicates(&files);

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
    let scanner = Scanner::new(
        text_fixtures_dir(),
        true,
        false,
        Some(output.clone()),
        true,
        MediaFilter::AnyFile,
    );
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
    assert!(!report.groups.is_empty(), "Should find duplicate images");

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

    assert!(
        image_a_group.is_some(),
        "Should have a group with image_a.png"
    );

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
    let scanner = Scanner::new(
        images_fixtures_dir(),
        false,
        false,
        Some(output.clone()),
        true,
        MediaFilter::All,
    );
    let result = scanner.execute();

    assert!(
        result.is_ok(),
        "Scanner should execute on images without error"
    );
    assert!(
        output.exists(),
        "Duplicates file should be created for images"
    );

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
    let scanner = Scanner::new(
        tmp.path().to_path_buf(),
        true,
        false,
        Some(output.clone()),
        true,
        MediaFilter::All,
    );
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

// ============================================================================
// Regression tests
// ============================================================================

fn scan(dir: &std::path::Path, filter: MediaFilter) -> Option<DuplicatesFile> {
    Scanner::new(dir.to_path_buf(), true, false, None, true, filter)
        .execute()
        .expect("Scan should succeed");
    let path = dir.join("duplicates.json");
    path.exists().then(|| DuplicatesFile::load(&path).unwrap())
}

#[test]
fn test_default_scan_ignores_non_media_files() {
    let tmp = temp_dir();
    std::fs::write(tmp.path().join("LICENSE"), "MIT").unwrap();
    std::fs::write(tmp.path().join("LICENSE.copy"), "MIT").unwrap();
    std::fs::write(tmp.path().join("notes.txt"), "same").unwrap();
    std::fs::write(tmp.path().join("notes2.txt"), "same").unwrap();

    assert!(
        scan(tmp.path(), MediaFilter::All).is_none(),
        "Non-media files must not be reported unless --media any is used"
    );
}

#[test]
fn test_empty_files_are_never_duplicates() {
    let tmp = temp_dir();
    std::fs::create_dir(tmp.path().join("a")).unwrap();
    std::fs::create_dir(tmp.path().join("b")).unwrap();
    std::fs::write(tmp.path().join("a/__init__.py"), "").unwrap();
    std::fs::write(tmp.path().join("b/__init__.py"), "").unwrap();
    std::fs::write(tmp.path().join("a/empty.png"), "").unwrap();
    std::fs::write(tmp.path().join("b/empty.png"), "").unwrap();

    assert!(scan(tmp.path(), MediaFilter::AnyFile).is_none());
}

#[test]
fn test_duplicates_file_uses_absolute_paths() {
    let tmp = temp_dir();
    std::fs::write(tmp.path().join("a.jpg"), "same").unwrap();
    std::fs::write(tmp.path().join("b.jpg"), "same").unwrap();

    let file = scan(tmp.path(), MediaFilter::All).expect("duplicates expected");
    let entry = &file.entries[0];
    assert!(entry.original.path.is_absolute());
    assert!(entry.duplicates.iter().all(|d| d.path.is_absolute()));
    assert!(file.root.is_absolute());
}

#[test]
fn test_rescan_without_duplicates_removes_stale_file() {
    let tmp = temp_dir();
    std::fs::write(tmp.path().join("a.jpg"), "same").unwrap();
    std::fs::write(tmp.path().join("b.jpg"), "same").unwrap();
    assert!(scan(tmp.path(), MediaFilter::All).is_some());

    std::fs::write(tmp.path().join("b.jpg"), "now different").unwrap();
    assert!(
        scan(tmp.path(), MediaFilter::All).is_none(),
        "An outdated duplicates.json must not survive a scan that finds nothing"
    );
}

#[test]
fn test_keeps_the_cleanest_path_for_exact_duplicates() {
    let tmp = temp_dir();
    std::fs::create_dir_all(tmp.path().join("backup/old")).unwrap();
    std::fs::write(tmp.path().join("photo.jpg"), "same").unwrap();
    std::fs::write(tmp.path().join("photo copy.jpg"), "same").unwrap();
    std::fs::write(tmp.path().join("backup/old/photo.jpg"), "same").unwrap();

    let file = scan(tmp.path(), MediaFilter::All).unwrap();
    assert_eq!(file.entries.len(), 1);
    assert_eq!(
        file.entries[0].original.path.file_name().unwrap(),
        "photo.jpg"
    );
    assert_eq!(file.entries[0].original.path.parent().unwrap(), file.root);
}

#[test]
fn test_no_recursive_skips_subdirectories() {
    let tmp = temp_dir();
    std::fs::create_dir(tmp.path().join("sub")).unwrap();
    std::fs::write(tmp.path().join("a.jpg"), "same").unwrap();
    std::fs::write(tmp.path().join("sub/b.jpg"), "same").unwrap();

    Scanner::new(
        tmp.path().to_path_buf(),
        false,
        false,
        None,
        true,
        MediaFilter::All,
    )
    .execute()
    .unwrap();
    assert!(!tmp.path().join("duplicates.json").exists());
}

#[test]
fn test_exact_only_skips_similar_images() {
    let files = get_image_files();
    let options = DetectionOptions {
        perceptual: false,
        ..DetectionOptions::default()
    };
    let report = duplicate::find_duplicates_with_options(&files, None, options).unwrap();

    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].duplicate_type, DuplicateType::Exact);
    assert_eq!(
        report.groups[0].files.len(),
        2,
        "image_b is similar, not identical"
    );
}

#[test]
fn test_similar_images_keep_the_largest_file() {
    let report = duplicate::find_duplicates(&get_image_files()).unwrap();
    let group = report
        .groups
        .iter()
        .find(|g| g.duplicate_type == DuplicateType::Perceptual)
        .expect("image_a and image_b are similar");
    let kept = report.size_of(group.original());
    assert!(group.duplicates().iter().all(|d| report.size_of(d) <= kept));
}

#[test]
fn test_scan_rejects_a_file_as_root() {
    let file = fixture_path("text/unique.txt");
    let result = Scanner::new(file, true, false, None, true, MediaFilter::All).execute();
    assert!(result.is_err());
}

// ============================================================================
// Video and audio (skipped when FFmpeg is not installed)
// ============================================================================

fn ffmpeg(args: &[&str]) {
    let status = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .expect("ffmpeg should run");
    assert!(status.success(), "ffmpeg {:?} failed", args);
}

fn skip_without_ffmpeg() -> bool {
    if hasher::is_ffmpeg_available() {
        return false;
    }
    eprintln!("skipping: ffmpeg/ffprobe not installed");
    true
}

#[test]
fn test_reencoded_video_is_similar() {
    if skip_without_ffmpeg() {
        return;
    }
    let tmp = temp_dir();
    let a = tmp.path().join("a.mp4");
    let b = tmp.path().join("b.mkv");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=s=320x240:d=6:r=30",
        a.to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-i",
        a.to_str().unwrap(),
        "-vf",
        "scale=240:180",
        "-crf",
        "35",
        b.to_str().unwrap(),
    ]);

    let report = duplicate::find_duplicates(&[a, b]).unwrap();
    assert_eq!(
        report.groups.len(),
        1,
        "A re-encoded, resized copy should match"
    );
}

#[test]
fn test_videos_sharing_a_black_intro_are_not_similar() {
    if skip_without_ffmpeg() {
        return;
    }
    let tmp = temp_dir();
    let a = tmp.path().join("a.mp4");
    let b = tmp.path().join("b.mp4");
    let make = |out: &std::path::Path, src: &str| {
        let filter = format!("color=black:s=320x240:d=5:r=30[i];{src}[m];[i][m]concat=n=2:v=1");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "nullsrc",
            "-filter_complex",
            &filter,
            "-t",
            "10",
            out.to_str().unwrap(),
        ]);
    };
    make(&a, "testsrc=s=320x240:d=5:r=30");
    make(&b, "mandelbrot=s=320x240:r=30,trim=duration=5");

    let report = duplicate::find_duplicates(&[a, b]).unwrap();
    assert!(
        report.groups.is_empty(),
        "Different videos with the same intro must not match"
    );
}

#[test]
fn test_audio_with_different_durations_is_not_similar() {
    if skip_without_ffmpeg() {
        return;
    }
    let tmp = temp_dir();
    let a = tmp.path().join("a.wav");
    let b = tmp.path().join("b.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=440:d=10", a.to_str().unwrap()]);
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=440:d=30", b.to_str().unwrap()]);

    let report = duplicate::find_duplicates(&[a, b]).unwrap();
    assert!(
        report.groups.is_empty(),
        "A 10s clip is not a duplicate of a 30s recording"
    );
}
