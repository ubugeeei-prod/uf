//! Deno's Flow loader, which is not a loader: an ahead-of-time transform and
//! the import map that points the project's own specifiers at what it wrote.
//!
//! # Why this exists at all
//!
//! Every other host uf runs on has a hook. Node has `register()` and
//! `packages/host/internal/node-hooks.js`; Bun has `Bun.plugin` and
//! `packages/host/bun-preload.js`. Both are the same move — the module is
//! transformed *as the runtime asks for it* — and Deno has nothing to install
//! one in. There is no `--loader`, no plugin API, no `register`.
//!
//! So the transform has to have already happened by the time Deno reads the
//! first line. This module runs it: every Flow module the run can reach is
//! compiled and written into `.uf/deno/`, mirroring the layout it came from,
//! and an import map redirects the original specifiers there.
//!
//! **One artefact answers both of ubugeeei-prod/uf#246's halves.** The Flow
//! syntax problem is answered by the transform, and the bare specifier problem
//! — `import "@uniflowed/test"` — is answered by the *same map*, because a map
//! that has to name `@uniflowed/test`'s new location is a map that has named
//! `@uniflowed/test`. Neither needed a separate mechanism, which is the whole
//! reason the issue asks for this shape rather than for two.
//!
//! # The map, and the one entry that is not obvious
//!
//! Three kinds of entry:
//!
//! * **one per `@uniflowed/*` export.** `@uniflowed/test` → the compiled
//!   `index.js` under [`OUTPUT`], `@uniflowed/host/transform` → the compiled
//!   `transform.js`, and a trailing-slash key per package so a deep path
//!   nobody declared still lands in the right tree.
//! * **the project root, as a prefix.** `file:///<root>/` → `file:///<out>/`,
//!   which is what makes the worker's own `import(pathToFileURL(file))` reach
//!   the compiled copy. It has to be a prefix key rather than one key per
//!   file, because the worker appends `?uf-run=<generation>` to bust its
//!   module cache and an exact key would not match a URL with a query on it.
//! * **the output directory, mapped to itself.** This is the entry a reader
//!   stops at. [`OUTPUT`] is *under* the project root, so the prefix above
//!   would rewrite `file:///<root>/.uf/deno/src/a.js` — a module that has
//!   already been redirected once — into `.uf/deno/.uf/deno/src/a.js`, and
//!   every relative import inside a compiled module would resolve to nothing.
//!   An import map resolves the longest matching prefix, so an identity entry
//!   for the longer path is how a subtree is held back from a rewrite that
//!   covers it. There is no "except" in an import map; this is the "except".
//!
//! # What it does not do
//!
//! It compiles what it can enumerate: the project's own source, as
//! `uf_project`'s scan reports it, and the `@uniflowed/*` packages reachable
//! from what that source imports. A module that arrives some other way — a
//! path computed at run time, a file written by the test that is then imported
//! — was never compiled and Deno meets it as Flow. A hook has no such gap
//! because it is asked about every module; this is the difference between the
//! two mechanisms, and it is why `uf_runtime::HOSTS` grades Deno
//! [`uf_runtime::SupportLevel::Experimental`] rather than implemented.
//!
//! It also does not watch. The pass runs once, before the host starts, so
//! `uf test --watch` on Deno would re-run a suite against the modules the
//! first pass wrote; `commands::test::watch` refuses rather than doing that.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::UniflowedConfig;
use uf_project::ProjectFile;
use uf_transform::is_flow_module;

use crate::commands::transform::{ProjectTransform, compile_for_a_loader};

/// Where the pass writes, under the project root.
///
/// Under `.uf` so that the project's own tooling already ignores it, and so
/// that the one writable path uf's permission set adds covers it without a
/// second grant.
pub(crate) const OUTPUT: [&str; 2] = [".uf", "deno"];

/// The import map's name inside [`OUTPUT`].
pub(crate) const IMPORT_MAP: &str = "import-map.json";

