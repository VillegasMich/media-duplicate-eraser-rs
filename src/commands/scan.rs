use std::path::{Path, PathBuf};
use std::sync::Mutex;

use console::style;
use walkdir::WalkDir;

use super::Command;
use super::erase::STAGING_DIR_NAME;
use crate::error::{Error, Result};
use crate::output::{self, display_path, human_size};
use crate::services::duplicate::{
    self, DetectionOptions, DuplicateReport, DuplicateType, MediaFilter, ProgressCallback,
};
use crate::services::hasher;
use crate::services::manifest::{DUPLICATES_FILENAME, DuplicatesFile};

pub struct Scanner {
    path: PathBuf,
    recursive: bool,
    include_hidden: bool,
    output: Option<PathBuf>,
    quiet: bool,
    options: DetectionOptions,
}

impl Scanner {
    pub fn new(
        path: PathBuf,
        recursive: bool,
        include_hidden: bool,
        output: Option<PathBuf>,
        quiet: bool,
        media_filter: MediaFilter,
    ) -> Self {
        Self {
            path,
            recursive,
            include_hidden,
            output,
            quiet,
            options: DetectionOptions {
                filter: media_filter,
                ..DetectionOptions::default()
            },
        }
    }

    /// Only report exact (byte-for-byte) duplicates, skipping the perceptual pass.
    pub fn exact_only(mut self, exact_only: bool) -> Self {
        self.options.perceptual = !exact_only;
        self
    }

    /// Sets the maximum Hamming distance for perceptual matches.
    pub fn threshold(mut self, threshold: u32) -> Self {
        self.options.threshold = threshold;
        self
    }

    /// Returns the output path for the duplicates file.
    /// If not specified, defaults to duplicates.json in the scanned directory.
    fn output_path(&self, root: &Path) -> Result<PathBuf> {
        match &self.output {
            Some(output) => Ok(std::path::absolute(output)?),
            None => Ok(root.join(DUPLICATES_FILENAME)),
        }
    }
}

impl Command for Scanner {
    fn execute(&self) -> Result<()> {
        // Absolute paths make the duplicates file valid from any working directory.
        let root = canonical_dir(&self.path)?;
        let output_path = self.output_path(&root)?;

        log::info!("Starting scan of directory: {:?}", root);
        log::debug!(
            "recursive: {}, include_hidden: {}, output: {:?}, options: {:?}",
            self.recursive,
            self.include_hidden,
            output_path,
            self.options
        );

        if self.options.perceptual
            && self.options.filter.needs_ffmpeg()
            && !hasher::is_ffmpeg_available()
        {
            if !self.quiet {
                println!(
                    "{} FFmpeg/ffprobe not found: similar videos and audio won't be detected (exact copies still will).",
                    output::warn()
                );
                println!("    Install FFmpeg, or use --media images to scan only images.");
            }
            log::info!("FFmpeg not available, video and audio perceptual hashing will be skipped");
        }

        let spinner = output::spinner(self.quiet, "Collecting files...");
        let files = list_files(&root, self.recursive, self.include_hidden, &output_path)?;
        if let Some(sp) = spinner {
            sp.finish_with_message(format!(
                "{} Found {} files",
                output::ok(),
                style(files.len()).cyan()
            ));
        }

        log::info!("Found {} files to analyze", files.len());

        if files.is_empty() {
            if !self.quiet {
                println!("{} No files found to scan.", output::info());
            }
            return Ok(());
        }

        let progress_bar = output::progress_bar(self.quiet, files.len() as u64, "Scanning...");
        let progress_callback: Option<ProgressCallback> = progress_bar.clone().map(|pb| {
            let phase_name = Mutex::new(String::new());
            Box::new(move |current: usize, total: usize, phase: &str| {
                let mut last = phase_name.lock().unwrap();
                if *last != phase {
                    pb.reset();
                    pb.set_length(total as u64);
                    pb.set_message(phase.to_string());
                    *last = phase.to_string();
                }
                pb.set_position(current as u64);
            }) as ProgressCallback
        });

        let report =
            duplicate::find_duplicates_with_options(&files, progress_callback, self.options)?;

        if let Some(pb) = progress_bar {
            pb.finish_and_clear();
        }

        if report.groups.is_empty() {
            print_report(&report, &root, self.quiet);
            // A duplicates file from an earlier scan no longer describes this
            // directory; leaving it would let `erase` act on stale data.
            if output_path.exists() {
                std::fs::remove_file(&output_path)?;
                if !self.quiet {
                    println!(
                        "{} Removed outdated {}",
                        output::info(),
                        style(output_path.display()).cyan()
                    );
                }
            }
            return Ok(());
        }

        let duplicates_file = DuplicatesFile::from_report(&report, &root);
        duplicates_file.save(&output_path)?;

        print_report(&report, &root, self.quiet);

        if !self.quiet {
            println!(
                "{} Duplicates saved to: {}",
                output::ok(),
                style(output_path.display()).cyan()
            );
            let erase_target = if self.output.is_some() {
                &output_path
            } else {
                &root
            };
            println!(
                "    Review the list (edit the JSON to keep files), then run: {}",
                style(format!("mde erase \"{}\"", erase_target.display())).bold()
            );
        }

        Ok(())
    }
}

