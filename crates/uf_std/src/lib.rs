#![deny(missing_docs)]
//! Native standard library contracts and lightweight primitives for `@uniflowed/std`.

use std::hash::Hasher;

use compact_str::{CompactString, ToCompactString};
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use uf_runtime::RuntimeStandard;

/// Inline export list for std module metadata.
pub type StdExports = SmallVec<[CompactString; 8]>;

/// Inline query pair list used by `@uniflowed/std/qs`.
pub type QueryPairs = SmallVec<[QueryPair; 8]>;

/// Small byte buffer optimized for common protocol and hashing paths.
pub type InlineBytes = SmallVec<[u8; 64]>;

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

/// File-system capability exposed by `@uniflowed/std/fs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FsCapability {
    /// Async read operations.
    Read,
    /// Async write operations.
    Write,
    /// Directory traversal.
    Walk,
    /// File watching.
    Watch,
    /// Atomic rename and replace operations.
    AtomicRename,
}

/// Virtual file-system entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VfsEntryKind {
    /// Virtual file.
    File,
    /// Virtual directory.
    Directory,
}

/// Normalized virtual path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPath {
    /// Slash-normalized path text.
    pub path: CompactString,
}

impl VirtualPath {
    /// Create a slash-normalized virtual path.
    pub fn new(path: &str) -> Self {
        Self {
            path: CompactString::from(path.replace('\\', "/")),
        }
    }
}

/// Lazy pipeline description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LazyPipeline {
    /// Pipeline identifier.
    pub name: CompactString,
    /// Deferred steps.
    pub steps: SmallVec<[PipelineStep; 8]>,
}

/// Deferred work descriptor for `@uniflowed/std/defer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredTask {
    /// Task identifier.
    pub id: CompactString,
    /// Scheduling phase.
    pub phase: DeferPhase,
}

impl DeferredTask {
    /// Create a deferred task descriptor.
    pub fn new(id: &str, phase: DeferPhase) -> Self {
        Self {
            id: id.to_compact_string(),
            phase,
        }
    }
}

/// Deferred scheduling phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeferPhase {
    /// Run after the current task boundary.
    Microtask,
    /// Run after I/O has yielded.
    Idle,
    /// Run after response streaming commits.
    PostResponse,
}

impl LazyPipeline {
    /// Create an empty lazy pipeline.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            steps: SmallVec::new(),
        }
    }

    /// Add a deferred step to the pipeline.
    pub fn then(mut self, step: PipelineStep) -> Self {
        self.steps.push(step);
        self
    }
}

/// Deferred pipeline step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineStep {
    /// Map over items.
    Map,
    /// Filter items.
    Filter,
    /// Batch items before execution.
    Batch,
    /// Collect results.
    Collect,
}

/// ANSI style used by `@uniflowed/std/colors`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnsiStyle {
    /// Bold text.
    Bold,
    /// Dim text.
    Dim,
    /// Red foreground.
    Red,
    /// Green foreground.
    Green,
    /// Cyan foreground.
    Cyan,
}

/// Apply an ANSI style to a string.
pub fn colorize(value: &str, style: AnsiStyle, enabled: bool) -> CompactString {
    if !enabled {
        return value.to_compact_string();
    }

    let code = match style {
        AnsiStyle::Bold => "1",
        AnsiStyle::Dim => "2",
        AnsiStyle::Red => "31",
        AnsiStyle::Green => "32",
        AnsiStyle::Cyan => "36",
    };
    let mut output = CompactString::new("");
    output.push_str("\x1b[");
    output.push_str(code);
    output.push('m');
    output.push_str(value);
    output.push_str("\x1b[0m");
    output
}

/// Hash bytes with the native fast hash used by hot uf maps.
pub fn fast_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

/// Hash UTF-8 text with the native fast hash.
pub fn fast_hash_str(value: &str) -> u64 {
    fast_hash_bytes(value.as_bytes())
}

/// Compare bytes without early return on content mismatch.
pub fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0u8;
    for index in 0..left.len() {
        diff |= left[index] ^ right[index];
    }
    diff == 0
}

/// Inline byte buffer for `@uniflowed/std/buffer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteBuffer {
    /// Buffer bytes.
    pub bytes: InlineBytes,
}

