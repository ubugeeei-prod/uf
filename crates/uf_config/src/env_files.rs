//! `.env` files: which ones are read, in what order, and what crosses to the
//! browser.
//!
//! # The cascade
//!
//! A command runs in a *mode* — `development` for `uf dev`, `production` for
//! `uf build`, `uf preview` and `uf start`, `test` for `uf test` — and the mode
//! chooses two of the four files:
//!
//! | File | Read |
//! | --- | --- |
//! | `.env` | always |
//! | `.env.local` | always |
//! | `.env.<mode>` | in that mode |
//! | `.env.<mode>.local` | in that mode |
//!
//! They are read in that order and a later file wins, so `.env.production`
//! overrides `.env`, and a `.local` file overrides the tracked one beside it.
//! A file that does not exist is skipped; a file that exists and does not parse
//! is an error, because a value nobody can read is not a value.
//!
//! `env.files` in `uf.config.js` replaces the whole cascade when it is set: the
//! files named there are read in the order given, later winning, and no others.
//! It is empty by default, which is what selects the cascade above.
//!
//! # The process environment always wins
//!
//! A variable that is already set in the environment uf was started with is
//! never overwritten by a file. `DATABASE_URL=… uf build` and a CI secret both
//! beat every file in the cascade, which is the property that makes a `.env`
//! file a default rather than a policy.
//!
//! The one exception is a value uf itself put there. `uf run dev` sets the
//! project's variables in the shell it starts, and that shell may run `uf dev`
//! — which would otherwise see its own parent's development values as
//! "environment" and refuse to load `.env.production` over them for a nested
//! `uf build`. So a uf process names what it injected in [`INJECTED`], and a uf
//! process that finds that list treats those names as coming from a file.
//!
//! # What crosses to the client
//!
//! Every variable is set in the environment of the process uf starts, so server
//! code — a route handler, a loader, a page rendered on the server — reads all
//! of them through `process.env`. Only a variable whose name starts with the
//! client prefix ([`DEFAULT_CLIENT_PREFIX`], or `vite.envPrefix` when the
//! project sets one) is substituted into browser code as `import.meta.env.NAME`
//! and therefore shipped in the bundle. That boundary is Vite's own and uf does
//! not widen it: `docs/security.md` records the decision, and
//! `crates/uf_cli/tests/env_files.rs` holds the test that a non-prefixed value
//! is absent from the built client assets.
//!
//! # The parser
//!
//! One pass, no regular expressions, over a file with a size ceiling. The rules
//! are written out in `docs/app/guide/env` and tested in this module:
//!
//! * A blank line, and a line whose first non-blank character is `#`, is
//!   ignored. `#` also begins a comment after an unquoted value when a space
//!   comes before it.
//! * `NAME=value`, optionally with `export ` in front and spaces around the
//!   `=`. A name starts with a letter or `_` and continues with letters, digits
//!   or `_`.
//! * A value in double quotes may span lines and understands the escapes
//!   `\n`, `\r`, `\t`, `\\`, `\"`, `\'` and `\$`; a backslash before anything
//!   else is a backslash.
//! * A value in single quotes may span lines and is taken exactly as written:
//!   no escapes, no interpolation.
//! * An unquoted value ends at the line or at a comment, and is trimmed.
//! * `$NAME` and `${NAME}` in an unquoted or double-quoted value expand to a
//!   variable that is *already* defined — earlier in this file, in a file
//!   earlier in the cascade, or in the process environment. One that is not
//!   defined is an error rather than an empty string, because the silent empty
//!   string is how a typo becomes a production incident. `\$` is a literal.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use thiserror::Error;

use crate::UniflowedConfig;

/// The prefix a variable's name needs before its value may reach the browser.
///
/// Vite's own default, and deliberately not a uf spelling of the same idea: the
/// substitution is Vite's, `vite.envPrefix` overrides it for a project that
/// wants another, and a second convention would only mean two names for one
/// boundary.
pub const DEFAULT_CLIENT_PREFIX: &str = "VITE_";

