//! Duplicate detection service.
//!
//! Implements a two-pass approach for finding duplicate images:
//! 1. **Fast pass**: Group by file size, then SHA256 hash (exact duplicates)
//! 2. **Slow pass**: Perceptual hash comparison (visually similar images)

use std::collections::HashMap;
use std::path::PathBuf;

use image_hasher::ImageHash;

use super::hasher;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateType {
    /// Exact byte-for-byte duplicates (same SHA256 hash).
    Exact,
    /// Visually similar images (similar perceptual hash).
    Perceptual,
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

/// Finds duplicate images using a two-pass approach.
pub fn find_duplicates(files: &[PathBuf]) -> Result<DuplicateReport> {
    let mut groups = Vec::new();
    let mut errors = 0;
    let total_files = files.len();

    log::info!("Starting duplicate detection for {} files", total_files);

    // Pass 1: Group by file size
    log::debug!("Pass 1: Grouping by file size");
    let size_groups = group_by_size(files, &mut errors);

    // Pass 2: Within each size group, find exact duplicates by SHA256
    log::debug!("Pass 2: Finding exact duplicates by SHA256");
    let mut remaining_files: Vec<PathBuf> = Vec::new();

    for (_size, paths) in size_groups {
        if paths.len() < 2 {
            // Only one file with this size, might still be perceptually similar
            remaining_files.extend(paths);
            continue;
        }

        let (exact_groups, non_duplicates) = find_exact_duplicates(&paths, &mut errors);
        groups.extend(exact_groups);
        remaining_files.extend(non_duplicates);
    }

    // Pass 3: Perceptual hash comparison for remaining files
    log::debug!("Pass 3: Finding perceptual duplicates");
    let perceptual_groups = find_perceptual_duplicates(&remaining_files, &mut errors);
    groups.extend(perceptual_groups);

    log::info!(
        "Duplicate detection complete: {} groups found",
        groups.len()
    );

    Ok(DuplicateReport {
        groups,
        total_files,
        errors,
    })
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

/// Finds exact duplicates within a group of files using SHA256.
/// Returns (duplicate groups, files that are not exact duplicates).
fn find_exact_duplicates(
    files: &[PathBuf],
    errors: &mut usize,
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

/// Finds perceptually similar images.
fn find_perceptual_duplicates(files: &[PathBuf], errors: &mut usize) -> Vec<DuplicateGroup> {
    // Compute perceptual hashes for all files
    let mut hashes: Vec<(PathBuf, ImageHash)> = Vec::new();

    for path in files {
        match hasher::perceptual_hash(path) {
            Ok(Some(hash)) => {
                hashes.push((path.clone(), hash));
            }
            Ok(None) => {
                // Not an image file, skip
                log::debug!("Skipping non-image file: {:?}", path);
            }
            Err(e) => {
                log::warn!("Could not compute perceptual hash for {:?}: {}", path, e);
                *errors += 1;
            }
        }
    }

    // Find similar images using union-find approach
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
