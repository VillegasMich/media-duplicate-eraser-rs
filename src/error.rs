//! Error types for the media-duplicate-eraser application.
//!
//! This module provides a unified error type [`Error`] and a convenient
//! [`Result`] type alias used throughout the application.

use std::path::PathBuf;

/// A type alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// The main error type for the application.
///
/// This enum represents all possible errors that can occur during
/// the execution of the program.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred.
    ///
    /// This variant wraps [`std::io::Error`] and is automatically
    /// converted via the `#[from]` attribute.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error occurred while traversing directories.
    ///
    /// This variant wraps [`walkdir::Error`] which can occur when
    /// walking directory trees.
    #[error("Directory traversal error: {0}")]
    WalkDir(#[from] walkdir::Error),

    /// An error occurred while parsing or writing JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The specified path does not exist.
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    /// The specified path is invalid.
    #[error("Invalid path: {path} - {reason}")]
    InvalidPath {
        /// The path that was invalid.
        path: PathBuf,
        /// The reason why the path is invalid.
        reason: String,
    },

    /// The duplicates file cannot be used by this version of the tool.
    #[error("Unusable duplicates file {path}: {reason}. Re-run 'mde scan' to regenerate it.")]
    InvalidDuplicatesFile {
        /// The duplicates file that was rejected.
        path: PathBuf,
        /// Why the file was rejected.
        reason: String,
    },

    /// A staging directory from an earlier erase is still present.
    ///
    /// It may contain files that could not be restored, so it is never
    /// removed automatically.
    #[error(
        "Staging directory {0} already exists from an interrupted erase. \
         It may contain your files (see restore.tsv inside). Restore or remove it manually, then retry."
    )]
    StagingDirExists(PathBuf),

    /// Erasing requires confirmation, but no terminal is available to ask.
    #[error(
        "Refusing to erase without confirmation in a non-interactive session. Pass --yes to proceed."
    )]
    ConfirmationRequired,

    /// Erasing failed partway through.
    #[error("Erase failed: {reason}")]
    EraseFailed {
        /// What went wrong, including how many files were affected.
        reason: String,
    },
}