impl ByteBuffer {
    /// Create a buffer from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buffer = InlineBytes::new();
        buffer.extend_from_slice(bytes);
        Self { bytes: buffer }
    }

    /// Create a UTF-8 buffer.
    pub fn from_utf8(value: &str) -> Self {
        Self::from_bytes(value.as_bytes())
    }

    /// Return the underlying byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the buffer length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Return whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Encode the buffer as lowercase hexadecimal text.
    pub fn to_hex(&self) -> CompactString {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = CompactString::new("");
        for byte in &self.bytes {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

/// Query string key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPair {
    /// Query key.
    pub key: CompactString,
    /// Query value.
    pub value: CompactString,
}

/// Parse a query string into ordered key-value pairs.
pub fn parse_query(query: &str) -> QueryPairs {
    let input = query.strip_prefix('?').unwrap_or(query);
    let mut pairs = QueryPairs::new();
    for part in input.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut split = part.splitn(2, '=');
        let key = split.next().unwrap_or("");
        let value = split.next().unwrap_or("");
        pairs.push(QueryPair {
            key: percent_decode(key),
            value: percent_decode(value),
        });
    }
    pairs
}

/// Stringify ordered key-value query pairs.
pub fn stringify_query(pairs: &[QueryPair]) -> CompactString {
    let mut output = CompactString::new("");
    for (index, pair) in pairs.iter().enumerate() {
        if index > 0 {
            output.push('&');
        }
        percent_encode(pair.key.as_str(), &mut output);
        output.push('=');
        percent_encode(pair.value.as_str(), &mut output);
    }
    output
}

fn percent_decode(value: &str) -> CompactString {
    let bytes = value.as_bytes();
    let mut output = CompactString::new("");
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hi = hex_value(bytes[index + 1]);
                let lo = hex_value(bytes[index + 2]);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        output.push((hi << 4 | lo) as char);
                        index += 3;
                    }
                    _ => {
                        output.push('%');
                        index += 1;
                    }
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }
    output
}

fn percent_encode(value: &str, output: &mut CompactString) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else if byte == b' ' {
            output.push('+');
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Parsed JSON document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonDocument {
    /// Parsed JSON value.
    pub value: serde_json::Value,
}

/// Parse JSON through serde's native engine.
pub fn parse_json(source: &str) -> Result<JsonDocument, serde_json::Error> {
    serde_json::from_str(source).map(|value| JsonDocument { value })
}

/// Minify JSON source.
pub fn minify_json(source: &str) -> Result<String, serde_json::Error> {
    serde_json::from_str::<serde_json::Value>(source)
        .and_then(|value| serde_json::to_string(&value))
}

/// Parsed TOML document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlDocument {
    /// Parsed TOML value.
    pub value: toml::Value,
}

/// Parse TOML through the native Rust TOML parser.
pub fn parse_toml(source: &str) -> Result<TomlDocument, toml::de::Error> {
    toml::from_str(source).map(|value| TomlDocument { value })
}

/// Coarse YAML document kind for fast dispatch before full parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YamlDocumentKind {
    /// YAML mapping document.
    Mapping,
    /// YAML sequence document.
    Sequence,
    /// YAML scalar document.
    Scalar,
    /// Empty YAML document.
    Empty,
}

/// Detect a YAML document's coarse shape without allocating a parser tree.
pub fn detect_yaml(source: &str) -> YamlDocumentKind {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("- ") {
            return YamlDocumentKind::Sequence;
        }
        if trimmed.contains(':') {
            return YamlDocumentKind::Mapping;
        }
        return YamlDocumentKind::Scalar;
    }
    YamlDocumentKind::Empty
}

/// HTTP method contract for `@uniflowed/std/http`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// GET.
    Get,
    /// POST.
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
    /// DELETE.
    Delete,
}

/// HTTP route descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRoute {
    /// Method matched by the route.
    pub method: HttpMethod,
    /// Route path.
    pub path: CompactString,
}

/// WebSocket mode used by `@uniflowed/std/ws`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSocketMode {
    /// WebSocket interface.
    WebSocket,
    /// Stream-oriented WebSocket contract.
    WebSocketStream,
}

/// SQL driver kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SqlDriverKind {
    /// SQLite driver.
    Sqlite,
    /// Postgres driver.
    Postgres,
    /// MySQL driver.
    Mysql,
}

/// SQL driver descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlDriver {
    /// Driver kind.
    pub kind: SqlDriverKind,
    /// Whether statements are prepared by default.
    pub prepared_by_default: bool,
}

/// Lock mode for mutex and reader-writer lock wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LockMode {
    /// Exclusive lock.
    Exclusive,
    /// Shared read lock.
    Shared,
}

