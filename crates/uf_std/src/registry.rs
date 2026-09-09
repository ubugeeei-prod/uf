//! The canonical `@uniflowed/std` module registry.
//!
//! One table naming every std specifier, the capability family it belongs to,
//! what it *is* today, and the Flow exports it owns. Docs generation, the
//! loader and the conformance checks all read this list, so it is the single
//! place a new std module has to be declared.
//!
//! # Why every entry carries a [`StdStatus`]
//!
//! Because for a long time none of them did, and the table was read as an
//! inventory while only ever having been written as a wish. Every entry was
//! constructed by one function that hardcoded `wintertcAligned: true` and
//! `nativeBinding: true`, so the table could not describe a shipped
//! pure-JavaScript module without lying about it — and forty-four of the
//! forty-five specifiers had no file behind them at all while `uf inspect`
//! printed the count as a project fact. This is ubugeeei-prod/uf#710, and it is
//! the same failure `UiReadiness` was added for in ubugeeei-prod/uf#249: a
//! roadmap in the shape of an inventory.
//!
//! No count is written down here, deliberately — a number in a comment is the
//! part of #249 that went wrong. `uf inspect` counts the statuses, and the
//! tests hold each status to `packages/std`, so the count is produced rather
//! than remembered.
//!
//! # What the statuses are held to
//!
//! Not here. Every one of these needs a checkout, and three of them need the
//! Flow parser, so they live in `uf_lib` — which is also where the sibling
//! table's guards are, and being next to
//! `the_registry_names_exactly_what_each_package_exports` is the point.
//!
//! In `crates/uf_lib/src/tests.rs`:
//!
//! * `the_std_registry_names_exactly_what_the_std_package_exports` — the
//!   [`StdStatus::Ships`] specifiers and `packages/std/package.json#exports`
//!   are the same set, each shipped entry's [`StdModule::exports`] is exactly
//!   what its file exports read with uf's own parser, and no `.js` in the
//!   package is left over.
//! * `the_flow_declaration_names_the_same_statuses_and_categories_as_the_registry`
//!   — `packages/std/index.js` declares `StdModule` a second time, for Flow,
//!   and its unions are these enums'.
//!
//! In `crates/uf_lib/tests/package_surface.rs`, where the scanners that read a
//! module without its comments are:
//!
//! * `a_shipping_std_module_is_the_one_that_runs` — a shipped module never
//!   calls `nativeRuntimeRequired`, and the [`StdStatus::Declared`] root always
//!   does. That is `tools/ci/publishable.sh`'s rule, applied per subpath.
//! * `a_std_module_that_claims_wintertc_alignment_names_no_host` — the flag is
//!   a reading rather than an intention: nothing that claims it may import
//!   `node:` anything or read `Buffer` or `process`.
//! * `every_advertised_module_resolves_to_a_package` now walks these specifiers
//!   too. It only ever walked `uf_lib::builtin_modules()`, which is why a std
//!   subpath could be advertised with nothing behind it and every check in the
//!   repository stayed green.
//!
//! And in `tests/library/std.test.js`, the one no cargo test can do: the six
//! specifiers here, `package.json#exports`, and the table in
//! `docs/app/reference/std` are one list in three places.
//!
//! # What `Planned` does not mean
//!
//! It means "nobody has written it", and nothing else. It is not a promise and
//! the number of them is not a roadmap: #710's next tranche — `io`, `bufio`,
//! `encoding/csv`, `encoding/binary`, `hash/crc32`, `slices.BinarySearch`,
//! `encoding/base32`, `container/list`, `time` durations, `path/filepath`,
//! `archive/zip` — is a checklist in that issue, and no row was added here for
//! any of it. A specifier in this table is a name uf advertises, and
//! advertising eleven more names nobody has written is the failure the status
//! field exists to end, not a use for it.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_runtime::RuntimeStandard;

/// Inline export list for std module metadata.
pub type StdExports = SmallVec<[CompactString; 8]>;