/// The file recording which build of `uf`, and which framing, wrote this tree.
const STAMP: &str = ".uf-deno-build";

/// Bumped when *this module's* framing of the output changes.
///
/// Not the compiler's version — that is the binary identity beside it in the
/// stamp — but what this pass puts around a transform: the layout under
/// [`OUTPUT`], the shape of the import map. A change to either makes every
/// tree already on disk wrong in a way no source edit would reveal.
const FRAMING: &str = "1";

/// The scope every package this pass mirrors belongs to.
const SCOPE: &str = "@uniflowed";

/// Where the mirrored packages go inside [`OUTPUT`].
///
/// Deliberately **not** `node_modules`. Nothing needs it to be: every specifier
/// into these packages is answered by an entry in the import map, so the
/// directory's name is uf's to choose. Choosing `node_modules` would have given
/// Deno a second, half-populated one *below* the project's real install — and a
/// host that resolves a bare specifier by walking up to the nearest
/// `node_modules` would then find uf's, which holds `@uniflowed/*` and no
/// `react`. The packages uf does not compile have to keep resolving from the
/// install the project actually made.
const PACKAGES: &str = "packages";

/// What one pass produced, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DenoLoader {
    /// The directory the compiled modules were written to.
    pub(crate) directory: Utf8PathBuf,
    /// The import map that points the original specifiers at them.
    pub(crate) import_map: Utf8PathBuf,
    /// The compiled `@uniflowed/test/worker.js`, which is what Deno runs.
    pub(crate) worker: Utf8PathBuf,
    /// How many modules this pass compiled.
    ///
    /// Not how many are in the tree: a module whose source has not moved since
    /// the last pass is left where it is. Reported so that "the transform ran"
    /// and "the transform had nothing to do" are distinguishable, which is the
    /// one thing an ahead-of-time pass most needs to be able to say.
    pub(crate) compiled: usize,
    /// How many files were copied rather than compiled.
    pub(crate) copied: usize,
}

/// Compile everything the run can reach and write the map that points at it.
///
/// `scope` is the installed `<node_modules>/@uniflowed` directory, `sources`
/// the project's own files as `uf_project` scanned them, and
/// `in_source_tests` whether `import.meta.uf.test` reaches uf's test API in
/// this run — the same flag the Node hook reads from `UF_IN_SOURCE_TESTS`,
/// passed rather than read because this process is not the one that will be
/// told.
///
/// # Errors
///
/// A module that is not valid Flow, a file that cannot be read, or an output
/// path that cannot be written. A `@uniflowed/*` specifier naming a package
/// that is not installed is *not* an error here: the host will report it
/// against the import that asked for it, which is where a reader can act on
/// it.
pub(crate) fn build(
    root: &Utf8Path,
    config: &UniflowedConfig,
    scope: &Utf8Path,
    sources: &[ProjectFile],
    in_source_tests: bool,
) -> Result<DenoLoader> {
    let mut directory = root.to_path_buf();
    directory.extend(OUTPUT);
    refresh_stamp(&directory)?;

    let project = ProjectTransform::from_config(config);
    let mut compiled = 0;
    let mut copied = 0;

    for file in sources {
        let to = directory.join(&file.relative_path);
        mirror(
            &project,
            &file.absolute_path,
            &to,
            in_source_tests,
            &mut compiled,
            &mut copied,
        )?;
    }

    let packages = reachable_packages(scope, sources);
    for (name, real) in &packages {
        let into = directory.join(PACKAGES).join(SCOPE).join(name.as_str());
        mirror_package(
            &project,
            real,
            &into,
            in_source_tests,
            &mut compiled,
            &mut copied,
        )?;
    }

    let map = directory.join(IMPORT_MAP);
    let document = import_map(root, &directory, &packages);
    write(&map, &document)?;

    Ok(DenoLoader {
        worker: directory.join(PACKAGES).join(SCOPE).join("test/worker.js"),
        import_map: map,
        directory,
        compiled,
        copied,
    })
}

