#![allow(clippy::disallowed_macros)]

//! `@uniflowed/config`'s Flow type and this crate, held to each other.
//!
//! # Why a test and not a person
//!
//! The guide's reason to write `uf.config.js` in Flow is that `defineConfig`
//! type-checks it where it is written. That makes the two halves — the Flow
//! type in `npm/config/internal/schema.js` and the `serde` shapes in this
//! crate — one surface with two authors, and for most of alpha nobody was
//! comparing them:
//!
//! * a key uf reads and the type does not declare is a documented option that
//!   fails to check. `lint.files`, `lint.ignore` and `app.router.enabled` were
//!   all three at once — documented in the configuration reference, accepted by
//!   the loader, absent from the type — which is ubugeeei-prod/uf#481, and they
//!   turned out to be a third of what was missing;
//! * a key the type declares and uf reads nowhere is the opposite defect with
//!   the same cause: an option a project can write, that checks, and that
//!   changes nothing.
//!
//! Neither shows up in a build. Both show up in somebody's project.
//!
//! # What is compared
//!
//! Key *paths*, in both directions, and not value types. `"biome" | "prettier"
//! | "none"` is a judgement the package makes and this test has no way to hold
//! it to one — several declarations are deliberately narrower than what the
//! loader will parse. What it can hold is the set of names, which is what
//! ubugeeei-prod/uf#481 is about.
//!
//! A map whose keys are the project's own — `tasks`, `lint.rules`,
//! `build.hooks`, `pm.scopes`, `env.toolchain`, `vite` — contributes its own
//! name and nothing under it. Both sides say that the same way: an indexer in
//! Flow, an empty map in the serialized config.
//!
//! # Whose reader
//!
//! The declared side is [`uf_config::schema`], the same reader `uf lsp`
//! completes `uf.config.js` from, over the same embedded copy of the file. A
//! key that reader walked past would be missing from an editor's completion
//! list and from this comparison at once; reading the schema twice, in two
//! ways, is how one of the two could be right about a key the other never saw.

use std::collections::BTreeSet;

use uf_config::schema::{SOURCE, Schema};
use uf_config::{
    ByteSize, LibraryConfig, NativeTestRunnerConfig, Permissions, SizeBudget, TestRunnerConfig,
    UniflowedConfig,
};

/// The Flow type this test reads, for the messages.
const SCHEMA: &str = "npm/config/internal/schema.js";

/// A config in which every optional *section* is present.
///
/// The walk below reads key paths out of a serialized config, so a section
/// held in an `Option` and left `None` serializes as `null` and hides every key
/// it has. Two are like that, and both are sections rather than scalars:
/// `permissions`, which is absent to mean "no permission model at all", and the
/// four `build.budgets` ceilings, which are absent to mean "do not police this
/// build". Filling them is what puts their keys in front of the comparison.
///
/// A scalar `Option` — `site.url`, `pm.registry`, `ignore` — needs nothing
/// here: it is a leaf either way, and `null` is as much a leaf as a string.
///
/// `lint.rules` is emptied for the opposite reason. It is a map whose keys are
/// rule ids, and it is the one such map with a non-empty default; left alone,
/// sixty-three rule names would arrive here looking like configuration keys.
fn every_section() -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.permissions = Some(Permissions::default());
    let budget = SizeBudget::new(ByteSize::from_bytes(0));
    config.build.budgets.total = Some(budget);
    config.build.budgets.initial_js = Some(budget);
    config.build.budgets.per_route = Some(budget);
    config.build.budgets.per_asset = Some(budget);
    // A library build is opt-in, so `build.lib` is `None` by default and the
    // walk below never sees inside it — which would read as "the schema
    // declares three keys uf does not have" for keys uf reads perfectly well.
    // Populating it is what makes the comparison about *names* rather than
    // about which sections happen to be on.
    config.build.lib = Some(LibraryConfig::default());
    config.app.router.native_links = Some(uf_config::NativeLinksConfig {
        origins: Vec::new(),
        routes: Vec::new(),
        ios_app_ids: Vec::new(),
        android_package: "com.example.app".into(),
        android_sha256: Vec::new(),
    });
    config.lint.rules.clear();
    config
}

/// Every key path the loader round-trips, as `a.b.c`.
///
/// Read from a serialized config rather than from the struct definitions,
/// because serialization is where `rename_all = "camelCase"`, `#[serde(skip)]`
/// and every other attribute have already been applied — which makes this the
/// set of names a `uf.config.js` may actually contain, rather than the set of
/// Rust field names that mostly resembles it.
///
/// A key that accepts two shapes contributes the paths of both, which is how
/// the Flow side reads a union: `test.runner` is a spec string or — deprecated,
/// and still read — the object it replaced, and one serialized config can only
/// hold one of the two. [`every_other_shape`] holds the other.
fn accepted_paths() -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for config in [every_section(), every_other_shape()] {
        let value = serde_json::to_value(config).expect("the config serializes");
        collect_json(&value, "", &mut paths);
    }
    paths
}

