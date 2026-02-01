//! Duplicate detection service.
//!
//! Implements a two-pass approach for finding duplicate media files:
//! 1. **Fast pass**: Group by file size, then SHA256 hash (exact duplicates)
//! 2. **Slow pass**: Perceptual hash comparison (visually similar images/videos)

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use image_hasher::ImageHash;
use serde::{Deserialize, Serialize};

use super::hasher::{self, MediaType};
use crate::error::Result;

/// Represents a group of duplicate files.
#[derive(Debug)]
pub struct DuplicateGroup {
    /// The files that are duplicates of each other.
    pub files: Vec<PathBuf>,
    /// The type of duplication detected.
    pub duplicate_type: DuplicateType,
}

/// The type of duplication detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateType {
    /// Exact byte-for-byte duplicates (same SHA256 hash).
    Exact,
    /// Visually similar media (similar perceptual hash).
    Perceptual,
}

/// Filter for which media types to scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaFilter {
    /// Scan all supported media types (images, videos, and audio).
    #[default]
    All,
    /// Scan only images.
    ImagesOnly,
    /// Scan only videos.
    VideosOnly,
    /// Scan only audio files.
    AudioOnly,
}

impl MediaFilter {
    /// Checks if a file should be included based on the filter.
    pub fn includes(&self, path: &Path) -> bool {
        let media_type = hasher::get_media_type(path);
        match self {
            MediaFilter::All => true,
            MediaFilter::ImagesOnly => media_type == MediaType::Image,
            MediaFilter::VideosOnly => media_type == MediaType::Video,
            MediaFilter::AudioOnly => media_type == MediaType::Audio,
        }
    }

    /// Checks if a file should be processed for perceptual hashing based on the filter.
    pub fn includes_for_perceptual(&self, path: &Path) -> bool {
        let media_type = hasher::get_media_type(path);
        match self {
            MediaFilter::All => {
                media_type == MediaType::Image
                    || media_type == MediaType::Video
                    || media_type == MediaType::Audio
            }
            MediaFilter::ImagesOnly => media_type == MediaType::Image,
            MediaFilter::VideosOnly => media_type == MediaType::Video,
            MediaFilter::AudioOnly => media_type == MediaType::Audio,
        }
    }
}

/// A duplicate entry in the output file.
/// Contains only the copies to be deleted, not the original.
#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicateEntry {
    /// The original file to keep.
    pub original: PathBuf,
    /// The duplicate files to delete.
    pub duplicates: Vec<PathBuf>,
    /// The type of duplication.
    pub duplicate_type: DuplicateType,
}

/// The duplicates file structure that will be saved to JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicatesFile {
    /// Version of the file format.
    pub version: String,
    /// Timestamp when the scan was performed.
    pub scanned_at: DateTime<Utc>,
    /// Total number of files scanned.
    pub total_files_scanned: usize,
    /// Number of duplicate groups found.
    pub duplicate_groups: usize,
    /// Total number of duplicate files (to be deleted).
    pub total_duplicates: usize,
    /// The duplicate entries.
    pub entries: Vec<DuplicateEntry>,
}

impl DuplicatesFile {
    /// Creates a new DuplicatesFile from a DuplicateReport.
    pub fn from_report(report: &DuplicateReport) -> Self {
        let entries: Vec<DuplicateEntry> = report
            .groups
            .iter()
            .map(|group| {
                let mut files = group.files.clone();
                // First file is the original to keep
                let original = files.remove(0);
                DuplicateEntry {
                    original,
                    duplicates: files,
                    duplicate_type: group.duplicate_type,
                }
            })
            .collect();

        let total_duplicates = entries.iter().map(|e| e.duplicates.len()).sum();

        Self {
            version: "1.0".to_string(),
            scanned_at: Utc::now(),
            total_files_scanned: report.total_files,
            duplicate_groups: report.groups.len(),
            total_duplicates,
            entries,
        }
    }

    /// Saves the duplicates file to the specified path.
    pub fn save(&self, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        log::info!("Duplicates file saved to {:?}", path);
        Ok(())
    }

    /// Loads a duplicates file from the specified path.
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let duplicates: DuplicatesFile = serde_json::from_reader(file)?;
        Ok(duplicates)
    }
}

/// Result of duplicate detection.
#[derive(Debug)]
pub struct DuplicateReport {
    /// Groups of duplicate files found.
    pub groups: Vec<DuplicateGroup>,
    /// Total number of files scanned.
    pub total_files: usize,
    /// Number of files that could not be processed.
    pub errors: usize,
}

