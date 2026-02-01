use clap::{Parser, Subcommand, ValueEnum};

use media_duplicate_eraser_rs::commands::clean::Cleaner;
use media_duplicate_eraser_rs::commands::erase::Eraser;
use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::error::Result;
use media_duplicate_eraser_rs::services::duplicate::MediaFilter;

use crate::logger;

/// Media type filter for scanning
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum MediaType {
    /// Scan all supported media types (images and videos)
    #[default]
    All,
    /// Scan only images
    Images,
    /// Scan only videos
    Videos,
}

impl From<MediaType> for MediaFilter {
    fn from(media_type: MediaType) -> Self {
        match media_type {
            MediaType::All => MediaFilter::All,
            MediaType::Images => MediaFilter::ImagesOnly,
            MediaType::Videos => MediaFilter::VideosOnly,
        }
    }
}

#[derive(Parser)]
#[command(name = "mde")]
#[command(author, version, about = "Find and remove duplicate media files", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan a directory for duplicate media files
    Scan {
        /// Directory to scan for duplicates
        #[arg(default_value = ".")]
        path: std::path::PathBuf,

        /// Perform recursive scan
        #[arg(short, long, default_value_t = true)]
        recursive: bool,

        /// Include hidden files (starting with '.')
        #[arg(long)]
        include_hidden: bool,

        /// Output file for duplicates (JSON format). Defaults to duplicates.json in the first scanned directory.
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,

        /// Filter by media type (all, images, or videos)
        #[arg(short, long, value_enum, default_value_t = MediaType::All)]
        media: MediaType,
    },

    /// Remove duplicates.json file from a directory
    Clean {
        /// Directory containing duplicates.json to remove
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },

    /// Delete duplicate files listed in duplicates.json (atomic operation)
    Erase {
        /// Directory containing duplicates.json
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    logger::init(cli.verbose, cli.quiet);

    let command: Box<dyn Command> = match cli.command {
        Commands::Scan {
            path,
            recursive,
            include_hidden,
            output,
            media,
        } => Box::new(Scanner::new(
            path,
            recursive,
            include_hidden,
            output,
            cli.quiet,
            media.into(),
        )),
        Commands::Clean { path } => Box::new(Cleaner::new(path, cli.quiet)),
        Commands::Erase { path } => Box::new(Eraser::new(path, cli.quiet)),
    };

    command.execute()
}