/// Debug event emitted by `@uniflowed/std/debug`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugEvent {
    /// Debug channel.
    pub channel: CompactString,
    /// Message payload.
    pub message: CompactString,
}

/// Create a debug event.
pub fn debug_event(channel: &str, message: &str) -> DebugEvent {
    DebugEvent {
        channel: channel.to_compact_string(),
        message: message.to_compact_string(),
    }
}

/// Split a slice into fixed-size borrowed chunks.
pub fn chunk<T>(items: &[T], size: usize) -> SmallVec<[&[T]; 8]> {
    let mut chunks = SmallVec::new();
    if size == 0 {
        return chunks;
    }
    for chunk in items.chunks(size) {
        chunks.push(chunk);
    }
    chunks
}

/// Clamp an integer between inclusive bounds.
pub fn clamp(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

/// Linearly interpolate two floating point values.
pub fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
}

/// Host OS family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OsFamily {
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
    /// Windows.
    Windows,
    /// Unknown or unsupported OS.
    Unknown,
}

/// Host OS descriptor for `@uniflowed/std/os`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    /// OS family.
    pub family: OsFamily,
    /// CPU architecture.
    pub arch: CompactString,
    /// Available parallelism.
    pub available_parallelism: usize,
}

impl OsInfo {
    /// Create an OS descriptor.
    pub fn new(family: OsFamily, arch: &str, available_parallelism: usize) -> Self {
        Self {
            family,
            arch: arch.to_compact_string(),
            available_parallelism,
        }
    }
}

/// DNS record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    /// IPv4 address record.
    A,
    /// IPv6 address record.
    Aaaa,
    /// Canonical name record.
    Cname,
    /// Mail exchange record.
    Mx,
    /// Text record.
    Txt,
}

/// DNS query descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsQuery {
    /// Record name.
    pub name: CompactString,
    /// Record type.
    pub record_type: DnsRecordType,
}

impl DnsQuery {
    /// Create a DNS query descriptor.
    pub fn new(name: &str, record_type: DnsRecordType) -> Self {
        Self {
            name: name.to_compact_string(),
            record_type,
        }
    }
}

/// Join path segments with slash normalization.
pub fn join_path(parts: &[&str]) -> CompactString {
    let mut output = CompactString::new("");
    for part in parts {
        let trimmed = part.trim_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('/');
        }
        output.push_str(trimmed);
    }
    output
}

/// Normalize slash separators and remove `.` path segments.
pub fn normalize_path(path: &str) -> CompactString {
    let replaced = path.replace('\\', "/");
    let mut segments = SmallVec::<[&str; 16]>::new();
    for segment in replaced.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            value => segments.push(value),
        }
    }
    join_path(&segments)
}

/// Stream direction for WinterTC-compatible stream wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StreamKind {
    /// Readable stream.
    Readable,
    /// Writable stream.
    Writable,
    /// Transform stream.
    Transform,
}

/// Lightweight stream descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDescriptor {
    /// Stream kind.
    pub kind: StreamKind,
    /// Whether backpressure is part of the contract.
    pub backpressure: bool,
}

impl StreamDescriptor {
    /// Create a stream descriptor.
    pub fn new(kind: StreamKind) -> Self {
        Self {
            kind,
            backpressure: true,
        }
    }
}

/// Parsed URL descriptor for typed wrappers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlParts {
    /// URL scheme without the trailing colon.
    pub scheme: CompactString,
    /// Host component.
    pub host: CompactString,
    /// Path component.
    pub path: CompactString,
}

/// Parse a simple absolute URL without allocating a full URL object graph.
pub fn parse_url(value: &str) -> Option<UrlParts> {
    let (scheme, rest) = value.split_once("://")?;
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path),
        None => (rest, ""),
    };
    Some(UrlParts {
        scheme: scheme.to_compact_string(),
        host: host.to_compact_string(),
        path: join_path(&[path]),
    })
}

/// WebAssembly module descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmModulePlan {
    /// Module name.
    pub name: CompactString,
    /// Whether the module should be compiled ahead of time.
    pub ahead_of_time: bool,
}

impl WasmModulePlan {
    /// Create a native WebAssembly module plan.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            ahead_of_time: true,
        }
    }
}

/// Glob pattern descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobPattern {
    /// Original pattern.
    pub pattern: CompactString,
    /// Whether dotfiles are matched.
    pub dotfiles: bool,
}