/// Throw the tree away when a different build of `uf`, or a different framing,
/// wrote it.
///
/// The reason is `packages/host/internal/node-hooks.js`'s, one directory over:
/// a compiled module is a function of the *compiler* as much as of the source,
/// and keying only on the source is how "edit `crates/uf_transform`, rebuild,
/// run the suite" came to answer with the previous build's output while
/// nothing about the run looked wrong. Here the whole tree is one generation,
/// so the check is one file and the answer is all or nothing rather than
/// per module.
fn refresh_stamp(directory: &Utf8Path) -> Result<()> {
    let want = stamp();
    let path = directory.join(STAMP);
    if fs::read_to_string(&path).is_ok_and(|found| found == want) {
        return Ok(());
    }
    if directory.exists() {
        fs::remove_dir_all(directory)
            .with_context(|| format!("could not clear the Deno loader's output at {directory}"))?;
    }
    write(&path, &want)
}

/// This framing, and the build of `uf` that is about to compile.
///
/// The binary is identified by its size and modification time rather than by a
/// version string, for the reason `packages/host/transform.js` gives: a
/// version does not change when `cargo build` writes a new binary over the old
/// one, and every run between two releases would share one identity. A
/// platform that will not say what is executing gives `unknown`, which
/// compares equal to itself and therefore keeps a tree — the cost of getting
/// that wrong is a stale answer, so it is stated here rather than left to be
/// inferred from a missing branch.
fn stamp() -> String {
    let identity = std::env::current_exe()
        .and_then(|path| {
            let data = fs::metadata(&path)?;
            let modified = data.modified()?;
            Ok(format!(
                "{}\u{0}{}\u{0}{modified:?}",
                path.display(),
                data.len()
            ))
        })
        .unwrap_or_else(|_| String::from("unknown"));
    format!("{FRAMING}\u{0}{identity}\n")
}

/// One file into the tree: compiled if it is Flow, copied if it is not.
///
/// Copied rather than skipped, because a compiled module's relative imports
/// resolve against its *new* location: a `./fixture.json` beside a test file
/// has to be beside the compiled test file too, or the import that used to
/// find it now finds nothing.
fn mirror(
    project: &ProjectTransform,
    from: &Utf8Path,
    to: &Utf8Path,
    in_source_tests: bool,
    compiled: &mut usize,
    copied: &mut usize,
) -> Result<()> {
    if current(from, to) {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {parent} for the Deno loader"))?;
    }
    if !is_flow_module(from.as_str()) {
        fs::copy(from, to).with_context(|| format!("could not copy {from} to {to}"))?;
        *copied += 1;
        return Ok(());
    }
    let source =
        fs::read_to_string(from).with_context(|| format!("could not read {from} to compile it"))?;
    // The *original* path is the id, so the source map names the file the
    // author wrote rather than the copy under `.uf/deno` — a stack frame
    // pointing into uf's own output directory would be a frame nobody can
    // open.
    let code = compile_for_a_loader(project, from.as_str(), &source, in_source_tests)
        .map_err(|error| anyhow::anyhow!("{from}: {error}"))?;
    write(to, &code)?;
    *compiled += 1;
    Ok(())
}

