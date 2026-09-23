//! Compile what a run's workers are about to import, once, before they start.
//!
//! # Why
//!
//! A worker imports every module through `@uniflowed/host`'s loader, which
//! reads `.uf/cache/transform/` and, on a miss, compiles the module through a
//! `uf transform` process *of its own*. On a warm cache that costs nothing. On
//! a cold one it costs every worker the same work: each of them misses on the
//! whole of `@uniflowed/test`'s runtime and on every module its files share,
//! starts its own `uf transform`, and compiles them all again. Measured on the
//! CI runner's thirty-two cores, the 50-file suite spent 5.9 s of CPU on a cold
//! run against 0.3 s on a warm one, most of it compiling the same thirty
//! modules thirty-two times over.
//!
//! So before the pool starts, this walks what the workers will import — the
//! test files, their relative imports, and the `@uniflowed/*` packages they and
//! the worker reach — and compiles whatever the cache does not hold yet, each
//! module once, spread over a few `uf transform` processes. The workers then
//! find every one of them on disk.
//!
//! # It cannot change what a module compiles to
//!
//! The entries are written exactly where, and exactly as, the loader would
//! have written them, and they are compiled by the loader's own service:
//!
//! * **The compiler** is `uf transform`, the process the loader spawns, started
//!   the way `packages/host/transform.js` starts it — `uf --cwd <root>
//!   transform`, with the environment a worker has — and asked with the options
//!   `packages/host/internal/flow-cache.js`'s `compileOptions` sends. Nothing
//!   here calls the transform in-process, so a module that crashes the
//!   compiler crashes a `uf transform` process and not the run, exactly as
//!   before.
//! * **The key** is `cacheEntryFor`'s: SHA-256 over the loader's cache version,
//!   the identity of the binary the service executes (read before it starts,
//!   as `TransformService` reads it), the in-source flag every worker compiles
//!   with, the module's real path and its text.
//! * **The entry** is `framed`'s: the code, and its source map as an inline
//!   `data:` URL.
//!
//! `crates/uf_cli/tests/testing.rs` holds the two to the same bytes: a run with
//! this switched off and a run with it on leave identical directories behind.
//!
//! # What it never does
//!
//! Fail a run. A module it cannot read, cannot resolve or cannot compile is
//! left for the worker, which meets it exactly as it did before this existed —
//! including a syntax error, which is the worker's to report with its code
//! frame. `UF_TEST_PREWARM=0` switches it off.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use camino::Utf8Path;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use uf_infra::FxHashSet;
use uf_rsc::{ImportKind, scan_imports};
use uf_test::{HostCommand, HostKind};

/// The loader's cache version, `CACHE_VERSION` in
/// `packages/host/internal/flow-cache.js`. The two must move together.
const LOADER_CACHE_VERSION: &str = "3";

/// Every worker compiles with in-source tests on; `uf_test::host` sets
/// `UF_IN_SOURCE_TESTS=1` on each one, and the loader keys on it.
const IN_SOURCE: &str = "in-source";

/// Most modules one walk visits. A suite that reaches more is warmed as far as
/// this and the workers compile the rest, as they always did.
const MAX_MODULES: usize = 20_000;

/// Fewest missing modules one `uf transform` process is started for: below
/// this, another process costs more to start than it saves.
const MODULES_PER_SERVICE: usize = 8;

/// The export conditions a worker's `import` resolves with.
const CONDITIONS: [&str; 4] = ["node", "import", "module-sync", "default"];

/// What [`warm`] did, for the tests and for anyone profiling a run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Warmed {
    /// Modules the walk reached that the loader would compile.
    pub(crate) reached: usize,
    /// Of those, how many the cache did not hold and were compiled here.
    pub(crate) compiled: usize,
}

