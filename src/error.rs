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
}
