use log::LevelFilter;

pub fn init(verbose: u8, quiet: bool) {
    let level = if quiet {
        LevelFilter::Error
    } else {
        match verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            _ => LevelFilter::Debug,
        }
    };

    env_logger::Builder::new()
        .filter_level(level)
        .format_timestamp(None)
        .init();

    log::debug!("Logger initialized with level: {:?}", level);
}
