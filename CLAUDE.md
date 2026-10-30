# CLAUDE.md

`mde` (crate `media-duplicate-eraser-rs`) is a Rust CLI that finds duplicate images, videos and audio, writes them to a reviewable `duplicates.json`, and deletes the copies on request. It is published on crates.io. **The tool deletes user files**, so safety is the top priority in every change.

## Commands

```bash
cargo build
cargo test                                  # all tests; video/audio tests self-skip without ffmpeg+ffprobe
cargo test --test integration_tests erase   # one area (scan | erase | clean)
cargo clippy --all-targets -- -D warnings   # CI fails on any warning
cargo fmt                                   # CI runs `cargo fmt --check`
cargo run -- scan media/                    # try it on the sample files in media/
cargo run -- erase media/ --dry-run
```

Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` before calling a change done.

## Layout

- `src/cli.rs`: clap definitions only; it builds a `Command` and calls `execute()`. Binary-only, so **don't put logic here**.
- `src/commands/{scan,erase,clean}.rs`: one struct per subcommand implementing `commands::Command`. Options beyond the constructor are builder methods (`Scanner::exact_only`, `Eraser::dry_run`, `Eraser::assume_yes`).
- `src/services/duplicate.rs`: detection pipeline (size → SHA-256 → perceptual → merge → keep-ordering).
- `src/services/hasher.rs`: media-type detection, SHA-256, `Fingerprint` (image hash; FFmpeg/ffprobe for video and audio).
- `src/services/manifest.rs`: the `duplicates.json` format (`DuplicatesFile`, `FileRecord`, `FORMAT_VERSION`).
- `src/output.rs`: all styled terminal output helpers (prefixes, progress bars, `human_size`, `display_path`).
- `docs/ARCHITECTURE.md`: full pipeline, JSON format and erase safety model. **Read it before changing detection or erase.**

## Safety invariants (never regress these)

Each one has a regression test in `tests/commands/erase.rs` or `tests/commands/scan.rs`.

1. `duplicates.json` stores **absolute** paths only; `DuplicatesFile::load` rejects relative paths and unknown versions.
2. `erase` re-verifies before deleting: the original exists and is unchanged, the duplicate's size and mtime are unchanged, and exact duplicates still have matching SHA-256. A path that is an `original` anywhere is never deleted.
3. `erase` asks for confirmation unless `--yes`. Without a TTY and without `--yes` it errors instead of deleting.
4. Deletion is staged in `<root>/.mde_erase_staging` and rolled back on any failure. A pre-existing staging directory is **never** removed automatically.
5. `scan` removes an outdated report when it finds no duplicates, and the scanner never walks into staging dirs.
6. By default only media files are considered, empty files are never duplicates, and perceptual matches only compare the same media type with matching durations.

If a change touches these, add or update the matching test first. Changing the JSON shape requires bumping `FORMAT_VERSION` and a CHANGELOG entry.

## Conventions

- Errors: add variants to `error::Error` (thiserror) with an actionable message. No `unwrap()` on I/O in library code.
- User-facing text goes to stdout via `output::{ok,info,warn,error}()` and is skipped when `quiet`. Diagnostics go through `log::{warn,info,debug}!` (stderr, controlled by `-v`/`-q`/`RUST_LOG`). Don't print the same message both ways.
- Show paths to users relative to the scan root (`output::display_path`). Always store them absolute.
- External tools: `std::process::Command` with `-nostdin`, stdin set to null, and the path via `.arg(path)` (never `to_string_lossy`, never a shell). Missing FFmpeg means "skip", not "error".
- Keep detection deterministic: sort before grouping, and order groups with `order_for_keeping`.
- Match the existing doc-comment style: `//!` module docs, `///` on public items, and a short comment only where the *why* isn't obvious.

## Tests

- Integration tests live in `tests/commands/*.rs`, are wired through `tests/integration_tests.rs`, and share helpers in `tests/common`. Unit tests sit in `#[cfg(test)]` modules next to the code.
- Never run `scan`/`erase` against real fixture directories in tests. Copy into `tempfile::tempdir()` first; erase tests must only delete temp files.
- Tests drive commands with `quiet = true`, and `Eraser` tests use `.assume_yes(true)` (there is no TTY).
- Text fixtures need `MediaFilter::AnyFile`, because the default filter ignores non-media.
- Video/audio tests generate media with `ffmpeg -f lavfi` at runtime and return early when `hasher::is_ffmpeg_available()` is false.

## Gotchas

- On macOS, `canonicalize` turns `/tmp/...` into `/private/tmp/...`. Compare against `DuplicatesFile::root` or canonicalized paths, not the raw temp path.
- `tests/fixtures/images/image_b.png` is a slightly shifted version of `image_a.png`, so it is *expected* to be grouped as SIMILAR.
- `media/` is for manual testing and is excluded from the crate (`Cargo.toml` `exclude`). Don't add large files to `tests/fixtures`.
- Releases are manual (`.github/workflows/release.yml`, `workflow_dispatch`): bump `version` in `Cargo.toml` and update `CHANGELOG.md` first. The workflow tags, builds 4 targets, creates the GitHub release, and publishes to crates.io.
