#![allow(clippy::disallowed_macros)]

//! Every key `uf.config.js` may declare, held to something that reads it.
//!
//! # Why
//!
//! `tests/flow_schema.rs` holds the Flow type and this crate's structs to the
//! same key *names*. It cannot say whether anything reads a name, and for most
//! of alpha a great many nothing did: an audit for ubugeeei-prod/uf#1387 found
//! sixty-seven keys declared, deserialized, completed by `uf lsp`, and read by
//! nothing — `story.*`, `std.*`, `app.orm.*`, a `*.module` string on half the
//! built-ins — and twenty-four more that reached a report and stopped there.
//! Each was an option a project could write, that checked, and that changed
//! nothing. This test is what keeps that list from growing back.
//!
//! # What counts as a read
//!
//! A key `a.b.c` is read when some code outside a test takes field `c` from a
//! value that is the `b` section:
//!
//! * a chain — `config.app.b.c` in Rust, `b?.c` or `b.c` in JavaScript;
//! * a binding of that section — `let b2 = &config.a.b;` then `b2.c`,
//!   `if let Some(b2) = &config.b`, `.map(|b2| b2.c)`, a parameter typed with
//!   the section's struct (`b2: &BConfig`), `self.c` inside `impl BConfig`, a
//!   struct pattern `BConfig { c, .. }`; in JavaScript `const b2 = a.b ?? {}`,
//!   `const { c } = b`, or a function called with `a.b` as its first argument;
//! * or, for a top-level key, any `.c` at all.
//!
//! The corpus is `src/` of this crate, of the crates it takes section types
//! from (`uf_bundle`'s budgets, `uf_runtime`'s permissions), and of every crate
//! that depends on it — nothing else can hold a `UniflowedConfig` — and every
//! non-test module under `packages/`, which is where the evaluated config's
//! JSON projection is read.
//!
//! It is a reading of *names*, not of types. A section is recognised by the
//! key it is written under and the struct it deserializes into, so two
//! sections sharing a key name can mask each other, and printing a value in
//! `uf inspect` counts as reading it. What the test does guarantee is the
//! direction #1387 was about: a key whose name no code reads off its section
//! fails here, whatever else it shares a name with.
//!
//! # Reading a failure
//!
//! *"declared, and read by nothing"* — wire the key, or remove it from
//! `packages/config/internal/schema.js` and [`uf_config::UniflowedConfig`]
//! together and give a project that sets it a `uf codemod` step, as
//! `unread-config-keys-1387` does for the ones #1387 removed. There is no list
//! of exceptions to add it to: the day this test landed nothing was on one.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use uf_config::schema::{SOURCE, Schema};
use uf_flow::scan::{Token, TokenKind, tokenize_jsx};