/// Every file of one `@uniflowed/*` package.
///
/// A `node_modules` *inside* the package is skipped: a nested install is that
/// package's own dependency tree, resolved by the host from where it is, and
/// copying it here would be copying a third party's JavaScript through a Flow
/// transform.
fn mirror_package(
    project: &ProjectTransform,
    real: &Utf8Path,
    into: &Utf8Path,
    in_source_tests: bool,
    compiled: &mut usize,
    copied: &mut usize,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(real).follow_links(false) {
        let entry = entry.with_context(|| format!("could not read {real}"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(path) = Utf8Path::from_path(entry.path()) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(real) else {
            continue;
        };
        if relative
            .components()
            .any(|part| matches!(part.as_str(), "node_modules" | ".git"))
        {
            continue;
        }
        mirror(
            project,
            path,
            &into.join(relative),
            in_source_tests,
            compiled,
            copied,
        )?;
    }
    Ok(())
}

/// Whether `to` already holds this version of `from`.
///
/// Modification time only. It is the same test `uf`'s other incremental steps
/// make, and it is wrong in exactly one direction — a source restored to an
/// older timestamp looks current — which the stamp above does not help with
/// and which `uf clean` does.
fn current(from: &Utf8Path, to: &Utf8Path) -> bool {
    let (Ok(source), Ok(output)) = (fs::metadata(from), fs::metadata(to)) else {
        return false;
    };
    match (source.modified(), output.modified()) {
        (Ok(source), Ok(output)) => output >= source,
        _ => false,
    }
}

fn write(path: &Utf8Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {parent} for the Deno loader"))?;
    }
    fs::write(path, contents).with_context(|| format!("could not write {path}"))
}

/// The `@uniflowed/*` packages this run can reach, by name, at their real path.
///
/// Two sources, both deliberately generous. The project's own source is
/// scanned for the *text* `@uniflowed/<name>`, which finds an import and also
/// finds a mention in a comment; and each package found that way contributes
/// its own `@uniflowed/*` dependencies, transitively. A name that turns out
/// not to be imported costs one directory copied and nothing else, while a
/// name missed costs a module Deno cannot parse — so the error this leans
/// towards is the one that is merely slower.
///
/// `@uniflowed/test` is always a seed: it is the worker, so it is in every run
/// whether the project mentions it or not.
fn reachable_packages(scope: &Utf8Path, sources: &[ProjectFile]) -> BTreeMap<String, Utf8PathBuf> {
    let mut queue: Vec<String> = vec![String::from("test")];
    for file in sources {
        collect_scope_names(&file.source, &mut queue);
    }

    let mut found = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(name) = queue.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let directory = scope.join(&name);
        // A workspace links `node_modules/@uniflowed/test` at `packages/test`,
        // and `walkdir` does not descend a symlinked root. The real path is
        // also what makes the transform's ids name a file somebody can open.
        let Ok(real) = directory.canonicalize_utf8() else {
            continue;
        };
        if !real.is_dir() {
            continue;
        }
        for dependency in scope_dependencies(&real) {
            queue.push(dependency);
        }
        found.insert(name, real);
    }
    found
}

/// Every `@uniflowed/<name>` spelled anywhere in `text`.
fn collect_scope_names(text: &str, into: &mut Vec<String>) {
    let mark = format!("{SCOPE}/");
    let mut rest = text;
    while let Some(at) = rest.find(&mark) {
        rest = &rest[at + mark.len()..];
        let end = rest
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
            })
            .unwrap_or(rest.len());
        if end > 0 {
            into.push(rest[..end].to_owned());
        }
    }
}

/// The `@uniflowed/*` entries in one package's `dependencies`.
fn scope_dependencies(directory: &Utf8Path) -> Vec<String> {
    let Some(manifest) = manifest(directory) else {
        return Vec::new();
    };
    let Some(dependencies) = manifest
        .get("dependencies")
        .and_then(|value| value.as_object())
    else {
        return Vec::new();
    };
    dependencies
        .keys()
        .filter_map(|key| key.strip_prefix(&format!("{SCOPE}/")))
        .map(ToOwned::to_owned)
        .collect()
}

