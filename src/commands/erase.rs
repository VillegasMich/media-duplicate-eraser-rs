use std::collections::HashSet;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use console::style;

use super::Command;
use crate::error::{Error, Result};
use crate::output::{self, display_path, human_size};
use crate::services::duplicate::DuplicateType;
use crate::services::hasher;
use crate::services::manifest::{DuplicatesFile, FileRecord, resolve_duplicates_path};

/// Name of the directory files are moved into before being deleted.
pub const STAGING_DIR_NAME: &str = ".mde_erase_staging";
/// File inside the staging directory mapping staged names back to original paths.
const RESTORE_FILENAME: &str = "restore.tsv";

pub struct Eraser {
    path: PathBuf,
    quiet: bool,
    dry_run: bool,
    assume_yes: bool,
}

/// Why a listed duplicate is not going to be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipReason {
    OriginalMissing,
    Missing,
    Changed,
    IsAnOriginal,
}

impl SkipReason {
    fn describe(self) -> &'static str {
        match self {
            SkipReason::OriginalMissing => "the file it duplicates is missing or changed",
            SkipReason::Missing => "no longer exists",
            SkipReason::Changed => "changed since the scan",
            SkipReason::IsAnOriginal => "is also listed as a file to keep",
        }
    }
}

/// The verified result of comparing the duplicates file with the disk.
struct ErasePlan {
    delete: Vec<FileRecord>,
    skipped: Vec<(PathBuf, SkipReason)>,
}

impl Eraser {
    /// Creates an eraser for `path`, which is either a directory containing
    /// `duplicates.json` or the path of a duplicates file.
    ///
    /// By default it asks for confirmation before deleting; see [`Eraser::assume_yes`].
    pub fn new(path: PathBuf, quiet: bool) -> Self {
        Self {
            path,
            quiet,
            dry_run: false,
            assume_yes: false,
        }
    }

    /// Only show what would be deleted, without deleting anything.
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Skip the confirmation prompt.
    pub fn assume_yes(mut self, assume_yes: bool) -> Self {
        self.assume_yes = assume_yes;
        self
    }

    fn say(&self, msg: impl std::fmt::Display) {
        if !self.quiet {
            println!("{}", msg);
        }
    }
}

impl Command for Eraser {
    fn execute(&self) -> Result<()> {
        let duplicates_path = resolve_duplicates_path(&self.path);

        log::info!("Looking for duplicates file at: {:?}", duplicates_path);

        if !duplicates_path.exists() {
            self.say(format!(
                "{} No duplicates file found at: {}\n    Run 'mde scan' first to detect duplicates.",
                output::info(),
                style(duplicates_path.display()).cyan()
            ));
            return Ok(());
        }

        let duplicates_file = DuplicatesFile::load(&duplicates_path)?;

        let spinner = output::spinner(self.quiet, "Verifying files against the scan...");
        let plan = verify(&duplicates_file);
        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }

        for (path, reason) in &plan.skipped {
            self.say(format!(
                "{} Keeping {} ({})",
                output::warn(),
                style(display_path(path, &duplicates_file.root)).yellow(),
                reason.describe()
            ));
        }

        if plan.delete.is_empty() {
            self.say(format!("{} Nothing to erase.", output::info()));
            return Ok(());
        }

        let total_bytes: u64 = plan.delete.iter().map(|r| r.size).sum();

        if self.dry_run {
            for record in &plan.delete {
                self.say(format!(
                    "  {} {}",
                    style("would delete").red(),
                    display_path(&record.path, &duplicates_file.root)
                ));
            }
            self.say(format!(
                "{} Dry run: {} files ({}) would be erased. Nothing was deleted.",
                output::info(),
                style(plan.delete.len()).cyan().bold(),
                human_size(total_bytes)
            ));
            return Ok(());
        }

        if !self.assume_yes && !confirm(plan.delete.len(), total_bytes)? {
            self.say(format!("{} Aborted. Nothing was deleted.", output::info()));
            return Ok(());
        }

        // Staging inside the scanned root keeps renames on the same filesystem
        // as the files, even when the duplicates file was saved elsewhere.
        let staging_dir = duplicates_file.root.join(STAGING_DIR_NAME);
        let paths: Vec<&Path> = plan.delete.iter().map(|r| r.path.as_path()).collect();
        let deleted_count = atomic_delete(&paths, &staging_dir, self.quiet)?;

        self.say(format!(
            "{} Erased {} duplicate files, freeing {}.",
            output::ok(),
            style(deleted_count).green().bold(),
            style(human_size(total_bytes)).green().bold()
        ));

        // The duplicates file describes a state that no longer exists.
        fs::remove_file(&duplicates_path)?;
        self.say(format!(
            "{} Removed: {}",
            output::ok(),
            style(duplicates_path.display()).cyan()
        ));
        if !plan.skipped.is_empty() {
            self.say(format!(
                "{} {} listed files were kept. Re-run 'mde scan' to re-check them.",
                output::info(),
                plan.skipped.len()
            ));
        }

        log::info!("Erase complete: {} files deleted", deleted_count);
        Ok(())
    }
}

