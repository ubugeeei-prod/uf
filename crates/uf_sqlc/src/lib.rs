#![cfg_attr(test, allow(clippy::disallowed_macros))]

//! sqlc's Flow target.
//!
//! [sqlc](https://sqlc.dev) parses SQL against a schema and hands a plugin a
//! `plugin.GenerateRequest`: the catalog, and every query with its parameters
//! and result columns already typed. This crate turns that into Flow modules
//! that call `@uniflowed/sql`. It does no SQL analysis of its own beyond
//! finding placeholders, which is what keeps it honest: every type it prints
//! is one sqlc inferred.
//!
//! `uf` answers sqlc's process-plugin call (`uf /plugin.CodegenService/Generate`)
//! with [`run_plugin`], formatting each file with `uf fmt`'s printer on the
//! way out. The `sqlc-gen-flow` binary is the same without the formatter, for
//! sqlc's WASM transport. `docs/sqlc.md` is the design record.

pub mod emit;
pub mod names;
pub mod options;
pub mod placeholders;
pub mod proto;
pub mod signed;
pub mod types;

pub use proto::{File, GenerateRequest};

/// The argument sqlc passes a process plugin, and the method a WASM plugin is
/// started with.
pub const PLUGIN_METHOD: &str = "/plugin.CodegenService/Generate";

/// How a request arrived, which is how its response must leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Protobuf,
    Json,
}

impl Encoding {
    /// Tell the two apart by the first byte: a protobuf `GenerateRequest`
    /// cannot start with `{`, which would be field 15 with the long-removed
    /// group wire type.
    #[must_use]
    pub fn detect(input: &[u8]) -> Self {
        match input.iter().find(|byte| !byte.is_ascii_whitespace()) {
            Some(b'{') => Self::Json,
            _ => Self::Protobuf,
        }
    }
}

/// Read a request in either encoding.
///
/// # Errors
///
/// When the bytes are not a `GenerateRequest`.
pub fn decode(input: &[u8]) -> Result<GenerateRequest, proto::DecodeError> {
    match Encoding::detect(input) {
        Encoding::Json => proto::request_from_json(input),
        Encoding::Protobuf => proto::request_from_protobuf(input),
    }
}

/// Generate the files for `request`, pass each through `format`, and sign it.
///
/// `format` is `uf fmt`'s printer inside `uf`, and the identity in the WASM
/// plugin. A file the formatter refuses is an error rather than written
/// unformatted, since that would mean the generator printed invalid Flow.
///
/// # Errors
///
/// When the request cannot be generated faithfully, or `format` fails.
pub fn generate(
    request: &GenerateRequest,
    format: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Vec<File>, String> {
    emit::generate(request)?
        .into_iter()
        .map(|file| {
            let formatted = format(&file.contents).map_err(|error| {
                uf_infra::into_string(uf_infra::cstr!("formatting {}: {error}", file.name))
            })?;
            Ok(File {
                name: file.name,
                contents: signed::sign(&formatted),
            })
        })
        .collect()
}

/// The whole plugin call: a request's bytes in, a response's bytes out, in
/// the encoding the request came in.
///
/// # Errors
///
/// When the request cannot be read or generated. The message is meant for
/// sqlc to print, which it does verbatim from the plugin's stderr.
pub fn run_plugin(
    input: &[u8],
    format: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Vec<u8>, String> {
    let encoding = Encoding::detect(input);
    let request = decode(input).map_err(|error| error.to_string())?;
    let files = generate(&request, format)?;
    Ok(match encoding {
        Encoding::Protobuf => proto::response_to_protobuf(&files),
        Encoding::Json => proto::response_to_json(&files),
    })
}
