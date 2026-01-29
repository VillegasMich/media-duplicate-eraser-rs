use std::path::PathBuf;

use console::style;

use super::Command;
use crate::error::Result;

const DUPLICATES_FILENAME: &str = "duplicates.json";

// Styled output prefixes (Classic ASCII)
const SUCCESS_PREFIX: &str = "[OK]";
const INFO_PREFIX: &str = "[*]";

pub struct Cleaner {
    path: PathBuf,
    quiet: bool,
}

impl Cleaner {
    pub fn new(path: PathBuf, quiet: bool) -> Self {
        Self { path, quiet }
    }
}

impl Command for Cleaner {
    fn execute(&self) -> Result<()> {
        let duplicates_file = self.path.join(DUPLICATES_FILENAME);

        log::debug!("Looking for duplicates file at: {:?}", duplicates_file);

        if duplicates_file.exists() {
            std::fs::remove_file(&duplicates_file)?;
            if !self.quiet {
                println!(
                    "{} Removed: {}",
                    style(SUCCESS_PREFIX).green().bold(),
                    style(duplicates_file.display()).cyan()
                );
            }
            log::info!("Duplicates file removed: {:?}", duplicates_file);
        } else {
            if !self.quiet {
                println!(
                    "{} No duplicates.json found in: {}",
                    style(INFO_PREFIX).blue().bold(),
                    style(self.path.display()).cyan()
                );
            }
            log::debug!("Duplicates file not found at: {:?}", duplicates_file);
        }

        Ok(())
    }
}