/// The Flow type this test reads, for the messages.
const SCHEMA: &str = "packages/config/internal/schema.js";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A field name as the Rust struct spells it.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for character in name.chars() {
        if character.is_ascii_uppercase() {
            out.push('_');
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// Every `(section, field)` pair some code reads, both halves in snake case.
#[derive(Default)]
struct Reads {
    pairs: BTreeSet<(String, String)>,
    /// Every field name read off anything, for the top-level keys.
    fields: BTreeSet<String>,
}

impl Reads {
    fn record(&mut self, section: &str, field: &str) {
        self.pairs.insert((section.to_owned(), field.to_owned()));
        self.fields.insert(field.to_owned());
    }
}

/// Which key names each config type is written under, and which methods
/// hand one back.
///
/// Read from `src/` of this crate and of the crates whose types it uses —
/// `build.budgets` is `uf_bundle`'s, `permissions` is `uf_runtime`'s — and
/// only for the types reachable from `UniflowedConfig`, so an unrelated
/// struct's `images: Vec<Image>` is not mistaken for a section.
/// `CoverageThresholdConfig` is held under `thresholds` and
/// `perFileThresholds`, so a parameter typed with it binds both.
#[derive(Default)]
struct Holders {
    sections: BTreeMap<String, BTreeSet<String>>,
    /// `fn native_runner(&self) -> &NativeTestRunnerConfig`: a call to one of
    /// these reads the sections its type is held under.
    accessors: BTreeMap<String, BTreeSet<String>>,
}

impl Holders {
    fn of(&self, type_name: &str) -> Option<&BTreeSet<String>> {
        self.sections.get(type_name)
    }

    fn read() -> Self {
        let mut index = TypeIndex::default();
        for krate in config_crates() {
            for file in rust_files(&krate.join("src")) {
                index.collect(&lex(&file));
            }
        }
        let mut reachable = BTreeSet::new();
        let mut pending = vec![String::from("UniflowedConfig")];
        while let Some(type_name) = pending.pop() {
            if !reachable.insert(type_name.clone()) {
                continue;
            }
            for (_, types) in index.fields.get(&type_name).into_iter().flatten() {
                pending.extend(types.iter().cloned());
            }
            pending.extend(
                index
                    .variants
                    .get(&type_name)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        let mut holders = Self::default();
        for owner in &reachable {
            for (field, types) in index.fields.get(owner).into_iter().flatten() {
                let mut pending: Vec<&String> = types.iter().collect();
                let mut seen = BTreeSet::new();
                while let Some(type_name) = pending.pop() {
                    if !seen.insert(type_name) || !reachable.contains(type_name) {
                        continue;
                    }
                    holders
                        .sections
                        .entry(type_name.clone())
                        .or_default()
                        .insert(field.clone());
                    pending.extend(index.variants.get(type_name).into_iter().flatten());
                }
            }
        }
        holders
            .sections
            .entry(String::from("UniflowedConfig"))
            .or_default()
            .insert(String::from("config"));
        for (method, returned) in &index.methods {
            let sections: BTreeSet<String> = returned
                .iter()
                .filter_map(|type_name| holders.sections.get(type_name))
                .flatten()
                .cloned()
                .collect();
            if !sections.is_empty() {
                holders.accessors.insert(method.clone(), sections);
            }
        }
        holders
    }
}

/// This crate, and the path dependencies it takes types from.
fn config_crates() -> Vec<PathBuf> {
    let crates = workspace().join("crates");
    let manifest = fs::read_to_string(crates.join("uf_config/Cargo.toml")).unwrap_or_default();
    let mut found = vec![crates.join("uf_config")];
    let dependencies = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap_or_default();
    for line in dependencies.lines() {
        if let Some((name, spec)) = line.split_once('=')
            && spec.contains("path = \"../")
        {
            found.push(crates.join(name.trim()));
        }
    }
    found
}

/// Structs, enums and `&self` methods, as written.
#[derive(Default)]
struct TypeIndex {
    /// Each struct's fields, as `(name, idents in its type)`.
    fields: BTreeMap<String, Vec<(String, Vec<String>)>>,
    /// Each enum's payload idents.
    variants: BTreeMap<String, Vec<String>>,
    /// Each `&self` method's return-type idents, by name.
    methods: BTreeMap<String, Vec<String>>,
}

impl TypeIndex {
    fn collect(&mut self, tokens: &[TokenTree]) {
        collect_types(tokens, self);
    }
}

/// Every `struct`, `enum` and `&self` method in `tokens`, at any depth.
fn collect_types(tokens: &[TokenTree], index: &mut TypeIndex) {
    for (at, token) in tokens.iter().enumerate() {
        let TokenTree::Ident(keyword) = token else {
            if let TokenTree::Group(group) = token {
                collect_types(&trees(group.stream()), index);
            }
            continue;
        };
        let keyword = keyword.to_string();
        // `fn name(&self …) -> &Type`.
        if keyword == "fn"
            && let Some(name) = ident(tokens.get(at + 1))
            && let Some(TokenTree::Group(parameters)) = tokens.get(at + 2)
            && parameters
                .stream()
                .into_iter()
                .take(3)
                .any(|token| token.to_string() == "self")
            && is_punct(tokens.get(at + 3), '-')
            && is_punct(tokens.get(at + 4), '>')
        {
            let mut returned = Vec::new();
            let end = tokens[at + 5..]
                .iter()
                .position(|token| matches!(token, TokenTree::Group(group) if group.delimiter() == Delimiter::Brace))
                .map_or(tokens.len(), |offset| at + 5 + offset);
            idents(&tokens[at + 5..end], &mut returned);
            index.methods.entry(name).or_default().extend(returned);
            continue;
        }
        if keyword != "struct" && keyword != "enum" {
            continue;
        }
        let Some(TokenTree::Ident(name)) = tokens.get(at + 1) else {
            continue;
        };
        let body = tokens[at + 2..].iter().find_map(|token| match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => Some(Some(group)),
            TokenTree::Punct(punct) if punct.as_char() == ';' => Some(None),
            _ => None,
        });
        let Some(Some(body)) = body else {
            continue;
        };
        let body = trees(body.stream());
        if keyword == "enum" {
            let mut inner = Vec::new();
            for token in &body {
                if let TokenTree::Group(group) = token {
                    idents(&trees(group.stream()), &mut inner);
                }
            }
            index.variants.insert(name.to_string(), inner);
            continue;
        }
        let starts: Vec<usize> = (0..body.len())
            .filter(|&at| is_field_colon(&body, at + 1) && matches!(body[at], TokenTree::Ident(_)))
            .collect();
        let fields = index.fields.entry(name.to_string()).or_default();
        for (position, &start) in starts.iter().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(body.len());
            let mut types = Vec::new();
            idents(&body[start + 2..end], &mut types);
            fields.push((unraw(&body[start].to_string()), types));
        }
    }
}

fn idents(tokens: &[TokenTree], out: &mut Vec<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => out.push(ident.to_string()),
            TokenTree::Group(group) => idents(&trees(group.stream()), out),
            _ => {}
        }
    }
}