/// Compile what `host`'s workers will import from `test_files` and is not yet
/// in the transform cache.
pub(crate) fn warm(host: &HostCommand, test_files: &[&Utf8Path]) -> Warmed {
    if std::env::var_os("UF_TEST_PREWARM").is_some_and(|value| value == "0") {
        return Warmed::default();
    }
    // The hosts whose loader reads `.uf/cache/transform`: Node's two loaders,
    // Deno's, and Bun's preload. A browser run's modules are served by the
    // driver, and a React Native run loads through Jest's module environment.
    if !matches!(host.kind, HostKind::Node | HostKind::Bun | HostKind::Deno)
        || host
            .env
            .iter()
            .any(|(name, value)| name == "UF_TEST_TARGET" && value == "react-native")
    {
        return Warmed::default();
    }
    let Some(binary) = host.uf_binary.as_deref() else {
        return Warmed::default();
    };
    // No identity, no cache: the loader neither reads nor writes one, so an
    // entry written here would be one nothing reads.
    let Some(identity) = binary_identity(binary.as_std_path()) else {
        return Warmed::default();
    };

    let mut seeds: Vec<PathBuf> = test_files
        .iter()
        .map(|file| file.as_std_path().to_path_buf())
        .collect();
    seeds.push(host.worker.as_std_path().to_path_buf());
    let modules = reach(&seeds);
    let directory = host
        .root
        .as_std_path()
        .join(".uf")
        .join("cache")
        .join("transform");
    let missing: Vec<Module> = modules
        .into_iter()
        .map(|(path, source)| {
            let entry = directory.join(entry_name(&identity, &path, &source));
            Module {
                path,
                source,
                entry,
            }
        })
        .collect();
    let reached = missing.len();
    let missing: Vec<Module> = missing
        .into_iter()
        .filter(|module| !module.entry.exists())
        .collect();
    if missing.is_empty() {
        return Warmed {
            reached,
            compiled: 0,
        };
    }
    let compiled = compile(host, binary, missing);
    Warmed { reached, compiled }
}

/// One module to compile, and the entry it belongs in.
struct Module {
    /// The real path, which is what the loader names a module by.
    path: String,
    source: String,
    entry: PathBuf,
}

/// The file name `cacheEntryFor` gives a module.
fn entry_name(identity: &str, path: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    for field in [LOADER_CACHE_VERSION, identity, IN_SOURCE, path] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(digest.len() * 2 + ".mjs".len());
    for byte in digest {
        name.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        name.push(char::from(b"0123456789abcdef"[usize::from(byte & 0xf)]));
    }
    name.push_str(".mjs");
    name
}

/// `ufBinaryIdentity`: the path as the worker is handed it, the size, and the
/// modification time in whole milliseconds.
fn binary_identity(binary: &Path) -> Option<String> {
    let metadata = std::fs::metadata(binary).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some(format!(
        "{}\0{}\0{modified}",
        binary.to_str()?,
        metadata.len()
    ))
}

/// Every module the loader would compile that `seeds` reach through static
/// imports, by real path, with its text.
///
/// Relative specifiers resolve exactly as written, as they do in an ES module;
/// `@uniflowed/*` ones through the package's `exports`. Anything else is a
/// package the loader leaves alone, or a builtin. A dynamic `import()` is not
/// followed: whether it runs is the program's decision, and warming a module
/// nothing loads is work for nothing.
fn reach(seeds: &[PathBuf]) -> Vec<(String, String)> {
    let mut seen: FxHashSet<PathBuf> = FxHashSet::default();
    let mut queue: VecDeque<PathBuf> = seeds.iter().cloned().collect();
    let mut found = Vec::new();
    while let Some(next) = queue.pop_front() {
        if seen.len() >= MAX_MODULES {
            break;
        }
        let Ok(path) = std::fs::canonicalize(&next) else {
            continue;
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(name) = path.to_str() else {
            continue;
        };
        if !uf_transform::is_flow_module(name) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let directory = path.parent().unwrap_or(Path::new("/"));
        for import in scan_imports(&source).iter() {
            if !matches!(import.kind, ImportKind::Static | ImportKind::ReExport) {
                continue;
            }
            let specifier = import.specifier.as_str();
            if specifier.starts_with("./") || specifier.starts_with("../") {
                queue.push_back(directory.join(specifier));
            } else if let Some(target) = resolve_uniflowed(directory, specifier) {
                queue.push_back(target);
            }
        }
        found.push((name.to_owned(), source));
    }
    found
}

/// Where an `@uniflowed/*` specifier imported from `directory` points, or
/// [`None`] for any other specifier or one this cannot follow.
fn resolve_uniflowed(directory: &Path, specifier: &str) -> Option<PathBuf> {
    let rest = specifier.strip_prefix("@uniflowed/")?;
    let (name, subpath) = match rest.split_once('/') {
        Some((name, sub)) => (name, format!("./{sub}")),
        None => (rest, String::from(".")),
    };
    if name.is_empty() {
        return None;
    }
    let package = directory.ancestors().find_map(|at| {
        let candidate = at.join("node_modules").join("@uniflowed").join(name);
        candidate
            .join("package.json")
            .is_file()
            .then_some(candidate)
    })?;
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(package.join("package.json")).ok()?).ok()?;
    let target = match manifest.get("exports") {
        Some(exports) => export_target(exports, &subpath)?,
        None if subpath == "." => manifest
            .get("main")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("./index.js")
            .to_owned(),
        None => subpath,
    };
    Some(package.join(target))
}

