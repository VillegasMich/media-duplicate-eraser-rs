# Architecture

`mde` is a small Rust crate with a library (`src/lib.rs`) and a thin binary (`src/main.rs`, `src/cli.rs`). The CLI only parses arguments and builds a command; all behavior lives in the library so integration tests can drive it directly.

```
src/
├── main.rs              # Entry point: runs the CLI, prints errors, sets exit code
├── cli.rs               # clap definitions → builds a Command (binary only)
├── logger.rs            # env_logger setup from -v/-q (binary only)
├── lib.rs               # Library root
├── error.rs             # Error enum (thiserror) + Result alias
├── output.rs            # Shared terminal output: prefixes, spinners, progress bars, sizes, paths
├── commands/
│   ├── mod.rs           # `Command` trait: fn execute(&self) -> Result<()>
│   ├── scan.rs          # Scanner: walk dir → detect → print report → write duplicates.json
│   ├── erase.rs         # Eraser: load → verify → confirm → staged delete with rollback
│   └── clean.rs         # Cleaner: remove duplicates.json
└── services/
    ├── hasher.rs        # Media type detection, SHA-256, perceptual fingerprints (FFmpeg for video/audio)
    ├── duplicate.rs     # Detection pipeline, grouping, keep-ordering
    └── manifest.rs      # duplicates.json format: save/load/validation

tests/
├── integration_tests.rs # Test crate entry point
├── common/mod.rs        # Fixture paths and temp-dir helpers
├── commands/            # scan.rs, erase.rs, clean.rs integration tests
└── fixtures/            # Small text and image fixtures
```

`media/` at the repository root holds sample files for trying the tool by hand; it is excluded from the published crate.

## Detection pipeline (`services/duplicate.rs`)

`find_duplicates_with_options(files, progress, DetectionOptions)`:

1. **Filter** by `MediaFilter`. `All` (the default) means images, videos and audio only. `AnyFile` includes everything.
2. **Size pass**: group by size. Zero-byte files are dropped (counted in `empty_files`).
3. **Exact pass**: SHA-256 (in parallel with rayon) for every file that shares its size with another. Identical hashes form `Exact` groups.
4. **Perceptual pass** (unless `perceptual == false`): every distinct media file (singletons, unmatched files, and one representative per exact group) gets a `Fingerprint`, computed in parallel. Files are sorted first so grouping is deterministic. Grouping is greedy: each ungrouped file seeds a group, and every later file with `is_similar` joins it.
5. **Merge**: a perceptual group that contains an exact group's representative absorbs the whole exact group and becomes `Perceptual`.
6. **Order** each group so `files[0]` is the file to keep (`order_for_keeping`):
   - `Exact`: fewest path components, then shortest path, then alphabetical.
   - `Perceptual`: largest file first, then the same path rules.

`Fingerprint::is_similar` requires the same `MediaType`, matching durations for video/audio (within max(1 s, 2%)), and a Hamming distance ≤ `threshold`.

### Fingerprints (`services/hasher.rs`)

| Media | Method |
|-------|--------|
| Image | `image_hasher` DoubleGradient, 16×16, on the decoded image |
| Video | `ffprobe` for the duration; `ffmpeg -ss` grabs one 160×120 frame at 10/30/50/70/90% of it; the five frames are stacked and hashed as one image |
| Audio | `ffprobe` for the duration; `ffmpeg showspectrumpic` renders a 512×256 spectrogram that is hashed as an image |

FFmpeg is called with `std::process::Command`, `-nostdin`, and the path passed as an `OsStr` argument (no shell, no lossy string conversion). When `ffmpeg`/`ffprobe` are missing, video and audio fingerprints return `None`, so those files can still match exactly but never as similar.

## `duplicates.json` format

Written by `scan` (`services/manifest.rs`), consumed by `erase`. Current format version: `"2"`.

```json
{
  "version": "2",
  "scanned_at": "2026-10-29T21:00:00Z",
  "root": "/Users/me/Photos",
  "total_files_scanned": 1532,
  "duplicate_groups": 1,
  "total_duplicates": 1,
  "reclaimable_bytes": 4404019,
  "entries": [
    {
      "original":   { "path": "/Users/me/Photos/IMG_001.jpg",      "size": 4404019, "modified": "2024-05-01T10:00:00.123456789Z" },
      "duplicates": [ { "path": "/Users/me/Photos/IMG_001 copy.jpg", "size": 4404019, "modified": "2024-05-01T10:00:00.123456789Z" } ],
      "duplicate_type": "exact"
    }
  ]
}
```

- All paths are **absolute** (the scan root is canonicalized). `load` rejects relative paths and other format versions, so a report can never be resolved against the wrong working directory.
- `size` and `modified` are captured at scan time and re-checked by `erase`.
- The file is written to `duplicates.json.tmp` and then renamed, so it is never half-written.
- Users may hand-edit it: removing an entry or a path from `duplicates` keeps those files. The summary counts are informational and are not re-validated.
- A scan that finds no duplicates removes an existing report at the output path, so a stale one can't be erased later.

## Erase safety model (`commands/erase.rs`)

`erase` never trusts the report blindly:

1. **Verify** (`verify`): a duplicate goes into the delete plan only if
   - its entry's original still exists with the recorded size and mtime,
   - the duplicate exists with the recorded size and mtime,
   - for `exact` entries, the duplicate's SHA-256 still equals the original's,
   - it isn't listed as an `original` anywhere in the file, and isn't listed twice.

   Everything else is reported as kept, with the reason.
2. **Dry run** (`--dry-run`) prints the plan and stops.
3. **Confirm**: prompts `[y/N]` on a TTY. Without a TTY it fails with `ConfirmationRequired` unless `--yes` is given.
4. **Stage**: creates `<root>/.mde_erase_staging/` (it lives in the scan root so renames stay on the same filesystem), writes `restore.tsv` (`staged<TAB>original`), then renames each file to `staging/<index>`. Any rename failure restores everything already moved.
5. **Delete**: `remove_dir_all` on the staging directory, then removes `duplicates.json`.

If a staging directory already exists, `erase` refuses to start (`StagingDirExists`), because it may hold files a failed rollback couldn't restore. It is never deleted automatically. The scanner also never descends into staging directories.

## Output conventions

- User-facing messages go to **stdout** through `output::{ok, info, warn, error}` prefixes, and are suppressed by `--quiet`.
- Diagnostics go to **stderr** through `log` macros: `warn!` for things the user should know, and `info!`/`debug!` for `-v`/`-vv`.
- Paths in reports are shown relative to the scan root (`output::display_path`). The JSON always stores absolute paths.
- Progress bars and spinners come from `output::progress_bar`/`output::spinner`, which return `None` in quiet mode.
