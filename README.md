<table align="center">
  <tr>
    <td style="vertical-align: bottom;">
      <h1 style="margin: 0;">Media Duplicate Eraser (mde)</h1>
    </td>
    <td style="vertical-align: bottom; padding-left: 14px;">
      <img
        src="https://github.com/user-attachments/assets/c97f6711-5ce5-4f43-b12c-d963333fe851"
        width="64"
        alt="mde icon"
      />
    </td>
  </tr>
</table>


A CLI tool to find and remove duplicate media files.

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/mde`.

## Usage

```bash
# Scan directories for duplicate files
mde scan /path/to/photos

# Scan multiple directories
mde scan /path/to/photos /path/to/videos ~/Downloads

# Include hidden files and directories
mde scan /path/to/photos --include-hidden

# Increase verbosity (-v for info, -vv for debug)
mde -v scan /path/to/photos
mde -vv scan /path/to/photos

# Quiet mode (errors only)
mde -q scan /path/to/photos

# Show help
mde --help
mde scan --help
```

## Example Output

```
Scanning 150 files for duplicates...

=== Duplicate Detection Report ===
Total files scanned: 150
Errors encountered: 0

Found 3 duplicate groups (1 exact, 2 perceptual)
Total duplicate files: 5 (2 exact, 3 perceptual)

Group 1 [EXACT] - 3 files:
  /photos/IMG_001.jpg
  /photos/backup/IMG_001.jpg
  /photos/old/IMG_001.jpg

Group 2 [SIMILAR] - 2 files:
  /photos/sunset.jpg
  /photos/sunset_edited.jpg
```

## How Duplicate Detection Works

The tool uses a **two-pass approach** to efficiently find both exact and visually similar duplicates:

### Pass 1: Exact Duplicates (Fast)

1. **Group by file size**: Files with different sizes cannot be identical
2. **SHA-256 hash**: Within each size group, compute cryptographic hashes
3. Files with identical hashes are **exact duplicates** (byte-for-byte identical)

### Pass 2: Perceptual Duplicates (Thorough)

For files that aren't exact duplicates, we use **perceptual hashing** to find visually similar images:

1. **Perceptual Hash (pHash)**: Each image is converted to a compact fingerprint that represents its visual content
2. **Hamming Distance**: Compare fingerprints using bitwise difference
3. Images with distance ≤ 10 are considered **similar**

#### What Perceptual Hashing Detects

| Detected | Not Detected |
|----------|--------------|
| Re-saved images (different compression) | Completely different images |
| Format conversions (PNG → JPEG) | Heavy cropping |
| Minor resizing | Significant edits |
| Screenshots of same content | Different photos of same subject |
| Exact copies with different names | |

### Pass 3: Merge Groups

When a perceptually similar image is found that relates to an exact duplicate group, all files are merged into a single group.

## Project Structure

```
src/
├── main.rs              # Entry point
├── lib.rs               # Library crate
├── cli.rs               # CLI argument parsing (clap)
├── error.rs             # Custom error types
├── logger.rs            # Logging configuration
├── commands/
│   ├── mod.rs           # Command trait
│   └── scan.rs          # Scanner implementation
└── services/
    ├── mod.rs           # Services module
    ├── hasher.rs        # SHA-256 and perceptual hashing
    └── duplicate.rs     # Duplicate detection logic

tests/
├── integration_tests.rs # Test entry point
├── common/              # Shared test utilities
├── commands/            # Command-specific tests
└── fixtures/            # Test files
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| [clap](https://crates.io/crates/clap) | CLI argument parsing |
| [thiserror](https://crates.io/crates/thiserror) | Custom error types |
| [env_logger](https://crates.io/crates/env_logger) | Logging |
| [walkdir](https://crates.io/crates/walkdir) | Directory traversal |
| [sha2](https://crates.io/crates/sha2) | SHA-256 hashing |
| [image_hasher](https://crates.io/crates/image_hasher) | Perceptual hashing |
| [image](https://crates.io/crates/image) | Image loading |

## Running Tests

```bash
cargo test
```

## TODO

- [ ] `erase` command to delete all duplicate files marked by the scan
- [ ] Support for more media types (videos and audio)
- [ ] Publish to [crates.io](https://crates.io)
- [ ] Implement CI/CD for contributions

## License

MIT
