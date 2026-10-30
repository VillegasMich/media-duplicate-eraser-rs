//! Hashing utilities for duplicate detection.
//!
//! This module provides two types of hashing:
//! - **Cryptographic (SHA256)**: For detecting exact duplicates
//! - **Perceptual (pHash)**: For detecting visually similar images and videos,
//!   and audio with similar spectrograms
//!
//! Video and audio fingerprints shell out to `ffmpeg` and `ffprobe`, which
//! must be on `PATH`. When they are missing those files are simply skipped.

use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use image_hasher::{HashAlg, HasherConfig, ImageHash};
use sha2::{Digest, Sha256};

use crate::error::Result;

/// Supported image extensions for perceptual hashing.
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "ico",
];

/// Supported video extensions for perceptual hashing.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpeg", "mpg", "3gp",
];

/// Supported audio extensions for perceptual hashing.
const AUDIO_EXTENSIONS: &[&str] = &[
    // Lossless
    "wav", "flac", "aiff", "ape", // Lossy
    "mp3", "m4a", "aac", "ogg", "opus", "wma",
];

/// Default threshold for considering two fingerprints perceptually similar.
/// Fingerprints with Hamming distance <= this value are considered duplicates.
pub const DEFAULT_SIMILARITY_THRESHOLD: u32 = 10;

/// Number of frames sampled from each video.
const VIDEO_FRAMES: usize = 5;
/// Frame dimensions for video hashing (smaller = faster).
const FRAME_WIDTH: u32 = 160;
const FRAME_HEIGHT: u32 = 120;

/// Media type classification for files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Unknown,
}

/// Determines the media type of a file based on its extension.
pub fn get_media_type(path: &Path) -> MediaType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some(e) if IMAGE_EXTENSIONS.contains(&e) => MediaType::Image,
        Some(e) if VIDEO_EXTENSIONS.contains(&e) => MediaType::Video,
        Some(e) if AUDIO_EXTENSIONS.contains(&e) => MediaType::Audio,
        _ => MediaType::Unknown,
    }
}

/// Computes the SHA256 hash of a file.
///
/// This is used for detecting exact duplicates (byte-identical files).
pub fn sha256_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// A perceptual fingerprint of a media file.
#[derive(Debug, Clone)]
pub struct Fingerprint {
    /// Which kind of media produced this fingerprint.
    pub media_type: MediaType,
    /// The perceptual hash of the image, video frames, or audio spectrogram.
    pub hash: ImageHash,
    /// Duration in seconds, for video and audio.
    pub duration: Option<f64>,
}

impl Fingerprint {
    /// Checks if two fingerprints are similar enough to be considered duplicates.
    ///
    /// Fingerprints of different media types never match, and video or audio
    /// with clearly different durations never match: a trimmed clip is not a
    /// duplicate of the full recording.
    pub fn is_similar(&self, other: &Fingerprint, threshold: u32) -> bool {
        if self.media_type != other.media_type {
            return false;
        }
        if let (Some(a), Some(b)) = (self.duration, other.duration)
            && !durations_match(a, b)
        {
            return false;
        }
        hamming_distance(&self.hash, &other.hash) <= threshold
    }
}

/// Two durations match if they differ by at most 1 second or 2%, whichever is larger.
fn durations_match(a: f64, b: f64) -> bool {
    let tolerance = (a.max(b) * 0.02).max(1.0);
    (a - b).abs() <= tolerance
}

fn image_hasher() -> image_hasher::Hasher {
    HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(16, 16)
        .to_hasher()
}

/// Computes the perceptual hash (pHash) of an image.
///
/// This is used for detecting visually similar images that may have
/// different compression, format, or minor modifications.
///
/// Returns `None` if the file is not a valid image.
pub fn perceptual_hash(path: &Path) -> Option<ImageHash> {
    match image::open(path) {
        Ok(img) => Some(image_hasher().hash_image(&img)),
        Err(e) => {
            log::debug!("Could not open image {:?}: {}", path, e);
            None
        }
    }
}

/// Returns the duration of a video or audio file in seconds, using `ffprobe`.
pub fn media_duration(path: &Path) -> Option<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        log::debug!(
            "ffprobe failed for {:?} (status: {:?})",
            path,
            output.status
        );
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|d| d.is_finite() && *d > 0.0)
}

/// Extracts a single scaled RGB frame at `seconds` into the video.
fn extract_frame(path: &Path, seconds: f64) -> Option<Vec<u8>> {
    let output = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-ss",
            &format!("{:.3}", seconds),
            "-i",
        ])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={}:{}", FRAME_WIDTH, FRAME_HEIGHT),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let frame_size = (FRAME_WIDTH * FRAME_HEIGHT * 3) as usize;
    (output.status.success() && output.stdout.len() == frame_size).then_some(output.stdout)
}

