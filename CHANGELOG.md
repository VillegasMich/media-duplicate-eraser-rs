# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

This release focuses on making `erase` safe to trust. Several of the fixes below prevented real data loss, and some defaults changed, so it should ship as **2.0.0**.

### Fixed (data safety)
- `erase` could delete files that were never scanned: `duplicates.json` stored paths relative to the directory `scan` ran from, and `erase` resolved them against its own working directory. Paths are now absolute, and relative paths are rejected.
- `erase` deleted every copy when the file to keep had been removed or changed after the scan. It now keeps duplicates whose original is missing or changed.
- `erase` deleted files that had been modified since the scan. It now checks size and modification time, and re-verifies SHA-256 for exact duplicates.
- A re-scan that found no duplicates left the previous `duplicates.json` in place, so a later `erase` acted on stale data. The outdated file is now removed.
- A leftover `.mde_erase_staging` directory, which can hold files a failed rollback couldn't restore, was silently deleted on the next run. `erase` now refuses to start and explains how to recover, using a new `restore.tsv` inside the staging directory.
- Rollback always reported "No files were deleted", even when some files couldn't be restored. Messages are now accurate.
- Hand-edited files listing a path both as a keeper and as a duplicate could delete the keeper. Such paths are now skipped.
- Non-media files (source files, licenses, configs) were scanned and deleted even though the tool only claimed to handle media. They are now ignored unless `--media any` is passed.
- Empty files were reported as duplicates of each other (for example every empty `__init__.py`). They are now always ignored.

### Fixed (detection)
- Video fingerprints only looked at frames 0–120, which is the first few seconds. Different videos sharing an intro or a fade from black were reported as duplicates. Frames are now sampled at 10/30/50/70/90% of the duration.
- Perceptual hashes of images, videos and audio were compared against each other. Only the same kind of media is compared now.
- Video and audio with clearly different durations (trimmed clips) are no longer reported as similar.
- `--recursive` could not be turned off. Added `--no-recursive`; `-r/--recursive` is still accepted as a no-op.
- The file kept from each group depended on directory iteration order. It is now deterministic: the cleanest path for exact duplicates, the largest file for similar ones.
- Progress bars restart correctly for each phase instead of reusing the first phase's length.

### Added
- `erase --dry-run` (`-n`) to list what would be deleted.
- A confirmation prompt before `erase` deletes anything. Use `--yes` (`-y`) to skip it; it is required in non-interactive sessions.
- `erase` and `clean` accept the path of a duplicates file, so reports saved with `scan -o` work.
- `scan --exact-only` to skip similarity detection.
- `scan --threshold N` to tune similarity strictness.
- `scan --media any` to include non-media files (exact matches only).
- A scan report that marks the file to **keep** and the ones to **delete** in each group, with file sizes, the total space freed, and the next command to run.
- Parallel hashing and fingerprinting (rayon).
- `-v`/`-q` work after the subcommand too (`mde scan . -v`), and `RUST_LOG` overrides the log level.
- `docs/ARCHITECTURE.md`, `CLAUDE.md`, and this changelog.
- Many regression tests, including video/audio tests that run when FFmpeg is installed.

### Changed
- **Breaking:** `duplicates.json` format version `2`: absolute paths, per-file `size`/`modified`, plus `root` and `reclaimable_bytes`. Reports from 1.x are rejected; re-run `mde scan`.
- **Breaking:** `--media all` (the default) now means media files only.
- **Breaking:** `erase` asks for confirmation (see `--yes`).
- Dropped the `ffmpeg-sidecar` dependency, and with it its unused auto-download code and the TLS/HTTP stack it pulled in. FFmpeg is now invoked directly, and `ffprobe` is also required for video/audio similarity.
- An unreadable subdirectory is skipped with a warning instead of aborting the scan.
- CI now checks formatting and clippy, and tests on Linux and macOS with FFmpeg installed.
- The release workflow passes the release notes input through an environment variable instead of interpolating it into the shell script.

## [1.0.0]

- Initial release: `scan`, `erase`, `clean`; SHA-256 exact matching and perceptual matching for images, videos and audio.