/// Checks every entry against the disk and decides what can safely be deleted.
///
/// A duplicate is only deleted when its original still exists unchanged, the
/// duplicate itself is unchanged since the scan, and, for exact duplicates,
/// both files still have identical content.
fn verify(file: &DuplicatesFile) -> ErasePlan {
    let originals: HashSet<&Path> = file
        .entries
        .iter()
        .map(|e| e.original.path.as_path())
        .collect();
    let mut seen: HashSet<&Path> = HashSet::new();
    let mut plan = ErasePlan {
        delete: Vec::new(),
        skipped: Vec::new(),
    };

    for entry in &file.entries {
        let original_ok = entry.original.is_unchanged();
        let mut original_hash: Option<Option<String>> = None;

        for dup in &entry.duplicates {
            if !seen.insert(dup.path.as_path()) {
                continue;
            }
            let reason =
                if dup.path == entry.original.path || originals.contains(dup.path.as_path()) {
                    Some(SkipReason::IsAnOriginal)
                } else if !original_ok {
                    Some(SkipReason::OriginalMissing)
                } else if !dup.path.exists() {
                    Some(SkipReason::Missing)
                } else if !dup.is_unchanged() {
                    Some(SkipReason::Changed)
                } else if entry.duplicate_type == DuplicateType::Exact {
                    let original_hash = original_hash
                        .get_or_insert_with(|| hasher::sha256_hash(&entry.original.path).ok());
                    let dup_hash = hasher::sha256_hash(&dup.path).ok();
                    (original_hash.is_none() || dup_hash != *original_hash)
                        .then_some(SkipReason::Changed)
                } else {
                    None
                };

            match reason {
                Some(reason) => plan.skipped.push((dup.path.clone(), reason)),
                None => plan.delete.push(dup.clone()),
            }
        }
    }

    plan
}

/// Asks the user to confirm the deletion on the terminal.
fn confirm(count: usize, bytes: u64) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(Error::ConfirmationRequired);
    }
    print!(
        "Permanently delete {} files ({})? [y/N] ",
        style(count).red().bold(),
        human_size(bytes)
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Deletes files by first moving them all to a staging directory, then
/// permanently deleting them.
///
/// If any file cannot be moved, every file already moved is restored and
/// nothing is deleted. A `restore.tsv` inside the staging directory maps each
/// staged file back to its original path, so files can be recovered by hand
/// if the process is interrupted.
fn atomic_delete(files: &[&Path], staging_dir: &Path, quiet: bool) -> Result<usize> {
    // A leftover staging directory may hold files a previous rollback could
    // not restore; never delete it automatically.
    if staging_dir.exists() {
        return Err(Error::StagingDirExists(staging_dir.to_path_buf()));
    }

    fs::create_dir_all(staging_dir)?;
    log::debug!("Created staging directory: {:?}", staging_dir);

    let staged: Vec<(PathBuf, PathBuf)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.to_path_buf(), staging_dir.join(i.to_string())))
        .collect();

    let restore_map: String = staged
        .iter()
        .map(|(original, staged)| format!("{}\t{}\n", staged.display(), original.display()))
        .collect();
    fs::write(staging_dir.join(RESTORE_FILENAME), restore_map)?;

    // Phase 1: Move all files to staging
    let progress_bar = output::progress_bar(quiet, files.len() as u64, "Staging files...");
    for (index, (original, staged_path)) in staged.iter().enumerate() {
        if let Err(e) = fs::rename(original, staged_path) {
            if let Some(ref pb) = progress_bar {
                pb.finish_and_clear();
            }
            log::error!("Failed to stage {:?}: {}", original, e);
            if !quiet {
                println!(
                    "{} Failed to move {}: {}",
                    output::error(),
                    style(original.display()).red(),
                    e
                );
            }
            rollback(&staged[..index], staging_dir, quiet);
            return Err(Error::EraseFailed {
                reason: format!(
                    "could not move {} into {} ({}). Nothing was deleted",
                    original.display(),
                    staging_dir.display(),
                    e
                ),
            });
        }
        log::debug!("Staged: {:?} -> {:?}", original, staged_path);
        if let Some(ref pb) = progress_bar {
            pb.inc(1);
        }
    }
    if let Some(ref pb) = progress_bar {
        pb.finish_and_clear();
    }

    // Phase 2: All files staged successfully, now permanently delete
    let spinner = output::spinner(quiet, "Finalizing deletion...");
    let result = fs::remove_dir_all(staging_dir);
    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    match result {
        Ok(()) => {
            log::info!("Permanently deleted {} files", staged.len());
            Ok(staged.len())
        }
        Err(e) => {
            log::error!("Failed to delete staging directory: {}", e);
            let remaining: Vec<(PathBuf, PathBuf)> =
                staged.into_iter().filter(|(_, s)| s.exists()).collect();
            let deleted = files.len() - remaining.len();
            rollback(&remaining, staging_dir, quiet);
            Err(Error::EraseFailed {
                reason: format!(
                    "could not finish deleting ({}). {} files were deleted, {} were restored",
                    e,
                    deleted,
                    remaining.len()
                ),
            })
        }
    }
}

/// Restores files from staging back to their original locations.
///
/// The staging directory is only removed if every file was restored;
/// otherwise it is left in place with its `restore.tsv` for manual recovery.
fn rollback(moved_files: &[(PathBuf, PathBuf)], staging_dir: &Path, quiet: bool) {
    log::warn!("Rolling back {} files...", moved_files.len());
    if !quiet {
        println!(
            "{} Restoring {} files...",
            output::info(),
            style(moved_files.len()).cyan()
        );
    }

    let mut failed = 0;
    for (original_path, staged_path) in moved_files {
        if let Err(e) = fs::rename(staged_path, original_path) {
            failed += 1;
            log::error!(
                "Failed to restore {:?} from {:?}: {}",
                original_path,
                staged_path,
                e
            );
            if !quiet {
                println!(
                    "{} Failed to restore: {}",
                    output::error(),
                    style(original_path.display()).red()
                );
            }
        }
    }

    if failed == 0 {
        let _ = fs::remove_dir_all(staging_dir);
        if !quiet {
            println!("{} All files restored.", output::info());
        }
    } else if !quiet {
        println!(
            "{} {} files could not be restored and are still in {} (see {} there).",
            output::error(),
            failed,
            style(staging_dir.display()).red(),
            RESTORE_FILENAME
        );
    }
}
