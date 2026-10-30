//! Shared helpers for terminal output: message prefixes, progress bars,
//! and human-friendly formatting of sizes and paths.

use std::path::Path;
use std::time::Duration;

use console::{StyledObject, style};
use indicatif::{ProgressBar, ProgressStyle};

pub fn ok() -> StyledObject<&'static str> {
    style("[OK]").green().bold()
}

pub fn info() -> StyledObject<&'static str> {
    style("[*]").blue().bold()
}

pub fn warn() -> StyledObject<&'static str> {
    style("[!]").yellow().bold()
}

pub fn error() -> StyledObject<&'static str> {
    style("[X]").red().bold()
}

/// Creates a spinner with the given message, or `None` in quiet mode.
pub fn spinner(quiet: bool, msg: &str) -> Option<ProgressBar> {
    if quiet {
        return None;
    }
    let sp = ProgressBar::new_spinner();
    sp.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("valid template"),
    );
    sp.set_message(msg.to_string());
    sp.enable_steady_tick(Duration::from_millis(100));
    Some(sp)
}

/// Creates a progress bar of the given length, or `None` in quiet mode.
pub fn progress_bar(quiet: bool, len: u64, msg: &str) -> Option<ProgressBar> {
    if quiet {
        return None;
    }
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg} ({eta})")
            .expect("valid template")
            .progress_chars("=>-"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    Some(pb)
}

/// Formats a byte count using binary units (e.g. `1.5 MiB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Displays `path` relative to `root` when possible, for shorter output.
pub fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_uses_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn display_path_strips_root() {
        let root = Path::new("/photos");
        assert_eq!(display_path(Path::new("/photos/a/b.jpg"), root), "a/b.jpg");
        assert_eq!(
            display_path(Path::new("/other/b.jpg"), root),
            "/other/b.jpg"
        );
    }
}
