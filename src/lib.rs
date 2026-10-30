//! Media Duplicate Eraser library.
//!
//! Finds exact and perceptually similar duplicate media files, records them
//! in a reviewable `duplicates.json`, and safely erases them.
//!
//! - [`services::duplicate`]: detection pipeline (size → SHA-256 → perceptual)
//! - [`services::hasher`]: SHA-256 and perceptual fingerprints (images, video, audio)
//! - [`services::manifest`]: the `duplicates.json` format
//! - [`commands`]: the `scan`, `erase` and `clean` commands

pub mod commands;
pub mod error;
pub mod output;
pub mod services;