impl GlobPattern {
    /// Create a glob pattern descriptor.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_compact_string(),
            dotfiles: false,
        }
    }

    /// Match the subset of glob syntax used for fast include filters.
    pub fn matches(&self, path: &str) -> bool {
        match self.pattern.split_once('*') {
            Some((prefix, suffix)) => path.starts_with(prefix) && path.ends_with(suffix),
            None => self.pattern == path,
        }
    }
}

/// Motion easing curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionEase {
    /// Linear easing.
    Linear,
    /// Standard ease-out.
    Out,
    /// Spring-like native easing.
    Spring,
}

/// Motion transition descriptor for `@uniflowed/std/motion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionTransition {
    /// Duration in milliseconds.
    pub duration_ms: u16,
    /// Easing curve.
    pub ease: MotionEase,
    /// Whether reduced motion is respected.
    pub respects_reduced_motion: bool,
}

/// Cron schedule descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronSchedule {
    /// Minute field.
    pub minute: CompactString,
    /// Hour field.
    pub hour: CompactString,
    /// Day-of-month field.
    pub day_of_month: CompactString,
    /// Month field.
    pub month: CompactString,
    /// Day-of-week field.
    pub day_of_week: CompactString,
}

/// Parse a five-field cron schedule.
pub fn parse_cron(source: &str) -> Option<CronSchedule> {
    let mut parts = source.split_whitespace();
    let minute = parts.next()?;
    let hour = parts.next()?;
    let day_of_month = parts.next()?;
    let month = parts.next()?;
    let day_of_week = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(CronSchedule {
        minute: minute.to_compact_string(),
        hour: hour.to_compact_string(),
        day_of_month: day_of_month.to_compact_string(),
        month: month.to_compact_string(),
        day_of_week: day_of_week.to_compact_string(),
    })
}

/// S3 object operation descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3ObjectRequest {
    /// Bucket name.
    pub bucket: CompactString,
    /// Object key.
    pub key: CompactString,
    /// Whether the operation should use SigV4 signing.
    pub sigv4: bool,
}

impl S3ObjectRequest {
    /// Create a signed S3 object request descriptor.
    pub fn new(bucket: &str, key: &str) -> Self {
        Self {
            bucket: bucket.to_compact_string(),
            key: key.to_compact_string(),
            sigv4: true,
        }
    }
}

/// SigV4 credential scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigV4Scope {
    /// AWS region.
    pub region: CompactString,
    /// Service name.
    pub service: CompactString,
}

impl SigV4Scope {
    /// Create a SigV4 credential scope.
    pub fn new(region: &str, service: &str) -> Self {
        Self {
            region: region.to_compact_string(),
            service: service.to_compact_string(),
        }
    }
}

/// Function runtime target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FunctionRuntime {
    /// Worker-compatible runtime.
    Worker,
    /// AWS Lambda-compatible runtime.
    Lambda,
}

/// Function descriptor used by deploy-anywhere adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDescriptor {
    /// Function name.
    pub name: CompactString,
    /// Runtime target.
    pub runtime: FunctionRuntime,
    /// Entry module.
    pub entry: CompactString,
}

impl FunctionDescriptor {
    /// Create a function descriptor.
    pub fn new(name: &str, runtime: FunctionRuntime, entry: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            runtime,
            entry: entry.to_compact_string(),
        }
    }
}

impl MotionTransition {
    /// Create a transition that respects reduced motion by default.
    pub fn new(duration_ms: u16, ease: MotionEase) -> Self {
        Self {
            duration_ms,
            ease,
            respects_reduced_motion: true,
        }
    }
}

/// Terminal color depth exposed by `@uniflowed/std/tui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalColorDepth {
    /// 16-color ANSI terminal.
    Ansi16,
    /// 256-color ANSI terminal.
    Ansi256,
    /// 24-bit true color terminal.
    TrueColor,
}

/// Terminal capability descriptor for native TUI rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCapabilities {
    /// Terminal columns.
    pub columns: u16,
    /// Terminal rows.
    pub rows: u16,
    /// Supported color depth.
    pub color_depth: TerminalColorDepth,
    /// Whether Unicode graphemes are supported.
    pub unicode: bool,
    /// Whether mouse input is supported.
    pub mouse: bool,
    /// Whether inline image protocols are available.
    pub inline_images: bool,
    /// Whether sixel images are available.
    pub sixel: bool,
}