fn print_report(report: &DuplicateReport, root: &Path, quiet: bool) {
    if quiet {
        return;
    }

    println!();
    println!(
        "{}",
        style("=== Duplicate Detection Report ===").bold().cyan()
    );
    println!(
        "Scanned:  {} files in {}",
        style(report.total_files).cyan(),
        root.display()
    );
    if report.empty_files > 0 {
        println!("Ignored:  {} empty files", report.empty_files);
    }
    if report.errors > 0 {
        println!(
            "{} {} files could not be read (run with -v for details)",
            output::warn(),
            style(report.errors).yellow()
        );
    }
    println!();

    if report.groups.is_empty() {
        println!("{} No duplicates found.", output::ok());
        return;
    }

    for (i, group) in report.groups.iter().enumerate() {
        let type_label = match group.duplicate_type {
            DuplicateType::Exact => style("EXACT").cyan().bold(),
            DuplicateType::Perceptual => style("SIMILAR").yellow().bold(),
        };

        println!(
            "{} {} {}",
            style(format!("Group {}", i + 1)).bold(),
            type_label,
            style(format!("({} files)", group.files.len())).dim()
        );
        println!(
            "  {} {}  {}",
            style("keep  ").green().bold(),
            display_path(group.original(), root),
            style(human_size(report.size_of(group.original()))).dim()
        );
        for file in group.duplicates() {
            println!(
                "  {} {}  {}",
                style("delete").red(),
                display_path(file, root),
                style(human_size(report.size_of(file))).dim()
            );
        }
        println!();
    }

    println!(
        "{} {} duplicate groups: {} files can be removed ({} exact, {} similar), freeing {}",
        style("Summary:").bold(),
        style(report.groups.len()).cyan().bold(),
        style(report.duplicate_count()).cyan().bold(),
        report.exact_duplicate_count(),
        report.perceptual_duplicate_count(),
        style(human_size(report.reclaimable_bytes())).green().bold()
    );
    if report.perceptual_duplicate_count() > 0 {
        println!(
            "    {} SIMILAR files are not identical. Check them before erasing, or rescan with --exact-only.",
            output::warn()
        );
    }
}

// Utils

/// Resolves the scan root to an absolute, canonical directory path.
fn canonical_dir(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(Error::PathNotFound(path.to_path_buf()));
    }
    let root = std::fs::canonicalize(path)?;
    if !root.is_dir() {
        return Err(Error::InvalidPath {
            path: path.to_path_buf(),
            reason: "not a directory".to_string(),
        });
    }
    Ok(root)
}

fn list_files(
    root: &Path,
    recursive: bool,
    include_hidden: bool,
    output_path: &Path,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let walker = if recursive {
        WalkDir::new(root)
    } else {
        WalkDir::new(root).max_depth(1)
    };

    let walker = walker.into_iter().filter_entry(|e| {
        e.depth() == 0 || (e.file_name() != STAGING_DIR_NAME && (include_hidden || !is_hidden(e)))
    });

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            // An unreadable subdirectory shouldn't abort the whole scan.
            Err(e) if e.depth() > 0 => {
                log::warn!("Skipping unreadable entry: {}", e);
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        if entry.file_type().is_file() && entry.path() != output_path {
            files.push(entry.into_path());
        }
    }

    Ok(files)
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}