/// A `:` that is not half of a `::`.
fn is_field_colon(tokens: &[TokenTree], at: usize) -> bool {
    let colon = |index: usize| matches!(tokens.get(index), Some(TokenTree::Punct(punct)) if punct.as_char() == ':');
    colon(at)
        && matches!(&tokens[at], TokenTree::Punct(punct) if punct.spacing() == Spacing::Alone)
        && !(at > 0 && colon(at - 1))
}

fn unraw(name: &str) -> String {
    name.strip_prefix("r#").unwrap_or(name).to_owned()
}

fn trees(stream: TokenStream) -> Vec<TokenTree> {
    stream.into_iter().collect()
}

fn lex(path: &Path) -> Vec<TokenTree> {
    let source = fs::read_to_string(path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
    let stream = TokenStream::from_str(&source)
        .unwrap_or_else(|error| panic!("{} does not lex: {error:?}", path.display()));
    without_tests(trees(stream))
}

/// The tokens with every `#[test]` and `#[cfg(test)]` item taken out, at
/// every depth.
fn without_tests(tokens: Vec<TokenTree>) -> Vec<TokenTree> {
    let mut kept = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let is_test_attribute = matches!(&tokens[index], TokenTree::Punct(punct) if punct.as_char() == '#')
            && matches!(tokens.get(index + 1), Some(TokenTree::Group(group))
                if group.delimiter() == Delimiter::Bracket
                    && matches!(group.stream().to_string().replace(' ', "").as_str(), "test" | "cfg(test)"));
        if is_test_attribute {
            index += 2;
            while index < tokens.len() {
                let end = match &tokens[index] {
                    TokenTree::Group(group) => group.delimiter() == Delimiter::Brace,
                    TokenTree::Punct(punct) => punct.as_char() == ';',
                    _ => false,
                };
                index += 1;
                if end {
                    break;
                }
            }
            continue;
        }
        kept.push(match &tokens[index] {
            TokenTree::Group(group) => {
                let inner: TokenStream = without_tests(trees(group.stream())).into_iter().collect();
                let mut rebuilt = proc_macro2::Group::new(group.delimiter(), inner);
                rebuilt.set_span(group.span());
                TokenTree::Group(rebuilt)
            }
            other => other.clone(),
        });
        index += 1;
    }
    kept
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" {
                    pending.push(path);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// `src/` of this crate and of every crate that depends on it.
fn rust_corpus() -> Vec<PathBuf> {
    let crates = workspace().join("crates");
    let mut files = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&crates)
        .expect("crates/ lists")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for krate in entries {
        let manifest = fs::read_to_string(krate.join("Cargo.toml")).unwrap_or_default();
        let depends = manifest
            .lines()
            .any(|line| line.trim_start().starts_with("uf_config"));
        if depends || config_crates().contains(&krate) {
            files.extend(rust_files(&krate.join("src")));
        }
    }
    files
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

/// Methods that hand back the value they were called on, or a view of it, so
/// a binding of their result is a binding of the section.
const TRANSPARENT: &[&str] = &[
    "as_deref",
    "as_ref",
    "borrow",
    "clone",
    "iter",
    "to_owned",
    "unwrap",
    "unwrap_or_default",
];

struct RustFile<'h> {
    holders: &'h Holders,
    aliases: BTreeMap<String, BTreeSet<String>>,
}

