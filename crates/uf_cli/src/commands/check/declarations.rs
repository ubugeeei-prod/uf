//! Typing a dependency that ships TypeScript declarations and no Flow.
//!
//! [`super::dependencies`] reads an installed package only when something in
//! it declares `@flow`, because a package that does not exports `any` whether
//! it is read or not. That held for every package written in TypeScript, which
//! is most of them: `zod`, `date-fns` and `@tanstack/query-core` all publish
//! declarations, and every schema, date and query a uf project took from them
//! was `any` — `const name: number = z.string().parse(input)` type checked.
//! ubugeeei-prod/uf#946.
//!
//! So a package with no Flow and with declarations is read through them. The
//! declaration file TypeScript would read for the specifier is found by
//! [`Manifest::declaration`] — in the package, or in its `@types/<name>`
//! package when the package ships none, which is TypeScript's order — and
//! [`uf_dts::translate`] turns it, and every declaration file it reaches, into
//! Flow declaration modules filed beside them as `<declaration>.flow`.
//!
//! # How the checker finds them
//!
//! Without being told anything new. The checker resolves a package through
//! its manifest and answers the `flow` condition of an `exports` map — which is
//! how `uf build --lib` publishes Flow sources — so the package is handed over
//! with a manifest that says exactly that: each subpath a specifier asked for,
//! under `flow`, naming the translation of its declaration file. The package's
//! real manifest stays on disk. The one in the batch describes what the
//! checker is being handed, which is the package's declarations and not its
//! JavaScript.
//!
//! A subpath is added when a specifier asks for it and not before. zod
//! publishes dozens, a project imports two or three, and a subpath nobody
//! imports is a translation nobody reads. When a later round asks for one
//! more, the package is translated again with it; the entries are the cache
//! key, so each set is translated once.
//!
//! # Between runs
//!
//! A translation is a function of the declaration files it read and of the
//! `uf` that read them, nothing else. So it is kept under `.uf/cache/dts`,
//! keyed by that `uf`, the package and the entries, beside the SHA-256 of
//! every file the translation read — and a record of every file it looked for
//! and did not find, since one appearing changes the answer as surely as one
//! changing. A warm run reads and hashes those files and translates nothing.
//!
//! # What is not typed
//!
//! Whatever the translation could not carry over is `any`, and each such place
//! is a [`uf_dts::Hole`] naming its declaration. The report counts a package's
//! holes once, beside its name, so a reader knows how much of a package is
//! typed without reading a list.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uf_check::TypeDiagnostic;
use uf_dts::{Manifest, Translation, flow_path, translate};
use uf_infra::cache::{CacheBound, sweep};
use uf_infra::{FxHashMap, FxHashSet};
use uf_lint::SourceFile;

/// How many declaration files one package's translation may read before the
/// package is left untyped.
///
/// [`super::dependencies`]'s guard for a package's Flow, for the same reason:
/// a limit on a directory nobody meant to publish, not a policy about package
/// size. `@types/node`, among the largest there is, reads about a hundred.
const MAX_DECLARATION_FILES: usize = 4_000;

/// The shape of a cache record. Bumped when it changes, so an old record is a
/// miss rather than a parse failure somebody has to diagnose.
const RECORD_VERSION: u32 = 1;

/// A package this run typed from its declarations, as the report names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TranslatedPackage {
    /// The name the project imports it by.
    pub(super) name: String,
    /// The version the declarations' manifest publishes.
    pub(super) version: Option<String>,
    /// Where the declarations were read from, project-relative: the package
    /// itself, or its `@types` package.
    pub(super) declarations: String,
    /// The `@types` package the declarations came from, when the package
    /// ships none of its own — named so a reader does not take its version
    /// for the package's.
    pub(super) types_package: Option<String>,
    /// How many declaration files were translated.
    pub(super) modules: usize,
    /// How many places in them are `any` because the translation could not
    /// say what TypeScript said.
    pub(super) holes: usize,
    /// How many errors Flow reports inside the translated modules, other than
    /// declaration-site variance.
    ///
    /// Each is a place where Flow reads a declaration differently from
    /// TypeScript — a conditional type it does not reduce, an array spread
    /// into a tuple, a member its own library definitions lack — so a use
    /// that reaches one may be typed less precisely than TypeScript types it,
    /// down to `any` where Flow recovers from a missing member. Counted beside
    /// the holes so a clean check says how much it rests on. Variance is left
    /// out because Flow reports it at the declaration and still types every
    /// use by the variance declared.
    pub(super) findings: usize,
    /// Whether the translation came back from `.uf/cache/dts` instead of being
    /// made by this run.
    pub(super) from_cache: bool,
}

