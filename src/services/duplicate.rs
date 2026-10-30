//! Duplicate detection service.
//!
//! Implements a multi-pass approach for finding duplicate media files:
//! 1. **Size pass**: Group by file size (files of different sizes can't be identical)
//! 2. **Exact pass**: SHA256 hash within each size group (exact duplicates)
//! 3. **Perceptual pass**: Perceptual fingerprint comparison (similar media)
//! 4. **Merge**: Exact groups whose representative is perceptually similar to
//!    other files are merged into a single group
//!
//! Within every group, files are ordered so the one to keep comes first.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::hasher::{self, Fingerprint, MediaType};
use crate::error::Result;

/// Represents a group of duplicate files.
#[derive(Debug)]
pub struct DuplicateGroup {
    /// The files that are duplicates of each other.
    /// The first file is the one to keep; the rest can be removed.
    pub files: Vec<PathBuf>,
    /// The type of duplication detected.
    pub duplicate_type: DuplicateType,
}

impl DuplicateGroup {
    /// The file that will be kept.
    pub fn original(&self) -> &Path {
        &self.files[0]
    }

    /// The files that can be removed.
    pub fn duplicates(&self) -> &[PathBuf] {
        &self.files[1..]
    }
}

/// The type of duplication detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateType {
    /// Exact byte-for-byte duplicates (same SHA256 hash).
    Exact,
    /// Visually similar media (similar perceptual hash).
    /// Groups that mix exact and similar files are also perceptual.
    Perceptual,
}

/// Filter for which files to scan.
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
    /// Scan every file. Non-media files only get exact (byte-for-byte) matching.
    AnyFile,
}

impl MediaFilter {
    /// Checks if a file should be included based on the filter.
    pub fn includes(&self, path: &Path) -> bool {
        let media_type = hasher::get_media_type(path);
        match self {
            MediaFilter::All => media_type != MediaType::Unknown,
            MediaFilter::ImagesOnly => media_type == MediaType::Image,
            MediaFilter::VideosOnly => media_type == MediaType::Video,
            MediaFilter::AudioOnly => media_type == MediaType::Audio,
            MediaFilter::AnyFile => true,
        }
    }

    /// Whether this filter can include video or audio files, which need FFmpeg.
    pub fn needs_ffmpeg(&self) -> bool {
        !matches!(self, MediaFilter::ImagesOnly)
    }
}

/// Options controlling duplicate detection.
#[derive(Debug, Clone, Copy)]
pub struct DetectionOptions {
    /// Which files to consider.
    pub filter: MediaFilter,
    /// Whether to run the perceptual pass. When false, only exact duplicates are found.
    pub perceptual: bool,
    /// Maximum Hamming distance for two fingerprints to be considered similar.
    pub threshold: u32,
}

impl Default for DetectionOptions {
    fn default() -> Self {
        Self {
            filter: MediaFilter::All,
            perceptual: true,
            threshold: hasher::DEFAULT_SIMILARITY_THRESHOLD,
        }
    }
}

/// Result of duplicate detection.
#[derive(Debug)]
pub struct DuplicateReport {
    /// Groups of duplicate files found.
    pub groups: Vec<DuplicateGroup>,
    /// Total number of files scanned.
    pub total_files: usize,
    /// Number of empty files that were ignored.
    pub empty_files: usize,
    /// Number of files that could not be processed.
    pub errors: usize,
    /// Size in bytes of every file that ended up in a group.
    sizes: HashMap<PathBuf, u64>,
}

impl DuplicateReport {
    /// Returns the total number of duplicate files (excluding one original per group).
    pub fn duplicate_count(&self) -> usize {
        self.groups.iter().map(|g| g.duplicates().len()).sum()
    }

    /// Returns the number of exact duplicates.
    pub fn exact_duplicate_count(&self) -> usize {
        self.count_of(DuplicateType::Exact)
    }

    /// Returns the number of perceptual duplicates.
    pub fn perceptual_duplicate_count(&self) -> usize {
        self.count_of(DuplicateType::Perceptual)
    }

    fn count_of(&self, duplicate_type: DuplicateType) -> usize {
        self.groups
            .iter()
            .filter(|g| g.duplicate_type == duplicate_type)
            .map(|g| g.duplicates().len())
            .sum()
    }

    /// Size of a file in a duplicate group, as measured during the scan.
    pub fn size_of(&self, path: &Path) -> u64 {
        self.sizes.get(path).copied().unwrap_or(0)
    }

