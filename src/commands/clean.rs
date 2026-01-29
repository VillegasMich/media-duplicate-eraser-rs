use std::path::PathBuf;

use super::Command;
use crate::error::Result;

const DUPLICATES_FILENAME: &str = "duplicates.json";

pub struct Cleaner {
    path: PathBuf,
}

impl Cleaner {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Command for Cleaner {
    fn execute(&self) -> Result<()> {
        let duplicates_file = self.path.join(DUPLICATES_FILENAME);

        log::debug!("Looking for duplicates file at: {:?}", duplicates_file);

        if duplicates_file.exists() {
            std::fs::remove_file(&duplicates_file)?;
            println!("Removed: {}", duplicates_file.display());
            log::info!("Duplicates file removed: {:?}", duplicates_file);
        } else {
            println!("No duplicates.json found in: {}", self.path.display());
            log::debug!("Duplicates file not found at: {:?}", duplicates_file);
        }

        Ok(())
    }
}
