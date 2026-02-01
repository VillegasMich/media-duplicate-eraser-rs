//! Hashing utilities for duplicate detection.
//!
//! This module provides two types of hashing:
//! - **Cryptographic (SHA256)**: For detecting exact duplicates
//! - **Perceptual (pHash)**: For detecting visually similar images and videos

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use image_hasher::{HashAlg, HasherConfig, ImageHash};
use sha2::{Digest, Sha256};

use crate::error::Result;

/// Size of the buffer used for reading files when computing SHA256.
const BUFFER_SIZE: usize = 8192;

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
    "wav", "flac", "aiff", "ape",
    // Lossy
    "mp3", "m4a", "aac", "ogg", "opus", "wma",
];

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

/// Computes the perceptual hash of a video by extracting key frames.
///
/// Extracts frames at regular intervals and computes a combined hash.
/// Returns `None` if the file is not a valid video or FFmpeg is not available.
pub fn video_perceptual_hash(path: &Path) -> Result<Option<ImageHash>> {
    use ffmpeg_sidecar::command::FfmpegCommand;
    use ffmpeg_sidecar::event::FfmpegEvent;

    // Number of frames to extract for hashing
    const FRAMES_TO_EXTRACT: usize = 5;
    // Frame dimensions for hashing (smaller = faster)
    const FRAME_WIDTH: u32 = 160;
    const FRAME_HEIGHT: u32 = 120;

    let path_str = path.to_string_lossy();

    // Use FFmpeg to extract frames as raw RGB data
    // We'll extract 5 frames evenly distributed throughout the video
    let mut child = match FfmpegCommand::new()
        .input(&*path_str)
        .args([
            "-vf",
            &format!("select='not(mod(n\\,30))',scale={}:{}", FRAME_WIDTH, FRAME_HEIGHT),
            "-frames:v",
            &FRAMES_TO_EXTRACT.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            log::debug!("Could not spawn FFmpeg for {:?}: {}", path, e);
            return Ok(None);
        }
    };

    let iter = match child.iter() {
        Ok(iter) => iter,
        Err(e) => {
            log::debug!("Could not create FFmpeg iterator for {:?}: {}", path, e);
            return Ok(None);
        }
    };

    let mut frame_data: Vec<u8> = Vec::new();
    let mut frames_collected = 0;
    let frame_size = (FRAME_WIDTH * FRAME_HEIGHT * 3) as usize;

    for event in iter {
        match event {
            FfmpegEvent::OutputFrame(frame) => {
                frame_data.extend_from_slice(&frame.data);
                frames_collected += 1;
                if frames_collected >= FRAMES_TO_EXTRACT {
                    break;
                }
            }
            FfmpegEvent::Error(e) => {
                log::debug!("FFmpeg error for {:?}: {}", path, e);
                return Ok(None);
            }
            _ => {}
        }
    }

    if frame_data.is_empty() {
        log::debug!("No frames extracted from {:?}", path);
        return Ok(None);
    }

    // Create a composite image from the extracted frames
    // Stack frames vertically to create a single image for hashing
    let actual_frames = frame_data.len() / frame_size;
    if actual_frames == 0 {
        return Ok(None);
    }

    let composite_height = FRAME_HEIGHT * actual_frames as u32;
    let composite_data: Vec<u8> = frame_data
        .iter()
        .take(actual_frames * frame_size)
        .copied()
        .collect();

    // Create an image buffer from the composite frame data
    let img_buffer = match image::RgbImage::from_raw(FRAME_WIDTH, composite_height, composite_data)
    {
        Some(buf) => buf,
        None => {
            log::debug!("Could not create image buffer from video frames {:?}", path);
            return Ok(None);
        }
    };

    let img = image::DynamicImage::ImageRgb8(img_buffer);

    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(16, 16)
        .to_hasher();

    let hash = hasher.hash_image(&img);
    Ok(Some(hash))
}

/// Computes the perceptual hash of an audio file by generating a spectrogram.
///
/// Uses FFmpeg to create a spectrogram image from the audio, then hashes it
/// like a regular image. Returns `None` if the file is not valid audio or
/// FFmpeg is not available.
pub fn audio_perceptual_hash(path: &Path) -> Result<Option<ImageHash>> {
    use std::process::{Command, Stdio};

    let path_str = path.to_string_lossy();

    // Use FFmpeg to generate spectrogram as PNG to stdout
    // showspectrumpic creates a single image from the entire audio
    let output = match Command::new("ffmpeg")
        .args([
            "-i",
            &path_str,
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
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            log::debug!("Could not spawn FFmpeg for audio {:?}: {}", path, e);
            return Ok(None);
        }
    };

    if !output.status.success() || output.stdout.is_empty() {
        log::debug!(
            "FFmpeg failed to generate spectrogram for {:?} (status: {:?})",
            path,
            output.status
        );
        return Ok(None);
    }

    // Load the PNG from stdout bytes
    let img = match image::load_from_memory(&output.stdout) {
        Ok(img) => img,
        Err(e) => {
            log::debug!("Could not decode spectrogram PNG for {:?}: {}", path, e);
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

/// Computes the perceptual hash for any supported media type.
///
/// Automatically detects whether the file is an image, video, or audio and uses
/// the appropriate hashing method.
///
/// Returns `None` if the file is not a supported media type or cannot be processed.
pub fn media_perceptual_hash(path: &Path) -> Result<Option<ImageHash>> {
    match get_media_type(path) {
        MediaType::Image => perceptual_hash(path),
        MediaType::Video => video_perceptual_hash(path),
        MediaType::Audio => audio_perceptual_hash(path),
        MediaType::Unknown => {
            // Try as image first (some formats might not have standard extensions)
            perceptual_hash(path)
        }
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

/// Checks if FFmpeg is available on the system.
pub fn is_ffmpeg_available() -> bool {
    ffmpeg_sidecar::command::ffmpeg_is_installed()
}

/// Attempts to download FFmpeg if not available.
/// Returns true if FFmpeg is available after this call.
pub fn ensure_ffmpeg() -> bool {
    if is_ffmpeg_available() {
        return true;
    }

    log::info!("FFmpeg not found, attempting to download...");
    match ffmpeg_sidecar::download::auto_download() {
        Ok(_) => {
            log::info!("FFmpeg downloaded successfully");
            true
        }
        Err(e) => {
            log::warn!("Could not download FFmpeg: {}", e);
            false
        }
    }
}