/// Computes the perceptual fingerprint of a video.
///
/// Samples frames evenly across the whole video (at 10%, 30%, 50%, 70% and
/// 90% of its duration), stacks them vertically and hashes the composite.
/// Sampling across the duration, rather than only the opening seconds, keeps
/// different videos that share an intro or a fade from black apart.
///
/// Returns `None` if the file is not a valid video or FFmpeg is not available.
pub fn video_fingerprint(path: &Path) -> Option<Fingerprint> {
    let duration = media_duration(path)?;

    let mut composite: Vec<u8> = Vec::new();
    for i in 0..VIDEO_FRAMES {
        let at = duration * (i as f64 + 0.5) / VIDEO_FRAMES as f64;
        match extract_frame(path, at) {
            Some(frame) => composite.extend_from_slice(&frame),
            None => {
                log::debug!("Could not extract frame at {:.1}s from {:?}", at, path);
                return None;
            }
        }
    }

    let height = FRAME_HEIGHT * VIDEO_FRAMES as u32;
    let buffer = image::RgbImage::from_raw(FRAME_WIDTH, height, composite)?;
    let img = image::DynamicImage::ImageRgb8(buffer);

    Some(Fingerprint {
        media_type: MediaType::Video,
        hash: image_hasher().hash_image(&img),
        duration: Some(duration),
    })
}

/// Computes the perceptual fingerprint of an audio file from its spectrogram.
///
/// Uses FFmpeg to create a spectrogram image from the audio, then hashes it
/// like a regular image. Returns `None` if the file is not valid audio or
/// FFmpeg is not available.
pub fn audio_fingerprint(path: &Path) -> Option<Fingerprint> {
    let duration = media_duration(path)?;

    // showspectrumpic creates a single image from the entire audio
    let output = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-lavfi",
            "showspectrumpic=s=512x256:color=intensity",
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        log::debug!(
            "FFmpeg failed to generate spectrogram for {:?} (status: {:?})",
            path,
            output.status
        );
        return None;
    }

    let img = match image::load_from_memory(&output.stdout) {
        Ok(img) => img,
        Err(e) => {
            log::debug!("Could not decode spectrogram PNG for {:?}: {}", path, e);
            return None;
        }
    };

    Some(Fingerprint {
        media_type: MediaType::Audio,
        hash: image_hasher().hash_image(&img),
        duration: Some(duration),
    })
}

/// Computes the perceptual fingerprint for any supported media type.
///
/// Returns `None` if the file is not a supported media type or cannot be processed.
pub fn media_fingerprint(path: &Path) -> Option<Fingerprint> {
    match get_media_type(path) {
        MediaType::Image => perceptual_hash(path).map(|hash| Fingerprint {
            media_type: MediaType::Image,
            hash,
            duration: None,
        }),
        MediaType::Video => video_fingerprint(path),
        MediaType::Audio => audio_fingerprint(path),
        MediaType::Unknown => None,
    }
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

/// Gets the file size in bytes.
pub fn file_size(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}

fn tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Checks if both `ffmpeg` and `ffprobe` are available on the system.
pub fn is_ffmpeg_available() -> bool {
    tool_available("ffmpeg") && tool_available("ffprobe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(media_type: MediaType, byte: u8, duration: Option<f64>) -> Fingerprint {
        Fingerprint {
            media_type,
            hash: ImageHash::from_bytes(&[byte; 8]).unwrap(),
            duration,
        }
    }

    #[test]
    fn media_type_is_detected_case_insensitively() {
        assert_eq!(get_media_type(Path::new("a.JPG")), MediaType::Image);
        assert_eq!(get_media_type(Path::new("a.mkv")), MediaType::Video);
        assert_eq!(get_media_type(Path::new("a.Flac")), MediaType::Audio);
        assert_eq!(get_media_type(Path::new("a.txt")), MediaType::Unknown);
        assert_eq!(get_media_type(Path::new("noext")), MediaType::Unknown);
    }

    #[test]
    fn different_media_types_are_never_similar() {
        let image = fingerprint(MediaType::Image, 0, None);
        let video = fingerprint(MediaType::Video, 0, Some(10.0));
        assert!(!image.is_similar(&video, DEFAULT_SIMILARITY_THRESHOLD));
    }

    #[test]
    fn different_durations_are_never_similar() {
        let a = fingerprint(MediaType::Audio, 0, Some(180.0));
        let b = fingerprint(MediaType::Audio, 0, Some(240.0));
        let c = fingerprint(MediaType::Audio, 0, Some(181.0));
        assert!(!a.is_similar(&b, DEFAULT_SIMILARITY_THRESHOLD));
        assert!(a.is_similar(&c, DEFAULT_SIMILARITY_THRESHOLD));
    }

    #[test]
    fn threshold_is_respected() {
        let a = fingerprint(MediaType::Image, 0b0000_0000, None);
        let b = fingerprint(MediaType::Image, 0b0000_0011, None); // 2 bits x 8 bytes = 16
        assert!(!a.is_similar(&b, 10));
        assert!(a.is_similar(&b, 16));
    }

    #[test]
    fn sha256_matches_known_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        std::fs::write(&path, "abc").unwrap();
        assert_eq!(
            sha256_hash(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