    /// Total bytes that would be freed by removing every duplicate.
    pub fn reclaimable_bytes(&self) -> u64 {
        self.groups
            .iter()
            .flat_map(|g| g.duplicates())
            .map(|p| self.size_of(p))
            .sum()
    }
}

/// Progress callback for duplicate detection.
/// Called with (files_done_in_phase, files_in_phase, phase_name).
pub type ProgressCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Finds duplicate media files using the default options.
pub fn find_duplicates(files: &[PathBuf]) -> Result<DuplicateReport> {
    find_duplicates_with_options(files, None, DetectionOptions::default())
}

/// Finds duplicate files with full options and an optional progress callback.
pub fn find_duplicates_with_options(
    files: &[PathBuf],
    progress: Option<ProgressCallback>,
    options: DetectionOptions,
) -> Result<DuplicateReport> {
    let filtered_files: Vec<&PathBuf> = files
        .iter()
        .filter(|p| options.filter.includes(p))
        .collect();
    let total_files = filtered_files.len();
    let mut errors = 0;

    log::info!("Starting duplicate detection for {} files", total_files);

    // Pass 1: Group by file size
    log::debug!("Pass 1: Grouping by file size");
    let mut sizes: HashMap<PathBuf, u64> = HashMap::new();
    let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut empty_files = 0;
    for path in filtered_files {
        match hasher::file_size(path) {
            // Empty files are all "identical" but never meaningful duplicates.
            Ok(0) => empty_files += 1,
            Ok(size) => {
                sizes.insert(path.clone(), size);
                size_groups.entry(size).or_default().push(path.clone());
            }
            Err(e) => {
                log::warn!("Could not get size of {:?}: {}", path, e);
                errors += 1;
            }
        }
    }

    // Pass 2: Within each size group, find exact duplicates by SHA256
    log::debug!("Pass 2: Finding exact duplicates by SHA256");
    let mut singletons: Vec<PathBuf> = Vec::new();
    let mut to_hash: Vec<PathBuf> = Vec::new();
    for (_size, paths) in size_groups {
        if paths.len() < 2 {
            singletons.extend(paths);
        } else {
            to_hash.extend(paths);
        }
    }

    let (exact_groups, non_duplicates) = find_exact_duplicates(&to_hash, &mut errors, &progress);

    // Pass 3: Perceptual comparison over every distinct file
    let perceptual_groups = if options.perceptual {
        log::debug!("Pass 3: Finding perceptual duplicates");
        let mut candidates: Vec<PathBuf> = singletons;
        candidates.extend(non_duplicates);
        candidates.extend(exact_groups.iter().map(|g| g.files[0].clone()));
        candidates.retain(|p| hasher::get_media_type(p) != MediaType::Unknown);
        // Sorting makes grouping deterministic regardless of hashing order.
        candidates.sort();
        find_perceptual_duplicates(&candidates, &progress, options.threshold)
    } else {
        Vec::new()
    };

    // Pass 4: Merge perceptual groups with exact groups where they overlap
    let mut final_groups = merge_groups(exact_groups, perceptual_groups);
    for group in &mut final_groups {
        order_for_keeping(group, &sizes);
    }
    final_groups.sort_by(|a, b| a.files[0].cmp(&b.files[0]));

    sizes.retain(|path, _| final_groups.iter().any(|g| g.files.contains(path)));

    log::info!(
        "Duplicate detection complete: {} groups found",
        final_groups.len()
    );

    Ok(DuplicateReport {
        groups: final_groups,
        total_files,
        empty_files,
        errors,
        sizes,
    })
}