fn is_punct(token: Option<&TokenTree>, character: char) -> bool {
    matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == character)
}

fn ident(token: Option<&TokenTree>) -> Option<String> {
    match token {
        Some(TokenTree::Ident(ident)) => Some(unraw(&ident.to_string())),
        _ => None,
    }
}

fn is_call(token: Option<&TokenTree>) -> bool {
    matches!(token, Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
}

impl RustFile<'_> {
    fn alias(&mut self, name: String, sections: impl IntoIterator<Item = String>) {
        self.aliases.entry(name).or_default().extend(sections);
    }

    /// What `name` stands for, following bindings of bindings.
    fn resolve(&self, name: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::from([name.to_owned()]);
        let mut pending = vec![name.to_owned()];
        while let Some(next) = pending.pop() {
            for section in self.aliases.get(&next).into_iter().flatten() {
                if found.insert(section.clone()) {
                    pending.push(section.clone());
                }
            }
        }
        found
    }

    /// The last field a chain starting at `start` reads, when the chain is a
    /// place rather than the result of a call that is not [`TRANSPARENT`].
    fn chain_target(tokens: &[TokenTree], mut at: usize) -> Option<String> {
        while is_punct(tokens.get(at), '&') || ident(tokens.get(at)).as_deref() == Some("mut") {
            at += 1;
        }
        let mut last = ident(tokens.get(at))?;
        loop {
            if is_punct(tokens.get(at + 1), '?') {
                at += 1;
                continue;
            }
            if !is_punct(tokens.get(at + 1), '.') {
                return Some(last);
            }
            let next = ident(tokens.get(at + 2))?;
            if is_call(tokens.get(at + 3)) {
                if !TRANSPARENT.contains(&next.as_str()) {
                    return None;
                }
                at += 3;
            } else {
                last = next;
                at += 2;
            }
        }
    }

    /// The field a method call at `dot` (the `.` before its name) is made on.
    fn receiver(tokens: &[TokenTree], dot: usize) -> Option<String> {
        let mut at = dot.checked_sub(1)?;
        loop {
            match &tokens[at] {
                TokenTree::Group(group)
                    if group.delimiter() == Delimiter::Parenthesis
                        && at >= 2
                        && is_punct(tokens.get(at - 2), '.') =>
                {
                    let name = ident(tokens.get(at - 1))?;
                    if !TRANSPARENT.contains(&name.as_str()) {
                        return None;
                    }
                    at = at.checked_sub(3)?;
                }
                TokenTree::Punct(punct) if punct.as_char() == '?' => at = at.checked_sub(1)?,
                TokenTree::Ident(ident) => return Some(unraw(&ident.to_string())),
                _ => return None,
            }
        }
    }

    fn bindings(&mut self, tokens: &[TokenTree]) {
        for (at, token) in tokens.iter().enumerate() {
            if let TokenTree::Group(group) = token {
                self.bindings(&trees(group.stream()));
                continue;
            }
            let Some(word) = ident(Some(token)) else {
                continue;
            };
            // `let x = &config.a.b;`, `if let Some(x) = &config.b`.
            if word == "let" {
                let mut at = at + 1;
                if ident(tokens.get(at)).as_deref() == Some("mut") {
                    at += 1;
                }
                let bound = match (tokens.get(at), tokens.get(at + 1)) {
                    (Some(TokenTree::Ident(name)), Some(TokenTree::Punct(punct)))
                        if punct.as_char() == '=' =>
                    {
                        Some((unraw(&name.to_string()), at + 2))
                    }
                    (Some(TokenTree::Ident(wrapper)), Some(TokenTree::Group(group)))
                        if matches!(wrapper.to_string().as_str(), "Some" | "Ok")
                            && is_punct(tokens.get(at + 2), '=') =>
                    {
                        let inner = trees(group.stream());
                        inner
                            .iter()
                            .rev()
                            .find_map(|token| ident(Some(token)))
                            .map(|name| (name, at + 3))
                    }
                    _ => None,
                };
                if let Some((name, rhs)) = bound
                    && let Some(section) = Self::chain_target(tokens, rhs)
                {
                    self.alias(name, [section]);
                }
                continue;
            }
            // `x: &CoverageConfig`, in a signature, a `let` or a struct.
            if is_field_colon(tokens, at + 1) {
                let annotation = tokens[at + 2..]
                    .iter()
                    .take_while(|token| {
                        !matches!(token, TokenTree::Punct(punct) if matches!(punct.as_char(), ',' | ';' | '='))
                    })
                    .take(8)
                    .filter_map(|token| ident(Some(token)));
                let sections: Vec<String> = annotation
                    .filter_map(|name| self.holders.of(&name).cloned())
                    .flatten()
                    .collect();
                if !sections.is_empty() {
                    self.alias(word.clone(), sections);
                }
            }
            // `config.b.as_ref().map(|x| x.c)`.
            if at > 0
                && is_punct(tokens.get(at - 1), '.')
                && let Some(TokenTree::Group(arguments)) = tokens.get(at + 1)
                && arguments.delimiter() == Delimiter::Parenthesis
            {
                let arguments = trees(arguments.stream());
                let mut index = 0;
                if is_punct(arguments.get(index), '|') {
                    index += 1;
                    if is_punct(arguments.get(index), '&') {
                        index += 1;
                    }
                    if let Some(name) = ident(arguments.get(index))
                        && is_punct(arguments.get(index + 1), '|')
                        && let Some(section) = Self::receiver(tokens, at - 1)
                    {
                        self.alias(name, [section]);
                    }
                }
            }
        }
    }

    fn reads(&self, tokens: &[TokenTree], this: Option<&BTreeSet<String>>, out: &mut Reads) {
        let mut at = 0;
        while at < tokens.len() {
            let token = &tokens[at];
            if let TokenTree::Group(group) = token {
                self.reads(&trees(group.stream()), this, out);
                at += 1;
                continue;
            }
            let Some(word) = ident(Some(token)) else {
                at += 1;
                continue;
            };
            // `impl … Type { … }`: `self` is the sections `Type` is held as.
            if word == "impl"
                && let Some(offset) = tokens[at..].iter().position(|token| {
                    matches!(token, TokenTree::Group(group) if group.delimiter() == Delimiter::Brace)
                })
            {
                let body = at + offset;
                let implemented = tokens[at..body]
                    .iter()
                    .rev()
                    .filter_map(|token| ident(Some(token)))
                    .find_map(|name| self.holders.of(&name));
                if let TokenTree::Group(group) = &tokens[body] {
                    self.reads(&trees(group.stream()), implemented.or(this), out);
                }
                at = body + 1;
                continue;
            }
            // `CoverageConfig { include, exclude, .. }`, and `Self { .. }`.
            if let Some(TokenTree::Group(group)) = tokens.get(at + 1)
                && group.delimiter() == Delimiter::Brace
            {
                let sections = if word == "Self" {
                    this
                } else {
                    self.holders.of(&word)
                };
                let inner = trees(group.stream());
                let rest = inner
                    .windows(2)
                    .any(|pair| is_punct(pair.first(), '.') && is_punct(pair.get(1), '.'));
                if let Some(sections) = sections
                    && rest
                {
                    for (index, field) in inner.iter().enumerate() {
                        let starts = index == 0 || is_punct(inner.get(index - 1), ',');
                        if let (true, Some(field)) = (starts, ident(Some(field))) {
                            for section in sections {
                                out.record(section, &field);
                            }
                        }
                    }
                }
            }
            // `config.test.native_runner().scheduler`.
            if let Some(sections) = self.holders.accessors.get(&word)
                && is_call(tokens.get(at + 1))
                && is_punct(tokens.get(at + 2), '.')
                && let Some(field) = ident(tokens.get(at + 3))
                && !is_call(tokens.get(at + 4))
            {
                for section in sections {
                    out.record(section, &field);
                }
            }
            // `a.b`, `a?.b`.
            let dot = if is_punct(tokens.get(at + 1), '?') {
                at + 2
            } else {
                at + 1
            };
            if is_punct(tokens.get(dot), '.')
                && let Some(field) = ident(tokens.get(dot + 1))
                && !is_call(tokens.get(dot + 2))
            {
                if word == "self" {
                    for section in this.into_iter().flatten() {
                        out.record(section, &field);
                    }
                }
                for section in self.resolve(&word) {
                    out.record(&section, &field);
                }
            }
            at += 1;
        }
    }
}