/// Every place a package typed from its declarations is `any`, or typed less
/// precisely than TypeScript types it: what `uf check --explain-any` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Explanation {
    /// The package name that was asked about.
    pub(super) package: String,
    /// Whether this run typed a package of that name from declarations.
    ///
    /// `false` is an answer of its own, and not the same as a package with
    /// nothing to explain: nothing imported the package, or it ships Flow, or
    /// it has no declarations. Two empty lists under `false` are not a clean
    /// bill.
    pub(super) translated: bool,
    /// Every hole the translation left, by declaration file and line.
    pub(super) holes: Vec<ExplainedHole>,
    /// Every error Flow reports inside the translation, other than
    /// declaration-site variance, by declaration file and line.
    pub(super) findings: Vec<ExplainedFinding>,
}

/// One hole, where a reader can find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExplainedHole {
    /// The declaration file, project-relative.
    pub(super) path: String,
    /// The one-based line in it.
    pub(super) line: u32,
    /// The declaration the hole is in.
    pub(super) declaration: String,
    /// The construct, as `uf_dts::Construct` spells it.
    pub(super) construct: &'static str,
    /// What the translation did instead.
    pub(super) reason: String,
}

/// One error Flow reports inside a translation, where a reader can find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExplainedFinding {
    /// The declaration file the translated line came from, project-relative.
    pub(super) path: String,
    /// The one-based line: the same line in the declaration file as in its
    /// translation, because a translation keeps every line where it was.
    pub(super) line: u32,
    /// The declaration the line is in, when it is in one.
    pub(super) declaration: Option<String>,
    /// Flow's error code.
    pub(super) code: Option<String>,
    /// Flow's message.
    pub(super) message: String,
}

/// Every package typed from declarations so far, and the sources that stand
/// in for them in the batch.
pub(super) struct Declarations {
    root: PathBuf,
    cache: Option<Cache>,
    /// Keyed by the directory the batch's manifest is filed under.
    packages: BTreeMap<String, Package>,
    /// Installed directories that turned out to ship Flow. A package with
    /// Flow of its own is typed by it and never by its declarations: Flow its
    /// authors wrote outranks a translation of TypeScript they also wrote.
    flow: FxHashSet<String>,
    /// Requests nothing could answer, as `<directory or name>\0<subpath>`, so
    /// a later round does not read the same manifests to learn the same thing.
    unanswered: FxHashSet<String>,
    sources: Vec<SourceFile>,
}

/// One package being typed from its declarations.
struct Package {
    name: String,
    version: Option<String>,
    /// Where the manifest the batch sees is filed.
    directory: String,
    /// Where the declaration files are.
    declarations: String,
    /// The `@types` package they are in, if they are in one.
    types_package: Option<String>,
    manifest: Manifest,
    /// Each subpath asked for that has a declaration, and that declaration.
    entries: BTreeMap<String, String>,
    /// Subpaths asked for that have none.
    unanswered: BTreeSet<String>,
    /// [`None`] until translated, and after a translation that was too large
    /// to keep.
    translation: Option<Translation>,
    from_cache: bool,
    /// Whether an entry was added since the last translation.
    stale: bool,
}

impl Declarations {
    /// Declarations for the project at `root`, cached under its `.uf/`.
    pub(super) fn open(root: &Utf8Path) -> Self {
        let root = root.as_std_path().to_path_buf();
        let cache = Cache::open(&root);
        Self::with_cache(root, cache)
    }

    /// Declarations that translate every time and keep nothing.
    #[cfg(test)]
    pub(super) fn uncached(root: &Utf8Path) -> Self {
        Self::with_cache(root.as_std_path().to_path_buf(), None)
    }

    fn with_cache(root: PathBuf, cache: Option<Cache>) -> Self {
        Self {
            root,
            cache,
            packages: BTreeMap::new(),
            flow: FxHashSet::default(),
            unanswered: FxHashSet::default(),
            sources: Vec::new(),
        }
    }