/// The file `exports` maps `subpath` to under [`CONDITIONS`].
fn export_target(exports: &serde_json::Value, subpath: &str) -> Option<String> {
    let entry = match exports {
        serde_json::Value::Object(map) if map.keys().any(|key| key.starts_with('.')) => {
            map.get(subpath)?
        }
        // A bare string, or an object of conditions, is what `"."` exports.
        other if subpath == "." => other,
        _ => return None,
    };
    conditional_target(entry)
}

fn conditional_target(entry: &serde_json::Value) -> Option<String> {
    match entry {
        serde_json::Value::String(target) => Some(target.clone()),
        serde_json::Value::Object(conditions) => conditions
            .iter()
            .filter(|(condition, _)| CONDITIONS.contains(&condition.as_str()))
            .find_map(|(_, target)| conditional_target(target)),
        serde_json::Value::Array(targets) => targets.iter().find_map(conditional_target),
        _ => None,
    }
}

/// A `uf transform` reply, the fields the loader reads.
#[derive(Deserialize)]
struct Reply {
    code: Option<String>,
    map: Option<String>,
    error: Option<String>,
}

/// Compile `modules` over a few `uf transform` processes and file what comes
/// back. Returns how many entries were written.
fn compile(host: &HostCommand, binary: &Utf8Path, modules: Vec<Module>) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let services = modules.len().div_ceil(MODULES_PER_SERVICE).clamp(1, cores);
    let mut shards: Vec<Vec<Module>> = (0..services).map(|_| Vec::new()).collect();
    for (index, module) in modules.into_iter().enumerate() {
        shards[index % services].push(module);
    }
    std::thread::scope(|scope| {
        let workers: Vec<_> = shards
            .into_iter()
            .map(|shard| scope.spawn(move || serve_shard(host, binary, &shard)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(0))
            .sum()
    })
}