impl DuplicateReport {
    /// Returns the total number of duplicate files (excluding one original per group).
    pub fn duplicate_count(&self) -> usize {
        self.groups
            .iter()
            .map(|g| g.files.len().saturating_sub(1))
            .sum()
    }

    /// Returns the number of exact duplicates.
    pub fn exact_duplicate_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| g.duplicate_type == DuplicateType::Exact)
            .map(|g| g.files.len().saturating_sub(1))
            .sum()
    }

    /// Returns the number of perceptual duplicates.
    pub fn perceptual_duplicate_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| g.duplicate_type == DuplicateType::Perceptual)
            .map(|g| g.files.len().saturating_sub(1))
            .sum()
    }
}

/// Progress callback for duplicate detection.
/// Called with (current_file_index, total_files, phase_name).
pub type ProgressCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Finds duplicate media files using a two-pass approach.
pub fn find_duplicates(files: &[PathBuf]) -> Result<DuplicateReport> {
    find_duplicates_with_options(files, None, MediaFilter::All)
}

/// Finds duplicate media files using a two-pass approach with optional progress callback.
pub fn find_duplicates_with_progress(
    files: &[PathBuf],
    progress: Option<ProgressCallback>,
) -> Result<DuplicateReport> {
    find_duplicates_with_options(files, progress, MediaFilter::All)
}

/// Finds duplicate media files with full options.
pub fn find_duplicates_with_options(
    files: &[PathBuf],
    progress: Option<ProgressCallback>,
    filter: MediaFilter,
) -> Result<DuplicateReport> {
    // Filter files based on media type if not scanning all
    let filtered_files: Vec<PathBuf> = if filter == MediaFilter::All {
        files.to_vec()
    } else {
        files
            .iter()
            .filter(|p| filter.includes(p))
            .cloned()
            .collect()
    };

    let mut exact_groups: Vec<DuplicateGroup> = Vec::new();
    let mut errors = 0;
    let total_files = filtered_files.len();

    log::info!("Starting duplicate detection for {} files", total_files);

    // Pass 1: Group by file size
    log::debug!("Pass 1: Grouping by file size");
    let size_groups = group_by_size(&filtered_files, &mut errors);

    // Pass 2: Within each size group, find exact duplicates by SHA256
    log::debug!("Pass 2: Finding exact duplicates by SHA256");
    let mut files_for_perceptual: Vec<PathBuf> = Vec::new();
    let mut processed = 0;

    for (_size, paths) in size_groups {
        if paths.len() < 2 {
            // Only one file with this size, still needs perceptual comparison
            files_for_perceptual.extend(paths.clone());
            processed += paths.len();
            if let Some(cb) = progress.as_ref() {
                cb(processed, total_files, "Hashing files");
            }
            continue;
        }

        let (groups, non_duplicates) =
            find_exact_duplicates_with_progress(&paths, &mut errors, &progress, &mut processed, total_files);

        // Add one representative from each exact duplicate group for perceptual comparison
        for group in &groups {
            if let Some(representative) = group.files.first() {
                files_for_perceptual.push(representative.clone());
            }
        }

        exact_groups.extend(groups);
        files_for_perceptual.extend(non_duplicates);
    }

    // Pass 3: Perceptual hash comparison
    log::debug!("Pass 3: Finding perceptual duplicates");
    let perceptual_groups =
        find_perceptual_duplicates_with_progress(&files_for_perceptual, &mut errors, &progress, filter);

    // Merge perceptual groups with exact groups where they overlap
    let final_groups = merge_groups(exact_groups, perceptual_groups);

    log::info!(
        "Duplicate detection complete: {} groups found",
        final_groups.len()
    );

    Ok(DuplicateReport {
        groups: final_groups,
        total_files,
        errors,
    })
}

