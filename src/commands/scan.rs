use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;

use super::Command;
use crate::error::{Error, Result};
use crate::services::duplicate::{self, DuplicateType, DuplicatesFile, MediaFilter, ProgressCallback};
use crate::services::hasher;

const DEFAULT_OUTPUT_FILENAME: &str = "duplicates.json";

// Styled output prefixes (Classic ASCII)
const SUCCESS_PREFIX: &str = "[OK]";
const INFO_PREFIX: &str = "[*]";
const WARNING_PREFIX: &str = "[!]";

pub struct Scanner {
    path: PathBuf,
    recursive: bool,
    include_hidden: bool,
    output: Option<PathBuf>,
    quiet: bool,
    media_filter: MediaFilter,
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
            media_filter,
        }
    }

    /// Returns the output path for the duplicates file.
    /// If not specified, defaults to duplicates.json in the scanned directory.
    fn output_path(&self) -> PathBuf {
        self.output
            .clone()
            .unwrap_or_else(|| self.path.join(DEFAULT_OUTPUT_FILENAME))
    }
}

impl Command for Scanner {
    fn execute(&self) -> Result<()> {
        log::info!("Starting scan of directory: {:?}", self.path);
        log::debug!(
            "Path: {:?}, recursive: {}, include_hidden: {}, output: {:?}, media_filter: {:?}",
            self.path,
            self.recursive,
            self.include_hidden,
            self.output,
            self.media_filter
        );

        // Check if FFmpeg is available for video/audio processing
        let has_ffmpeg = hasher::is_ffmpeg_available();
        let needs_ffmpeg = self.media_filter == MediaFilter::All
            || self.media_filter == MediaFilter::VideosOnly
            || self.media_filter == MediaFilter::AudioOnly;
        if !has_ffmpeg && needs_ffmpeg {
            if !self.quiet {
                println!(
                    "{} FFmpeg not found. Video and audio perceptual hashing disabled.",
                    style(WARNING_PREFIX).yellow().bold()
                );
                println!(
                    "   Install FFmpeg or run with --media images to scan only images."
                );
            }
            log::warn!("FFmpeg not available, video and audio perceptual hashing will be skipped");
        }

        // Spinner for file collection
        let spinner = if !self.quiet {
            let sp = ProgressBar::new_spinner();
            sp.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}")
                    .unwrap(),
            );
            sp.set_message("Collecting files...");
            sp.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(sp)
        } else {
            None
        };

        let files = list_files(&self.path, self.recursive, self.include_hidden)?;

        if let Some(sp) = spinner {
            sp.finish_with_message(format!(
                "{} Found {} files",
                style(SUCCESS_PREFIX).green().bold(),
                style(files.len()).cyan()
            ));
        }

        log::info!("Found {} files to analyze", files.len());

        if files.is_empty() {
            if !self.quiet {
                println!(
                    "{} No files found to scan.",
                    style(INFO_PREFIX).blue().bold()
                );
            }
            return Ok(());
        }

        // Progress bar for duplicate detection
        let progress_bar = if !self.quiet {
            let pb = ProgressBar::new(files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=>-"),
            );
            pb.set_message("Scanning...");
            Some(pb)
        } else {
            None
        };

        // Create progress callback
        let progress_callback: Option<ProgressCallback> = if let Some(ref pb) = progress_bar {
            let pb_clone = pb.clone();
            let last_msg = Arc::new(Mutex::new(String::new()));
            Some(Box::new(move |current, _total, phase| {
                pb_clone.set_position(current as u64);
                let mut last = last_msg.lock().unwrap();
                if *last != phase {
                    pb_clone.set_message(phase.to_string());
                    *last = phase.to_string();
                }
            }))
        } else {
            None
        };

        let report = duplicate::find_duplicates_with_options(&files, progress_callback, self.media_filter)?;

        if let Some(pb) = progress_bar {
            pb.finish_and_clear();
        }

        print_report(&report, self.quiet);

        // Save duplicates file if there are duplicates
        if !report.groups.is_empty() {
            let output_path = self.output_path();
            let duplicates_file = DuplicatesFile::from_report(&report);
            duplicates_file.save(&output_path)?;
            if !self.quiet {
                println!(
                    "{} Duplicates saved to: {}",
                    style(SUCCESS_PREFIX).green().bold(),
                    style(output_path.display()).cyan()
                );
            }
        }

        Ok(())
    }
}

fn print_report(report: &duplicate::DuplicateReport, quiet: bool) {
    if quiet {
        return;
    }

    println!();
    println!(
        "{}",
        style("=== Duplicate Detection Report ===").bold().cyan()
    );
    println!(
        "Total files scanned: {}",
        style(report.total_files).cyan()
    );
    println!("Errors encountered: {}", report.errors);
    println!();

    if report.groups.is_empty() {
        println!(
            "{} No duplicates found.",
            style(SUCCESS_PREFIX).green().bold()
        );
        return;
    }

    let exact_count = report.exact_duplicate_count();
    let perceptual_count = report.perceptual_duplicate_count();
    let exact_groups = report
        .groups
        .iter()
        .filter(|g| g.duplicate_type == DuplicateType::Exact)
        .count();
    let perceptual_groups = report
        .groups
        .iter()
        .filter(|g| g.duplicate_type == DuplicateType::Perceptual)
        .count();

    println!(
        "Found {} duplicate groups ({} exact, {} perceptual)",
        style(report.groups.len()).cyan().bold(),
        style(exact_groups).cyan(),
        style(perceptual_groups).yellow()
    );
    println!(
        "Total duplicate files: {} ({} exact, {} perceptual)",
        style(report.duplicate_count()).cyan().bold(),
        style(exact_count).cyan(),
        style(perceptual_count).yellow()
    );
    println!();

    for (i, group) in report.groups.iter().enumerate() {
        let type_label = match group.duplicate_type {
            DuplicateType::Exact => style("[EXACT]").cyan().bold(),
            DuplicateType::Perceptual => style("[SIMILAR]").yellow().bold(),
        };

        println!(
            "Group {} {} - {} files:",
            style(i + 1).bold(),
            type_label,
            style(group.files.len()).bold()
        );
        for file in &group.files {
            println!("  {}", file.display());
        }
        println!();
    }
}

// Utils

fn list_files(path: &PathBuf, recursive: bool, include_hidden: bool) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Err(Error::PathNotFound(path.clone()));
    }

    let mut files = Vec::new();

    let walker = if recursive {
        WalkDir::new(path)
    } else {
        WalkDir::new(path).max_depth(1)
    };

    let walker = walker
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || include_hidden || !is_hidden(e));

    for entry in walker {
        let entry = entry?;

        if entry.file_type().is_file() {
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
