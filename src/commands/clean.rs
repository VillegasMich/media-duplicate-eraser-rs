use std::path::PathBuf;

use console::style;

use super::Command;
use crate::error::Result;
use crate::output;
use crate::services::manifest::resolve_duplicates_path;

pub struct Cleaner {
    path: PathBuf,
    quiet: bool,
}

impl Cleaner {
    /// Creates a cleaner for `path`, which is either a directory containing
    /// `duplicates.json` or the path of a duplicates file.
    pub fn new(path: PathBuf, quiet: bool) -> Self {
        Self { path, quiet }
    }
}

impl Command for Cleaner {
    fn execute(&self) -> Result<()> {
        let duplicates_file = resolve_duplicates_path(&self.path);

        log::debug!("Looking for duplicates file at: {:?}", duplicates_file);

        if duplicates_file.is_file() {
            std::fs::remove_file(&duplicates_file)?;
            if !self.quiet {
                println!(
                    "{} Removed: {}",
                    output::ok(),
                    style(duplicates_file.display()).cyan()
                );
            }
            log::info!("Duplicates file removed: {:?}", duplicates_file);
        } else {
            if !self.quiet {
                println!(
                    "{} No duplicates file found at: {}",
                    output::info(),
                    style(duplicates_file.display()).cyan()
                );
            }
            log::debug!("Duplicates file not found at: {:?}", duplicates_file);
        }

        Ok(())
    }
}