/// Merges exact and perceptual groups, expanding exact groups when their
/// representative is found in a perceptual group.
fn merge_groups(
    exact_groups: Vec<DuplicateGroup>,
    perceptual_groups: Vec<DuplicateGroup>,
) -> Vec<DuplicateGroup> {
    let mut final_groups: Vec<DuplicateGroup> = Vec::new();

    // Build a map from file path to exact group index
    let mut file_to_exact_group: HashMap<PathBuf, usize> = HashMap::new();
    for (idx, group) in exact_groups.iter().enumerate() {
        for file in &group.files {
            file_to_exact_group.insert(file.clone(), idx);
        }
    }

    // Track which exact groups have been merged
    let mut merged_exact_groups: Vec<bool> = vec![false; exact_groups.len()];

    // Process perceptual groups
    for perceptual_group in perceptual_groups {
        let mut merged_files: Vec<PathBuf> = Vec::new();
        let mut has_exact_duplicates = false;

        for file in perceptual_group.files {
            if let Some(&exact_idx) = file_to_exact_group.get(&file) {
                // This file is part of an exact group, include all files from that group
                if !merged_exact_groups[exact_idx] {
                    merged_files.extend(exact_groups[exact_idx].files.clone());
                    merged_exact_groups[exact_idx] = true;
                    has_exact_duplicates = true;
                }
            } else {
                merged_files.push(file);
            }
        }

        if merged_files.len() > 1 {
            // Deduplicate in case of overlaps
            merged_files.sort();
            merged_files.dedup();

            final_groups.push(DuplicateGroup {
                files: merged_files,
                // If it contains exact duplicates, mark as perceptual since it's a mixed group
                duplicate_type: if has_exact_duplicates {
                    DuplicateType::Perceptual
                } else {
                    DuplicateType::Perceptual
                },
            });
        }
    }

    // Add remaining exact groups that weren't merged
    for (idx, group) in exact_groups.into_iter().enumerate() {
        if !merged_exact_groups[idx] {
            final_groups.push(group);
        }
    }

    final_groups
}

/// Groups files by their size.
fn group_by_size(files: &[PathBuf], errors: &mut usize) -> HashMap<u64, Vec<PathBuf>> {
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for path in files {
        match hasher::file_size(path) {
            Ok(size) => {
                size_map.entry(size).or_default().push(path.clone());
            }
            Err(e) => {
                log::warn!("Could not get size of {:?}: {}", path, e);
                *errors += 1;
            }
        }
    }

    size_map
}

/// Finds exact duplicates with progress reporting.
fn find_exact_duplicates_with_progress(
    files: &[PathBuf],
    errors: &mut usize,
    progress: &Option<ProgressCallback>,
    processed: &mut usize,
    total: usize,
) -> (Vec<DuplicateGroup>, Vec<PathBuf>) {
    let mut hash_map: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for path in files {
        match hasher::sha256_hash(path) {
            Ok(hash) => {
                hash_map.entry(hash).or_default().push(path.clone());
            }
            Err(e) => {
                log::warn!("Could not hash {:?}: {}", path, e);
                *errors += 1;
            }
        }
        *processed += 1;
        if let Some(cb) = progress {
            cb(*processed, total, "Hashing files");
        }
    }

    let mut groups = Vec::new();
    let mut non_duplicates = Vec::new();

    for (_hash, paths) in hash_map {
        if paths.len() > 1 {
            groups.push(DuplicateGroup {
                files: paths,
                duplicate_type: DuplicateType::Exact,
            });
        } else {
            non_duplicates.extend(paths);
        }
    }

    (groups, non_duplicates)
}

/// Finds perceptually similar media files with progress reporting.
fn find_perceptual_duplicates_with_progress(
    files: &[PathBuf],
    errors: &mut usize,
    progress: &Option<ProgressCallback>,
    filter: MediaFilter,
) -> Vec<DuplicateGroup> {
    // Compute perceptual hashes for all supported media files
    let mut hashes: Vec<(PathBuf, ImageHash)> = Vec::new();
    let total = files.len();

    for (i, path) in files.iter().enumerate() {
        // Check if file should be processed based on filter
        if !filter.includes_for_perceptual(path) {
            if let Some(cb) = progress {
                cb(i + 1, total, "Analyzing media");
            }
            continue;
        }

        // Use the unified media perceptual hash function
        match hasher::media_perceptual_hash(path) {
            Ok(Some(hash)) => {
                hashes.push((path.clone(), hash));
            }
            Ok(None) => {
                // Not a supported media file, skip
                log::debug!("Skipping unsupported file: {:?}", path);
            }
            Err(e) => {
                log::warn!("Could not compute perceptual hash for {:?}: {}", path, e);
                *errors += 1;
            }
        }
        if let Some(cb) = progress {
            cb(i + 1, total, "Analyzing media");
        }
    }

    // Find similar media using union-find approach
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut used: Vec<bool> = vec![false; hashes.len()];

    for i in 0..hashes.len() {
        if used[i] {
            continue;
        }

        let mut group_files = vec![hashes[i].0.clone()];
        used[i] = true;

        for j in (i + 1)..hashes.len() {
            if used[j] {
                continue;
            }

            if hasher::are_similar(&hashes[i].1, &hashes[j].1) {
                group_files.push(hashes[j].0.clone());
                used[j] = true;
            }
        }

        if group_files.len() > 1 {
            groups.push(DuplicateGroup {
                files: group_files,
                duplicate_type: DuplicateType::Perceptual,
            });
        }
    }

    groups
}
