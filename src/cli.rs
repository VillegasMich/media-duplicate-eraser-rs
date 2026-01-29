use clap::{Parser, Subcommand};

use media_duplicate_eraser_rs::commands::clean::Cleaner;
use media_duplicate_eraser_rs::commands::scan::Scanner;
use media_duplicate_eraser_rs::commands::Command;
use media_duplicate_eraser_rs::error::Result;

use crate::logger;

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
    /// Scan directories for duplicate media files
    Scan {
        /// Directories to scan for duplicates
        #[arg(required = true)]
        paths: Vec<std::path::PathBuf>,

        /// Perform recursive scan
        #[arg(short, long, default_value_t = true)]
        recursive: bool,

        /// Include hidden files (starting with '.')
        #[arg(long)]
        include_hidden: bool,

        /// Output file for duplicates (JSON format). Defaults to duplicates.json in the first scanned directory.
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },

    /// Remove duplicates.json file from a directory
    Clean {
        /// Directory containing duplicates.json to remove
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    logger::init(cli.verbose, cli.quiet);

    let command: Box<dyn Command> = match cli.command {
        Commands::Scan {
            paths,
            recursive,
            include_hidden,
            output,
        } => Box::new(Scanner::new(paths, recursive, include_hidden, output)),
        Commands::Clean { path } => Box::new(Cleaner::new(path)),
    };

    command.execute()
}