/// Orders a group's files so the best one to keep comes first.
///
/// - Exact duplicates are byte-identical, so the file with the "cleanest" path
///   is kept: the shallowest, then the shortest path (so `photo.jpg` beats
///   `photo copy.jpg` and `backup/old/photo.jpg`), then alphabetical.
/// - Perceptual groups keep the largest file, which is usually the highest
///   quality version (original resolution, least compression), with the same
///   path rules as a tie-breaker.
fn order_for_keeping(group: &mut DuplicateGroup, sizes: &HashMap<PathBuf, u64>) {
    let path_key = |p: &PathBuf| (p.components().count(), p.as_os_str().len(), p.clone());
    match group.duplicate_type {
        DuplicateType::Exact => group.files.sort_by_key(path_key),
        DuplicateType::Perceptual => group
            .files
            .sort_by_key(|p| (Reverse(sizes.get(p).copied().unwrap_or(0)), path_key(p))),
    }
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

    for perceptual_group in perceptual_groups {
        let mut merged_files: Vec<PathBuf> = Vec::new();

        for file in perceptual_group.files {
            if let Some(&exact_idx) = file_to_exact_group.get(&file) {
                // This file is part of an exact group, include all files from that group
                if !merged_exact_groups[exact_idx] {
                    merged_files.extend(exact_groups[exact_idx].files.iter().cloned());
                    merged_exact_groups[exact_idx] = true;
                }
            } else {
                merged_files.push(file);
            }
        }

        if merged_files.len() > 1 {
            merged_files.sort();
            merged_files.dedup();

            final_groups.push(DuplicateGroup {
                files: merged_files,
                duplicate_type: DuplicateType::Perceptual,
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

/// Finds exact duplicates by SHA256, hashing files in parallel.
///
/// Returns the exact duplicate groups and the files that had no exact match.
fn find_exact_duplicates(
    files: &[PathBuf],
    errors: &mut usize,
    progress: &Option<ProgressCallback>,
) -> (Vec<DuplicateGroup>, Vec<PathBuf>) {
    let total = files.len();
    let done = AtomicUsize::new(0);

    let hashes: Vec<(PathBuf, Result<String>)> = files
        .par_iter()
        .map(|path| {
            let hash = hasher::sha256_hash(path);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(cb) = progress {
                cb(n, total, "Hashing files");
            }
            (path.clone(), hash)
        })
        .collect();

    let mut hash_map: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (path, hash) in hashes {
        match hash {
            Ok(hash) => hash_map.entry(hash).or_default().push(path),
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

/// Finds perceptually similar media files.
///
/// Fingerprints are computed in parallel. Grouping is greedy: each file not yet
/// grouped starts a group, and every later file similar to it joins.
fn find_perceptual_duplicates(
    files: &[PathBuf],
    progress: &Option<ProgressCallback>,
    threshold: u32,
) -> Vec<DuplicateGroup> {
    let total = files.len();
    let done = AtomicUsize::new(0);

    let fingerprints: Vec<(PathBuf, Fingerprint)> = files
        .par_iter()
        .filter_map(|path| {
            let fingerprint = hasher::media_fingerprint(path);
            if fingerprint.is_none() {
                log::debug!("No perceptual fingerprint for {:?}", path);
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(cb) = progress {
                cb(n, total, "Analyzing media");
            }
            fingerprint.map(|f| (path.clone(), f))
        })
        .collect();

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut used: Vec<bool> = vec![false; fingerprints.len()];

    for i in 0..fingerprints.len() {
        if used[i] {
            continue;
        }

        let mut group_files = vec![fingerprints[i].0.clone()];
        used[i] = true;

        for j in (i + 1)..fingerprints.len() {
            if !used[j] && fingerprints[i].1.is_similar(&fingerprints[j].1, threshold) {
                group_files.push(fingerprints[j].0.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn group(files: &[&str], duplicate_type: DuplicateType) -> DuplicateGroup {
        DuplicateGroup {
            files: files.iter().map(PathBuf::from).collect(),
            duplicate_type,
        }
    }

    #[test]
    fn exact_groups_keep_the_cleanest_path() {
        let mut g = group(
            &[
                "/p/backup/old/photo.jpg",
                "/p/photo copy.jpg",
                "/p/photo.jpg",
            ],
            DuplicateType::Exact,
        );
        order_for_keeping(&mut g, &HashMap::new());
        assert_eq!(g.original(), Path::new("/p/photo.jpg"));
        assert_eq!(g.duplicates()[0], PathBuf::from("/p/photo copy.jpg"));
    }

    #[test]
    fn perceptual_groups_keep_the_largest_file() {
        let mut g = group(
            &["/p/a.jpg", "/p/sub/a_full.png"],
            DuplicateType::Perceptual,
        );
        let sizes = HashMap::from([
            (PathBuf::from("/p/a.jpg"), 100),
            (PathBuf::from("/p/sub/a_full.png"), 5000),
        ]);
        order_for_keeping(&mut g, &sizes);
        assert_eq!(g.original(), Path::new("/p/sub/a_full.png"));
    }

    #[test]
    fn media_filter_all_excludes_non_media() {
        assert!(MediaFilter::All.includes(Path::new("a.png")));
        assert!(!MediaFilter::All.includes(Path::new("a.txt")));
        assert!(MediaFilter::AnyFile.includes(Path::new("a.txt")));
        assert!(!MediaFilter::ImagesOnly.includes(Path::new("a.mp4")));
    }
}
