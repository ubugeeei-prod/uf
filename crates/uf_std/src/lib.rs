#![deny(missing_docs)]
//! Native standard library contracts and lightweight primitives for `@uniflowed/std`.
//!
//! The crate is organised the way [`StdCategory`] is: one module per capability
//! family, and [`std_modules`] names every `@uniflowed/std/*` specifier those
//! families back.

mod cloud;
mod data;
mod diagnostics;
mod environment;
mod fs;
mod network;
mod pipeline;
mod platform;
mod registry;
mod serialization;

pub use cloud::{
    CronSchedule, FunctionDescriptor, FunctionRuntime, S3ObjectRequest, SigV4Scope, parse_cron,
};
pub use data::{
    ByteBuffer, DigestAlgorithm, InlineBytes, ParsedUuid, UuidVersion, ZipCompression, ZipEntry,
    chunk, clamp, constant_time_equal, digest_bytes, fast_hash_bytes, fast_hash_str, lerp,
    parse_uuid,
};
pub use diagnostics::{AnsiStyle, DebugEvent, colorize, debug_event};
pub use environment::{DotEnvPair, DotEnvPairs, ImportMeta, parse_dotenv};
pub use fs::{FsCapability, GlobPattern, VfsEntryKind, VirtualPath, join_path, normalize_path};
pub use network::{
    DnsQuery, DnsRecordType, HttpMethod, HttpRoute, SqlDriver, SqlDriverKind, WebSocketMode,
};
pub use pipeline::{DeferPhase, DeferredTask, LazyPipeline, LockMode, PipelineStep};
pub use platform::{
    MotionEase, MotionTransition, OsFamily, OsInfo, StreamDescriptor, StreamKind,
    TerminalCapabilities, TerminalColorDepth, UrlParts, WasmModulePlan, parse_url,
    terminal_capabilities,
};
pub use registry::{
    StdCategory, StdExports, StdModule, StdModuleList, std_modules, std_runtime_standard,
};
pub use serialization::{
    JsonDocument, QueryPair, QueryPairs, TomlDocument, YamlDocumentKind, detect_yaml, minify_json,
    parse_json, parse_query, parse_toml, stringify_query,
};

#[cfg(test)]
mod tests;