/// One `uf transform` process over one shard, requests pipelined.
fn serve_shard(host: &HostCommand, binary: &Utf8Path, shard: &[Module]) -> usize {
    let mut command = Command::new(binary.as_std_path());
    command
        .args(["--cwd", host.root.as_str(), "transform"])
        .current_dir(host.root.as_std_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // The environment the loader's service gets inside a worker: this
    // process's, the project's `.env` values, and what `uf_test::host` sets.
    for (name, value) in &host.env {
        command.env(name, value);
    }
    command
        .env("UF_PROJECT_ROOT", host.root.as_str())
        .env("UF_IN_SOURCE_TESTS", "1")
        .env("UF_BINARY", binary.as_str())
        .env_remove("UF_TRANSFORM_BOOTSTRAP_CONFIG");
    let Ok(mut child) = command.spawn() else {
        return 0;
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return 0;
    };
    let root = host.root.as_str();
    let written = std::thread::scope(|scope| {
        scope.spawn(move || {
            for module in shard {
                let request = serde_json::json!({
                    "id": module.path,
                    "code": module.source,
                    // `compileOptions` in `packages/host/internal/flow-cache.js`.
                    "options": {
                        "root": root,
                        "development": true,
                        "sourceMap": true,
                        "inSourceTests": true,
                        "configBootstrap": false,
                    },
                });
                if serde_json::to_writer(&mut stdin, &request).is_err()
                    || stdin.write_all(b"\n").is_err()
                {
                    break;
                }
            }
            // Dropping stdin ends the service once it has answered.
        });
        let mut written = 0;
        let mut lines = BufReader::new(stdout).lines();
        for module in shard {
            let Some(Ok(line)) = lines.next() else {
                break;
            };
            let Ok(reply) = serde_json::from_str::<Reply>(&line) else {
                break;
            };
            if reply.error.is_some() {
                continue;
            }
            let Some(code) = reply.code else {
                continue;
            };
            if file_entry(&module.entry, &framed(code, reply.map.as_deref())) {
                written += 1;
            }
        }
        written
    });
    let _ = child.wait();
    written
}

/// `framed` in `packages/host/internal/flow-cache.js`.
fn framed(code: String, map: Option<&str>) -> String {
    match map {
        Some(map) if !map.is_empty() => format!(
            "{code}\n//# sourceMappingURL=data:application/json;base64,{}\n",
            base64(map.as_bytes())
        ),
        _ => code,
    }
}

/// Standard, padded base64 — what `Buffer#toString("base64")` writes.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(
                    ALPHABET[usize::try_from((n >> shift) & 0x3f).unwrap_or(0)],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Write `contents` to `entry` the way the loader does: beside it, then renamed
/// over it, so a worker reading it never sees half a module.
fn file_entry(entry: &Path, contents: &str) -> bool {
    let Some(directory) = entry.parent() else {
        return false;
    };
    if std::fs::create_dir_all(directory).is_err() {
        return false;
    }
    let staging = entry.with_extension(format!("mjs.{}.prewarm.tmp", std::process::id()));
    if std::fs::write(&staging, contents).is_err() {
        return false;
    }
    if std::fs::rename(&staging, entry).is_err() {
        let _ = std::fs::remove_file(&staging);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_name_is_the_loaders() {
        // What `cacheEntryFor("/c", "/uf\012\034", "export {};\n", "/p/a.js")`
        // in `packages/host/internal/flow-cache.js` answered, run under
        // `UF_IN_SOURCE_TESTS=1`. If either side changes its key, this fails.
        assert_eq!(
            entry_name("/uf\u{0}12\u{0}34", "/p/a.js", "export {};\n"),
            "5d5a1c769c6aac5c31fdfc621d3dc43b78648c145751649155e1a0d79be1a0ac.mjs"
        );
    }

    #[test]
    fn a_module_is_framed_with_its_map_inline_and_without_one_bare() {
        assert_eq!(
            framed(String::from("x;"), Some("{}")),
            "x;\n//# sourceMappingURL=data:application/json;base64,e30=\n"
        );
        assert_eq!(framed(String::from("x;"), None), "x;");
        // `Buffer.from(…).toString("base64")` for each length modulo three.
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("{\"é\":1}".as_bytes()), "eyLDqSI6MX0=");
        assert_eq!(framed(String::from("x;"), Some("")), "x;");
    }

    #[test]
    fn exports_resolve_under_the_conditions_an_import_carries() {
        let exports = serde_json::json!({
            ".": { "uniflowed-bun-test": "./bun/index.js", "default": "./index.js" },
            "./app": { "browser": "./app-browser.js", "default": "./app.js" },
            "./worker": "./worker.js",
        });
        assert_eq!(export_target(&exports, ".").as_deref(), Some("./index.js"));
        assert_eq!(
            export_target(&exports, "./app").as_deref(),
            Some("./app.js")
        );
        assert_eq!(
            export_target(&exports, "./worker").as_deref(),
            Some("./worker.js")
        );
        assert_eq!(export_target(&exports, "./missing"), None);
        let sugar = serde_json::json!({ "import": "./esm.js", "require": "./cjs.cjs" });
        assert_eq!(export_target(&sugar, ".").as_deref(), Some("./esm.js"));
    }
}
