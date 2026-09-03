//! Format one file and print the result.
//!
//! `cargo run -p uf_fmt --example format -- path/to/file.js`
//!
//! A development aid for diffing the printer against Prettier on a file
//! that is not yet a fixture. `UF_FMT_WIDTH` sets the line width and
//! `UF_FMT_CONFIG` picks one of the named fixture configurations, so the
//! same names the test harness knows can be reproduced by hand.

use std::process::ExitCode;

use uf_config::{FmtConfig, QuoteStyle};
use uf_fmt::format_source;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: format <file>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut config = FmtConfig::default();
    match std::env::var("UF_FMT_CONFIG").as_deref() {
        Ok("config_narrow_lines") => config.line_width = 40,
        Ok("config_single_quotes") => config.quotes = QuoteStyle::Single,
        Ok("config_no_semicolons") => config.semicolons = false,
        Ok("config_wide_indent") => config.indent_width = 4,
        _ => {}
    }
    if let Ok(width) = std::env::var("UF_FMT_WIDTH")
        && let Ok(width) = width.parse()
    {
        config.line_width = width;
    }

    match format_source(&source, &config) {
        Ok(result) => {
            print!("{}", result.output);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{path}: {error}");
            ExitCode::FAILURE
        }
    }
}
