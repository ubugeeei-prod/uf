//! sqlc's Flow target as a standalone plugin: a request on stdin, a response
//! on stdout. Built for `wasm32-wasip1` this is a sqlc WASM plugin; natively it
//! is a process plugin. Its output is not run through `uf fmt`'s printer — `uf`
//! itself is the plugin that does that.

use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut input) {
        eprintln!("sqlc-gen-flow: could not read the request: {error}");
        return ExitCode::FAILURE;
    }
    match uf_sqlc::run_plugin(&input, &|source| Ok(source.to_owned())) {
        Ok(output) => {
            if let Err(error) = std::io::stdout().write_all(&output) {
                eprintln!("sqlc-gen-flow: could not write the response: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sqlc-gen-flow: {error}");
            ExitCode::FAILURE
        }
    }
}