/// Inline std module list for registry and docs generation.
pub type StdModuleList = SmallVec<[StdModule; 64]>;

/// What a `@uniflowed/std` specifier is today.
///
/// The three the package's own survey is sorted into — what ships, what is
/// next, what is not ours — plus the one the root needs, because
/// `packages/std/index.js` is a file that exists and does not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StdStatus {
    /// `packages/std` has a file for it, `package.json#exports` names the
    /// subpath, and [`StdModule::exports`] is exactly what that file exports.
    /// Checked against the package on every test run, in both directions.
    Ships,
    /// A declaration surface: the file exists and every function in it raises
    /// `NativeRuntimeRequiredError`. True of `@uniflowed/std` itself and of
    /// nothing else, which is the distinction #710 opens with — a package that
    /// said it had seventy-four functions and ran none of them.
    Declared,
    /// Nobody has written it. The exports are the shape it is expected to
    /// take, which is a design note and not a promise.
    Planned,
    /// Deliberately not a module in this package: the platform or another
    /// `@uniflowed/*` already answers it, or no runtime-agnostic module can.
    /// The entry stays rather than being deleted, because a decision nobody
    /// can find where they went looking for the module gets re-made by the
    /// next contributor — and the comment on each one says which of #710's
    /// buckets it came from and what would reopen it.
    Declined,
}

/// Metadata for a `@uniflowed/std/*` module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdModule {
    /// Module specifier exposed to Flow.
    pub specifier: CompactString,
    /// Capability family for the module.
    pub category: StdCategory,
    /// Whether this is code, a declaration, a plan, or a decision not to write
    /// one. Read [`StdStatus`] before reading any other field: it says which
    /// of them are facts.
    pub status: StdStatus,
    /// Whether the module was *read* and found to use only web primitives.
    ///
    /// A reading, not an intention, and only a [`StdStatus::Ships`] module can
    /// carry one — there is nothing to read for the others. It was a hardcoded
    /// `true` on all forty-five entries, including `@uniflowed/std/net`, whose
    /// stated surface is `TcpListener` and `UdpSocket`.
    pub wintertc_aligned: bool,
    /// Whether the implementation crosses into Rust.
    ///
    /// False everywhere, and that is a fact about this workspace rather than a
    /// default: there is no N-API crate and no `wasm-bindgen` in it, so there
    /// is no boundary for a std module to cross. #710 measured what one would
    /// buy — `bytes.equal` on sixteen bytes is *faster* in JavaScript than
    /// Node's own C++ `Buffer#equals`, because comparing sixteen bytes costs
    /// less than crossing into C++ to do it — and settled the rule: write it in
    /// JavaScript, measure it, and only then argue for native, with the
    /// boundary cost in the argument.
    /// `no_std_module_claims_a_binding_this_workspace_does_not_have` holds
    /// this to the workspace's manifests rather than to care.
    pub native_binding: bool,
    /// Flow exports owned by the module.
    ///
    /// For a [`StdStatus::Ships`] module, exactly the values its file exports.
    /// For a planned or declined one, the shape it was imagined to take. Empty
    /// for the [`StdStatus::Declared`] root, because `uf_lib::builtin_modules()`
    /// owns that list and two lists of the same names is the drift this file's
    /// status field exists to end.
    pub exports: StdExports,
}

impl StdModule {
    /// A module `packages/std` ships.
    pub fn ships(specifier: &str, category: StdCategory, exports: &[&str]) -> Self {
        Self::new(specifier, category, StdStatus::Ships, true, exports)
    }

    /// The root declaration surface, which names its exports elsewhere.
    pub fn declared(specifier: &str, category: StdCategory) -> Self {
        Self::new(specifier, category, StdStatus::Declared, false, &[])
    }

    /// A module nobody has written, and the shape it is expected to take.
    pub fn planned(specifier: &str, category: StdCategory, exports: &[&str]) -> Self {
        Self::new(specifier, category, StdStatus::Planned, false, exports)
    }

