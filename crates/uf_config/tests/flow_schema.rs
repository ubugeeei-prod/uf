//! `@uniflowed/config`'s Flow type and this crate, held to each other.
//!
//! # Why a test and not a person
//!
//! The guide's reason to write `uf.config.js` in Flow is that `defineConfig`
//! type-checks it where it is written. That makes the two halves — the Flow
//! type in `packages/config/internal/schema.js` and the `serde` shapes in this
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

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use uf_config::{ByteSize, Permissions, SizeBudget, UniflowedConfig};
use uf_flow::Loc;
use uf_flow::ast::{statement, types};

/// This checkout, found by walking out of the crate rather than by counting.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the repository")
}

/// The Flow type this test reads.
const SCHEMA: &str = "packages/config/internal/schema.js";

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
fn accepted_paths() -> BTreeSet<String> {
    let value = serde_json::to_value(every_section()).expect("the config serializes");
    let mut paths = BTreeSet::new();
    collect_json(&value, "", &mut paths);
    paths
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

/// The type aliases one Flow module declares, by name.
type Aliases<'a> = BTreeMap<String, &'a types::Type<Loc, Loc>>;

/// Every key path `UniflowedConfig` declares in the Flow schema.
fn declared_paths(source: &str) -> BTreeSet<String> {
    let parsed = uf_flow::parse(source).expect("the schema parses");
    assert!(
        parsed.diagnostics.is_empty(),
        "{SCHEMA} does not parse: {:?}",
        parsed.diagnostics
    );

    let mut aliases = Aliases::new();
    for node in parsed.program.statements.iter() {
        match &**node {
            statement::StatementInner::TypeAlias { inner, .. } => {
                aliases.insert(inner.id.name.to_string(), &inner.right);
            }
            statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                if let Some(declaration) = &inner.declaration
                    && let statement::StatementInner::TypeAlias { inner, .. } = &**declaration
                {
                    aliases.insert(inner.id.name.to_string(), &inner.right);
                }
            }
            _ => {}
        }
    }

    let root = aliases
        .get("UniflowedConfig")
        .unwrap_or_else(|| panic!("{SCHEMA} declares `UniflowedConfig`"));
    let mut paths = BTreeSet::new();
    descend(root, "", &aliases, &mut paths);
    paths
}

/// The keys a type contributes under `prefix`.
///
/// A union contributes every member's, because `TaskDefinition` is
/// `string | { command, … }` and the object half is as much part of the surface
/// as the string half. A name declared in the same file is the shape it stands
/// for; a name that is not — `$ReadOnlyArray<Something>` — contributes nothing,
/// which is what makes a list of objects a leaf here exactly as a `Vec` is on
/// the other side.
fn descend(
    ty: &types::Type<Loc, Loc>,
    prefix: &str,
    aliases: &Aliases<'_>,
    out: &mut BTreeSet<String>,
) {
    match &**ty {
        types::TypeInner::Object { inner, .. } => object_paths(inner, prefix, aliases, out),
        types::TypeInner::Nullable { inner, .. } => descend(&inner.argument, prefix, aliases, out),
        types::TypeInner::Union { inner, .. } => {
            let (first, second, rest) = &inner.types;
            for member in [first, second].into_iter().chain(rest.iter()) {
                descend(member, prefix, aliases, out);
            }
        }
        types::TypeInner::Generic { inner, .. } => {
            let types::generic::Identifier::Unqualified(id) = &inner.id else {
                return;
            };
            if let Some(target) = aliases.get(id.name.as_str()) {
                descend(target, prefix, aliases, out);
            }
        }
        _ => {}
    }
}

fn object_paths(
    object: &types::Object<Loc, Loc>,
    prefix: &str,
    aliases: &Aliases<'_>,
    out: &mut BTreeSet<String>,
) {
    for property in object.properties.iter() {
        // An indexer is a map whose keys belong to the project — `tasks`,
        // `lint.rules`, `vite`. It names none of them, and neither does the
        // config it is compared against.
        let types::object::Property::NormalProperty(property) = property else {
            continue;
        };
        let Some(name) = key_name(&property.key) else {
            continue;
        };
        let path = join(prefix, &name);
        out.insert(path.clone());
        if let types::object::PropertyValue::Init(Some(value)) = &property.value {
            descend(value, &path, aliases, out);
        }
    }
}

fn key_name(key: &uf_flow::ast::expression::object::Key<Loc, Loc>) -> Option<String> {
    use uf_flow::ast::expression::object::Key;

    match key {
        Key::Identifier(id) => Some(id.name.to_string()),
        Key::StringLiteral((_, literal)) => Some(literal.value.to_string()),
        _ => None,
    }
}

/// The Flow type declares every key uf reads, and no key it does not.
///
/// # Reading a failure
///
/// The two lists are the two defects, and they have different fixes.
///
/// *"uf reads … and the schema does not declare"* — a key was added to this
/// crate and not to `packages/config/internal/schema.js`. A project that writes
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
    let schema = repository_root().join(SCHEMA);
    let source = fs::read_to_string(&schema)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", schema.display()));

    let declared = declared_paths(&source);
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
}
