use std::io::Write;

use log::LevelFilter;

/// Initializes logging to stderr.
///
/// The level comes from `-v`/`-q`; `RUST_LOG` can override it for debugging.
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
        .parse_default_env()
        .format(|buf, record| {
            let level = record.level();
            let style = buf.default_level_style(level);
            writeln!(
                buf,
                "{style}{}{style:#}: {}",
                level.as_str().to_lowercase(),
                record.args()
            )
        })
        .init();

    log::debug!("Logger initialized with level: {:?}", level);
}
