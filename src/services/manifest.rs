//! The duplicates file (`duplicates.json`) written by `scan` and consumed by `erase`.
//!
//! All paths are absolute, and every file is recorded with the size and
//! modification time observed during the scan, so `erase` can refuse to
//! delete anything that changed in the meantime.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::duplicate::{DuplicateReport, DuplicateType};
use crate::error::{Error, Result};

/// Default name of the duplicates file.
pub const DUPLICATES_FILENAME: &str = "duplicates.json";

/// Current version of the duplicates file format.
pub const FORMAT_VERSION: &str = "2";

/// A file as observed during the scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    /// Absolute path of the file.
    pub path: PathBuf,
    /// Size in bytes at scan time.
    pub size: u64,
    /// Last modification time at scan time, if the platform provides it.
    pub modified: Option<DateTime<Utc>>,
}

impl FileRecord {
    /// Records the current metadata of `path`.
    pub fn capture(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified().ok().map(DateTime::<Utc>::from),
        })
    }

    /// Whether the file on disk still has the size and modification time recorded at scan time.
    pub fn is_unchanged(&self) -> bool {
        match fs::metadata(&self.path) {
            Ok(metadata) => {
                metadata.is_file()
                    && metadata.len() == self.size
                    && metadata.modified().ok().map(DateTime::<Utc>::from) == self.modified
            }
            Err(_) => false,
        }
    }
}

/// A duplicate entry in the output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateEntry {
    /// The original file to keep.
    pub original: FileRecord,
    /// The duplicate files to delete.
    pub duplicates: Vec<FileRecord>,
    /// The type of duplication.
    pub duplicate_type: DuplicateType,
}

/// The duplicates file structure that will be saved to JSON.
///
/// The file is meant to be human-reviewable: removing an entry, or a path from
/// an entry's `duplicates`, before running `erase` keeps those files.
#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicatesFile {
    /// Version of the file format.
    pub version: String,
    /// Timestamp when the scan was performed.
    pub scanned_at: DateTime<Utc>,
    /// The directory that was scanned.
    pub root: PathBuf,
    /// Total number of files scanned.
    pub total_files_scanned: usize,
    /// Number of duplicate groups found.
    pub duplicate_groups: usize,
    /// Total number of duplicate files (to be deleted).
    pub total_duplicates: usize,
    /// Bytes that deleting every duplicate would free.
    pub reclaimable_bytes: u64,
    /// The duplicate entries.
    pub entries: Vec<DuplicateEntry>,
}

/// Just enough of the file to check its version before parsing the rest.
#[derive(Deserialize)]
struct VersionProbe {
    version: String,
}

impl DuplicatesFile {
    /// Creates a new DuplicatesFile from a DuplicateReport.
    ///
    /// Files that disappeared since detection are left out.
    pub fn from_report(report: &DuplicateReport, root: &Path) -> Self {
        let capture = |path: &Path| match FileRecord::capture(path) {
            Ok(record) => Some(record),
            Err(e) => {
                log::warn!("Could not read metadata of {:?}: {}", path, e);
                None
            }
        };

        let entries: Vec<DuplicateEntry> = report
            .groups
            .iter()
            .filter_map(|group| {
                let original = capture(group.original())?;
                let duplicates: Vec<FileRecord> = group
                    .duplicates()
                    .iter()
                    .filter_map(|p| capture(p))
                    .collect();
                (!duplicates.is_empty()).then_some(DuplicateEntry {
                    original,
                    duplicates,
                    duplicate_type: group.duplicate_type,
                })
            })
            .collect();

        let total_duplicates = entries.iter().map(|e| e.duplicates.len()).sum();
        let reclaimable_bytes = entries
            .iter()
            .flat_map(|e| &e.duplicates)
            .map(|d| d.size)
            .sum();

        Self {
            version: FORMAT_VERSION.to_string(),
            scanned_at: Utc::now(),
            root: root.to_path_buf(),
            total_files_scanned: report.total_files,
            duplicate_groups: entries.len(),
            total_duplicates,
            reclaimable_bytes,
            entries,
        }
    }

    /// Saves the duplicates file to the specified path.
    ///
    /// Writes to a temporary file first so an interrupted save never leaves a
    /// truncated duplicates file behind.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        {
            let writer = BufWriter::new(File::create(&tmp)?);
            serde_json::to_writer_pretty(writer, self)?;
        }
        fs::rename(&tmp, path)?;
        log::info!("Duplicates file saved to {:?}", path);
        Ok(())
    }

    /// Loads a duplicates file from the specified path.
    ///
    /// Rejects files written in another format version, and files whose
    /// paths are not absolute (relative paths would be resolved against
    /// whatever directory `erase` happens to run in).
    pub fn load(path: &Path) -> Result<Self> {
        let invalid = |reason: String| Error::InvalidDuplicatesFile {
            path: path.to_path_buf(),
            reason,
        };

        let content = fs::read_to_string(path)?;
        let probe: VersionProbe = serde_json::from_str(&content)
            .map_err(|e| invalid(format!("not a duplicates file ({})", e)))?;
        if probe.version != FORMAT_VERSION {
            return Err(invalid(format!(
                "format version {} is not supported (expected {})",
                probe.version, FORMAT_VERSION
            )));
        }

        let file: DuplicatesFile =
            serde_json::from_str(&content).map_err(|e| invalid(e.to_string()))?;

        let relative = file
            .entries
            .iter()
            .flat_map(|e| std::iter::once(&e.original).chain(&e.duplicates))
            .find(|r| !r.path.is_absolute());
        if let Some(record) = relative {
            return Err(invalid(format!("path {:?} is not absolute", record.path)));
        }

        Ok(file)
    }
}

/// Resolves a user-supplied target to a duplicates file path.
///
/// A directory resolves to `<dir>/duplicates.json`; anything else is taken
/// as the path of the duplicates file itself.
pub fn resolve_duplicates_path(target: &Path) -> PathBuf {
    if target.is_dir() {
        target.join(DUPLICATES_FILENAME)
    } else {
        target.to_path_buf()
    }
}
