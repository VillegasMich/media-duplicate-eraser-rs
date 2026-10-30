<table align="center">
  <tr>
    <td style="vertical-align: bottom;">
      <h1 style="margin: 0;">Media Duplicate Eraser (mde)</h1>
    </td>
    <td style="vertical-align: bottom; padding-left: 14px;">
      <img
        src="assets/icon.png"
        width="64"
        alt="mde icon"
      />
    </td>
  </tr>
</table>

A CLI tool to find duplicate images, videos and audio, show you exactly what it found, and then safely erase the copies.

[![Crates.io](https://img.shields.io/crates/v/media-duplicate-eraser-rs.svg)](https://crates.io/crates/media-duplicate-eraser-rs)
[![Crates.io Downloads](https://img.shields.io/crates/d/media-duplicate-eraser-rs.svg)](https://crates.io/crates/media-duplicate-eraser-rs)
[![CI](https://github.com/VillegasMich/media-duplicate-eraser-rs/actions/workflows/build_test.yml/badge.svg)](https://github.com/VillegasMich/media-duplicate-eraser-rs/actions/workflows/build_test.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

- **Exact duplicates**: byte-for-byte identical files (SHA-256).
- **Similar media**: the same picture, video or song re-encoded, resized or converted to another format (perceptual hashing).
- **Review before deleting**: `scan` only writes a `duplicates.json` report. Nothing is deleted until you run `erase`, which verifies every file again and asks for confirmation.

## Installation

### From crates.io (recommended)

```bash
cargo install media-duplicate-eraser-rs
```

The binary is installed to `~/.cargo/bin/mde`. Make sure `~/.cargo/bin` is in your `PATH`.

### From a release

Download the archive for your platform from the [releases page](https://github.com/VillegasMich/media-duplicate-eraser-rs/releases), extract it, and put `mde` in your `PATH`.

### From source

```bash
git clone https://github.com/VillegasMich/media-duplicate-eraser-rs.git
cd media-duplicate-eraser-rs
cargo install --path .
```

### Optional: FFmpeg

Detecting **similar** videos and audio needs `ffmpeg` and `ffprobe` on your `PATH` (`brew install ffmpeg`, `apt install ffmpeg`, …). Without them, images still work fully, and exact copies of videos and audio are still found.

## Usage

The workflow is **scan → review → erase**:

```bash
mde scan ~/Photos             # 1. find duplicates, writes ~/Photos/duplicates.json
mde erase ~/Photos --dry-run  # 2. see exactly what would be deleted
mde erase ~/Photos            # 3. delete, after confirming
```

### Scan for duplicates

```bash
mde scan                          # current directory, recursively
mde scan ~/Photos --media images  # only images (also: videos, audio, all)
mde scan ~/Photos --exact-only    # only byte-identical copies, no similarity matching
mde scan ~/Photos --threshold 5   # stricter similarity (default 10, 0 = near-identical)
mde scan ~/Photos --no-recursive  # don't descend into subdirectories
mde scan ~/Photos --include-hidden
mde scan ~/Photos -o ~/dups.json  # save the report somewhere else
mde scan ~/Code --media any       # every file type (non-media only matches exactly)
```

Only media files are considered by default. Empty files are always ignored.

### Review

The scan report marks, for every group, which file is **kept** and which would be **deleted**, with sizes and the total space you would free:

```
=== Duplicate Detection Report ===
Scanned:  1532 files in /Users/me/Photos

Group 1 EXACT (3 files)
  keep   IMG_001.jpg  4.2 MiB
  delete IMG_001 copy.jpg  4.2 MiB
  delete backup/old/IMG_001.jpg  4.2 MiB

Group 2 SIMILAR (2 files)
  keep   sunset.png  8.1 MiB
  delete exports/sunset.jpg  1.3 MiB

Summary: 2 duplicate groups: 3 files can be removed (2 exact, 1 similar), freeing 9.7 MiB
    [!] SIMILAR files are not identical. Check them before erasing, or rescan with --exact-only.
[OK] Duplicates saved to: /Users/me/Photos/duplicates.json
    Review the list (edit the JSON to keep files), then run: mde erase "/Users/me/Photos"
```

**Which file is kept?**

- Exact duplicates are identical, so the file with the cleanest path is kept: the shallowest, then the shortest (`photo.jpg` over `photo copy.jpg` or `backup/old/photo.jpg`).
- Similar files keep the **largest** one, which is usually the original quality (full resolution, least compression).

To change the decision, edit `duplicates.json` before erasing: remove a whole entry to keep every file in it, or remove a path from an entry's `duplicates` list to keep that file. The format is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#duplicatesjson-format).

### Erase duplicates

```bash
mde erase ~/Photos              # asks for confirmation
mde erase ~/Photos --dry-run    # list what would be deleted; delete nothing
mde erase ~/Photos --yes        # no prompt (required in scripts / non-interactive shells)
mde erase ~/dups.json           # use a report saved with -o
```

Before deleting anything, `erase` re-checks every listed file. A duplicate is **kept** (and reported) if:

- the file it duplicates is gone or has changed, so you never lose the last copy,
- it has changed since the scan (different size or modification time),
- for exact duplicates, its content no longer matches the original,
- it is also listed as a file to keep in another group.

Deletion is all-or-nothing: files are first moved into a `.mde_erase_staging` directory inside the scanned folder, and only deleted once every move has succeeded. If any move fails, everything is put back. If an erase is ever interrupted, the staging directory holds a `restore.tsv` mapping each staged file to its original path, and `mde` refuses to run again until you deal with it.

> Deleted files do **not** go to the system trash. Use `--dry-run` first.

### Clean up

```bash
mde clean ~/Photos     # remove duplicates.json without deleting anything else
mde clean ~/dups.json
```

### Verbosity

```bash
mde scan ~/Photos -v    # info
mde scan ~/Photos -vv   # debug (why a file was skipped, per-file details)
mde scan ~/Photos -q    # errors only
RUST_LOG=debug mde scan ~/Photos
```

## How duplicate detection works

1. **Group by size**: files with different sizes can't be identical.
2. **SHA-256**: within each size group, identical hashes are **exact duplicates**.
3. **Perceptual fingerprints**: every distinct media file gets a fingerprint, computed in parallel:
   - **Images**: a 16×16 double-gradient perceptual hash.
   - **Videos**: five frames sampled at 10%, 30%, 50%, 70% and 90% of the duration, stacked and hashed together.
   - **Audio**: a spectrogram of the whole track, hashed like an image.

   Two fingerprints are **similar** when their Hamming distance is ≤ the threshold (default 10), they are the same kind of media, and, for video and audio, their durations match (within 1 s or 2%).
4. **Merge**: an exact group whose file is similar to other files becomes one similar group.

| Detected | Not detected |
|----------|--------------|
| Exact copies under any name or folder | Heavy crops or significant edits |
| Re-saved media (different compression) | Trimmed clips (different duration) |
| Format conversions (PNG → JPEG, MP4 → MKV, MP3 → AAC) | Different recordings of the same subject |
| Resized images and videos | |

## Supported formats

| Kind | Extensions |
|------|------------|
| Images | `jpg` `jpeg` `png` `gif` `bmp` `webp` `tiff` `tif` `ico` |
| Videos (similarity needs FFmpeg) | `mp4` `mkv` `avi` `mov` `wmv` `flv` `webm` `m4v` `mpeg` `mpg` `3gp` |
| Audio (similarity needs FFmpeg) | `wav` `flac` `aiff` `ape` `mp3` `m4a` `aac` `ogg` `opus` `wma` |

## Development

```bash
cargo test                                   # unit + integration tests (video/audio tests need FFmpeg)
cargo clippy --all-targets -- -D warnings
cargo fmt
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the code is organized, and [CHANGELOG.md](CHANGELOG.md) for what changed between versions. Releases are cut with the manual workflow described in [.github/workflows/README.md](.github/workflows/README.md).

## Roadmap

- [x] `erase` and `clean` commands
- [x] Video and audio support
- [x] Publish to [crates.io](https://crates.io/crates/media-duplicate-eraser-rs)
- [x] CI for contributions
- [ ] Move to system trash instead of deleting (`--trash`)
- [ ] Choose the keep policy (`--keep oldest|newest|largest|path`)
- [ ] Interactive review of similar groups
- [ ] Publish to [Homebrew](https://brew.sh/)

## License

MIT