    /// Record that the package installed at `directory` ships Flow.
    pub(super) fn typed_by_flow(&mut self, directory: &str) {
        self.flow.insert(directory.to_owned());
    }

    /// Ask for `subpath` of the package `name`.
    ///
    /// `installed` is the directory Node's climb found the package in, if it
    /// found one, and `types` finds its `@types` package the same way — asked
    /// only when the package itself has no declarations, because TypeScript
    /// does not consult `@types` for a package that publishes its own.
    ///
    /// Nothing is translated here. A round asks for everything it could not
    /// resolve and then [`Self::flush`]es, so a package asked for twice in one
    /// round is translated once.
    pub(super) fn request(
        &mut self,
        name: &str,
        subpath: &str,
        installed: Option<&str>,
        types: &mut dyn FnMut() -> Option<String>,
    ) {
        if installed.is_some_and(|directory| self.flow.contains(directory)) {
            return;
        }
        // By where the package was found, because two copies of one name are
        // two packages; by name when it was not found at all.
        let key = format!("{}\0{subpath}", installed.unwrap_or(name));
        if self.unanswered.contains(&key) {
            return;
        }
        if !self.answer(name, subpath, installed, types) {
            self.unanswered.insert(key);
        }
    }

    /// Whether `subpath` of `name` has declarations, adding them when it does.
    fn answer(
        &mut self,
        name: &str,
        subpath: &str,
        installed: Option<&str>,
        types: &mut dyn FnMut() -> Option<String>,
    ) -> bool {
        if let Some(directory) = installed {
            if let Some(package) = self.packages.get_mut(directory) {
                package.ask(&self.root, subpath);
                return package.entries.contains_key(subpath);
            }
            let declarations = real_directory(&self.root, directory);
            if let Some(package) =
                Package::read(&self.root, name, directory, &declarations, None, subpath)
            {
                self.packages.insert(directory.to_owned(), package);
                return true;
            }
        }
        let Some(types) = types() else {
            return false;
        };
        // A package that is not installed at all can still be described: the
        // manifest goes where the package would be, beside its `@types`, so
        // the same climb that found the `@types` finds it.
        let directory = match installed {
            Some(directory) => directory.to_owned(),
            None => match beside_types(&types, name) {
                Some(directory) => directory,
                None => return false,
            },
        };
        if let Some(package) = self.packages.get_mut(&directory) {
            package.ask(&self.root, subpath);
            return package.entries.contains_key(subpath);
        }
        let declarations = real_directory(&self.root, &types);
        match Package::read(
            &self.root,
            name,
            &directory,
            &declarations,
            Some(uf_dts::types_package(name)),
            subpath,
        ) {
            Some(package) => {
                self.packages.insert(directory, package);
                true
            }
            None => false,
        }
    }

    /// Translate every package something new was asked of. Returns whether the
    /// batch's sources changed.
    pub(super) fn flush(&mut self) -> bool {
        let mut changed = false;
        for package in self.packages.values_mut().filter(|package| package.stale) {
            package.stale = false;
            changed = true;
            let base = self.root.join(&package.declarations);
            let entries: Vec<&str> = package
                .entries
                .values()
                .map(String::as_str)
                .collect::<BTreeSet<&str>>()
                .into_iter()
                .collect();
            let record = self
                .cache
                .as_ref()
                .map(|cache| cache.record(package, &entries));
            if let (Some(cache), Some(record)) = (self.cache.as_ref(), record.as_ref())
                && let Some(translation) = cache.read(record, &base)
            {
                package.translation = Some(translation);
                package.from_cache = true;
                continue;
            }
            package.from_cache = false;
            package.translation = match translate_package(&base, &entries) {
                Some((translation, reads)) => {
                    if let (Some(cache), Some(record)) = (self.cache.as_mut(), record.as_ref()) {
                        cache.write(record, &reads, &translation);
                    }
                    Some(translation)
                }
                None => None,
            };
        }
        if changed {
            self.sources = self.packages.values().flat_map(Package::sources).collect();
        }
        changed
    }

    /// The sources standing in for every translated package: each one's
    /// manifest and its translated modules.
    pub(super) fn sources(&self) -> &[SourceFile] {
        &self.sources
    }