/// Where `uf env use` writes the profile, relative to the project root.
///
/// Under `.uf/`, which is the one directory uf keeps per-project state in and
/// the one every `.gitignore` in a uf project already has. It was
/// `.uniflowed/profile`, beside a `.uniflowed/env/` full of symlinks that is
/// not project state at all and now lives beside the store — so a project had
/// two uf directories, one of them ignored and one of them not.
pub const PROFILE_FILE: &str = ".uf/profile";

/// Where an older uf wrote it.
///
/// Read when [`PROFILE_FILE`] is absent, and moved by `uf env install`, so a
/// project that upgrades keeps the profile it chose rather than silently
/// falling back to the default.
pub const LEGACY_PROFILE_FILE: &str = ".uniflowed/profile";

/// The variable naming what a uf process put in its child's environment.
///
/// Comma-separated names, in the child's environment, meaning "these came from
/// a file, not from a shell". See the module docs.
pub const INJECTED: &str = "UF_ENV_INJECTED";

/// The largest `.env` file uf reads.
///
/// A ceiling rather than a stream, because everything above reads the whole
/// file into memory and an environment file is a page of `NAME=value`. A
/// checkout that points `env.files` at a gigabyte of something else gets a
/// sentence rather than an allocation.
const MAX_FILE_BYTES: u64 = 1 << 20;

/// Why a `.env` file, or a mode, could not be used.
#[derive(Debug, Error)]
pub enum EnvFileError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is larger than {} KiB, which is not an environment file", MAX_FILE_BYTES / 1024)]
    TooLarge { path: Utf8PathBuf },
    #[error("{path}:{line}: {message}")]
    Syntax {
        path: Utf8PathBuf,
        line: usize,
        message: String,
    },
    #[error(
        "`{entry}` is not an environment file this project may read; a `.env` file is named \
         relative to the project root and lives inside it"
    )]
    OutsideProject { entry: String },
    #[error("{name:?} is not a mode: {reason}")]
    InvalidMode { name: String, reason: &'static str },
}

/// The environment a command runs with: the mode, the files behind it, and the
/// values they defined.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectEnv {
    mode: String,
    files: Vec<Utf8PathBuf>,
    values: BTreeMap<String, String>,
    injected: BTreeSet<String>,
    client_prefixes: Vec<String>,
}

impl ProjectEnv {
    /// The mode these files were selected for.
    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// The files that existed and were read, in the order they were read.
    #[must_use]
    pub fn files(&self) -> &[Utf8PathBuf] {
        &self.files
    }

    /// Every variable a file defined that the process environment had not
    /// already set, by name.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    /// Whether a name is one the client prefix lets into browser code.
    #[must_use]
    pub fn is_client_visible(&self, name: &str) -> bool {
        self.client_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
    }

    /// The prefixes that let a value into browser code.
    #[must_use]
    pub fn client_prefixes(&self) -> &[String] {
        &self.client_prefixes
    }

    /// Every variable to set in a process uf starts, [`INJECTED`] included.
    ///
    /// The marker carries the names this process was told about as well as the
    /// ones it is setting: a name a parent injected is still a name that came
    /// from a file, whether or not this project's files mention it.
    #[must_use]
    pub fn exported(&self) -> Vec<(String, String)> {
        self.exported_beneath(&BTreeSet::new())
    }

    /// The same, for a caller that is about to set `overridden` itself.
    ///
    /// Those names are left out of both the values and [`INJECTED`]. That
    /// second half is the point: the marker means "this came from a file, so a
    /// nested uf may let its own files overrule it", and a value the caller
    /// wrote down — a task's `env` block — is the opposite of that. It is the
    /// caller saying `NAME=… uf build`, and the documented answer to that is
    /// that no file overrules it.
    fn exported_beneath(&self, overridden: &BTreeSet<&str>) -> Vec<(String, String)> {
        let mut exported: Vec<(String, String)> = self
            .values
            .iter()
            .filter(|(name, _)| !overridden.contains(name.as_str()))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let mut names: BTreeSet<&str> = self.injected.iter().map(String::as_str).collect();
        names.extend(self.values.keys().map(String::as_str));
        names.retain(|name| !overridden.contains(name));
        if !names.is_empty() {
            exported.push((
                INJECTED.to_owned(),
                names.into_iter().collect::<Vec<_>>().join(","),
            ));
        }
        exported
    }

