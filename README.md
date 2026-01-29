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

[![Crates.io](https://img.shields.io/crates/v/media-duplicate-eraser-rs.svg)](https://crates.io/crates/media-duplicate-eraser-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Installation

### From crates.io (recommended)

```bash
cargo install media-duplicate-eraser-rs
```

The binary will be installed to `~/.cargo/bin/mde`. Make sure `~/.cargo/bin` is in your PATH:

```bash
# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export PATH="$HOME/.cargo/bin:$PATH"
```

### From Homebrew

```bash
# Coming soon
brew install mde
```

### From source

```bash
git clone https://github.com/mvillegas/media-duplicate-eraser-rs.git
cd media-duplicate-eraser-rs
cargo build --release

# Option 1: Copy to a directory in your PATH
sudo cp target/release/mde /usr/local/bin/

# Option 2: Add target/release to your PATH
export PATH="$PWD/target/release:$PATH"
```

## Usage

### Scan for duplicates

```bash
# Scan current directory for duplicate files
mde scan

# Scan a specific directory
mde scan /path/to/photos

# Include hidden files and directories
mde scan /path/to/photos --include-hidden

# Specify custom output file
mde scan /path/to/photos -o duplicates.json

# Increase verbosity (-v for info, -vv for debug)
mde -v scan /path/to/photos
mde -vv scan /path/to/photos

# Quiet mode (errors only)
mde -q scan /path/to/photos
```

### Erase duplicates

```bash
# Delete duplicate files listed in duplicates.json (keeps originals)
mde erase /path/to/photos

# Erase duplicates in current directory
mde erase
```

The erase command uses atomic deletion with rollback - either all duplicates are deleted or none are. This protects against partial deletions from interrupted processes.

### Clean up

```bash
# Remove duplicates.json file from a directory
mde clean /path/to/photos

# Clean current directory
mde clean
```

### Help

```bash
mde --help
mde scan --help
mde erase --help
mde clean --help
```

## Example Output

### Scan Command

```
⠋ Collecting files...
[OK] Found 150 files

⠹ [========================================] 150/150 Analyzing images

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

[OK] Duplicates saved to: /photos/duplicates.json
```

### Erase Command

```
[*] Found 5 duplicate files to erase from 3 groups.
⠋ Validating files...
[!] 1 files no longer exist and will be skipped.

⠹ [========================================] 4/4 Staging files...
⠋ Finalizing deletion...

[OK] Successfully erased 4 duplicate files.
[OK] Removed: /photos/duplicates.json
```

### Clean Command

```
[OK] Removed: /photos/duplicates.json
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
│   ├── scan.rs          # Scanner - find duplicates
│   ├── erase.rs         # Eraser - delete duplicates
│   └── clean.rs         # Cleaner - remove duplicates.json
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
| [serde](https://crates.io/crates/serde) | Serialization |
| [serde_json](https://crates.io/crates/serde_json) | JSON output |
| [chrono](https://crates.io/crates/chrono) | Timestamps |
| [indicatif](https://crates.io/crates/indicatif) | Progress bars and spinners |
| [console](https://crates.io/crates/console) | Styled terminal output |

## Running Tests

```bash
cargo test
```

## TODO

- [x] `erase` command to delete all duplicate files marked by the scan
- [x] `clean` command to remove duplicates.json file
- [ ] Support for more media types (videos and audio)
- [x] Publish to [crates.io](https://crates.io/crates/media-duplicate-eraser-rs)
- [ ] Implement CI/CD for contributions
- [ ] Publish to [Homebrew](https://brew.sh/)
- [ ] Announce v1.0 release on [r/rust](https://reddit.com/r/rust) and [r/linux](https://reddit.com/r/linux)

## License

MIT