    /// What the report says about each translated package.
    ///
    /// `diagnostics` is everything the check produced, before any is dropped
    /// for being about a file nobody asked about — the errors inside a
    /// translation are about exactly such files, and they are what
    /// [`TranslatedPackage::findings`] counts.
    pub(super) fn translated(&self, diagnostics: &[TypeDiagnostic]) -> Vec<TranslatedPackage> {
        let translated: Vec<(&Package, &Translation)> = self
            .packages
            .values()
            .filter_map(|package| Some((package, package.translation.as_ref()?)))
            .collect();
        let mut owners: FxHashMap<String, usize> = FxHashMap::default();
        for (index, (package, translation)) in translated.iter().enumerate() {
            for module in &translation.modules {
                owners.insert(
                    format!("{}/{}", package.declarations, flow_path(&module.path)),
                    index,
                );
            }
        }
        let mut findings = vec![0; translated.len()];
        for diagnostic in diagnostics {
            // See `TranslatedPackage::findings` for why variance is not one.
            if !diagnostic.is_error() || diagnostic.code == Some("incompatible-variance") {
                continue;
            }
            if let Some(index) = owners.get(diagnostic.primary.path.as_str()) {
                findings[*index] += 1;
            }
        }
        translated
            .into_iter()
            .zip(findings)
            .map(|((package, translation), findings)| TranslatedPackage {
                name: package.name.clone(),
                version: package.version.clone(),
                declarations: package.declarations.clone(),
                types_package: package.types_package.clone(),
                modules: translation.modules.len(),
                holes: translation.holes().count(),
                findings,
                from_cache: package.from_cache,
            })
            .collect()
    }
}

impl Declarations {
    /// Every hole and finding of each package named `name` this run typed from
    /// its declarations — of every copy, since two installed copies are two
    /// translations.
    ///
    /// A finding is named by reading its declaration file again and asking
    /// [`uf_dts::declaration_at`] which declaration the line is in: a parse of
    /// each file that has a finding, paid only when somebody asked.
    pub(super) fn explain(&self, name: &str, diagnostics: &[TypeDiagnostic]) -> Explanation {
        let translated: Vec<(&Package, &Translation)> = self
            .packages
            .values()
            .filter(|package| package.name == name)
            .filter_map(|package| Some((package, package.translation.as_ref()?)))
            .collect();

        let mut holes = Vec::new();
        // Each translated module's path in the batch, to the declaration file
        // it was translated from.
        let mut declaration_files: FxHashMap<String, String> = FxHashMap::default();
        for (package, translation) in &translated {
            for module in &translation.modules {
                let path = format!("{}/{}", package.declarations, module.path);
                declaration_files.insert(
                    format!("{}/{}", package.declarations, flow_path(&module.path)),
                    path.clone(),
                );
                holes.extend(module.holes.iter().map(|hole| ExplainedHole {
                    path: path.clone(),
                    line: hole.line,
                    declaration: hole.declaration.to_string(),
                    construct: hole.construct.as_str(),
                    reason: hole.reason.to_string(),
                }));
            }
        }

        let mut sources: FxHashMap<String, Option<String>> = FxHashMap::default();
        let mut findings = Vec::new();
        for diagnostic in diagnostics {
            // Counted as `TranslatedPackage::findings` counts them.
            if !diagnostic.is_error() || diagnostic.code == Some("incompatible-variance") {
                continue;
            }
            let Some(path) = declaration_files.get(diagnostic.primary.path.as_str()) else {
                continue;
            };
            let line = diagnostic.primary.start.line;
            let source = sources
                .entry(path.clone())
                .or_insert_with(|| fs::read_to_string(self.root.join(path)).ok());
            findings.push(ExplainedFinding {
                path: path.clone(),
                line,
                declaration: source
                    .as_deref()
                    .and_then(|source| uf_dts::declaration_at(source, line)),
                code: diagnostic.code.map(str::to_owned),
                message: diagnostic.message_text(),
            });
        }
        findings.sort_by(|left, right| (&left.path, left.line).cmp(&(&right.path, right.line)));

        Explanation {
            package: name.to_owned(),
            translated: !translated.is_empty(),
            holes,
            findings,
        }
    }
}

