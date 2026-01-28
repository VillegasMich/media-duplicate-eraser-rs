//! Hashing utilities for duplicate detection.
//!
//! This module provides two types of hashing:
//! - **Cryptographic (SHA256)**: For detecting exact duplicates
//! - **Perceptual (pHash)**: For detecting visually similar images

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use image_hasher::{HashAlg, HasherConfig, ImageHash};
use sha2::{Digest, Sha256};

use crate::error::Result;

/// Size of the buffer used for reading files when computing SHA256.
const BUFFER_SIZE: usize = 8192;

/// Computes the SHA256 hash of a file.
///
/// This is used for detecting exact duplicates (byte-identical files).
pub fn sha256_hash(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Computes the perceptual hash (pHash) of an image.
///
/// This is used for detecting visually similar images that may have
/// different compression, format, or minor modifications.
///
/// Returns `None` if the file is not a valid image.
pub fn perceptual_hash(path: &Path) -> Result<Option<ImageHash>> {
    let img = match image::open(path) {
        Ok(img) => img,
        Err(e) => {
            log::debug!("Could not open image {:?}: {}", path, e);
            return Ok(None);
        }
    };

    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(16, 16)
        .to_hasher();

    let hash = hasher.hash_image(&img);
    Ok(Some(hash))
}

/// Calculates the Hamming distance between two perceptual hashes.
///
/// Lower distance means more similar images.
/// - 0: Identical images
/// - 1-10: Very similar (likely duplicates)
/// - 10+: Different images
pub fn hamming_distance(hash1: &ImageHash, hash2: &ImageHash) -> u32 {
    hash1.dist(hash2)
}

/// Threshold for considering two images as perceptually similar.
/// Images with Hamming distance <= this value are considered duplicates.
pub const SIMILARITY_THRESHOLD: u32 = 10;

/// Checks if two perceptual hashes are similar enough to be considered duplicates.
pub fn are_similar(hash1: &ImageHash, hash2: &ImageHash) -> bool {
    hamming_distance(hash1, hash2) <= SIMILARITY_THRESHOLD
}

/// Gets the file size in bytes.
pub fn file_size(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}
