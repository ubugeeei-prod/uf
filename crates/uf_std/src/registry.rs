//! The canonical `@uniflowed/std` module registry.
//!
//! One table naming every std specifier, the capability family it belongs to,
//! and the Flow exports it owns. Docs generation, the loader and the conformance
//! checks all read this list, so it is the single place a new std module has to
//! be declared.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_runtime::RuntimeStandard;

/// Inline export list for std module metadata.
pub type StdExports = SmallVec<[CompactString; 8]>;

/// Inline std module list for registry and docs generation.
pub type StdModuleList = SmallVec<[StdModule; 64]>;

/// Metadata for a `@uniflowed/std/*` module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdModule {
    /// Module specifier exposed to Flow.
    pub specifier: CompactString,
    /// Capability family for the module.
    pub category: StdCategory,
    /// Whether the public API follows WinterTC-compatible web primitives.
    pub wintertc_aligned: bool,
    /// Whether the backing implementation is intended to be Rust native.
    pub native_binding: bool,
    /// Flow exports owned by the module.
    pub exports: StdExports,
}

impl StdModule {
    /// Create module metadata.
    pub fn new(specifier: &str, category: StdCategory, exports: &[&str]) -> Self {
        Self {
            specifier: specifier.to_compact_string(),
            category,
            wintertc_aligned: true,
            native_binding: true,
            exports: exports
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
        }
    }
}

/// `@uniflowed/std` capability family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StdCategory {
    /// Virtual and host file systems.
    FileSystem,
    /// Typed utility declarations and compatibility definitions.
    Types,
    /// Lazy computation and effect pipelines.
    Pipeline,
    /// Host environment and stdio.
    Environment,
    /// Formatting, ANSI, and debug helpers.
    Diagnostics,
    /// Hashing, equality, buffers, and encodings.
    Data,
    /// HTTP, WebSocket, and SQL networking.
    Network,
    /// JSON, YAML, and TOML serialization.
    Serialization,
    /// Host OS, URL, stream, WebAssembly, glob, and motion helpers.
    Platform,
    /// Cloud storage, signatures, functions, and schedulers.
    Cloud,
}