    /// Set every variable on a command uf is about to run.
    pub fn apply(&self, command: &mut std::process::Command) {
        for (name, value) in self.exported() {
            command.env(name, value);
        }
    }

    /// The same, with `overrides` set over the file values.
    ///
    /// The two cannot be done in two calls, because the order in which they are
    /// set is not the whole of the difference between them: a name in
    /// `overrides` must also stop being listed in [`INJECTED`]. Setting the
    /// files and then the overrides leaves the child holding the caller's value
    /// under uf's own label, and the next uf in the chain reads that label and
    /// lets a file win over it.
    /// The marker is *removed* first, and that is not belt and braces. A child
    /// inherits this process's environment, and `Command::env` only adds to it
    /// — so when every name has been overridden there is nothing left to write
    /// and the child would inherit the marker a parent uf set on this one,
    /// naming the very variable the caller just took ownership of. The loop
    /// below writes it again whenever a name survives. [`Self::apply`] needs no
    /// such line: it can only ever widen the marker it was given.
    pub fn apply_over(&self, command: &mut std::process::Command, overrides: &[(&str, &str)]) {
        command.env_remove(INJECTED);
        let overridden: BTreeSet<&str> = overrides.iter().map(|(name, _)| *name).collect();
        for (name, value) in self.exported_beneath(&overridden) {
            command.env(name, value);
        }
        for (name, value) in overrides {
            command.env(name, value);
        }
    }
}

/// Load the project's environment for `mode`, from the real process environment.
///
/// # Errors
///
/// When a file that exists cannot be read or does not parse. A file that is
/// simply absent is skipped.
pub fn load(
    root: &Utf8Path,
    config: &UniflowedConfig,
    mode: &str,
) -> Result<ProjectEnv, EnvFileError> {
    let process: BTreeMap<String, String> = std::env::vars().collect();
    load_from(root, config, mode, &process)
}

/// The same, against a caller-supplied process environment.
///
/// # Errors
///
/// As [`load`].
pub fn load_from(
    root: &Utf8Path,
    config: &UniflowedConfig,
    mode: &str,
    process: &BTreeMap<String, String>,
) -> Result<ProjectEnv, EnvFileError> {
    check_mode(mode)?;

    // Names a parent `uf` put here are file values wearing an environment's
    // clothes; this project's own files are allowed to overrule them.
    let injected: BTreeSet<String> = process
        .get(INJECTED)
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let environment: BTreeMap<&str, &str> = process
        .iter()
        .filter(|(name, _)| name.as_str() != INJECTED && !injected.contains(name.as_str()))
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();

    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut files = Vec::new();
    for name in file_names(config, mode)? {
        let path = root.join(&name);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }
        // Named inside the project is not the same as *being* inside it: a
        // symlink can be committed, and one at `.env` pointing at
        // `~/.aws/credentials` would parse as `NAME=value` like anything else.
        // Resolved and checked, the way `uf_plugin::resolve` checks a plugin
        // path and for the same reason — see `docs/security.md`.
        if !contained(root, &path) {
            return Err(EnvFileError::OutsideProject { entry: name });
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(EnvFileError::TooLarge { path });
        }
        let source = fs::read_to_string(&path).map_err(|source| EnvFileError::Io {
            path: path.clone(),
            source,
        })?;
        parse_into(&source, &path, &environment, &mut values)?;
        files.push(path);
    }

    // Written last so it is impossible to write it wrong: whatever the files
    // said, a name the shell already carries keeps the shell's value.
    values.retain(|name, _| !environment.contains_key(name.as_str()));

    Ok(ProjectEnv {
        mode: mode.to_owned(),
        files,
        values,
        injected,
        client_prefixes: client_prefixes(config),
    })
}