fn manifest(directory: &Utf8Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(directory.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The map itself. See this module's header for what each kind of entry is for.
fn import_map(
    root: &Utf8Path,
    directory: &Utf8Path,
    packages: &BTreeMap<String, Utf8PathBuf>,
) -> String {
    let mut imports: BTreeMap<String, String> = BTreeMap::new();

    for (name, real) in packages {
        let into = directory.join(PACKAGES).join(SCOPE).join(name.as_str());
        imports.insert(format!("{SCOPE}/{name}/"), directory_url(&into));
        for (subpath, target) in exports(real) {
            let key = if subpath == "." {
                format!("{SCOPE}/{name}")
            } else {
                format!("{SCOPE}/{name}/{}", subpath.trim_start_matches("./"))
            };
            imports.insert(key, file_url(&into.join(target.trim_start_matches("./"))));
        }
    }

    // Longest prefix first is the import map's own rule, so the identity entry
    // for the output directory holds a compiled module back from the project
    // rewrite that would otherwise apply to it a second time.
    imports.insert(directory_url(directory), directory_url(directory));
    imports.insert(directory_url(root), directory_url(directory));

    let mut document = serde_json::Map::new();
    let mut table = serde_json::Map::new();
    for (key, value) in imports {
        table.insert(key, serde_json::Value::String(value));
    }
    document.insert("imports".to_string(), serde_json::Value::Object(table));
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Object(document))
        .unwrap_or_else(|_| String::from("{\"imports\":{}}"));
    text.push('\n');
    text
}

/// What one package's `package.json` says can be imported from it.
///
/// Subpaths carrying a `*` are skipped: an import map key means a prefix only
/// when it ends in `/`, so a pattern would have to be expanded against the
/// package's files to be expressed here — and the trailing-slash key emitted
/// beside these already covers every path such a pattern could name.
fn exports(directory: &Utf8Path) -> Vec<(String, String)> {
    let Some(manifest) = manifest(directory) else {
        return Vec::new();
    };
    let Some(exports) = manifest.get("exports") else {
        let main = manifest
            .get("main")
            .and_then(|value| value.as_str())
            .unwrap_or("index.js");
        return vec![(String::from("."), main.to_owned())];
    };
    if let Some(target) = condition(exports) {
        return vec![(String::from("."), target)];
    }
    let Some(table) = exports.as_object() else {
        return Vec::new();
    };
    table
        .iter()
        .filter(|(key, _)| key.starts_with('.') && !key.contains('*'))
        .filter_map(|(key, value)| condition(value).map(|target| (key.clone(), target)))
        .collect()
}

/// The file one export entry resolves to, following conditions.
///
/// `import` before `default` because uf's packages are ES modules and that is
/// the condition a host asking for one matches first; `require` is not
/// followed at all, since a CommonJS entry point is not what Deno will be
/// handed here.
fn condition(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(target) => Some(target.clone()),
        serde_json::Value::Object(table) => ["import", "module", "default"]
            .iter()
            .find_map(|name| table.get(*name).and_then(condition)),
        serde_json::Value::Array(entries) => entries.iter().find_map(condition),
        _ => None,
    }
}

/// A path as a `file:` URL, which is what an import map key has to be to match
/// one.
///
/// A key that does not parse as a URL is a *bare specifier* to an import map,
/// so a path written plainly would silently never match anything — the entry
/// would be there, the resolution would not happen, and the failure would
/// arrive as a Flow syntax error from a module nobody redirected.
///
/// Encoded conservatively: everything outside the unreserved set and the
/// handful of sub-delimiters a path may carry unescaped. `%` is encoded, so a
/// path that already contains an escape is not read as one.
fn file_url(path: &Utf8Path) -> String {
    let text = path.as_str().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for byte in text.bytes() {
        let character = byte as char;
        if character.is_ascii_alphanumeric()
            || matches!(
                character,
                '-' | '.'
                    | '_'
                    | '~'
                    | '/'
                    | '!'
                    | '$'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | '+'
                    | ','
                    | ';'
                    | '='
                    | ':'
                    | '@'
            )
        {
            out.push(character);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The same, for a directory: an import map key is a prefix only when it ends
/// in a slash, and so is the value it maps to.
fn directory_url(path: &Utf8Path) -> String {
    let mut url = file_url(path);
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

#[cfg(test)]
mod tests;