impl Package {
    /// The package whose declarations are at `declarations`, if they answer
    /// `subpath`.
    fn read(
        root: &Path,
        name: &str,
        directory: &str,
        declarations: &str,
        types_package: Option<String>,
        subpath: &str,
    ) -> Option<Self> {
        let base = root.join(declarations);
        let manifest = Manifest::parse(&fs::read_to_string(base.join("package.json")).ok()?)?;
        let entry = manifest.declaration(subpath, &mut |path| base.join(path).is_file())?;
        Some(Self {
            name: name.to_owned(),
            version: manifest.version().map(str::to_owned),
            directory: directory.to_owned(),
            declarations: declarations.to_owned(),
            types_package,
            manifest,
            entries: BTreeMap::from([(subpath.to_owned(), entry)]),
            unanswered: BTreeSet::new(),
            translation: None,
            from_cache: false,
            stale: true,
        })
    }

    /// Add `subpath` if it has a declaration and is not already known.
    fn ask(&mut self, root: &Path, subpath: &str) {
        if self.entries.contains_key(subpath) || self.unanswered.contains(subpath) {
            return;
        }
        let base = root.join(&self.declarations);
        match self
            .manifest
            .declaration(subpath, &mut |path| base.join(path).is_file())
        {
            Some(entry) => {
                self.entries.insert(subpath.to_owned(), entry);
                self.stale = true;
            }
            None => {
                self.unanswered.insert(subpath.to_owned());
            }
        }
    }

    /// The manifest the batch sees and every module that translated.
    fn sources(&self) -> Vec<SourceFile> {
        let Some(translation) = &self.translation else {
            return Vec::new();
        };
        let mut sources = Vec::with_capacity(translation.modules.len() + 1);
        sources.push(SourceFile {
            path: format!("{}/package.json", self.directory),
            source: self.batch_manifest(),
        });
        for module in &translation.modules {
            if let Some(flow) = &module.flow {
                sources.push(SourceFile {
                    path: format!("{}/{}", self.declarations, flow_path(&module.path)),
                    source: flow.clone(),
                });
            }
        }
        sources
    }

    /// A manifest publishing each entry's translation under `flow`.
    ///
    /// Only the subpaths somebody asked for, so a subpath nobody asked for is
    /// unresolved rather than resolved to a translation that was never made —
    /// and the next round asks for it.
    fn batch_manifest(&self) -> String {
        let exports: Map<String, Value> = self
            .entries
            .iter()
            .map(|(subpath, entry)| {
                let target =
                    relative_target(&self.directory, &self.declarations, &flow_path(entry));
                (subpath.clone(), json!({ "flow": target }))
            })
            .collect();
        let mut manifest = Map::new();
        manifest.insert("name".to_owned(), Value::String(self.name.clone()));
        if let Some(version) = &self.version {
            manifest.insert("version".to_owned(), Value::String(version.clone()));
        }
        manifest.insert("exports".to_owned(), Value::Object(exports));
        serde_json::to_string_pretty(&Value::Object(manifest)).unwrap_or_default()
    }
}

/// Translate `entries` of the package at `base`, and what the translation read.
///
/// [`None`] when it read more than [`MAX_DECLARATION_FILES`]: half a package
/// resolves imports to files that are missing for no reason a reader could
/// find, and none of it leaves the specifier untyped where the footer names it.
fn translate_package(base: &Path, entries: &[&str]) -> Option<(Translation, Vec<Read>)> {
    let mut reads: Vec<Read> = Vec::new();
    let mut too_large = false;
    let translation = translate(entries, &mut |path| {
        if reads.len() == MAX_DECLARATION_FILES {
            too_large = true;
            return None;
        }
        let text = fs::read_to_string(base.join(path)).ok();
        reads.push(Read {
            path: path.to_owned(),
            sha256: text.as_deref().map(digest),
        });
        text
    });
    (!too_large).then_some((translation, reads))
}

/// Where a package that is not installed is described from: beside its
/// `@types` package, as `node_modules/<name>`.
fn beside_types(types: &str, name: &str) -> Option<String> {
    let base = types.strip_suffix(&uf_dts::types_package(name))?;
    Some(format!("{base}{name}"))
}

