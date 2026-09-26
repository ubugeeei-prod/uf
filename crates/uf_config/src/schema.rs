//! The shape of `uf.config.js`, read out of the Flow type that declares it.
//!
//! # One reader, and who reads through it
//!
//! `packages/config/internal/schema.js` is the type `defineConfig` checks a
//! config against. Two things in uf read it as *data* rather than as a type:
//!
//! * `tests/flow_schema.rs`, which holds the key names it declares to the ones
//!   this crate deserializes, in both directions;
//! * `uf lsp`, which completes keys and values in `uf.config.js` and explains
//!   the key under the cursor.
//!
//! Both read it through this module. A reader with a hole — a shape it walks
//! past without seeing — would otherwise be a completion list missing a key
//! *and* a consistency test reporting agreement about that same key: one
//! defect in two places, with nothing positioned to notice either.
//!
//! # Why the Flow type, and not the structs beside it
//!
//! The `serde` structs in this crate know every name uf accepts and nothing a
//! person should be told about one. The documentation lives in the Flow file,
//! and so do the judgements narrower than the loader: `fmt.nonFlow.formatter`
//! is `"biome" | "prettier" | "none"` there and any string here. An editor
//! offering values should offer the ones the package means, which are the ones
//! a type check of the config accepts.
//!
//! # Compiled in
//!
//! [`SOURCE`] is the file, embedded when uf is built, so reading it needs
//! nothing installed. An editor opens `uf.config.js` before `uf install` has
//! run as often as after it, and an answer that depended on `node_modules`
//! would be an answer that changed with it.
//!
//! # What counts as documentation
//!
//! A comment documents a key when it sits directly above it: nothing but other
//! comments between the two, no blank line, and not trailing the line of the
//! key before. Both kinds of comment count on a key, because the schema has
//! always documented keys with `//` as often as with `/** */`.
//!
//! On a *type alias* only a `/** */` block counts. The `//` comments above the
//! aliases are the history of the declaration — which values were removed and
//! why, which lint rule a spelling trips — written for whoever edits the file
//! rather than whoever writes the key. A key typed by an alias carries the
//! alias's documentation after its own, which is how `pm.packageManager`, with
//! no comment of its own, still explains what `"auto"` reads.
//!
//! Lint directives (`uf-lint-disable …`, `@flow`) are never documentation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use compact_str::CompactString;
use thiserror::Error;
use uf_flow::ast::{Comment, CommentKind, expression, statement, types};
use uf_flow::{Loc, ParseFailure, Position};

/// `packages/config/internal/schema.js`, as this build of uf was compiled with.
pub const SOURCE: &str = include_str!("../../../packages/config/internal/schema.js");

/// The alias `defineConfig` takes, which is the root of every path.
const ROOT: &str = "UniflowedConfig";

/// The declared shape of `uf.config.js`.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    root: Shape,
}

/// Why a schema could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaError {
    /// The parser refused the source before reading it: too large, or nested
    /// past a ceiling.
    #[error("the schema was refused before it was parsed: {0}")]
    Refused(#[from] ParseFailure),
    /// The source is not valid Flow.
    #[error("the schema does not parse: {message} (line {line})")]
    Syntax {
        /// The parser's message.
        message: String,
        /// One-based line, or zero when the parser named none.
        line: u32,
    },
    /// The source parses and declares no `UniflowedConfig`.
    #[error("the schema declares no `UniflowedConfig` type")]
    MissingRoot,
    /// The thread the parser needs could not be started or joined.
    #[error("the thread reading the schema could not run")]
    Thread,
}

/// One step from a config object towards a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<'a> {
    /// The value of a key. A name no object declares still resolves through an
    /// indexer: `tasks.build` is the value of a task the project named.
    Key(&'a str),
    /// An element of a list.
    Element,
}

/// What a value may be.
///
/// Deliberately coarser than Flow's type grammar: this is what an editor can
/// offer or explain, and a type it has nothing to say about is
/// [`Shape::Other`] rather than a variant nobody reads.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// An object type: the keys it declares, and what an indexer maps to.
    Object(Object),
    /// `$ReadOnlyArray<T>`, `Array<T>` or `T[]`, as its element.
    Array(Box<Shape>),
    /// Every member of a union. `?T` is `T | null`.
    Union(Vec<Shape>),
    /// A type alias declared in the schema, and what it stands for.
    ///
    /// Kept rather than flattened away, because the name is information: a
    /// key typed by `PackageManagerPreference` is a key about package
    /// managers, whatever its members are.
    Alias {
        /// The alias as declared.
        name: CompactString,
        /// The type it stands for.
        target: Box<Shape>,
    },
    /// `string`.
    String,
    /// `number`.
    Number,
    /// `boolean`.
    Boolean,
    /// `null`.
    Null,
    /// A string literal type, by its value.
    StringLiteral(CompactString),
    /// A number literal type, as written.
    NumberLiteral(CompactString),
    /// `true` or `false` as a type.
    BooleanLiteral(bool),
    /// Anything else: `mixed`, a function, an alias declared somewhere else, or
    /// an alias that refers to itself.
    Other,
}

