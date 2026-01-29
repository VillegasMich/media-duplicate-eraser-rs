use std::path::PathBuf;

use walkdir::WalkDir;

use super::Command;
use crate::error::{Error, Result};
use crate::services::duplicate::{self, DuplicateType, DuplicatesFile};

const DEFAULT_OUTPUT_FILENAME: &str = "duplicates.json";

pub struct Scanner {
    path: PathBuf,
    recursive: bool,
    include_hidden: bool,
    output: Option<PathBuf>,
}

impl Scanner {
    pub fn new(
        path: PathBuf,
        recursive: bool,
        include_hidden: bool,
        output: Option<PathBuf>,
    ) -> Self {
        Self {
            path,
            recursive,
            include_hidden,
            output,
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
            "Path: {:?}, recursive: {}, include_hidden: {}, output: {:?}",
            self.path,
            self.recursive,
            self.include_hidden,
            self.output
        );

        let files = list_files(&self.path, self.recursive, self.include_hidden)?;
        log::info!("Found {} files to analyze", files.len());

        if files.is_empty() {
            println!("No files found to scan.");
            return Ok(());
        }

        println!("Scanning {} files for duplicates...", files.len());

        let report = duplicate::find_duplicates(&files)?;

        print_report(&report);

        // Save duplicates file if there are duplicates
        if !report.groups.is_empty() {
            let output_path = self.output_path();
            let duplicates_file = DuplicatesFile::from_report(&report);
            duplicates_file.save(&output_path)?;
            println!("Duplicates saved to: {}", output_path.display());
        }

        Ok(())
    }
}

fn print_report(report: &duplicate::DuplicateReport) {
    println!();
    println!("=== Duplicate Detection Report ===");
    println!("Total files scanned: {}", report.total_files);
    println!("Errors encountered: {}", report.errors);
    println!();

    if report.groups.is_empty() {
        println!("No duplicates found.");
        return;
    }

    let exact_count = report.exact_duplicate_count();
    let perceptual_count = report.perceptual_duplicate_count();

    println!(
        "Found {} duplicate groups ({} exact, {} perceptual)",
        report.groups.len(),
        report
            .groups
            .iter()
            .filter(|g| g.duplicate_type == DuplicateType::Exact)
            .count(),
        report
            .groups
            .iter()
            .filter(|g| g.duplicate_type == DuplicateType::Perceptual)
            .count()
    );
    println!(
        "Total duplicate files: {} ({} exact, {} perceptual)",
        report.duplicate_count(),
        exact_count,
        perceptual_count
    );
    println!();

    for (i, group) in report.groups.iter().enumerate() {
        let type_label = match group.duplicate_type {
            DuplicateType::Exact => "EXACT",
            DuplicateType::Perceptual => "SIMILAR",
        };

        println!(
            "Group {} [{}] - {} files:",
            i + 1,
            type_label,
            group.files.len()
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