fn read_rust(holders: &Holders, out: &mut Reads) {
    for path in rust_corpus() {
        let tokens = lex(&path);
        let mut file = RustFile {
            holders,
            aliases: BTreeMap::new(),
        };
        file.bindings(&tokens);
        file.reads(&tokens, None, out);
    }
}

// ---------------------------------------------------------------------------
// JavaScript
// ---------------------------------------------------------------------------

fn js_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![workspace().join("packages")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !matches!(name.as_str(), "node_modules" | "tests" | "fixtures") {
                    pending.push(path);
                }
            } else if name.ends_with(".js")
                && !name.ends_with(".test.js")
                && !path.ends_with("config/internal/schema.js")
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

struct JsFile<'s> {
    source: &'s str,
    tokens: Vec<Token>,
    aliases: BTreeMap<String, BTreeSet<String>>,
    /// Each `function name(first, …)` declared here, as `name -> first`.
    functions: BTreeMap<String, String>,
}

impl<'s> JsFile<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            tokens: tokenize_jsx(source),
            aliases: BTreeMap::new(),
            functions: BTreeMap::new(),
        }
    }

    fn text(&self, at: usize) -> Option<&str> {
        self.tokens.get(at).map(|token| token.text(self.source))
    }

    fn ident(&self, at: usize) -> Option<String> {
        let token = self.tokens.get(at)?;
        (token.kind == TokenKind::Ident).then(|| snake(token.text(self.source)))
    }

    fn punct(&self, at: usize, byte: u8) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.is_punct(byte))
    }

    /// The index of the `.` of a member access at `at`: `.` or `?.`.
    fn member(&self, at: usize) -> Option<usize> {
        if self.punct(at, b'.') {
            Some(at)
        } else if self.punct(at, b'?') && self.punct(at + 1, b'.') {
            Some(at + 1)
        } else {
            None
        }
    }

    /// The last property a chain starting at `at` reads, and where it ends,
    /// when the chain is a place rather than a call.
    fn chain_target(&self, at: usize) -> Option<(String, usize)> {
        let mut last = self.ident(at)?;
        let mut at = at + 1;
        while let Some(dot) = self.member(at) {
            last = self.ident(dot + 1)?;
            at = dot + 2;
        }
        if self.punct(at, b'(') || self.punct(at, b'[') {
            return None;
        }
        Some((last, at))
    }

    fn alias(&mut self, name: String, section: String) {
        if name != section {
            self.aliases.entry(name).or_default().insert(section);
        }
    }

    /// Bindings within the file, and every call whose first argument is a
    /// section, for [`JsFile::parameters`] to match across files: `index.js`
    /// calls `highlightPlugin(mdxConfig.highlight)` and `internal/highlight.js`
    /// declares it.
    fn bindings(&mut self, first_arguments: &mut Vec<(String, String)>) {
        for at in 0..self.tokens.len() {
            match self.text(at) {
                Some("const" | "let" | "var") => {
                    if let Some(name) = self.ident(at + 1)
                        && self.punct(at + 2, b'=')
                        && let Some((section, _)) = self.chain_target(at + 3)
                    {
                        self.alias(name, section);
                    }
                    // `const { a, b: c, d = {} } = section`.
                    if self.punct(at + 1, b'{')
                        && let Some(close) = self.close(at + 1)
                        && self.punct(close + 1, b'=')
                        && let Some((section, _)) = self.chain_target(close + 2)
                    {
                        for (key, bound) in self.pattern_keys(at + 1, close) {
                            self.alias(bound, key.clone());
                            self.aliases
                                .entry(format!("\0{section}"))
                                .or_default()
                                .insert(key);
                        }
                    }
                }
                Some("function") => {
                    if let Some(name) = self.ident(at + 1)
                        && self.punct(at + 2, b'(')
                        && let Some(parameter) = self.ident(at + 3)
                    {
                        self.functions.insert(name, parameter);
                    }
                }
                _ => {
                    if let Some(name) = self.ident(at)
                        && self.punct(at + 1, b'(')
                        && (at == 0 || !self.punct(at - 1, b'.'))
                        && let Some((section, end)) = self.chain_target(at + 2)
                        && (self.punct(end, b',') || self.punct(end, b')'))
                    {
                        first_arguments.push((name, section));
                    }
                }
            }
        }
    }

    /// A function this file declares, called anywhere with a section as its
    /// first argument, binds its first parameter to that section.
    fn parameters(&mut self, first_arguments: &[(String, String)]) {
        for (function, section) in first_arguments {
            if let Some(parameter) = self.functions.get(function).cloned() {
                self.alias(parameter, section.clone());
            }
        }
    }

    fn close(&self, open: usize) -> Option<usize> {
        uf_flow::scan::matching_close(&self.tokens, open, b'{', b'}')
    }

    /// `(key, binding)` for each top-level property of an object pattern.
    fn pattern_keys(&self, open: usize, close: usize) -> Vec<(String, String)> {
        let mut keys = Vec::new();
        let mut depth = 0usize;
        let mut at = open + 1;
        while at < close {
            match self.tokens[at].kind {
                TokenKind::Punct(b'{' | b'[' | b'(') => depth += 1,
                TokenKind::Punct(b'}' | b']' | b')') => depth = depth.saturating_sub(1),
                TokenKind::Ident
                    if depth == 0 && (self.punct(at - 1, b'{') || self.punct(at - 1, b',')) =>
                {
                    let key = self.ident(at).unwrap_or_default();
                    let bound = if self.punct(at + 1, b':') {
                        self.ident(at + 2).unwrap_or_else(|| key.clone())
                    } else {
                        key.clone()
                    };
                    keys.push((key, bound));
                }
                _ => {}
            }
            at += 1;
        }
        keys
    }

    fn resolve(&self, name: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::from([name.to_owned()]);
        let mut pending = vec![name.to_owned()];
        while let Some(next) = pending.pop() {
            for section in self.aliases.get(&next).into_iter().flatten() {
                if found.insert(section.clone()) {
                    pending.push(section.clone());
                }
            }
        }
        found
    }

    fn reads(&self, out: &mut Reads) {
        for (key, fields) in &self.aliases {
            if let Some(section) = key.strip_prefix('\0') {
                for resolved in self.resolve(section) {
                    for field in fields {
                        out.record(&resolved, field);
                    }
                }
            }
        }
        for at in 0..self.tokens.len() {
            let Some(object) = self.ident(at) else {
                continue;
            };
            let Some(dot) = self.member(at + 1) else {
                continue;
            };
            let Some(field) = self.ident(dot + 1) else {
                continue;
            };
            if self.punct(dot + 2, b'(') {
                continue;
            }
            for section in self.resolve(&object) {
                out.record(&section, &field);
            }
        }
    }
}

