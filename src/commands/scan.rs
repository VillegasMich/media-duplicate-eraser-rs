use std::path::PathBuf;

use walkdir::WalkDir;

use super::Command;
use crate::error::{Error, Result};

pub struct Scanner {
    paths: Vec<PathBuf>,
    recursive: bool,
    include_hidden: bool,
}

impl Scanner {
    pub fn new(paths: Vec<PathBuf>, recursive: bool, include_hidden: bool) -> Self {
        Self {
            paths,
            recursive,
            include_hidden,
        }
    }
}

impl Command for Scanner {
    fn execute(&self) -> Result<()> {
        log::info!("Starting scan of {} directories", self.paths.len());
        log::debug!(
            "Paths: {:?}, recursive: {}, include_hidden: {}",
            self.paths,
            self.recursive,
            self.include_hidden
        );

        let files = list_files(&self.paths, self.recursive, self.include_hidden)?;

        log::info!("Found {} files", files.len());
        for file in &files {
            println!("{}", file.display());
        }

        Ok(())
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