/// The files to read for `mode`, in the order they are read.
///
/// # Errors
///
/// When `env.files` names something that is not a path inside the project.
/// `uf.config.js` in a repository somebody has just cloned is untrusted input,
/// and reading a file outside the project would put whatever parses as
/// `NAME=value` in it — `~/.aws/credentials` does — into the environment of
/// every process uf starts, and any name in it behind the client prefix into
/// the bundle.
fn file_names(config: &UniflowedConfig, mode: &str) -> Result<Vec<String>, EnvFileError> {
    if !config.env.files.is_empty() {
        let mut named = Vec::with_capacity(config.env.files.len());
        for entry in &config.env.files {
            check_entry(entry)?;
            named.push(entry.to_string());
        }
        return Ok(named);
    }
    Ok(vec![
        String::from(".env"),
        String::from(".env.local"),
        format!(".env.{mode}"),
        format!(".env.{mode}.local"),
    ])
}

/// Whether an `env.files` entry can name a file inside the project.
///
/// A closed grammar checked before the filesystem is touched, so a path that
/// could never be inside the project is refused whether or not it happens to
/// exist on this machine. `\\` is refused on every platform rather than only on
/// Windows, which is the mistake `uf_plugin::resolve` records.
fn check_entry(entry: &str) -> Result<(), EnvFileError> {
    let refuse = || {
        Err(EnvFileError::OutsideProject {
            entry: entry.to_owned(),
        })
    };
    if entry.is_empty()
        || entry.starts_with('~')
        || entry.contains('\\')
        // A drive letter, and every URL scheme, in one character.
        || entry.contains(':')
        || entry.chars().any(char::is_control)
    {
        return refuse();
    }
    if Utf8Path::new(entry).components().any(|component| {
        matches!(
            component,
            Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
        )
    }) {
        return refuse();
    }
    Ok(())
}

/// Whether `path` is inside `root` once both are resolved.
///
/// Components rather than a string prefix: `/app-secrets/.env` starts with
/// `/app` as text and is not inside it.
fn contained(root: &Utf8Path, path: &Utf8Path) -> bool {
    let (Ok(root), Ok(path)) = (root.canonicalize_utf8(), path.canonicalize_utf8()) else {
        return false;
    };
    path.starts_with(root)
}