fn read_js(out: &mut Reads) {
    let sources: Vec<String> = js_files()
        .iter()
        .map(|path| fs::read_to_string(path).unwrap_or_default())
        .collect();
    let mut files: Vec<JsFile<'_>> = sources.iter().map(|source| JsFile::new(source)).collect();
    let mut first_arguments = Vec::new();
    for file in &mut files {
        file.bindings(&mut first_arguments);
    }
    for file in &mut files {
        file.parameters(&first_arguments);
        file.reads(out);
    }
}

// ---------------------------------------------------------------------------

/// The declared keys a reader could read: those with no keys below them.
fn declared_leaves() -> BTreeSet<String> {
    let paths = Schema::parse(SOURCE).expect("the schema reads").key_paths();
    paths
        .iter()
        .filter(|path| {
            let prefix = format!("{path}.");
            !paths.iter().any(|other| other.starts_with(&prefix))
        })
        .cloned()
        .collect()
}

fn is_read(path: &str, reads: &Reads) -> bool {
    let mut segments = path.rsplit('.');
    let field = snake(segments.next().unwrap_or_default());
    match segments.next() {
        Some(section) => reads.pairs.contains(&(snake(section), field)),
        None => reads.fields.contains(&field),
    }
}

#[test]
fn every_declared_key_is_read_by_something() {
    let holders = Holders::read();
    assert!(
        holders
            .of("CoverageThresholdConfig")
            .is_some_and(|held| held.len() == 2),
        "the type index is not reading this crate's structs: {:?}",
        holders.of("CoverageThresholdConfig")
    );
    let mut reads = Reads::default();
    read_rust(&holders, &mut reads);
    read_js(&mut reads);

    let leaves = declared_leaves();
    assert!(leaves.len() > 100, "the schema reader found almost nothing");
    let unread: Vec<&str> = leaves
        .iter()
        .map(String::as_str)
        .filter(|path| !is_read(path, &reads))
        .collect();
    assert!(
        unread.is_empty(),
        "declared in {SCHEMA}, and read by nothing: {unread:#?}"
    );
}