impl TerminalCapabilities {
    /// Create terminal capabilities with true-color Unicode defaults.
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            columns,
            rows,
            color_depth: TerminalColorDepth::TrueColor,
            unicode: true,
            mouse: true,
            inline_images: false,
            sixel: false,
        }
    }

    /// Enable inline image protocols.
    pub fn with_inline_images(mut self) -> Self {
        self.inline_images = true;
        self
    }

    /// Return whether high fidelity rendering is available.
    pub fn high_fidelity(&self) -> bool {
        self.color_depth == TerminalColorDepth::TrueColor && self.unicode
    }
}

/// Create a terminal capability descriptor.
pub fn terminal_capabilities(columns: u16, rows: u16) -> TerminalCapabilities {
    TerminalCapabilities::new(columns, rows)
}

/// Dotenv key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotEnvPair {
    /// Environment variable name.
    pub key: CompactString,
    /// Environment variable value.
    pub value: CompactString,
}

/// Runtime-safe `import.meta` descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMeta {
    /// Module URL.
    pub url: CompactString,
    /// Directory name, when available for the host.
    pub dirname: Option<CompactString>,
    /// File name, when available for the host.
    pub filename: Option<CompactString>,
}

impl ImportMeta {
    /// Create an import meta descriptor.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_compact_string(),
            dirname: None,
            filename: None,
        }
    }

    /// Attach directory and filename fields.
    pub fn with_file(mut self, dirname: &str, filename: &str) -> Self {
        self.dirname = Some(dirname.to_compact_string());
        self.filename = Some(filename.to_compact_string());
        self
    }
}

/// Inline dotenv pair list.
pub type DotEnvPairs = SmallVec<[DotEnvPair; 16]>;

/// Parse simple `.env` files without executing shell syntax.
pub fn parse_dotenv(source: &str) -> DotEnvPairs {
    let mut pairs = DotEnvPairs::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        pairs.push(DotEnvPair {
            key: key.trim().to_compact_string(),
            value: trim_env_value(value.trim()),
        });
    }
    pairs
}

fn trim_env_value(value: &str) -> CompactString {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let quoted = (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'');
        if quoted {
            return value[1..value.len() - 1].to_compact_string();
        }
    }
    value.to_compact_string()
}

/// Supported crypto digest algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DigestAlgorithm {
    /// Fast non-cryptographic hash for caches and maps.
    FastHash,
    /// SHA-256 contract for WebCrypto-compatible bindings.
    Sha256,
}

/// Digest bytes with the requested algorithm.
pub fn digest_bytes(algorithm: DigestAlgorithm, bytes: &[u8]) -> CompactString {
    match algorithm {
        DigestAlgorithm::FastHash => hex_u64(fast_hash_bytes(bytes)),
        DigestAlgorithm::Sha256 => hex_bytes(Sha256::digest(bytes).as_slice()),
    }
}

fn hex_bytes(bytes: &[u8]) -> CompactString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = CompactString::new("");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_u64(value: u64) -> CompactString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = CompactString::new("");
    for shift in (0..64).step_by(4).rev() {
        let index = ((value >> shift) & 0x0f) as usize;
        output.push(HEX[index] as char);
    }
    output
}

/// UUID version supported by the native generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UuidVersion {
    /// Random UUID v4.
    V4,
    /// Time-ordered UUID v7.
    V7,
}

/// Parsed UUID descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedUuid {
    /// Lowercase canonical UUID text.
    pub value: CompactString,
    /// UUID version nibble when present.
    pub version: Option<u8>,
}

/// Parse canonical UUID text.
pub fn parse_uuid(value: &str) -> Option<ParsedUuid> {
    let bytes = value.as_bytes();
    let dash_positions = [8, 13, 18, 23];
    if bytes.len() != 36 {
        return None;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if dash_positions.contains(&index) {
            if *byte != b'-' {
                return None;
            }
        } else if !byte.is_ascii_hexdigit() {
            return None;
        }
    }
    let version = hex_value(bytes[14]);
    Some(ParsedUuid {
        value: value.to_ascii_lowercase().into(),
        version,
    })
}

/// ZIP compression mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZipCompression {
    /// Store files without compression.
    Store,
    /// Deflate compression.
    Deflate,
}

/// ZIP archive entry descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipEntry {
    /// Entry path inside the archive.
    pub path: CompactString,
    /// Compression mode.
    pub compression: ZipCompression,
    /// Uncompressed size in bytes.
    pub size: u64,
}

#[cfg(test)]
mod tests;
