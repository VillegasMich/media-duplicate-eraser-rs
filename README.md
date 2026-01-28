# Media Duplicate Eraser (mde)

A CLI tool to find and remove duplicate media files.

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/mde`.

## Usage

```bash
# Scan directories for files
mde scan /path/to/photos /path/to/videos

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

## Project Structure

```
src/
├── main.rs           # Entry point
├── cli.rs            # CLI argument parsing (clap)
├── error.rs          # Custom error types
├── logger.rs         # Logging configuration
└── commands/
    ├── mod.rs        # Command trait
    └── scan.rs       # Scanner implementation
```

## License

MIT