/// The shapes [`every_section`] cannot hold at the same time as its own.
///
/// `test.runner` is `null` there — not written — which is a leaf. Written as
/// the object it used to be, its keys are the ones the schema's object half
/// declares, and a schema that dropped that half would be refusing configs uf
/// still reads.
fn every_other_shape() -> UniflowedConfig {
    let mut config = every_section();
    config.test.runner = Some(TestRunnerConfig::Object(NativeTestRunnerConfig::default()));
    config
}

fn collect_json(value: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    let serde_json::Value::Object(fields) = value else {
        return;
    };
    for (key, value) in fields {
        let path = join(prefix, key);
        out.insert(path.clone());
        collect_json(value, &path, out);
    }
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Every key path a Flow source's `UniflowedConfig` declares.
fn declared_paths(source: &str) -> BTreeSet<String> {
    Schema::parse(source)
        .unwrap_or_else(|error| panic!("{SCHEMA} does not read: {error}"))
        .key_paths()
}

/// The Flow type declares every key uf reads, and no key it does not.
///
/// # Reading a failure
///
/// The two lists are the two defects, and they have different fixes.
///
/// *"uf reads … and the schema does not declare"* — a key was added to this
/// crate and not to `npm/config/internal/schema.js`. A project that writes
/// it gets a Flow error on a working config, which is the whole of
/// ubugeeei-prod/uf#481.
///
/// *"the schema declares … and uf reads nothing there"* — usually the reverse:
/// a key removed or renamed here and left in the type, so a project can write
/// something that checks and does nothing. It is also what a *new* `Option`
/// section looks like: `None` serializes as `null` and hides its own keys, so
/// if the key you just added holds a section, add it to [`every_section`] too.
#[test]
fn the_flow_schema_declares_every_key_uf_reads() {
    let declared = declared_paths(SOURCE);
    let accepted = accepted_paths();

    // A floor rather than an exact count: the number moves with every key
    // anybody adds, and a test that has to be edited to add a config key is a
    // test people learn to edit without reading. What it guards is the walk
    // itself — a reader that silently found nothing would otherwise report two
    // empty sets as perfect agreement.
    assert!(
        declared.len() > 150,
        "the schema reader found almost nothing, so it is not checking anything: {}",
        declared.len()
    );

    let missing: Vec<&String> = accepted.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&accepted).collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "`@uniflowed/config` and `uf_config` disagree about {}:\n  \
         uf reads, and the schema does not declare: {missing:#?}\n  \
         the schema declares, and uf reads nothing there: {extra:#?}",
        SCHEMA
    );
}

/// The reader that the comparison trusts, held to the shapes the schema uses.
///
/// A reader with a hole reports agreement about a key it never saw, which is
/// worse than having no check at all: it is the same answer with a signature on
/// it. Every case below is a shape `schema.js` actually contains.
#[test]
fn the_schema_reader_sees_every_shape_the_schema_uses() {
    let paths = |source: &str| declared_paths(source);

    // Nesting, and the optional marker every key carries.
    assert_eq!(
        paths("export type UniflowedConfig = { readonly a?: { readonly b?: boolean } };"),
        ["a", "a.b"].map(str::to_owned).into_iter().collect()
    );

    // A named alias declared in the same file is the shape it stands for.
    assert_eq!(
        paths(
            "type Inner = { readonly b?: string };\n\
             export type UniflowedConfig = { readonly a?: Inner };"
        ),
        ["a", "a.b"].map(str::to_owned).into_iter().collect()
    );

    // An indexer is a map whose keys are the project's. `TaskDefinition`'s
    // object half is behind one, and neither side of the comparison can
    // enumerate what a project will call its tasks.
    assert_eq!(
        paths(
            "type Task = string | { readonly command: string };\n\
             export type UniflowedConfig = { readonly tasks?: { readonly [string]: Task } };"
        ),
        ["tasks"].map(str::to_owned).into_iter().collect()
    );

    // A list of objects is a leaf, the way a `Vec` is on the other side:
    // `plugins` is a key, and a plugin's `order` is not `plugins.order`.
    assert_eq!(
        paths(
            "type Entry = string | { readonly name: string, readonly order?: string };\n\
             export type UniflowedConfig = { readonly plugins?: $ReadOnlyArray<Entry> };"
        ),
        ["plugins"].map(str::to_owned).into_iter().collect()
    );

    // Both halves of a union that is not behind an indexer. Nothing in the
    // schema is written this way today, which is exactly why the reader has to
    // be: the day one is, it must not be read as half a shape.
    assert_eq!(
        paths(
            "export type UniflowedConfig = \
             { readonly a?: { readonly b?: string } | { readonly c?: string } };"
        ),
        ["a", "a.b", "a.c"].map(str::to_owned).into_iter().collect()
    );

    // And through `?`, which is a union with `null` spelled differently.
    assert_eq!(
        paths("export type UniflowedConfig = { readonly a?: ?{ readonly b?: string } };"),
        ["a", "a.b"].map(str::to_owned).into_iter().collect()
    );
}