/// Return the canonical std module registry.
pub fn std_modules() -> StdModuleList {
    smallvec::smallvec![
        StdModule::new(
            "@uniflowed/std",
            StdCategory::Types,
            &["modules", "wintertc", "native"],
        ),
        StdModule::new(
            "@uniflowed/std/vfs",
            StdCategory::FileSystem,
            &["Vfs", "VirtualPath", "mount", "read", "write"],
        ),
        StdModule::new(
            "@uniflowed/std/fs",
            StdCategory::FileSystem,
            &["readFile", "writeFile", "stat", "watch", "capabilities"],
        ),
        StdModule::new(
            "@uniflowed/std/types",
            StdCategory::Types,
            &["Brand", "Opaque", "JsonValue", "Result", "AsyncResult"],
        ),
        StdModule::new(
            "@uniflowed/std/pipeline",
            StdCategory::Pipeline,
            &["lazy", "pipe", "map", "filter", "collect"],
        ),
        StdModule::new(
            "@uniflowed/std/effect",
            StdCategory::Pipeline,
            &["Effect", "call", "fork", "race", "resource"],
        ),
        StdModule::new(
            "@uniflowed/std/env",
            StdCategory::Environment,
            &["env", "getEnv", "requiredEnv", "loadDotEnv"],
        ),
        StdModule::new(
            "@uniflowed/std/format",
            StdCategory::Diagnostics,
            &["formatBytes", "formatDuration", "formatList"],
        ),
        StdModule::new(
            "@uniflowed/std/stdio",
            StdCategory::Environment,
            &["stdin", "stdout", "stderr", "print", "readLine"],
        ),
        StdModule::new(
            "@uniflowed/std/hash",
            StdCategory::Data,
            &["fastHash", "hashBytes", "hashString"],
        ),
        StdModule::new(
            "@uniflowed/std/debug",
            StdCategory::Diagnostics,
            &["debug", "trace", "span", "channel"],
        ),
        StdModule::new(
            "@uniflowed/std/defs",
            StdCategory::Types,
            &["definePackageTypes", "defineGlobalTypes", "resolveTypes"],
        ),
        StdModule::new(
            "@uniflowed/std/lock",
            StdCategory::Pipeline,
            &["Mutex", "RwLock", "withLock"],
        ),
        StdModule::new(
            "@uniflowed/std/colors",
            StdCategory::Diagnostics,
            &["color", "bold", "dim", "red", "green", "cyan"],
        ),
        StdModule::new(
            "@uniflowed/std/qs",
            StdCategory::Serialization,
            &["parseQuery", "stringifyQuery", "appendQuery"],
        ),
        StdModule::new(
            "@uniflowed/std/equality",
            StdCategory::Data,
            &["sameBytes", "constantTimeEqual", "shallowEqual"],
        ),
        StdModule::new(
            "@uniflowed/std/http",
            StdCategory::Network,
            &["serve", "route", "headers", "status"],
        ),
        StdModule::new(
            "@uniflowed/std/buffer",
            StdCategory::Data,
            &["Buffer", "fromBytes", "fromUtf8", "toHex"],
        ),
        StdModule::new(
            "@uniflowed/std/ws",
            StdCategory::Network,
            &["WebSocket", "WebSocketStream", "upgrade", "channel"],
        ),
        StdModule::new(
            "@uniflowed/std/sql",
            StdCategory::Network,
            &["sql", "driver", "transaction", "migrate"],
        ),
        StdModule::new(
            "@uniflowed/std/json",
            StdCategory::Serialization,
            &["parseJson", "stringifyJson", "minifyJson"],
        ),
        StdModule::new(
            "@uniflowed/std/yaml",
            StdCategory::Serialization,
            &["parseYaml", "stringifyYaml", "detectYaml"],
        ),
        StdModule::new(
            "@uniflowed/std/toml",
            StdCategory::Serialization,
            &["parseToml", "stringifyToml"],
        ),
        StdModule::new(
            "@uniflowed/std/collections",
            StdCategory::Data,
            &["chunk", "uniq", "partition", "groupBy"],
        ),
        StdModule::new(
            "@uniflowed/std/crypto",
            StdCategory::Data,
            &["digest", "randomBytes", "timingSafeEqual"],
        ),
        StdModule::new(
            "@uniflowed/std/dotenv",
            StdCategory::Environment,
            &["parseDotEnv", "loadDotEnv", "mergeEnv"],
        ),
        StdModule::new(
            "@uniflowed/std/math",
            StdCategory::Data,
            &["clamp", "lerp", "mean", "percentile"],
        ),
        StdModule::new(
            "@uniflowed/std/os",
            StdCategory::Platform,
            &["platform", "arch", "availableParallelism", "homedir"],
        ),
        StdModule::new(
            "@uniflowed/std/net",
            StdCategory::Network,
            &["TcpListener", "TcpStream", "UdpSocket", "DnsResolver"],
        ),
        StdModule::new(
            "@uniflowed/std/dns",
            StdCategory::Network,
            &["resolve", "lookup", "DnsQuery", "DnsRecord"],
        ),
        StdModule::new(
            "@uniflowed/std/path",
            StdCategory::FileSystem,
            &["join", "normalize", "dirname", "basename"],
        ),
        StdModule::new(
            "@uniflowed/std/stream",
            StdCategory::Platform,
            &["ReadableStream", "WritableStream", "TransformStream"],
        ),
        StdModule::new(
            "@uniflowed/std/url",
            StdCategory::Platform,
            &["URL", "URLPattern", "parseUrl", "joinUrl"],
        ),
        StdModule::new(
            "@uniflowed/std/wasm",
            StdCategory::Platform,
            &["compileWasm", "instantiateWasm", "WasmModule"],
        ),
        StdModule::new(
            "@uniflowed/std/glob",
            StdCategory::FileSystem,
            &["glob", "matchGlob", "GlobPattern"],
        ),
        StdModule::new(
            "@uniflowed/std/motion",
            StdCategory::Platform,
            &["animate", "timeline", "spring", "reducedMotion"],
        ),
        StdModule::new(
            "@uniflowed/std/tui",
            StdCategory::Platform,
            &[
                "TerminalCapabilities",
                "detectTerminal",
                "ansi",
                "mouse",
                "images",
            ],
        ),
        StdModule::new(
            "@uniflowed/std/cron",
            StdCategory::Cloud,
            &["CronSchedule", "parseCron", "nextRun"],
        ),
        StdModule::new(
            "@uniflowed/std/s3",
            StdCategory::Cloud,
            &["S3Client", "getObject", "putObject", "presign"],
        ),
        StdModule::new(
            "@uniflowed/std/sigv4",
            StdCategory::Cloud,
            &["canonicalRequest", "credentialScope", "signRequest"],
        ),
        StdModule::new(
            "@uniflowed/std/functions",
            StdCategory::Cloud,
            &["defineWorker", "defineLambda", "invokeFunction"],
        ),
        StdModule::new(
            "@uniflowed/std/uuid",
            StdCategory::Data,
            &["uuidV4", "uuidV7", "parseUuid"],
        ),
        StdModule::new(
            "@uniflowed/std/zip",
            StdCategory::Data,
            &["ZipReader", "ZipWriter", "deflate", "inflate"],
        ),
        StdModule::new(
            "@uniflowed/std/import-meta",
            StdCategory::Environment,
            &["importMeta", "resolve", "dirname", "filename"],
        ),
        StdModule::new(
            "@uniflowed/std/defer",
            StdCategory::Pipeline,
            &["defer", "deferred", "flushDefer", "DeferQueue"],
        ),
    ]
}

/// Return the runtime standard expected by every std module.
pub fn std_runtime_standard() -> RuntimeStandard {
    RuntimeStandard::WinterTc
}