/// An object type's keys, in declaration order, and its indexer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Object {
    keys: Vec<Key>,
    indexer: Option<Box<Shape>>,
}

/// One declared key.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    name: CompactString,
    documentation: Option<String>,
    type_text: String,
    shape: Shape,
}

impl Schema {
    /// Read a schema out of Flow source.
    ///
    /// The parser runs on a thread of its own with the stack
    /// [`uf_flow::PARSE_STACK_BYTES`] names, so this can be called from any
    /// thread, including an editor server's.
    ///
    /// # Errors
    ///
    /// [`SchemaError`] when the source does not parse, declares no
    /// `UniflowedConfig`, or the parser's thread cannot run.
    pub fn parse(source: &str) -> Result<Self, SchemaError> {
        std::thread::scope(|scope| {
            let worker = std::thread::Builder::new()
                .name("uf-config-schema".into())
                .stack_size(uf_flow::PARSE_STACK_BYTES)
                .spawn_scoped(scope, || read(source))
                .map_err(|_| SchemaError::Thread)?;
            worker.join().map_err(|_| SchemaError::Thread)?
        })
    }

    /// [`SOURCE`], read once per process.
    ///
    /// A schema that does not read comes back empty rather than as an error:
    /// the caller is an editor server, and an empty answer is one it can give
    /// on every request where a panic would end the session. It cannot happen
    /// to a build of uf that passed its own tests —
    /// `the_embedded_schema_reads` is one of them.
    pub fn embedded() -> &'static Self {
        static EMBEDDED: OnceLock<Schema> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            Self::parse(SOURCE).unwrap_or_else(|_| Self {
                root: Shape::Object(Object::default()),
            })
        })
    }

    /// The shape of the object `defineConfig` takes.
    pub fn root(&self) -> &Shape {
        &self.root
    }

    /// Every shape the value at `path` may have.
    ///
    /// More than one when a union has more than one member the path passes
    /// through — `TaskDefinition` is a string or an object, and only the object
    /// has keys. Empty when nothing declared is there.
    pub fn resolve(&self, path: &[Step<'_>]) -> Vec<&Shape> {
        let mut current = vec![&self.root];
        for step in path {
            let mut next = Vec::new();
            for shape in current {
                match *step {
                    Step::Key(name) => {
                        for object in shape.objects() {
                            match object.key(name) {
                                Some(key) => next.push(&key.shape),
                                None => next.extend(object.indexer()),
                            }
                        }
                    }
                    Step::Element => next.extend(shape.elements()),
                }
            }
            if next.is_empty() {
                return next;
            }
            current = next;
        }
        current
    }

    /// The declared key the last [`Step::Key`] of `path` names.
    ///
    /// Trailing [`Step::Element`]s are skipped, so the key behind a list
    /// element is the list's key: an entry of `app.targets` is described by
    /// `app.targets`.
    pub fn key(&self, path: &[Step<'_>]) -> Option<&Key> {
        let last = path.iter().rposition(|step| matches!(step, Step::Key(_)))?;
        let Step::Key(name) = path[last] else {
            return None;
        };
        self.resolve(&path[..last])
            .into_iter()
            .flat_map(Shape::objects)
            .find_map(|object| object.key(name))
    }

    /// Every key path the schema declares, as `a.b.c`.
    ///
    /// The set `tests/flow_schema.rs` compares with the loader, which is why it
    /// stops where the loader's serialized config stops: the keys of a union's
    /// object members count, and nothing under a list or an indexer does. A
    /// plugin's `order` is not `plugins.order`, and nobody can enumerate what a
    /// project will name its tasks.
    pub fn key_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        collect_paths(&self.root, "", &mut paths);
        paths
    }
}