/// The prefixes that let a value into browser code, honouring `vite.envPrefix`.
///
/// Read rather than re-declared: `envPrefix` is Vite's option and reaches uf
/// through the `vite` passthrough, so reading it here is what keeps that
/// passthrough meaning something now that uf, not Vite, reads the files. An
/// empty prefix is refused for the reason Vite refuses it — it would put every
/// variable in the bundle — and falls back to the default rather than failing a
/// build, since [`crate::UniflowedConfig::vite`] is untyped and this is not the
/// place that validates it.
///
/// Public because a command that reports the boundary rather than enforcing it
/// needs the same answer: `uf explain` names the prefix without loading a file,
/// and naming `VITE_` at a project that configured `PUBLIC_` would be telling
/// its reader that a public value is a private one.
#[must_use]
pub fn client_prefixes(config: &UniflowedConfig) -> Vec<String> {
    let declared = config
        .vite
        .as_ref()
        .and_then(|vite| vite.get("envPrefix"))
        .map(|prefix| match prefix {
            serde_json::Value::String(one) => vec![one.clone()],
            serde_json::Value::Array(many) => many
                .iter()
                .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default();
    let usable: Vec<String> = declared
        .into_iter()
        .filter(|prefix| !prefix.is_empty())
        .collect();
    if usable.is_empty() {
        vec![String::from(DEFAULT_CLIENT_PREFIX)]
    } else {
        usable
    }
}

/// Which mode a command runs in.
///
/// In order: what the command line asked for, then the profile `uf env use`
/// wrote, then `env.active` in `uf.config.js`, then the command's own default —
/// `development` for a dev server, `production` for a build.
///
/// # Errors
///
/// When the name cannot be a mode, or the profile file cannot be read.
pub fn resolve_mode(
    root: &Utf8Path,
    config: &UniflowedConfig,
    requested: Option<&str>,
    default: &str,
) -> Result<String, EnvFileError> {
    if let Some(mode) = requested {
        check_mode(mode)?;
        return Ok(mode.to_owned());
    }
    if let Some(profile) = active_profile(root)? {
        check_mode(&profile)?;
        return Ok(profile);
    }
    let active = config.env.active.trim();
    if !active.is_empty() {
        check_mode(active)?;
        return Ok(active.to_owned());
    }
    check_mode(default)?;
    Ok(default.to_owned())
}

/// The profile `uf env use` recorded, if it recorded one.
///
/// # Errors
///
/// When the file exists and cannot be read.
pub fn active_profile(root: &Utf8Path) -> Result<Option<String>, EnvFileError> {
    // The old location is read when the new one is absent, so a project that
    // has not run `uf env install` since the move keeps the profile it chose.
    // `uf env install` moves it; until then this is what makes the two
    // locations one answer.
    match read_profile(&root.join(PROFILE_FILE))? {
        Some(name) => Ok(Some(name)),
        None => read_profile(&root.join(LEGACY_PROFILE_FILE)),
    }
}

/// One profile file, or [`None`] when it is absent or says nothing.
fn read_profile(path: &Utf8Path) -> Result<Option<String>, EnvFileError> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let name = text.trim().to_owned();
            Ok((!name.is_empty()).then_some(name))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(EnvFileError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Whether a name can be a mode.
///
/// A mode is the last part of a file name, so it has to be one: a name that
/// could climb out of the project would make `--mode ../../etc` read a file
/// somewhere else. `local` is refused for Vite's reason — `.env.local` is
/// already the file that overrides every mode, so `.env.local.local` would be
/// two different things with one name.
///
/// # Errors
///
/// When the name is empty, is `local`, or holds anything but letters, digits,
/// `_`, `-` and `.`.
pub fn check_mode(name: &str) -> Result<(), EnvFileError> {
    let invalid = |reason| {
        Err(EnvFileError::InvalidMode {
            name: name.to_owned(),
            reason,
        })
    };
    if name.is_empty() {
        return invalid("a mode needs a name");
    }
    if name == "local" {
        return invalid("`.env.local` is already the file that overrides every mode");
    }
    if name.starts_with('.') {
        return invalid("a mode is the end of a file name, so it cannot start with `.`");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return invalid(
            "a mode is the end of a file name, so it holds only letters, digits, `_`, `-` and `.`",
        );
    }
    Ok(())
}

/// Parse one file into `values`, expanding against the environment and what the
/// earlier files said.
fn parse_into(
    source: &str,
    path: &Utf8Path,
    environment: &BTreeMap<&str, &str>,
    values: &mut BTreeMap<String, String>,
) -> Result<(), EnvFileError> {
    let characters: Vec<char> = source.chars().collect();
    let mut scanner = Scanner {
        characters: &characters,
        at: 0,
        line: 1,
        path,
    };

    loop {
        scanner.skip_blank();
        if scanner.done() {
            return Ok(());
        }
        if scanner.peek() == Some('#') {
            scanner.skip_line();
            continue;
        }
        let line = scanner.line;
        scanner.skip_export();
        let name = scanner.name()?;
        scanner.skip_spaces();
        if scanner.peek() != Some('=') {
            return Err(scanner.error(
                line,
                format!("`{name}` has no `=`; every line is `NAME=value`, a comment, or blank"),
            ));
        }
        scanner.advance();
        scanner.skip_spaces();
        // uf's own protocol variable, and not something a file may set: it is
        // the list of names a parent uf injected, and a file that could write
        // it could tell the *next* uf command that a value the shell really
        // set — a CI secret — came from a file, and so may be overridden.
        if name == INJECTED {
            return Err(scanner.error(
                line,
                format!("`{INJECTED}` is uf's own; a file cannot set it"),
            ));
        }
        let value = scanner.value(&name, &|wanted| {
            environment
                .get(wanted)
                .map(|value| (*value).to_owned())
                .or_else(|| values.get(wanted).cloned())
        })?;
        values.insert(name, value);
    }
}

/// A cursor over one file's characters.
struct Scanner<'a> {
    characters: &'a [char],
    at: usize,
    line: usize,
    path: &'a Utf8Path,
}

impl Scanner<'_> {
    fn done(&self) -> bool {
        self.at >= self.characters.len()
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.at).copied()
    }

    fn advance(&mut self) {
        if self.peek() == Some('\n') {
            self.line += 1;
        }
        self.at += 1;
    }

    fn error(&self, line: usize, message: String) -> EnvFileError {
        EnvFileError::Syntax {
            path: self.path.to_path_buf(),
            line,
            message,
        }
    }

    /// Past whitespace, newlines included.
    fn skip_blank(&mut self) {
        while let Some(character) = self.peek() {
            if character.is_whitespace() {
                self.advance();
            } else {
                return;
            }
        }
    }

    /// Past spaces and tabs, stopping at the end of the line.
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.advance();
        }
    }

    /// Past everything up to and including the next newline.
    fn skip_line(&mut self) {
        while let Some(character) = self.peek() {
            self.advance();
            if character == '\n' {
                return;
            }
        }
    }

    /// Past an `export ` prefix, if there is one.
    fn skip_export(&mut self) {
        const EXPORT: [char; 6] = ['e', 'x', 'p', 'o', 'r', 't'];
        if self.characters[self.at..].starts_with(&EXPORT)
            && matches!(
                self.characters.get(self.at + EXPORT.len()),
                Some(' ' | '\t')
            )
        {
            self.at += EXPORT.len();
            self.skip_spaces();
        }
    }

    /// The name at the cursor.
    fn name(&mut self) -> Result<String, EnvFileError> {
        let line = self.line;
        let start = self.at;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            self.advance();
        }
        let name: String = self.characters[start..self.at].iter().collect();
        let valid = name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
        if valid {
            return Ok(name);
        }
        // Whatever is here is not a name, and showing it is more use to the
        // reader than naming the character — but only as far as the `=`. What
        // comes after one is a value, and an error message is a thing people
        // paste into issues.
        let mut rest = name;
        while let Some(character) = self.peek() {
            if character == '\n' || character == '=' {
                break;
            }
            rest.push(character);
            self.advance();
        }
        let shown = rest.trim();
        // And a ceiling on that, so a file with no newline in it cannot turn
        // one mistake into a screen of text.
        const SHOWN: usize = 40;
        let shown: String = if shown.chars().count() > SHOWN {
            shown.chars().take(SHOWN).chain("…".chars()).collect()
        } else {
            shown.to_owned()
        };
        Err(self.error(
            line,
            format!(
                "`{shown}` is not a variable name; names start with a letter or `_` and continue \
                 with letters, digits or `_`"
            ),
        ))
    }

    /// The value at the cursor, expanded.
    fn value(
        &mut self,
        name: &str,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<String, EnvFileError> {
        match self.peek() {
            Some('\'') => {
                self.advance();
                let value = self.quoted(name, '\'', false, lookup)?;
                self.end_of_line(name)?;
                Ok(value)
            }
            Some('"') => {
                self.advance();
                let value = self.quoted(name, '"', true, lookup)?;
                self.end_of_line(name)?;
                Ok(value)
            }
            _ => self.unquoted(lookup),
        }
    }

    /// A quoted value, up to its closing quote.
    fn quoted(
        &mut self,
        name: &str,
        quote: char,
        escapes: bool,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<String, EnvFileError> {
        let opened = self.line;
        let mut value = String::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error(
                    opened,
                    format!("the value for `{name}` opens with {quote} and is never closed"),
                ));
            };
            if character == quote {
                self.advance();
                return Ok(value);
            }
            if escapes && character == '\\' {
                self.advance();
                let Some(escaped) = self.peek() else {
                    return Err(self.error(
                        opened,
                        format!("the value for `{name}` ends with a backslash"),
                    ));
                };
                self.advance();
                value.push_str(&unescape(escaped));
                continue;
            }
            if escapes && character == '$' {
                self.expand(&mut value, lookup)?;
                continue;
            }
            self.advance();
            value.push(character);
        }
    }

    /// An unquoted value: the rest of the line, minus a comment, trimmed.
    fn unquoted(
        &mut self,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<String, EnvFileError> {
        let mut value = String::new();
        // Where the value would end if a `#` started a comment here, which is
        // only true when a space came first: `pass#word` is a password.
        let mut after_space = true;
        while let Some(character) = self.peek() {
            if character == '\n' {
                break;
            }
            if character == '#' && after_space {
                self.skip_line();
                break;
            }
            if character == '\\' {
                // Only `\$` is an escape here: an unquoted value is written the
                // way a shell would take it, and a Windows path in one should
                // not need doubling.
                if self.characters.get(self.at + 1) == Some(&'$') {
                    self.at += 1;
                    self.advance();
                    value.push('$');
                    continue;
                }
                self.advance();
                value.push('\\');
                after_space = false;
                continue;
            }
            if character == '$' {
                self.expand(&mut value, lookup)?;
                after_space = false;
                continue;
            }
            after_space = character == ' ' || character == '\t';
            self.advance();
            value.push(character);
        }
        Ok(value.trim_end().to_owned())
    }

    /// Expand the `$NAME` or `${NAME}` at the cursor into `value`.
    fn expand(
        &mut self,
        value: &mut String,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<(), EnvFileError> {
        let line = self.line;
        self.advance();
        let braced = self.peek() == Some('{');
        if braced {
            self.advance();
        }
        let start = self.at;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            self.advance();
        }
        let wanted: String = self.characters[start..self.at].iter().collect();
        if braced {
            if self.peek() != Some('}') {
                return Err(self.error(
                    line,
                    format!("`${{{wanted}` is never closed; write `${{{wanted}}}`"),
                ));
            }
            self.advance();
        } else if wanted.is_empty() {
            // A bare `$` before something that cannot be a name is a dollar.
            value.push('$');
            return Ok(());
        }
        match lookup(&wanted) {
            Some(found) => {
                value.push_str(&found);
                Ok(())
            }
            None => Err(self.error(
                line,
                format!(
                    "`{}` is not defined; define it earlier, set it in the environment, or write \
                     `\\${}` for a literal dollar",
                    if braced {
                        format!("${{{wanted}}}")
                    } else {
                        format!("${wanted}")
                    },
                    if braced {
                        format!("{{{wanted}}}")
                    } else {
                        wanted.clone()
                    }
                ),
            )),
        }
    }

    /// After a closing quote, only spaces and a comment may follow.
    fn end_of_line(&mut self, name: &str) -> Result<(), EnvFileError> {
        self.skip_spaces();
        match self.peek() {
            None | Some('\n') => Ok(()),
            Some('#') => {
                self.skip_line();
                Ok(())
            }
            Some(_) => {
                let line = self.line;
                Err(self.error(
                    line,
                    format!(
                        "there is text after the quoted value for `{name}`; end the line, or \
                         start a comment with `#`"
                    ),
                ))
            }
        }
    }
}

/// What a backslash escape stands for inside double quotes.
fn unescape(character: char) -> String {
    match character {
        'n' => String::from("\n"),
        'r' => String::from("\r"),
        't' => String::from("\t"),
        // A backslash before anything else is a backslash, so a value that
        // holds a Windows path or a regular expression survives being quoted.
        '\\' | '"' | '\'' | '$' => character.to_string(),
        other => format!("\\{other}"),
    }
}

#[cfg(test)]
mod tests;
