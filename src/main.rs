mod cli;
mod logger;

fn main() {
    if let Err(e) = cli::run() {
        log::error!("{}", e);
        std::process::exit(1);
    }
}