/// The reader this test trusts, on the shapes it claims to see.
#[test]
fn the_reader_sees_the_shapes_it_claims() {
    let holders = Holders {
        sections: BTreeMap::from([(String::from("BConfig"), BTreeSet::from([String::from("b")]))]),
        accessors: BTreeMap::from([(
            String::from("b_section"),
            BTreeSet::from([String::from("b")]),
        )]),
    };
    let rust = |source: &str| {
        let tokens = without_tests(trees(TokenStream::from_str(source).unwrap()));
        let mut file = RustFile {
            holders: &holders,
            aliases: BTreeMap::new(),
        };
        file.bindings(&tokens);
        let mut reads = Reads::default();
        file.reads(&tokens, None, &mut reads);
        reads
            .pairs
            .contains(&(String::from("b"), String::from("c")))
    };
    assert!(rust("fn f() { config.a.b.c }"));
    assert!(rust("fn f() { config.a.b?.c }"));
    assert!(rust("fn f() { let x = &config.a.b; x.c }"));
    assert!(rust("fn f() { let x = config.a.b.clone(); x.c }"));
    assert!(rust("fn f() { if let Some(x) = &config.b { x.c } }"));
    assert!(rust("fn f() { config.b.as_ref().map(|x| x.c) }"));
    assert!(rust("fn f(x: &BConfig) { x.c }"));
    assert!(rust("impl BConfig { fn f(&self) { self.c } }"));
    assert!(rust("fn f(x: BConfig) { let BConfig { c, .. } = x; }"));
    assert!(rust("fn f() { json!({ \"c\": config.b.c }) }"));
    assert!(rust("fn f() { config.b_section().c }"));
    // Not reads: a method call, a construction, and test code.
    assert!(!rust("fn f() { config.b.c() }"));
    assert!(!rust("fn f() -> BConfig { BConfig { c: 1 } }"));
    assert!(!rust("#[cfg(test)] mod tests { fn f() { config.b.c } }"));
    assert!(!rust("#[test] fn f() { config.b.c }"));
    assert!(!rust("fn f() { let x = config.b.build(); x.c }"));

    let js = |source: &str| {
        let mut file = JsFile::new(source);
        let mut calls = Vec::new();
        file.bindings(&mut calls);
        file.parameters(&calls);
        let mut reads = Reads::default();
        file.reads(&mut reads);
        reads
            .pairs
            .contains(&(String::from("b"), String::from("c_d")))
    };
    assert!(js("const x = config.a?.b?.cD;"));
    assert!(js("const x = a.b ?? {}; x.cD;"));
    assert!(js("const { cD } = a.b;"));
    assert!(js("const { cD = 1 } = config.b ?? {};"));
    assert!(js("function f(x) { return x.cD; }\nf(a.b);"));
    assert!(!js("a.b.cD();"));
    assert!(!js("const x = a.b(); x.cD;"));
}