    /// A module this package has decided not to have, and the surface that was
    /// once imagined for it.
    pub fn declined(specifier: &str, category: StdCategory, exports: &[&str]) -> Self {
        Self::new(specifier, category, StdStatus::Declined, false, exports)
    }

    fn new(
        specifier: &str,
        category: StdCategory,
        status: StdStatus,
        wintertc_aligned: bool,
        exports: &[&str],
    ) -> Self {
        Self {
            specifier: specifier.to_compact_string(),
            category,
            status,
            wintertc_aligned,
            // Not a parameter. Nothing may set it while the workspace has no
            // binding to cross, and the day something can, the argument for it
            // belongs in the entry rather than in an argument list.
            native_binding: false,
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
///
/// Grouped by [`StdStatus`] rather than by [`StdCategory`], because that is
/// the question this table is now asked: what ships, what is next, and what is
/// not ours. Nothing depends on the order.
pub fn std_modules() -> StdModuleList {
    smallvec::smallvec![
        // ---------------------------------------------------------------
        // The root.
        // ---------------------------------------------------------------
        //
        // `packages/std/index.js` is 274 lines of functions that raise
        // `NativeRuntimeRequiredError`, and it is deliberately still that: the
        // six modules below are separate subpaths so that importing a hex
        // codec brings in a hex codec. Its export list is not repeated here —
        // `uf_lib::builtin_modules()` carries it, and
        // `the_registry_names_exactly_what_each_package_exports` already holds
        // that one to the file.
        StdModule::declared("@uniflowed/std", StdCategory::Types),
        // ---------------------------------------------------------------
        // What ships. ubugeeei-prod/uf#711, the first tranche.
        // ---------------------------------------------------------------
        //
        // Every list below is exactly what the file exports, and a test reads
        // the file rather than trusting that. All six are pure Flow over
        // `Uint8Array`, `TextEncoder`, `AbortController`, `Promise` and
        // `setTimeout`: no `node:` import, no `Buffer`, no binding.
        StdModule::ships(
            "@uniflowed/std/errors",
            StdCategory::Diagnostics,
            &["as", "chain", "is", "join", "unwrap", "wrap"],
        ),
        StdModule::ships(
            "@uniflowed/std/sync",
            StdCategory::Pipeline,
            &["Group", "Mutex", "Semaphore", "WaitGroup", "once"],
        ),
        StdModule::ships(
            "@uniflowed/std/context",
            StdCategory::Pipeline,
            &[
                "CANCELLED",
                "DEADLINE_EXCEEDED",
                "background",
                "fromSignal",
                "key",
                "withCancel",
                "withDeadline",
                "withTimeout",
                "withValue",
            ],
        ),
        StdModule::ships(
            "@uniflowed/std/bytes",
            StdCategory::Data,
            &[
                "Builder",
                "compare",
                "concat",
                "contains",
                "equal",
                "fromUtf8",
                "hasPrefix",
                "hasSuffix",
                "indexOf",
                "join",
                "lastIndexOf",
                "repeat",
                "split",
                "toUtf8",
                "trimPrefix",
                "trimSuffix",
            ],
        ),
        StdModule::ships(
            "@uniflowed/std/heap",
            StdCategory::Data,
            &["Heap", "heapify"],
        ),
        StdModule::ships(
            "@uniflowed/std/hex",
            StdCategory::Data,
            &[
                "InvalidHexError",
                "decode",
                "decodedLength",
                "dump",
                "encode",
                "encodedLength",
                "isValid",
            ],
        ),
        // ---------------------------------------------------------------
        // Planned: nobody has written it.
        // ---------------------------------------------------------------
        //
        // Read [`StdStatus::Planned`] before reading any of these. The exports
        // are the shape somebody imagined, they are checked by nothing, and
        // being here is not a commitment that the module will exist. Four of
        // them are named in #710's next tranche and say so.
        StdModule::planned(
            "@uniflowed/std/vfs",
            StdCategory::FileSystem,
            &["Vfs", "VirtualPath", "mount", "read", "write"],
        ),
        StdModule::planned(
            "@uniflowed/std/types",
            StdCategory::Types,
            &["Brand", "Opaque", "JsonValue", "Result", "AsyncResult"],
        ),
        StdModule::planned(
            "@uniflowed/std/pipeline",
            StdCategory::Pipeline,
            &["lazy", "pipe", "map", "filter", "collect"],
        ),
        StdModule::planned(
            "@uniflowed/std/env",
            StdCategory::Environment,
            &["env", "getEnv", "requiredEnv", "loadDotEnv"],
        ),
        StdModule::planned(
            "@uniflowed/std/format",
            StdCategory::Diagnostics,
            &["formatBytes", "formatDuration", "formatList"],
        ),
        StdModule::planned(
            "@uniflowed/std/stdio",
            StdCategory::Environment,
            &["stdin", "stdout", "stderr", "print", "readLine"],
        ),
        // #710's next tranche names `hash/crc32`, `hash/fnv` and
        // `hash/adler32`, and calls them the strongest native candidate in the
        // list because CRC32 has a hardware instruction. This is the specifier
        // that would answer them; the measurement comes before the binding.
        StdModule::planned(
            "@uniflowed/std/hash",
            StdCategory::Data,
            &["fastHash", "hashBytes", "hashString"],
        ),
        StdModule::planned(
            "@uniflowed/std/debug",
            StdCategory::Diagnostics,
            &["debug", "trace", "span", "channel"],
        ),
        StdModule::planned(
            "@uniflowed/std/defs",
            StdCategory::Types,
            &["definePackageTypes", "defineGlobalTypes", "resolveTypes"],
        ),
        StdModule::planned(
            "@uniflowed/std/colors",
            StdCategory::Diagnostics,
            &["color", "bold", "dim", "red", "green", "cyan"],
        ),
        // `sameBytes` is `bytes.equal`, which ships. What is left is
        // `constantTimeEqual`: WebCrypto has no timing-safe compare and Node's
        // is Node's, so the gap is real and smaller than this entry.
        StdModule::planned(
            "@uniflowed/std/equality",
            StdCategory::Data,
            &["sameBytes", "constantTimeEqual", "shallowEqual"],
        ),
        StdModule::planned(
            "@uniflowed/std/ws",
            StdCategory::Network,
            &["WebSocket", "WebSocketStream", "upgrade", "channel"],
        ),
        StdModule::planned(
            "@uniflowed/std/yaml",
            StdCategory::Serialization,
            &["parseYaml", "stringifyYaml", "detectYaml"],
        ),
        StdModule::planned(
            "@uniflowed/std/toml",
            StdCategory::Serialization,
            &["parseToml", "stringifyToml"],
        ),
        // `groupBy` is `Object.groupBy` and belongs in the declined list;
        // `chunk`, `uniq` and `partition` are not on any platform. The entry
        // stays planned for the three, not the four.
        StdModule::planned(
            "@uniflowed/std/collections",
            StdCategory::Data,
            &["chunk", "uniq", "partition", "groupBy"],
        ),
        StdModule::planned(
            "@uniflowed/std/dotenv",
            StdCategory::Environment,
            &["parseDotEnv", "loadDotEnv", "mergeEnv"],
        ),
        // Not Go's `math`, which is `Math` and is declined below. These four
        // are the ones `Math` does not have.
        StdModule::planned(
            "@uniflowed/std/math",
            StdCategory::Data,
            &["clamp", "lerp", "mean", "percentile"],
        ),
        // #710's next tranche: `path/filepath` — `join`, `clean`, `rel`,
        // `ext`, `base`, `dir` and a real glob matcher. Named in the root
        // declaration surface as `joinPath` and `normalizePath`, and
        // unimplemented there.
        StdModule::planned(
            "@uniflowed/std/path",
            StdCategory::FileSystem,
            &["join", "normalize", "dirname", "basename"],
        ),
        StdModule::planned(
            "@uniflowed/std/wasm",
            StdCategory::Platform,
            &["compileWasm", "instantiateWasm", "WasmModule"],
        ),
        // The other half of `path/filepath`.
        StdModule::planned(
            "@uniflowed/std/glob",
            StdCategory::FileSystem,
            &["glob", "matchGlob", "GlobPattern"],
        ),
        StdModule::planned(
            "@uniflowed/std/cron",
            StdCategory::Cloud,
            &["CronSchedule", "parseCron", "nextRun"],
        ),
        StdModule::planned(
            "@uniflowed/std/s3",
            StdCategory::Cloud,
            &["S3Client", "getObject", "putObject", "presign"],
        ),
        StdModule::planned(
            "@uniflowed/std/sigv4",
            StdCategory::Cloud,
            &["canonicalRequest", "credentialScope", "signRequest"],
        ),
        StdModule::planned(
            "@uniflowed/std/functions",
            StdCategory::Cloud,
            &["defineWorker", "defineLambda", "invokeFunction"],
        ),
        StdModule::planned(
            "@uniflowed/std/uuid",
            StdCategory::Data,
            &["uuidV4", "uuidV7", "parseUuid"],
        ),
        // #710's next tranche: `archive/zip` and `archive/tar`, reading at
        // least. The compression half is `DecompressionStream` on all four
        // runtimes; the container format is not.
        StdModule::planned(
            "@uniflowed/std/zip",
            StdCategory::Data,
            &["ZipReader", "ZipWriter", "deflate", "inflate"],
        ),
        StdModule::planned(
            "@uniflowed/std/import-meta",
            StdCategory::Environment,
            &["importMeta", "resolve", "dirname", "filename"],
        ),
        StdModule::planned(
            "@uniflowed/std/defer",
            StdCategory::Pipeline,
            &["defer", "deferred", "flushDefer", "DeferQueue"],
        ),
        // ---------------------------------------------------------------
        // Declined: not this package's to write.
        // ---------------------------------------------------------------
        //
        // Each of these fails one of #710's two tests. Either the platform or
        // another `@uniflowed/*` already answers it — in which case a port
        // would be slower, less correct, and a second name for something that
        // has one — or a module in `@uniflowed/std` has to work on Node, Bun,
        // Deno and the edge runtimes, and this one cannot.
        //
        // They stay in the table with the reason attached rather than being
        // deleted, because a decision nobody can find where they went looking
        // for the module gets re-made by the next contributor.

        // The host owns the file system. `readFile` is a capability seam the
        // way `uf_std::fs` already is, not a portable module — workerd has no
        // file system at all, and the four runtimes that do disagree about
        // every part of it. Reopened by a host capability contract, not by
        // somebody writing this.
        StdModule::declined(
            "@uniflowed/std/fs",
            StdCategory::FileSystem,
            &["readFile", "writeFile", "stat", "watch", "capabilities"],
        ),
        // Go's `net`: raw TCP, UDP and DNS. Every runtime has its own
        // incompatible sockets and workerd has none, so a portable module
        // cannot be written and a non-portable one is red line 6.
        StdModule::declined(
            "@uniflowed/std/net",
            StdCategory::Network,
            &["TcpListener", "TcpStream", "UdpSocket", "DnsResolver"],
        ),
        // The DNS half of the same row.
        StdModule::declined(
            "@uniflowed/std/dns",
            StdCategory::Network,
            &["resolve", "lookup", "DnsQuery", "DnsRecord"],
        ),
        // Go's `os`, `os/exec`, `os/signal` and `os/user`. `homedir` on
        // workerd is not a smaller answer, it is no answer.
        StdModule::declined(
            "@uniflowed/std/os",
            StdCategory::Platform,
            &["platform", "arch", "availableParallelism", "homedir"],
        ),
        // Go's `net/http`, client and server: `fetch`, `Request`/`Response`,
        // and `@uniflowed/server` for the serving half.
        StdModule::declined(
            "@uniflowed/std/http",
            StdCategory::Network,
            &["serve", "route", "headers", "status"],
        ),
        // Go's `database/sql`: `@uniflowed/orm`.
        StdModule::declined(
            "@uniflowed/std/sql",
            StdCategory::Network,
            &["sql", "driver", "transaction", "migrate"],
        ),
        // Go's `encoding/json`: `JSON`, which is in the runtime and faster
        // than anything written on top of it.
        StdModule::declined(
            "@uniflowed/std/json",
            StdCategory::Serialization,
            &["parseJson", "stringifyJson", "minifyJson"],
        ),
        // Go's `net/url`: `URL`, `URLSearchParams`, `URLPattern`. Two of the
        // four names here are the platform's own, spelled again.
        StdModule::declined(
            "@uniflowed/std/url",
            StdCategory::Platform,
            &["URL", "URLPattern", "parseUrl", "joinUrl"],
        ),
        // The query-string half of `net/url`: `URLSearchParams`.
        StdModule::declined(
            "@uniflowed/std/qs",
            StdCategory::Serialization,
            &["parseQuery", "stringifyQuery", "appendQuery"],
        ),
        // Go's `crypto/sha*`, `crypto/hmac`, `crypto/aes` and `crypto/rand`:
        // WebCrypto, which `crates/uf_check/libdefs/web-crypto.js` already
        // types. A digest written in Flow would be slower and less reviewed.
        StdModule::declined(
            "@uniflowed/std/crypto",
            StdCategory::Data,
            &["digest", "randomBytes", "timingSafeEqual"],
        ),
        // Web Streams are on all four runtimes. #710's `io` item is a bridge
        // to them — `Reader`/`Writer` over `Uint8Array` with adapters — and
        // re-exporting `ReadableStream` under a second name is not that.
        StdModule::declined(
            "@uniflowed/std/stream",
            StdCategory::Platform,
            &["ReadableStream", "WritableStream", "TransformStream"],
        ),
        // `@uniflowed/std/bytes` and `@uniflowed/std/hex` ship, and between
        // them are what this entry wanted. `Buffer` itself is Node's, which
        // makes the name here the wrong one twice over.
        StdModule::declined(
            "@uniflowed/std/buffer",
            StdCategory::Data,
            &["Buffer", "fromBytes", "fromUtf8", "toHex"],
        ),
        // `@uniflowed/std/sync` ships `Mutex` and `Semaphore`. `RWMutex` is
        // deliberately absent: readers and writers cannot run at the same time
        // in one runtime, so it would buy a fairness policy and nothing else,
        // and it should arrive with the workload that needs one.
        StdModule::declined(
            "@uniflowed/std/lock",
            StdCategory::Pipeline,
            &["Mutex", "RwLock", "withLock"],
        ),
        // `@uniflowed/effect` is the package. Not Go's standard library
        // either, on any reading.
        StdModule::declined(
            "@uniflowed/std/effect",
            StdCategory::Pipeline,
            &["Effect", "call", "fork", "race", "resource"],
        ),
        // `@uniflowed/tui` ships the renderer, and `uf inspect` lists its four
        // components. Two names for one thing is the drift this status field
        // exists to end.
        StdModule::declined(
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
        // `@uniflowed/motion` is the name uf advertises for this, and an
        // animation timeline is not in any standard library.
        StdModule::declined(
            "@uniflowed/std/motion",
            StdCategory::Platform,
            &["animate", "timeline", "spring", "reducedMotion"],
        ),
    ]
}

/// Return the runtime standard expected by every std module.
pub fn std_runtime_standard() -> RuntimeStandard {
    RuntimeStandard::WinterTc
}
