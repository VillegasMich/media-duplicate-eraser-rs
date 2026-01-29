use std::fs;
use std::path::{Path, PathBuf};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use super::Command;
use crate::error::{Error, Result};
use crate::services::duplicate::DuplicatesFile;

const DUPLICATES_FILENAME: &str = "duplicates.json";
const STAGING_DIR_NAME: &str = ".mde_erase_staging";

// Styled output prefixes (Classic ASCII)
const SUCCESS_PREFIX: &str = "[OK]";
const WARNING_PREFIX: &str = "[!]";
const ERROR_PREFIX: &str = "[X]";
const INFO_PREFIX: &str = "[*]";

pub struct Eraser {
    path: PathBuf,
    quiet: bool,
}

impl Eraser {
    pub fn new(path: PathBuf, quiet: bool) -> Self {
        Self { path, quiet }
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
            if !self.quiet {
                println!(
                    "{} No duplicates.json found in: {}\n   Run 'mde scan' first to detect duplicates.",
                    style(INFO_PREFIX).blue().bold(),
                    style(self.path.display()).cyan()
                );
            }
            return Ok(());
        }

        // Load the duplicates file
        let duplicates_file = DuplicatesFile::load(&duplicates_path)?;

        if duplicates_file.entries.is_empty() {
            if !self.quiet {
                println!(
                    "{} No duplicates to erase.",
                    style(INFO_PREFIX).blue().bold()
                );
            }
            return Ok(());
        }

        // Collect all files to delete
        let files_to_delete: Vec<PathBuf> = duplicates_file
            .entries
            .iter()
            .flat_map(|entry| entry.duplicates.clone())
            .collect();

        if files_to_delete.is_empty() {
            if !self.quiet {
                println!(
                    "{} No duplicate files to erase.",
                    style(INFO_PREFIX).blue().bold()
                );
            }
            return Ok(());
        }

        if !self.quiet {
            println!(
                "{} Found {} duplicate files to erase from {} groups.",
                style(INFO_PREFIX).blue().bold(),
                style(files_to_delete.len()).cyan().bold(),
                style(duplicates_file.entries.len()).cyan()
            );
        }

        // Verify all files exist - show spinner while checking
        let spinner = if !self.quiet {
            let sp = ProgressBar::new_spinner();
            sp.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}")
                    .unwrap(),
            );
            sp.set_message("Validating files...");
            sp.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(sp)
        } else {
            None
        };

        let missing_files: Vec<&PathBuf> = files_to_delete
            .iter()
            .filter(|f| !f.exists())
            .collect();

        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }

        if !missing_files.is_empty() {
            log::warn!("Some files no longer exist: {:?}", missing_files);
            if !self.quiet {
                println!(
                    "{} {} files no longer exist and will be skipped.",
                    style(WARNING_PREFIX).yellow().bold(),
                    style(missing_files.len()).yellow()
                );
            }
        }

        let existing_files: Vec<&PathBuf> = files_to_delete
            .iter()
            .filter(|f| f.exists())
            .collect();

        if existing_files.is_empty() {
            if !self.quiet {
                println!(
                    "{} No existing files to erase.",
                    style(INFO_PREFIX).blue().bold()
                );
            }
            return Ok(());
        }

        // Perform atomic deletion
        match atomic_delete(&existing_files, &self.staging_dir(), self.quiet) {
            Ok(deleted_count) => {
                if !self.quiet {
                    println!(
                        "{} Successfully erased {} duplicate files.",
                        style(SUCCESS_PREFIX).green().bold(),
                        style(deleted_count).green().bold()
                    );
                }

                // Remove the duplicates.json file after successful deletion
                fs::remove_file(&duplicates_path)?;
                if !self.quiet {
                    println!(
                        "{} Removed: {}",
                        style(SUCCESS_PREFIX).green().bold(),
                        style(duplicates_path.display()).cyan()
                    );
                }

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
fn atomic_delete(files: &[&PathBuf], staging_dir: &Path, quiet: bool) -> Result<usize> {
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

    // Progress bar for Phase 1: Staging files
    let progress_bar = if !quiet {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );
        pb.set_message("Staging files...");
        Some(pb)
    } else {
        None
    };

    // Phase 1: Move all files to staging
    for (index, file) in files.iter().enumerate() {
        let staged_path = staging_dir.join(format!("{}", index));

        match fs::rename(file, &staged_path) {
            Ok(()) => {
                log::debug!("Staged: {:?} -> {:?}", file, staged_path);
                moved_files.push(((*file).clone(), staged_path));
                if let Some(ref pb) = progress_bar {
                    pb.set_position((index + 1) as u64);
                }
            }
            Err(e) => {
                log::error!("Failed to stage {:?}: {}", file, e);

                if let Some(ref pb) = progress_bar {
                    pb.finish_and_clear();
                }

                if !quiet {
                    println!(
                        "{} Failed to stage: {}",
                        style(ERROR_PREFIX).red().bold(),
                        style(file.display()).red()
                    );
                }

                // Rollback: restore all moved files
                rollback(&moved_files, quiet)?;

                // Clean up staging directory
                if staging_dir.exists() {
                    let _ = fs::remove_dir_all(staging_dir);
                }

                return Err(Error::Io(e));
            }
        }
    }

    if let Some(ref pb) = progress_bar {
        pb.finish_and_clear();
    }

    // Phase 2: All files staged successfully, now permanently delete
    // Show spinner during final deletion
    let spinner = if !quiet {
        let sp = ProgressBar::new_spinner();
        sp.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        sp.set_message("Finalizing deletion...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
    } else {
        None
    };

    let deleted_count = moved_files.len();

    match fs::remove_dir_all(staging_dir) {
        Ok(()) => {
            if let Some(sp) = spinner {
                sp.finish_and_clear();
            }
            log::info!("Permanently deleted {} files", deleted_count);
            Ok(deleted_count)
        }
        Err(e) => {
            if let Some(sp) = spinner {
                sp.finish_and_clear();
            }
            log::error!("Failed to delete staging directory: {}", e);

            if !quiet {
                println!(
                    "{} Failed to finalize deletion, rolling back...",
                    style(ERROR_PREFIX).red().bold()
                );
            }

            // Try to rollback
            rollback(&moved_files, quiet)?;

            Err(Error::Io(e))
        }
    }
}

/// Restores files from staging back to their original locations.
fn rollback(moved_files: &[(PathBuf, PathBuf)], quiet: bool) -> Result<()> {
    log::warn!("Rolling back {} files...", moved_files.len());

    if !quiet {
        println!(
            "{} Rolling back {} files...",
            style(INFO_PREFIX).cyan().bold(),
            style(moved_files.len()).cyan()
        );
    }

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
                    if !quiet {
                        println!(
                            "{} Failed to restore: {}",
                            style(ERROR_PREFIX).red().bold(),
                            style(original_path.display()).red()
                        );
                    }
                    // Continue trying to restore other files
                }
            }
        }
    }

    if !quiet {
        println!(
            "{} Rollback complete. No files were deleted.",
            style(INFO_PREFIX).cyan().bold()
        );
    }
    Ok(())
}