fn collect_paths(shape: &Shape, prefix: &str, out: &mut BTreeSet<String>) {
    for object in shape.objects() {
        for key in &object.keys {
            let path = if prefix.is_empty() {
                key.name.to_string()
            } else {
                uf_infra::into_string(compact_str::format_compact!("{prefix}.{}", key.name))
            };
            collect_paths(&key.shape, &path, out);
            out.insert(path);
        }
    }
}

impl Shape {
    /// What this shape can be once aliases and unions are seen through.
    ///
    /// Never an [`Shape::Alias`] or a [`Shape::Union`] itself; each member in
    /// the order it was declared.
    pub fn members(&self) -> Vec<&Shape> {
        let mut found = Vec::new();
        let mut pending = vec![self];
        while let Some(shape) = pending.pop() {
            match shape {
                Self::Alias { target, .. } => pending.push(target),
                Self::Union(members) => pending.extend(members.iter().rev()),
                other => found.push(other),
            }
        }
        found
    }

    /// The object types among [`Shape::members`].
    pub fn objects(&self) -> Vec<&Object> {
        self.members()
            .into_iter()
            .filter_map(|member| match member {
                Self::Object(object) => Some(object),
                _ => None,
            })
            .collect()
    }

    /// The element types of the lists among [`Shape::members`].
    pub fn elements(&self) -> Vec<&Shape> {
        self.members()
            .into_iter()
            .filter_map(|member| match member {
                Self::Array(element) => Some(&**element),
                _ => None,
            })
            .collect()
    }

    /// The aliases this shape is written through, outermost first, at any depth
    /// of alias or union.
    pub fn aliases(&self) -> Vec<&str> {
        let mut found = Vec::new();
        let mut pending = vec![self];
        while let Some(shape) = pending.pop() {
            match shape {
                Self::Alias { name, target } => {
                    found.push(name.as_str());
                    pending.push(target);
                }
                Self::Union(members) => pending.extend(members.iter().rev()),
                _ => {}
            }
        }
        found
    }
}

impl Object {
    /// The declared keys, in declaration order. A key declared twice is one
    /// key, carrying both declarations' documentation.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// The declared key called `name`.
    pub fn key(&self, name: &str) -> Option<&Key> {
        self.keys.iter().find(|key| key.name == name)
    }

    /// What an indexer maps a name nobody declared to, when there is one.
    pub fn indexer(&self) -> Option<&Shape> {
        self.indexer.as_deref()
    }
}

impl Key {
    /// The key as it is written in `uf.config.js`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Markdown: the comment above the key, then the documentation of any
    /// alias its type names.
    pub fn documentation(&self) -> Option<&str> {
        self.documentation.as_deref()
    }

    /// The declared type, on one line, with an object type abbreviated to
    /// `{ … }` — the keys inside one are completions of their own, and a
    /// sixty-line detail is not a detail.
    pub fn type_text(&self) -> &str {
        &self.type_text
    }

    /// What the key's value may be.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
}

/// A type alias as the schema declares it.
struct Declared<'a> {
    ty: &'a types::Type<Loc, Loc>,
    documentation: Option<String>,
}

/// Read a parsed schema. Runs on the parser's thread.
fn read(source: &str) -> Result<Schema, SchemaError> {
    let parsed = uf_flow::parse(source)?;
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(SchemaError::Syntax {
            message: diagnostic.message.clone(),
            line: diagnostic.line.unwrap_or(0),
        });
    }

    let comments = parsed.comments();
    let mut aliases: BTreeMap<&str, Declared<'_>> = BTreeMap::new();
    let mut previous: Option<Position> = None;
    for node in parsed.program.statements.iter() {
        let loc = node.loc();
        if let Some(alias) = type_alias(node) {
            aliases.insert(
                alias.id.name.as_str(),
                Declared {
                    ty: &alias.right,
                    documentation: documentation(comments, previous, loc.start, Wanted::DocBlocks),
                },
            );
        }
        previous = Some(loc.end);
    }

    let root = aliases.get(ROOT).ok_or(SchemaError::MissingRoot)?.ty;
    let mut reader = Reader {
        source,
        lines: Lines::new(source),
        comments,
        aliases: &aliases,
        expanding: Vec::new(),
    };
    Ok(Schema {
        root: reader.shape(root),
    })
}

