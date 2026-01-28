use std::path::PathBuf;

use walkdir::WalkDir;

use super::Command;
use crate::error::{Error, Result};
use crate::services::duplicate::{self, DuplicateType, DuplicatesFile};

pub struct Scanner {
    paths: Vec<PathBuf>,
    recursive: bool,
    include_hidden: bool,
    output: PathBuf,
}

impl Scanner {
    pub fn new(paths: Vec<PathBuf>, recursive: bool, include_hidden: bool, output: PathBuf) -> Self {
        Self {
            paths,
            recursive,
            include_hidden,
            output,
        }
    }
}

impl Command for Scanner {
    fn execute(&self) -> Result<()> {
        log::info!("Starting scan of {} directories", self.paths.len());
        log::debug!(
            "Paths: {:?}, recursive: {}, include_hidden: {}, output: {:?}",
            self.paths,
            self.recursive,
            self.include_hidden,
            self.output
        );

        let files = list_files(&self.paths, self.recursive, self.include_hidden)?;
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
            let duplicates_file = DuplicatesFile::from_report(&report);
            duplicates_file.save(&self.output)?;
            println!("Duplicates saved to: {}", self.output.display());
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

fn list_files(
    paths: &[PathBuf],
    recursive: bool,
    include_hidden: bool,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        if !path.exists() {
            return Err(Error::PathNotFound(path.clone()));
        }

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
