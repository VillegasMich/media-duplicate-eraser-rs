use std::fs;
use std::path::{Path, PathBuf};

use super::Command;
use crate::error::{Error, Result};
use crate::services::duplicate::DuplicatesFile;

const DUPLICATES_FILENAME: &str = "duplicates.json";
const STAGING_DIR_NAME: &str = ".mde_erase_staging";

pub struct Eraser {
    path: PathBuf,
}

impl Eraser {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns the path to the duplicates.json file.
    fn duplicates_file_path(&self) -> PathBuf {
        self.path.join(DUPLICATES_FILENAME)
    }

    /// Returns the path to the staging directory.
    fn staging_dir(&self) -> PathBuf {
        self.path.join(STAGING_DIR_NAME)
    }
}

impl Command for Eraser {
    fn execute(&self) -> Result<()> {
        let duplicates_path = self.duplicates_file_path();

        log::info!("Looking for duplicates file at: {:?}", duplicates_path);

        if !duplicates_path.exists() {
            println!(
                "No duplicates.json found in: {}\nRun 'mde scan' first to detect duplicates.",
                self.path.display()
            );
            return Ok(());
        }

        // Load the duplicates file
        let duplicates_file = DuplicatesFile::load(&duplicates_path)?;

        if duplicates_file.entries.is_empty() {
            println!("No duplicates to erase.");
            return Ok(());
        }

        // Collect all files to delete
        let files_to_delete: Vec<PathBuf> = duplicates_file
            .entries
            .iter()
            .flat_map(|entry| entry.duplicates.clone())
            .collect();

        if files_to_delete.is_empty() {
            println!("No duplicate files to erase.");
            return Ok(());
        }

        println!(
            "Found {} duplicate files to erase from {} groups.",
            files_to_delete.len(),
            duplicates_file.entries.len()
        );

        // Verify all files exist before starting
        let missing_files: Vec<&PathBuf> = files_to_delete
            .iter()
            .filter(|f| !f.exists())
            .collect();

        if !missing_files.is_empty() {
            log::warn!("Some files no longer exist: {:?}", missing_files);
            println!(
                "Warning: {} files no longer exist and will be skipped.",
                missing_files.len()
            );
        }

        let existing_files: Vec<&PathBuf> = files_to_delete
            .iter()
            .filter(|f| f.exists())
            .collect();

        if existing_files.is_empty() {
            println!("No existing files to erase.");
            return Ok(());
        }

        // Perform atomic deletion
        match atomic_delete(&existing_files, &self.staging_dir()) {
            Ok(deleted_count) => {
                println!("Successfully erased {} duplicate files.", deleted_count);

                // Remove the duplicates.json file after successful deletion
                fs::remove_file(&duplicates_path)?;
                println!("Removed: {}", duplicates_path.display());

                log::info!(
                    "Erase complete: {} files deleted, duplicates.json removed",
                    deleted_count
                );
            }
            Err(e) => {
                log::error!("Erase failed, all files restored: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}

/// Atomically deletes files by first moving them to a staging directory,
/// then permanently deleting them. If any operation fails, all files are restored.
fn atomic_delete(files: &[&PathBuf], staging_dir: &Path) -> Result<usize> {
    // Clean up any leftover staging directory from previous failed runs
    if staging_dir.exists() {
        log::warn!("Found leftover staging directory, cleaning up...");
        fs::remove_dir_all(staging_dir)?;
    }

    // Create staging directory
    fs::create_dir_all(staging_dir)?;
    log::debug!("Created staging directory: {:?}", staging_dir);

    // Track moved files for potential rollback
    let mut moved_files: Vec<(PathBuf, PathBuf)> = Vec::new();

    // Phase 1: Move all files to staging
    for (index, file) in files.iter().enumerate() {
        let staged_path = staging_dir.join(format!("{}", index));

        match fs::rename(file, &staged_path) {
            Ok(()) => {
                log::debug!("Staged: {:?} -> {:?}", file, staged_path);
                moved_files.push(((*file).clone(), staged_path));
            }
            Err(e) => {
                log::error!("Failed to stage {:?}: {}", file, e);

                // Rollback: restore all moved files
                rollback(&moved_files)?;

                // Clean up staging directory
                if staging_dir.exists() {
                    let _ = fs::remove_dir_all(staging_dir);
                }

                return Err(Error::Io(e));
            }
        }
    }

    // Phase 2: All files staged successfully, now permanently delete
    let deleted_count = moved_files.len();

    match fs::remove_dir_all(staging_dir) {
        Ok(()) => {
            log::info!("Permanently deleted {} files", deleted_count);
            Ok(deleted_count)
        }
        Err(e) => {
            log::error!("Failed to delete staging directory: {}", e);

            // Try to rollback
            rollback(&moved_files)?;

            Err(Error::Io(e))
        }
    }
}

/// Restores files from staging back to their original locations.
fn rollback(moved_files: &[(PathBuf, PathBuf)]) -> Result<()> {
    log::warn!("Rolling back {} files...", moved_files.len());

    for (original_path, staged_path) in moved_files {
        if staged_path.exists() {
            match fs::rename(staged_path, original_path) {
                Ok(()) => {
                    log::debug!("Restored: {:?}", original_path);
                }
                Err(e) => {
                    log::error!(
                        "Failed to restore {:?} from {:?}: {}",
                        original_path,
                        staged_path,
                        e
                    );
                    // Continue trying to restore other files
                }
            }
        }
    }

    println!("Rollback complete. No files were deleted.");
    Ok(())
}
