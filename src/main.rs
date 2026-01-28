mod cli;
pub mod commands;
mod error;
mod logger;

fn main() {
    if let Err(e) = cli::run() {
        log::error!("{}", e);
        std::process::exit(1);
    }
}
