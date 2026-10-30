use clap::{Parser, Subcommand, ValueEnum};

use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::commands::clean::Cleaner;
use media_duplicate_eraser_rs::commands::erase::Eraser;
use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::error::Result;
use media_duplicate_eraser_rs::services::duplicate::MediaFilter;
use media_duplicate_eraser_rs::services::hasher::DEFAULT_SIMILARITY_THRESHOLD;

use crate::logger;

/// Media type filter for scanning
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum MediaType {
    /// Scan all supported media types (images, videos, and audio)
    #[default]
    All,
    /// Scan only images
    Images,
    /// Scan only videos
    Videos,
    /// Scan only audio files
    Audio,
    /// Scan every file, including non-media (non-media files only match exactly)
    Any,
}

impl From<MediaType> for MediaFilter {
    fn from(media_type: MediaType) -> Self {
        match media_type {
            MediaType::All => MediaFilter::All,
            MediaType::Images => MediaFilter::ImagesOnly,
            MediaType::Videos => MediaFilter::VideosOnly,
            MediaType::Audio => MediaFilter::AudioOnly,
            MediaType::Any => MediaFilter::AnyFile,
        }
    }
}

#[derive(Parser)]
#[command(name = "mde")]
#[command(author, version, about = "Find and remove duplicate media files", long_about = None)]
#[command(
    after_help = "Typical workflow:\n  mde scan ~/Photos           # find duplicates, writes ~/Photos/duplicates.json\n  mde erase ~/Photos --dry-run\n  mde erase ~/Photos          # asks before deleting"
)]
pub struct Cli {
    /// Increase verbosity (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan a directory for duplicate media files and save them to duplicates.json
    Scan {
        /// Directory to scan for duplicates
        #[arg(default_value = ".")]
        path: std::path::PathBuf,

        /// Only scan the top-level directory, without descending into subdirectories
        #[arg(long)]
        no_recursive: bool,

        /// Scan subdirectories (the default; kept for compatibility)
        #[arg(short, long, hide = true)]
        recursive: bool,

        /// Include hidden files (starting with '.')
        #[arg(long)]
        include_hidden: bool,

        /// Output file for duplicates (JSON format). Defaults to duplicates.json in the scanned directory.
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,

        /// Filter by media type
        #[arg(short, long, value_enum, default_value_t = MediaType::All)]
        media: MediaType,

        /// Only find exact (byte-for-byte) duplicates; skip similarity detection
        #[arg(long)]
        exact_only: bool,

        /// Maximum perceptual hash distance to treat media as similar (lower is stricter)
        #[arg(long, default_value_t = DEFAULT_SIMILARITY_THRESHOLD, value_parser = clap::value_parser!(u32).range(0..=64))]
        threshold: u32,
    },

    /// Remove the duplicates.json file without deleting anything else
    Clean {
        /// Directory containing duplicates.json, or the path of a duplicates file
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },

    /// Delete the duplicate files listed in duplicates.json, keeping one of each group
    Erase {
        /// Directory containing duplicates.json, or the path of a duplicates file
        #[arg(default_value = ".")]
        path: std::path::PathBuf,

        /// Show what would be deleted without deleting anything
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    logger::init(cli.verbose, cli.quiet);

    let command: Box<dyn Command> = match cli.command {
        Commands::Scan {
            path,
            no_recursive,
            recursive: _,
            include_hidden,
            output,
            media,
            exact_only,
            threshold,
        } => Box::new(
            Scanner::new(
                path,
                !no_recursive,
                include_hidden,
                output,
                cli.quiet,
                media.into(),
            )
            .exact_only(exact_only)
            .threshold(threshold),
        ),
        Commands::Clean { path } => Box::new(Cleaner::new(path, cli.quiet)),
        Commands::Erase { path, dry_run, yes } => Box::new(
            Eraser::new(path, cli.quiet)
                .dry_run(dry_run)
                .assume_yes(yes),
        ),
    };

    command.execute()
}