/// The directory's real path, project-relative, when it resolves inside the
/// project.
///
/// pnpm installs a public `node_modules/name` symlink to a package in
/// `node_modules/.store/.../node_modules/name`. TypeScript declaration files
/// inside that package resolve their own bare imports from the real store
/// location, while the importing project still reaches the package through the
/// public symlink. The synthetic manifest stays at the public path; translated
/// declaration modules are filed at the real path so their imports climb from
/// the same place Node and TypeScript use.
fn real_directory(root: &Path, directory: &str) -> String {
    let Ok(root) = root.canonicalize() else {
        return directory.to_owned();
    };
    let Ok(resolved) = root.join(directory).canonicalize() else {
        return directory.to_owned();
    };
    let Ok(relative) = resolved.strip_prefix(&root) else {
        return directory.to_owned();
    };
    let Some(relative) = relative.to_str() else {
        return directory.to_owned();
    };
    let relative = relative.replace('\\', "/");
    if relative.is_empty() {
        directory.to_owned()
    } else {
        relative
    }
}

/// `file` under `declarations`, as an `exports` target written in the
/// manifest at `directory`.
fn relative_target(directory: &str, declarations: &str, file: &str) -> String {
    let from: Vec<&str> = directory.split('/').collect();
    let to: Vec<&str> = declarations.split('/').collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut target = String::from("./");
    for _ in shared..from.len() {
        target.push_str("../");
    }
    for segment in &to[shared..] {
        target.push_str(segment);
        target.push('/');
    }
    target.push_str(file);
    target
}

/// One file a translation asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Read {
    /// The path, relative to the declarations' directory.
    path: String,
    /// The SHA-256 of what was read, or [`None`] when there was nothing.
    sha256: Option<String>,
}

/// A translation as it is kept on disk.
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    reads: Vec<Read>,
    translation: Translation,
}

/// `.uf/cache/dts`.
struct Cache {
    directory: PathBuf,
    /// The binary this process is running, for the reason `uf_check`'s cache
    /// keys by it: builds between two releases share a version, and it is
    /// builds that change what a translation says.
    identity: String,
    swept: bool,
}

impl Cache {
    fn open(root: &Path) -> Option<Self> {
        Some(Self {
            directory: root.join(".uf").join("cache").join("dts"),
            identity: binary_identity()?,
            swept: false,
        })
    }