/// The alias a statement declares, exported or not.
fn type_alias(node: &statement::Statement<Loc, Loc>) -> Option<&statement::TypeAlias<Loc, Loc>> {
    match &**node {
        statement::StatementInner::TypeAlias { inner, .. } => Some(inner),
        statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
            match inner.declaration.as_deref() {
                Some(statement::StatementInner::TypeAlias { inner, .. }) => Some(inner),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The walk from a type to a [`Shape`], with what it needs to hand.
struct Reader<'a> {
    source: &'a str,
    lines: Lines,
    comments: &'a [Comment<Loc>],
    aliases: &'a BTreeMap<&'a str, Declared<'a>>,
    /// The aliases being expanded right now, innermost last. An alias found
    /// here again refers to itself, and reads as [`Shape::Other`] instead of
    /// recursing until the stack runs out.
    expanding: Vec<&'a str>,
}

impl<'a> Reader<'a> {
    fn shape(&mut self, ty: &'a types::Type<Loc, Loc>) -> Shape {
        match &**ty {
            types::TypeInner::Object { loc, inner } => Shape::Object(self.object(loc, inner)),
            types::TypeInner::Nullable { inner, .. } => {
                Shape::Union(vec![self.shape(&inner.argument), Shape::Null])
            }
            types::TypeInner::Union { inner, .. } => {
                let (first, second, rest) = &inner.types;
                Shape::Union(
                    [first, second]
                        .into_iter()
                        .chain(rest.iter())
                        .map(|member| self.shape(member))
                        .collect(),
                )
            }
            types::TypeInner::Array { inner, .. } => {
                Shape::Array(Box::new(self.shape(&inner.argument)))
            }
            types::TypeInner::Generic { inner, .. } => self.generic(inner),
            types::TypeInner::String { .. } => Shape::String,
            types::TypeInner::Number { .. } => Shape::Number,
            types::TypeInner::Boolean { .. } => Shape::Boolean,
            types::TypeInner::Null { .. } => Shape::Null,
            types::TypeInner::StringLiteral { literal, .. } => {
                Shape::StringLiteral(CompactString::from(literal.value.as_str()))
            }
            types::TypeInner::NumberLiteral { literal, .. } => {
                Shape::NumberLiteral(CompactString::from(literal.raw.as_str()))
            }
            types::TypeInner::BooleanLiteral { literal, .. } => {
                Shape::BooleanLiteral(literal.value)
            }
            _ => Shape::Other,
        }
    }

    /// A named type: a list, an alias this file declares, or something else.
    fn generic(&mut self, generic: &'a types::Generic<Loc, Loc>) -> Shape {
        let types::generic::Identifier::Unqualified(id) = &generic.id else {
            return Shape::Other;
        };
        let name = id.name.as_str();
        if is_list(name) {
            let element = generic
                .targs
                .as_ref()
                .and_then(|targs| targs.arguments.first());
            return Shape::Array(Box::new(
                element.map_or(Shape::Other, |element| self.shape(element)),
            ));
        }

        let aliases = self.aliases;
        let Some(declared) = aliases.get(name) else {
            return Shape::Other;
        };
        if self.expanding.contains(&name) {
            return Shape::Other;
        }
        self.expanding.push(name);
        let target = self.shape(declared.ty);
        self.expanding.pop();
        Shape::Alias {
            name: CompactString::from(name),
            target: Box::new(target),
        }
    }

    fn object(&mut self, loc: &Loc, object: &'a types::Object<Loc, Loc>) -> Object {
        let mut keys: Vec<Key> = Vec::new();
        let mut indexer = None;
        // Where the previous member ended, so a comment trailing it is not
        // read as the next one's documentation. The first member's is the
        // object's own `{`.
        let mut after = loc.start;

        for property in object.properties.iter() {
            match property {
                types::object::Property::NormalProperty(property) => {
                    let above = documentation(
                        self.comments,
                        Some(after),
                        property.loc.start,
                        Wanted::Everything,
                    );
                    after = property.loc.end;
                    let (Some(name), types::object::PropertyValue::Init(Some(value))) =
                        (key_name(&property.key), &property.value)
                    else {
                        continue;
                    };
                    let documentation = join(above, self.alias_documentation(value));

                    // `app.router.enabled` is declared twice, each time with a
                    // different paragraph about it. Flow accepts that, a
                    // completion list must not offer the key twice, and both
                    // paragraphs are true.
                    if let Some(existing) = keys.iter_mut().find(|key| key.name == name) {
                        existing.documentation = join(existing.documentation.take(), documentation);
                        continue;
                    }
                    keys.push(Key {
                        type_text: self.render(value),
                        shape: self.shape(value),
                        name,
                        documentation,
                    });
                }
                types::object::Property::Indexer(entry) => {
                    after = entry.loc.end;
                    indexer = Some(Box::new(self.shape(&entry.value)));
                }
                types::object::Property::SpreadProperty(spread) => after = spread.loc.end,
                types::object::Property::CallProperty(call) => after = call.loc.end,
                types::object::Property::InternalSlot(slot) => after = slot.loc.end,
                types::object::Property::MappedType(mapped) => after = mapped.loc.end,
                types::object::Property::PrivateField(field) => after = field.loc.end,
            }
        }

        Object { keys, indexer }
    }

    /// The documentation of every alias a key's type names directly: through a
    /// union, a `?`, or a list, and not inside an object, whose keys are
    /// documented where they are declared.
    fn alias_documentation(&self, ty: &'a types::Type<Loc, Loc>) -> Option<String> {
        let mut found = None;
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match &**ty {
                types::TypeInner::Generic { inner, .. } => {
                    let types::generic::Identifier::Unqualified(id) = &inner.id else {
                        continue;
                    };
                    let name = id.name.as_str();
                    if is_list(name) {
                        if let Some(targs) = &inner.targs {
                            pending.extend(targs.arguments.iter().rev());
                        }
                    } else if let Some(declared) = self.aliases.get(name) {
                        found = join(found, declared.documentation.clone());
                    }
                }
                types::TypeInner::Nullable { inner, .. } => pending.push(&inner.argument),
                types::TypeInner::Array { inner, .. } => pending.push(&inner.argument),
                types::TypeInner::Union { inner, .. } => {
                    let (first, second, rest) = &inner.types;
                    pending.extend(rest.iter().rev());
                    pending.push(second);
                    pending.push(first);
                }
                _ => {}
            }
        }
        found
    }

    /// A type as one line of text, the way it was written, with object types
    /// abbreviated.
    fn render(&self, ty: &types::Type<Loc, Loc>) -> String {
        match &**ty {
            types::TypeInner::Object { inner, .. } => {
                let mut properties = inner.properties.iter();
                match (properties.next(), properties.next()) {
                    (None, _) => String::from("{}"),
                    // A map whose keys are the project's is worth spelling
                    // out: `{ [string]: TaskDefinition }` says what goes in it,
                    // and there are no declared keys to complete instead.
                    (Some(types::object::Property::Indexer(indexer)), None) => {
                        uf_infra::into_string(compact_str::format_compact!(
                            "{{ [{}]: {} }}",
                            self.render(&indexer.key),
                            self.render(&indexer.value)
                        ))
                    }
                    _ => String::from("{ … }"),
                }
            }
            types::TypeInner::Union { inner, .. } => {
                let (first, second, rest) = &inner.types;
                [first, second]
                    .into_iter()
                    .chain(rest.iter())
                    .map(|member| self.render(member))
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
            types::TypeInner::Nullable { inner, .. } => uf_infra::into_string(
                compact_str::format_compact!("?{}", self.grouped(&inner.argument)),
            ),
            types::TypeInner::Array { inner, .. } => uf_infra::into_string(
                compact_str::format_compact!("{}[]", self.grouped(&inner.argument)),
            ),
            types::TypeInner::Generic { inner, .. } => {
                let name = match &inner.id {
                    types::generic::Identifier::Unqualified(id) => id.name.to_string(),
                    _ => self.slice(ty.loc()),
                };
                match &inner.targs {
                    Some(targs) if !targs.arguments.is_empty() => {
                        uf_infra::into_string(compact_str::format_compact!(
                            "{name}<{}>",
                            targs
                                .arguments
                                .iter()
                                .map(|argument| self.render(argument))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    }
                    _ => name,
                }
            }
            types::TypeInner::StringLiteral { literal, .. } => literal.raw.to_string(),
            types::TypeInner::NumberLiteral { literal, .. } => literal.raw.to_string(),
            types::TypeInner::BooleanLiteral { literal, .. } => literal.value.to_string(),
            types::TypeInner::String { .. } => String::from("string"),
            types::TypeInner::Number { .. } => String::from("number"),
            types::TypeInner::Boolean { .. } => String::from("boolean"),
            types::TypeInner::Null { .. } => String::from("null"),
            types::TypeInner::Void { .. } => String::from("void"),
            types::TypeInner::Mixed { .. } => String::from("mixed"),
            types::TypeInner::Any { .. } => String::from("any"),
            _ => self.slice(ty.loc()),
        }
    }

    /// [`Reader::render`], in parentheses when a prefix or suffix would
    /// otherwise bind to only part of it.
    fn grouped(&self, ty: &types::Type<Loc, Loc>) -> String {
        let text = self.render(ty);
        match &**ty {
            types::TypeInner::Union { .. } | types::TypeInner::Intersection { .. } => {
                uf_infra::into_string(compact_str::format_compact!("({text})"))
            }
            _ => text,
        }
    }

    /// The source a location covers, with its whitespace collapsed to one line.
    fn slice(&self, loc: &Loc) -> String {
        let start = self.lines.offset(self.source, loc.start);
        let end = self.lines.offset(self.source, loc.end).max(start);
        self.source
            .get(start..end)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The two names a list type goes by.
fn is_list(name: &str) -> bool {
    matches!(name, "$ReadOnlyArray" | "Array")
}

fn key_name(key: &expression::object::Key<Loc, Loc>) -> Option<CompactString> {
    match key {
        expression::object::Key::Identifier(id) => Some(CompactString::from(id.name.as_str())),
        expression::object::Key::StringLiteral((_, literal)) => {
            Some(CompactString::from(literal.value.as_str()))
        }
        _ => None,
    }
}

/// Which comments may document a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wanted {
    /// `//` and `/** */` alike: a key.
    Everything,
    /// `/** */` only: a type alias. See the module header.
    DocBlocks,
}

/// The documentation of whatever starts at `before`, as markdown.
///
/// The comments taken are the run that ends directly above `before`, walking
/// upwards while each one touches the next — no blank line between — and
/// stopping at `after`, where the previous declaration ended. A comment that
/// starts on `after`'s line trails that declaration rather than documenting
/// this one.
fn documentation(
    comments: &[Comment<Loc>],
    after: Option<Position>,
    before: Position,
    wanted: Wanted,
) -> Option<String> {
    let end = comments.partition_point(|comment| comment.loc.end <= before);
    let mut run: Vec<&Comment<Loc>> = Vec::new();
    let mut reaches = before.line;
    for comment in comments[..end].iter().rev() {
        if let Some(after) = after
            && (comment.loc.start < after || comment.loc.start.line == after.line)
        {
            break;
        }
        if comment.loc.end.line + 1 < reaches {
            break;
        }
        run.push(comment);
        reaches = comment.loc.start.line;
    }
    run.reverse();

    let mut out = String::new();
    // Whether the last thing written was a `//` line, which the next `//` line
    // continues rather than starting a new paragraph.
    let mut in_line_run = false;
    for comment in run {
        match comment.kind {
            CommentKind::Line => {
                if wanted == Wanted::DocBlocks {
                    in_line_run = false;
                    continue;
                }
                let text = comment.text.strip_prefix(' ').unwrap_or(&comment.text);
                if is_directive(text) {
                    continue;
                }
                if in_line_run {
                    out.push('\n');
                } else if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(text.trim_end());
                in_line_run = true;
            }
            CommentKind::Block => {
                let Some(body) = comment.text.strip_prefix('*') else {
                    continue;
                };
                let text = doc_block_text(body);
                if text.is_empty() || is_directive(&text) {
                    continue;
                }
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&text);
                in_line_run = false;
            }
        }
    }

    let text = out.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The text of a `/** */` block: each line's leading ` * ` gone, and the blank
/// lines the delimiters leave at either end dropped.
fn doc_block_text(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .map(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('*').unwrap_or(line);
            line.strip_prefix(' ').unwrap_or(line).trim_end()
        })
        .collect();
    let first = lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(lines.len());
    let last = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(first, |last| last + 1);
    lines[first..last.max(first)].join("\n")
}

/// Whether a comment is an instruction to a tool rather than prose.
fn is_directive(text: &str) -> bool {
    let text = text.trim_start();
    [
        "uf-lint-",
        "@flow",
        "@noflow",
        "$FlowFixMe",
        "$FlowExpectedError",
        "flowlint",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

/// Two paragraphs of documentation, either of which may be missing.
///
/// A paragraph already present is not repeated, which is what happens when a
/// key declared twice names the same alias both times.
fn join(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) if first.contains(&second) => Some(first),
        (Some(first), Some(second)) => Some(uf_infra::into_string(compact_str::format_compact!(
            "{first}\n\n{second}"
        ))),
        (first, second) => first.or(second),
    }
}

/// Byte offsets of line starts, for turning a parser position into a slice.
///
/// The port counts a column in UTF-8 bytes from the start of its line, which
/// `uf_fmt`'s `SourceText` measured and relies on too.
struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(at, _)| at + 1),
        );
        Self { starts }
    }

    fn offset(&self, source: &str, position: Position) -> usize {
        let line = usize::try_from(position.line).unwrap_or(0).max(1) - 1;
        let column = usize::try_from(position.column).unwrap_or(0);
        let Some(&start) = self.starts.get(line) else {
            return source.len();
        };
        let mut at = (start + column).min(source.len());
        while at > start && !source.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(source: &str) -> Schema {
        Schema::parse(source).unwrap_or_else(|error| panic!("{error}\n{source}"))
    }

    fn documented<'a>(schema: &'a Schema, path: &[Step<'_>]) -> &'a str {
        schema
            .key(path)
            .and_then(Key::documentation)
            .unwrap_or_else(|| panic!("{path:?} carries no documentation"))
    }

    /// The file this build embeds reads, and reads as the whole surface.
    #[test]
    fn the_embedded_schema_reads() {
        let schema = Schema::parse(SOURCE).expect("the embedded schema reads");

        // A floor, for the reason `tests/flow_schema.rs` gives: a reader that
        // silently found nothing would agree with everything.
        assert!(
            schema.key_paths().len() > 150,
            "{}",
            schema.key_paths().len()
        );
        for key in ["app", "build", "fmt", "test", "vite"] {
            assert!(
                schema.key(&[Step::Key(key)]).is_some(),
                "`{key}` is not a top-level key"
            );
        }
        // And `embedded` is that same reading, not the empty fallback.
        assert_eq!(Schema::embedded(), &schema);
    }

    /// What an editor shows for keys a person actually writes.
    #[test]
    fn the_embedded_schema_documents_the_keys_people_write() {
        let schema = Schema::embedded();

        assert!(
            documented(schema, &[Step::Key("test"), Step::Key("coverage")])
                .contains("What `uf test --coverage` measures")
        );
        // Documented by a `//` comment, not a doc block.
        assert!(
            documented(schema, &[Step::Key("dev"), Step::Key("allowedHosts")])
                .contains("`--host` refuses to bind a routable address")
        );
        // No comment of its own; the alias it is typed by has one.
        assert!(
            documented(schema, &[Step::Key("pm"), Step::Key("packageManager")])
                .contains("The package manager uf drives")
        );

        let quotes = schema
            .key(&[Step::Key("fmt"), Step::Key("quotes")])
            .unwrap();
        assert_eq!(quotes.type_text(), r#""single" | "double""#);
        assert_eq!(
            quotes.shape().members(),
            [
                &Shape::StringLiteral("single".into()),
                &Shape::StringLiteral("double".into())
            ]
        );
    }

    #[test]
    fn a_key_is_documented_by_the_comments_directly_above_it() {
        let schema = schema(
            "export type UniflowedConfig = {\n\
             \x20 /**\n\
             \x20  * The first paragraph.\n\
             \x20  *\n\
             \x20  * The **second**.\n\
             \x20  */\n\
             \x20 readonly block?: boolean,\n\
             \x20 // One line,\n\
             \x20 // and its continuation.\n\
             \x20 readonly line?: boolean, // trails `line`, documents nothing\n\
             \x20 readonly bare?: boolean,\n\
             \x20 // Detached by the blank line below.\n\
             \n\
             \x20 readonly detached?: boolean,\n\
             };\n",
        );

        assert_eq!(
            documented(&schema, &[Step::Key("block")]),
            "The first paragraph.\n\nThe **second**."
        );
        assert_eq!(
            documented(&schema, &[Step::Key("line")]),
            "One line,\nand its continuation."
        );
        assert_eq!(
            schema.key(&[Step::Key("bare")]).unwrap().documentation(),
            None
        );
        assert_eq!(
            schema
                .key(&[Step::Key("detached")])
                .unwrap()
                .documentation(),
            None
        );
    }

    /// Above an alias, only a doc block is documentation; `//` notes and lint
    /// directives are for whoever edits the file.
    #[test]
    fn an_alias_is_documented_by_its_doc_block_and_lends_it_to_its_keys() {
        let schema = schema(
            "/**\n\
             \x20* Which one.\n\
             \x20*/\n\
             // Why the list is this short: history nobody writing a config needs.\n\
             // uf-lint-disable some/rule\n\
             export type Choice = \"a\" | \"b\";\n\
             export type UniflowedConfig = {\n\
             \x20 // The key's own sentence.\n\
             \x20 readonly pick?: Choice,\n\
             \x20 readonly picks?: $ReadOnlyArray<Choice>,\n\
             };\n",
        );

        assert_eq!(
            documented(&schema, &[Step::Key("pick")]),
            "The key's own sentence.\n\nWhich one."
        );
        assert_eq!(documented(&schema, &[Step::Key("picks")]), "Which one.");
        assert_eq!(
            schema.key(&[Step::Key("pick")]).unwrap().shape().aliases(),
            ["Choice"]
        );
    }

    #[test]
    fn a_type_is_rendered_as_written_with_objects_abbreviated() {
        let schema = schema(
            "type Task = string | { readonly command: string };\n\
             export type UniflowedConfig = {\n\
             \x20 readonly mode?: \"server\" | \"client\",\n\
             \x20 readonly list?: $ReadOnlyArray<string>,\n\
             \x20 readonly nested?: { readonly a?: boolean },\n\
             \x20 readonly tasks?: { readonly [string]: Task },\n\
             \x20 readonly maybe?: ?(string | number),\n\
             \x20 readonly level?: 0 | 1 | true,\n\
             };\n",
        );
        let text = |key| {
            schema
                .key(&[Step::Key(key)])
                .unwrap()
                .type_text()
                .to_owned()
        };

        assert_eq!(text("mode"), r#""server" | "client""#);
        assert_eq!(text("list"), "$ReadOnlyArray<string>");
        assert_eq!(text("nested"), "{ … }");
        assert_eq!(text("tasks"), "{ [string]: Task }");
        assert_eq!(text("maybe"), "?(string | number)");
        assert_eq!(text("level"), "0 | 1 | true");
    }

    /// The paths a value can be reached by: an alias, a union with a string
    /// member, an indexer, a list.
    #[test]
    fn a_path_resolves_through_aliases_unions_indexers_and_lists() {
        let schema = schema(
            "type Task = string | { readonly command: string, readonly cache?: boolean };\n\
             type Plugin = string | { readonly name: string };\n\
             export type UniflowedConfig = {\n\
             \x20 readonly tasks?: { readonly [string]: Task },\n\
             \x20 readonly plugins?: $ReadOnlyArray<Plugin>,\n\
             };\n",
        );

        let task = schema.resolve(&[Step::Key("tasks"), Step::Key("build")]);
        let keys: Vec<&str> = task
            .iter()
            .flat_map(|shape| shape.objects())
            .flat_map(|object| object.keys().iter().map(Key::name))
            .collect();
        assert_eq!(keys, ["command", "cache"]);

        let plugin = schema.resolve(&[Step::Key("plugins"), Step::Element]);
        assert!(plugin.iter().any(|shape| {
            shape
                .objects()
                .iter()
                .any(|object| object.key("name").is_some())
        }));
        // The key behind a list element is the list's key.
        assert_eq!(
            schema
                .key(&[Step::Key("plugins"), Step::Element])
                .map(Key::name),
            Some("plugins")
        );
        // Nothing declared, nothing resolved.
        assert!(schema.resolve(&[Step::Key("nope")]).is_empty());
    }

    #[test]
    fn a_self_referential_alias_reads_as_a_shape_instead_of_recursing() {
        let schema = schema(
            "type Json = string | { readonly [string]: Json };\n\
             export type UniflowedConfig = { readonly data?: Json };\n",
        );

        // One expansion, and the reference inside it reads as a shape there is
        // nothing to say about — not as a stack overflow.
        assert_eq!(
            schema.resolve(&[Step::Key("data"), Step::Key("x")]),
            [&Shape::Other]
        );
        assert!(
            schema
                .resolve(&[Step::Key("data"), Step::Key("x"), Step::Key("y")])
                .is_empty()
        );
    }

    #[test]
    fn a_key_declared_twice_is_one_key_with_both_paragraphs() {
        let schema = schema(
            "export type UniflowedConfig = {\n\
             \x20 // The first.\n\
             \x20 readonly enabled?: boolean,\n\
             \x20 // The second.\n\
             \x20 readonly enabled?: boolean,\n\
             };\n",
        );

        let Shape::Object(root) = schema.root() else {
            panic!("the root is an object");
        };
        assert_eq!(root.keys().len(), 1);
        assert_eq!(
            documented(&schema, &[Step::Key("enabled")]),
            "The first.\n\nThe second."
        );
    }

    #[test]
    fn a_schema_that_is_not_one_says_why() {
        assert_eq!(
            Schema::parse("export type Other = {};\n"),
            Err(SchemaError::MissingRoot)
        );
        assert!(matches!(
            Schema::parse("export type UniflowedConfig = {"),
            Err(SchemaError::Syntax { .. })
        ));
    }
}