    /// The record `package`'s translation of `entries` is kept in.
    fn record(&self, package: &Package, entries: &[&str]) -> PathBuf {
        let mut hasher = Sha256::new();
        let version = package.version.as_deref().unwrap_or_default();
        for part in [
            self.identity.as_str(),
            package.name.as_str(),
            version,
            package.declarations.as_str(),
        ]
        .into_iter()
        .chain(entries.iter().copied())
        {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        self.directory
            .join(format!("{}.json", hex(&hasher.finalize())))
    }

    /// The translation kept in `record`, when every file it read still reads
    /// the same.
    fn read(&self, record: &Path, base: &Path) -> Option<Translation> {
        let kept: Record = serde_json::from_slice(&fs::read(record).ok()?).ok()?;
        let fresh = kept.version == RECORD_VERSION
            && kept.reads.iter().all(|read| {
                fs::read_to_string(base.join(&read.path))
                    .ok()
                    .as_deref()
                    .map(digest)
                    == read.sha256
            });
        fresh.then_some(kept.translation)
    }

    /// Keep a translation. Best effort: a cache that cannot be written is a
    /// slower next run, not a failed one.
    fn write(&mut self, record: &Path, reads: &[Read], translation: &Translation) {
        if !self.swept {
            self.swept = true;
            sweep(&self.directory, CacheBound::default());
        }
        let kept = json!({
            "version": RECORD_VERSION,
            "reads": reads,
            "translation": translation,
        });
        let Ok(bytes) = serde_json::to_vec(&kept) else {
            return;
        };
        if fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        // Written aside and renamed, so a run that reads while another writes
        // sees the old record or the new one and never half of one.
        let aside = record.with_extension(format!("json.{}", std::process::id()));
        if fs::write(&aside, bytes).is_ok() && fs::rename(&aside, record).is_err() {
            let _ = fs::remove_file(&aside);
        }
    }
}

/// The SHA-256 of `text`, as lowercase hex.
fn digest(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The `uf` this process is running, as path, size and modification time —
/// `uf_check`'s cache identity, for the same reason.
fn binary_identity() -> Option<String> {
    uf_infra::cache::binary_identity()
}

#[cfg(test)]
mod tests {
    use super::*;

    use camino::Utf8PathBuf;

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        for (path, source) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().expect("a parent")).expect("the directory is made");
            fs::write(path, source).expect("the file is written");
        }
        root
    }

    fn root_of(directory: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("a UTF-8 path")
    }

    fn paths(declarations: &Declarations) -> Vec<&str> {
        declarations
            .sources()
            .iter()
            .map(|source| source.path.as_str())
            .collect()
    }

    const SCHEMA: &str = "export interface Schema<T> { parse(input: unknown): T; }\n\
                          export declare function string(): Schema<string>;\n";

    #[test]
    fn a_package_is_handed_over_as_its_translation_and_a_manifest_naming_it() {
        let directory = project(&[
            (
                "node_modules/tiny/package.json",
                r#"{ "name": "tiny", "version": "1.2.3", "main": "./index.js", "types": "./index.d.ts" }"#,
            ),
            (
                "node_modules/tiny/index.js",
                "exports.string = () => ({});\n",
            ),
            ("node_modules/tiny/index.d.ts", SCHEMA),
        ]);
        let mut declarations = Declarations::uncached(&root_of(&directory));

        declarations.request("tiny", ".", Some("node_modules/tiny"), &mut || None);
        assert!(declarations.flush());

        assert_eq!(
            paths(&declarations),
            [
                "node_modules/tiny/package.json",
                "node_modules/tiny/index.d.ts.flow"
            ]
        );
        let manifest: Value =
            serde_json::from_str(&declarations.sources()[0].source).expect("the manifest is JSON");
        assert_eq!(manifest["exports"]["."]["flow"], "./index.d.ts.flow");
        assert_eq!(
            declarations.translated(&[]),
            [TranslatedPackage {
                name: "tiny".to_owned(),
                version: Some("1.2.3".to_owned()),
                declarations: "node_modules/tiny".to_owned(),
                types_package: None,
                modules: 1,
                holes: 0,
                findings: 0,
                from_cache: false,
            }]
        );

        // Asked again, nothing changes and nothing is translated again.
        declarations.request("tiny", ".", Some("node_modules/tiny"), &mut || None);
        assert!(!declarations.flush());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_package_translates_declarations_from_its_real_directory() {
        let directory = project(&[
            (
                "node_modules/.store/facade@1.0.0/node_modules/facade/package.json",
                r#"{ "name": "facade", "version": "1.0.0", "types": "./index.d.ts" }"#,
            ),
            (
                "node_modules/.store/facade@1.0.0/node_modules/facade/index.d.ts",
                SCHEMA,
            ),
        ]);
        std::os::unix::fs::symlink(
            ".store/facade@1.0.0/node_modules/facade",
            directory.path().join("node_modules/facade"),
        )
        .expect("the pnpm-style public package link is made");
        let mut declarations = Declarations::uncached(&root_of(&directory));

        declarations.request("facade", ".", Some("node_modules/facade"), &mut || None);
        assert!(declarations.flush());

        assert_eq!(
            paths(&declarations),
            [
                "node_modules/facade/package.json",
                "node_modules/.store/facade@1.0.0/node_modules/facade/index.d.ts.flow",
            ]
        );
        let manifest: Value =
            serde_json::from_str(&declarations.sources()[0].source).expect("the manifest is JSON");
        assert_eq!(
            manifest["exports"]["."]["flow"],
            "./../.store/facade@1.0.0/node_modules/facade/index.d.ts.flow"
        );
        let translated = declarations.translated(&[]);
        assert_eq!(translated[0].types_package, None);
        assert_eq!(
            translated[0].declarations,
            "node_modules/.store/facade@1.0.0/node_modules/facade"
        );
    }

    #[test]
    fn a_package_with_no_declarations_of_its_own_is_described_by_its_types_package() {
        let directory = project(&[
            (
                "node_modules/plain/package.json",
                r#"{ "name": "plain", "main": "./index.js" }"#,
            ),
            ("node_modules/plain/index.js", "module.exports = {};\n"),
            (
                "node_modules/@types/plain/package.json",
                r#"{ "name": "@types/plain", "version": "9.0.0" }"#,
            ),
            ("node_modules/@types/plain/index.d.ts", SCHEMA),
        ]);
        let mut declarations = Declarations::uncached(&root_of(&directory));

        declarations.request("plain", ".", Some("node_modules/plain"), &mut || {
            Some("node_modules/@types/plain".to_owned())
        });
        assert!(declarations.flush());

        assert_eq!(
            paths(&declarations),
            [
                "node_modules/plain/package.json",
                "node_modules/@types/plain/index.d.ts.flow"
            ]
        );
        let manifest: Value =
            serde_json::from_str(&declarations.sources()[0].source).expect("the manifest is JSON");
        assert_eq!(
            manifest["exports"]["."]["flow"],
            "./../@types/plain/index.d.ts.flow"
        );
    }

    #[test]
    fn a_package_that_ships_flow_is_never_typed_from_its_declarations() {
        let directory = project(&[
            (
                "node_modules/both/package.json",
                r#"{ "name": "both", "types": "./index.d.ts" }"#,
            ),
            ("node_modules/both/index.d.ts", SCHEMA),
        ]);
        let mut declarations = Declarations::uncached(&root_of(&directory));

        declarations.typed_by_flow("node_modules/both");
        declarations.request("both", ".", Some("node_modules/both"), &mut || None);

        assert!(!declarations.flush());
        assert!(declarations.sources().is_empty());
    }

    #[test]
    fn a_later_subpath_translates_the_package_again_with_both_entries() {
        let directory = project(&[
            (
                "node_modules/multi/package.json",
                r#"{ "name": "multi", "exports": { ".": { "types": "./index.d.ts" }, "./extra": { "types": "./extra.d.ts" } } }"#,
            ),
            ("node_modules/multi/index.d.ts", SCHEMA),
            (
                "node_modules/multi/extra.d.ts",
                "export declare const extra: number;\n",
            ),
        ]);
        let mut declarations = Declarations::uncached(&root_of(&directory));
        declarations.request("multi", ".", Some("node_modules/multi"), &mut || None);
        assert!(declarations.flush());

        declarations.request("multi", "./extra", Some("node_modules/multi"), &mut || None);
        declarations.request("multi", "./absent", Some("node_modules/multi"), &mut || {
            None
        });
        assert!(declarations.flush());

        assert_eq!(
            paths(&declarations),
            [
                "node_modules/multi/package.json",
                "node_modules/multi/extra.d.ts.flow",
                "node_modules/multi/index.d.ts.flow",
            ]
        );
        let manifest: Value =
            serde_json::from_str(&declarations.sources()[0].source).expect("the manifest is JSON");
        assert_eq!(manifest["exports"]["./extra"]["flow"], "./extra.d.ts.flow");
        assert!(manifest["exports"].get("./absent").is_none());
    }

    #[test]
    fn a_warm_translation_is_read_back_until_a_file_it_read_changes() {
        let directory = project(&[
            (
                "node_modules/tiny/package.json",
                r#"{ "name": "tiny", "types": "./index.d.ts" }"#,
            ),
            ("node_modules/tiny/index.d.ts", SCHEMA),
        ]);
        let root = root_of(&directory);
        let cache = || Cache {
            directory: directory.path().join(".uf/cache/dts"),
            identity: "test".to_owned(),
            swept: false,
        };
        let run = || {
            let mut declarations =
                Declarations::with_cache(root.as_std_path().to_path_buf(), Some(cache()));
            declarations.request("tiny", ".", Some("node_modules/tiny"), &mut || None);
            declarations.flush();
            declarations
        };

        assert!(
            !run().translated(&[])[0].from_cache,
            "the first run translates"
        );
        let warm = run();
        assert!(
            warm.translated(&[])[0].from_cache,
            "the second reads it back"
        );
        assert_eq!(warm.sources().len(), 2);

        fs::write(
            directory.path().join("node_modules/tiny/index.d.ts"),
            "export declare const changed: string;\n",
        )
        .expect("the declaration is rewritten");
        let edited = run();
        assert!(
            !edited.translated(&[])[0].from_cache,
            "an edit translates again"
        );
        assert!(edited.sources()[1].source.contains("changed"));
    }

    #[test]
    fn an_exports_target_is_written_relative_to_the_manifest() {
        assert_eq!(
            relative_target("node_modules/zod", "node_modules/zod", "index.d.cts.flow"),
            "./index.d.cts.flow"
        );
        assert_eq!(
            relative_target(
                "node_modules/@scope/pkg",
                "node_modules/@types/scope__pkg",
                "index.d.ts.flow"
            ),
            "./../../@types/scope__pkg/index.d.ts.flow"
        );
        assert_eq!(
            beside_types("../node_modules/@types/scope__pkg", "@scope/pkg").as_deref(),
            Some("../node_modules/@scope/pkg")
        );
    }
}
