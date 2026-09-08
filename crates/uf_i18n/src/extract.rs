//! The walk: every `message(…)` a project declares, and where.
//!
//! # What is read
//!
//! A message is a `message(<source>, <parameters>)` call whose `message` came
//! from `@uniflowed/i18n`, and whose key is the name it is declared under:
//!
//! ```js
//! import { message, number, string } from "@uniflowed/i18n";
//!
//! const UNREAD = `…`;
//! const messages = {
//!   greeting: message("Hello, {$name}!", { name: string }),  // key: greeting
//!   unread: message(UNREAD, { count: number }),              // key: unread
//! };
//! const farewell = message("Bye.", {});                      // key: farewell
//! ```
//!
//! The import is checked rather than assumed. `message` is an ordinary word —
//! a repository this size has several functions called it — and a walk that
//! matched on the name alone would put a WebSocket frame in a translation
//! file. Aliases (`import { message as msg }`) and namespace imports
//! (`import * as i18n`, `i18n.message(…)`) are both followed, because both
//! are the same import written differently and a tool that refused one would
//! be refusing valid code.
//!
//! # What is refused rather than guessed
//!
//! Everything the walk cannot read *with certainty* becomes an
//! [`ExtractProblem`] naming the file and the line, and the command fails.
//! Never a message quietly absent from the file a vendor is sent: that failure
//! is invisible in the extraction, invisible in review, and visible only to a
//! user reading English on a Japanese page.
//!
//! So: a source argument that is neither a literal nor a module-level constant
//! holding one, a parameter whose value is not one of the four kinds, a
//! `message(…)` declared under no name at all, and two messages under one key.
//!
//! Resolving `message(UNREAD, …)` through a module-level `const UNREAD = "…"`
//! is the one indirection followed, and it is followed because it is the one
//! the package's own documentation writes: a `.match` message is several lines
//! long and does not belong inline. Module-level only, so there is no scope in
//! which the name could mean something else.
//!
//! # Cost
//!
//! This runs over every source file in a repository, so the first thing it
//! does to each is look for the bytes `@uniflowed/i18n` and move on. A project
//! with messages in four modules parses four modules; the rest cost one
//! substring search over source discovery already read. The walk itself is the
//! port's own [`AstVisitor`], on the stack the parser documents for its
//! ceilings, and each file is visited twice — once for the imports and the
//! module-level constants, once for the messages — because that is what makes
//! `const UNREAD` below `const messages` read the same as above it.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8Path;
use serde::Serialize;
use uf_config::UniflowedConfig;
use uf_flow::ast::{expression, statement};
use uf_flow::ast_visitor::{self, AstVisitor};
use uf_flow::{Loc, ast};
use uf_project::{SourceKind, scan_source_files};

use crate::{CATALOGUE_FORMAT, I18nError, MessageCatalogue, MessageEntry, ParamKind, digest_of};

/// The bytes a file must contain before it is worth parsing.
///
/// The package's own specifier. A module that declares a message imports it,
/// including through a subpath (`@uniflowed/i18n/catalogue`), so this prefix
/// catches both.
const SPECIFIER: &str = "@uniflowed/i18n";

/// What `uf i18n extract` was asked for.
#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    /// The source locale, when the caller named one with `--locale`.
    ///
    /// Otherwise it is read from the project's `defineCatalogue` calls; see
    /// [`I18nError::NoSourceLocale`] and
    /// [`I18nError::AmbiguousSourceLocale`] for the two ways that fails.
    pub locale: Option<String>,
}

/// Why a `message(…)` in the source is not a message in the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    /// The file mentions the package and does not parse.
    Unparsable,
    /// A `message(…)` call declared under no name this can read.
    NoKey,
    /// A source argument that is not a literal, and not a name of one.
    SourceNotALiteral,
    /// A parameter object this cannot read as `{ name: kind }`.
    ParametersNotReadable,
    /// Two messages declared under one key.
    DuplicateKey,
}

impl ProblemKind {
    /// A short label, for a report that groups by kind.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Unparsable => "does not parse",
            Self::NoKey => "has no key",
            Self::SourceNotALiteral => "source is not a literal",
            Self::ParametersNotReadable => "parameters are not readable",
            Self::DuplicateKey => "duplicate key",
        }
    }
}

/// One message the walk found and could not put in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProblem {
    /// Where it is, as `path:line` relative to the project root.
    pub at: String,
    /// Which of the five it is.
    pub kind: ProblemKind,
    /// The sentence a reader needs, naming the fix where there is one.
    pub detail: String,
}

/// What the walk found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractReport {
    /// How many source files discovery offered.
    pub files_seen: usize,
    /// How many of them mentioned the package and were parsed.
    pub files_parsed: usize,
    /// How many declared at least one message.
    pub modules: usize,
    /// The catalogue, ready to write.
    pub catalogue: MessageCatalogue,
    /// Messages found and not extracted; see [`ExtractProblem`].
    pub problems: Vec<ExtractProblem>,
    /// Files discovery could not read at all, as `path: reason`.
    ///
    /// Reported rather than fatal, the way `uf doc` reports them: a file that
    /// is not UTF-8 has no syntax to be wrong about, and one stray byte used
    /// to stop a whole walk.
    pub unreadable: Vec<String>,
}

impl ExtractReport {
    /// Whether anything here should fail the command.
    #[must_use]
    pub fn has_problems(&self) -> bool {
        !self.problems.is_empty() || !self.unreadable.is_empty()
    }
}

/// Every message the project declares, as a catalogue in the source locale.
///
/// # Errors
///
/// Discovery and parser failures, and the two ways the source locale is not
/// knowable: no `defineCatalogue` named one and no `--locale` was given, or
/// they disagreed.
pub fn extract(
    root: &Utf8Path,
    config: &UniflowedConfig,
    options: &ExtractOptions,
) -> Result<ExtractReport, I18nError> {
    let scan = scan_source_files(root, config)?;

    // On a thread with the stack the parser documents for its ceilings, the
    // way `uf_doc` and `uf_fmt` do: both the parse and the visitor recurse
    // once per level of nesting, and a main thread's 8 MiB is not enough for a
    // source at `MAX_CHAIN_DEPTH`. Freeing the tree recurses as well and needs
    // nothing from here — `uf_flow::Parsed` takes a deep one to its own thread.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uf-i18n".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || walk(scan, options))
            .map_err(|_| I18nError::Worker {
                reason: "could not be started",
            })?
            .join()
            .unwrap_or(Err(I18nError::Worker { reason: "panicked" }))
    })
}

/// One key's declaration, before it is known whether it is the only one.
struct Found {
    key: String,
    source: MessageSource,
    parameters: BTreeMap<String, ParamKind>,
    at: String,
}

/// What was written where a message's MF2 source goes.
enum MessageSource {
    /// A literal, read.
    Text(String),
    /// An identifier, to be resolved against the module's own constants.
    Named(String),
}

/// What one file declared, before the project is folded together.
///
/// The unit of testing as well as the unit of work: [`read_module`] is a pure
/// function of one file's text, so every shape a declaration can take is a
/// test over a string rather than a directory.
pub(crate) struct FileMessages {
    /// Each key with its entry, in source order.
    pub(crate) entries: Vec<(String, MessageEntry)>,
    /// What could not be read, in source order.
    pub(crate) problems: Vec<ExtractProblem>,
    /// Locales named by a `defineCatalogue` in this file.
    pub(crate) locales: BTreeSet<String>,
}

/// Read one module: its messages, its problems and the locale it declares.
///
/// `path` appears in every location this produces, so it is the project-
/// relative path rather than an absolute one.
///
/// # Errors
///
/// Only the parser's own refusals — a source over [`uf_flow::MAX_PARSE_BYTES`]
/// or past its nesting ceiling. A *syntax* error is not one of them: it comes
/// back as a [`ProblemKind::Unparsable`] problem, because one unparsable
/// module must not stop a repository-wide walk.
pub(crate) fn read_module(source: &str, path: &str) -> Result<FileMessages, I18nError> {
    let mut found = FileMessages {
        entries: Vec::new(),
        problems: Vec::new(),
        locales: BTreeSet::new(),
    };

    let parsed = uf_flow::parse(source)?;
    if !parsed.is_ok() {
        let said = parsed.diagnostics.first().map_or_else(
            || "a syntax error".to_owned(),
            |first| first.message.clone(),
        );
        found.problems.push(ExtractProblem {
            at: path.to_owned(),
            kind: ProblemKind::Unparsable,
            detail: format!(
                "this file imports {SPECIFIER} and does not parse, so any message in it would \
                 be missing from the catalogue without this line: {said}"
            ),
        });
        return Ok(found);
    }

    let Some(module) = Module::read(&parsed.program, path) else {
        // It named the specifier in a comment or a string, not an import.
        return Ok(found);
    };

    let mut visitor = Messages::new(&module);
    // The visitor's error type is `()` and no hook here returns `Err`, so the
    // walk cannot fail. The result is bound rather than discarded so that a
    // hook which one day can fail is a compile error at this line.
    let walked: Result<(), ()> = visitor.program(&parsed.program);
    debug_assert!(
        walked.is_ok(),
        "the message walk reports through its own lists"
    );

    found.locales = std::mem::take(&mut visitor.locales);
    found.problems = visitor.take_problems();

    for declared in std::mem::take(&mut visitor.found) {
        let text = match declared.source {
            MessageSource::Text(text) => text,
            MessageSource::Named(name) => match module.constants.get(&name) {
                Some(text) => text.clone(),
                None => {
                    found.problems.push(ExtractProblem {
                        at: declared.at,
                        kind: ProblemKind::SourceNotALiteral,
                        detail: format!(
                            "the message {:?} is written from `{name}`, which is not a \
                             module-level string constant in this file; extraction reads a \
                             string literal, a template literal with no substitutions, or a \
                             module-level `const` holding one",
                            declared.key
                        ),
                    });
                    continue;
                }
            },
        };
        found.entries.push((
            declared.key,
            MessageEntry {
                digest: digest_of(&text, &declared.parameters),
                translation: text.clone(),
                source: text,
                parameters: declared.parameters,
                declared_at: declared.at,
            },
        ));
    }

    found.problems.sort_by(|left, right| left.at.cmp(&right.at));
    Ok(found)
}

fn walk(
    scan: uf_project::SourceScan,
    options: &ExtractOptions,
) -> Result<ExtractReport, I18nError> {
    let files_seen = scan.files.len();
    let mut report = ExtractReport {
        files_seen,
        files_parsed: 0,
        modules: 0,
        catalogue: MessageCatalogue {
            format: CATALOGUE_FORMAT.to_owned(),
            source_locale: String::new(),
            locale: String::new(),
            messages: BTreeMap::new(),
        },
        problems: Vec::new(),
        unreadable: scan
            .unreadable
            .iter()
            .map(|file| format!("{}: {}", file.relative_path, file.reason))
            .collect(),
    };

    // Where each key was declared, so a second declaration can name the first
    // rather than saying only that there was one.
    let mut declared_at: BTreeMap<String, String> = BTreeMap::new();
    let mut locales: BTreeSet<String> = BTreeSet::new();

    for file in scan
        .files
        .into_iter()
        .filter(|file| file.kind == SourceKind::JavaScript)
    {
        // The whole cost of this walk on a repository with no messages in it.
        // Source discovery has already read the bytes; nothing below happens
        // for a file that does not name the package.
        if !file.source.contains(SPECIFIER) {
            continue;
        }
        report.files_parsed += 1;

        let found = read_module(&file.source, &file.relative_path)?;
        locales.extend(found.locales);
        report.problems.extend(found.problems);

        let mut declared_here = 0_usize;
        for (key, entry) in found.entries {
            if let Some(first) = declared_at.get(&key) {
                report.problems.push(ExtractProblem {
                    at: entry.declared_at,
                    kind: ProblemKind::DuplicateKey,
                    detail: format!(
                        "the key {key:?} is already declared at {first}; a catalogue file has \
                         one entry per key, and two messages under one name would send a \
                         translator one of them at random"
                    ),
                });
                continue;
            }
            declared_at.insert(key.clone(), entry.declared_at.clone());
            declared_here += 1;
            report.catalogue.messages.insert(key, entry);
        }
        if declared_here > 0 {
            report.modules += 1;
        }
    }

    let locale = match &options.locale {
        Some(named) => named.clone(),
        None => {
            let mut found: Vec<String> = locales.into_iter().collect();
            match found.len() {
                0 => return Err(I18nError::NoSourceLocale),
                1 => found.remove(0),
                _ => return Err(I18nError::AmbiguousSourceLocale { found }),
            }
        }
    };
    report.catalogue.source_locale = locale.clone();
    report.catalogue.locale = locale;

    Ok(report)
}

/// What one module imported from `@uniflowed/i18n`, and what it holds.
struct Module {
    /// Local names bound to `message`.
    message: BTreeSet<String>,
    /// Local names bound to `defineCatalogue`.
    define_catalogue: BTreeSet<String>,
    /// Local name to parameter kind, for the four kind exports.
    kinds: BTreeMap<String, ParamKind>,
    /// Local names of `import * as …` from the package.
    namespaces: BTreeSet<String>,
    /// Module-level `const NAME = "…"`, for a message written from one.
    constants: BTreeMap<String, String>,
    /// The file, for the `path:line` a problem carries.
    path: String,
}

impl Module {
    /// Read a module's imports and module-level string constants.
    ///
    /// [`None`] when it imports nothing from the package after all — the
    /// substring that got it parsed was in a comment or a string. Cheaper to
    /// answer here than to walk the tree and find nothing.
    fn read(program: &ast::Program<Loc, Loc>, path: &str) -> Option<Self> {
        let mut module = Self {
            message: BTreeSet::new(),
            define_catalogue: BTreeSet::new(),
            kinds: BTreeMap::new(),
            namespaces: BTreeSet::new(),
            constants: BTreeMap::new(),
            path: path.to_owned(),
        };
        let mut imported = false;

        for node in program.statements.iter() {
            match &**node {
                statement::StatementInner::ImportDeclaration { inner, .. } => {
                    imported |= module.read_import(inner);
                }
                statement::StatementInner::VariableDeclaration { inner, .. } => {
                    module.read_constants(inner);
                }
                statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                    if let Some(declaration) = &inner.declaration
                        && let statement::StatementInner::VariableDeclaration {
                            inner: variables,
                            ..
                        } = &**declaration
                    {
                        module.read_constants(variables);
                    }
                }
                _ => {}
            }
        }

        imported.then_some(module)
    }

    /// Record what one `import … from "@uniflowed/i18n"` binds.
    ///
    /// Returns whether this import was from the package at all. `import type`
    /// binds no value, so a module that only imports the types declares no
    /// message and is not one of ours.
    fn read_import(&mut self, declaration: &statement::ImportDeclaration<Loc, Loc>) -> bool {
        if declaration.import_kind != statement::ImportKind::ImportValue {
            return false;
        }
        let specifier = declaration.source.1.value.as_str();
        let ours = specifier == SPECIFIER
            || specifier
                .strip_prefix(SPECIFIER)
                .is_some_and(|rest| rest.starts_with('/'));
        if !ours {
            return false;
        }

        match &declaration.specifiers {
            Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(names)) => {
                for named in names {
                    if named.kind == Some(statement::ImportKind::ImportType)
                        || named.kind == Some(statement::ImportKind::ImportTypeof)
                    {
                        continue;
                    }
                    let local = named
                        .local
                        .as_ref()
                        .unwrap_or(&named.remote)
                        .name
                        .as_str()
                        .to_owned();
                    match named.remote.name.as_str() {
                        "message" => {
                            self.message.insert(local);
                        }
                        "defineCatalogue" => {
                            self.define_catalogue.insert(local);
                        }
                        other => {
                            if let Some(kind) = ParamKind::from_name(other) {
                                self.kinds.insert(local, kind);
                            }
                        }
                    }
                }
            }
            Some(statement::import_declaration::Specifier::ImportNamespaceSpecifier((_, name))) => {
                self.namespaces.insert(name.name.as_str().to_owned());
            }
            None => {}
        }
        true
    }

    /// Record every `const NAME = <string literal>` in one declaration.
    fn read_constants(&mut self, declaration: &statement::VariableDeclaration<Loc, Loc>) {
        if declaration.kind != ast::VariableKind::Const {
            return;
        }
        for declarator in declaration.declarations.iter() {
            let ast::pattern::Pattern::Identifier { inner: id, .. } = &declarator.id else {
                continue;
            };
            let Some(init) = &declarator.init else {
                continue;
            };
            if let Some(text) = literal_text(init) {
                self.constants
                    .insert(id.name.name.as_str().to_owned(), text);
            }
        }
    }

    /// Whether `callee` is the package's `name`, however it was imported.
    fn calls(&self, callee: &expression::Expression<Loc, Loc>, name: &str) -> bool {
        let locals = match name {
            "message" => &self.message,
            "defineCatalogue" => &self.define_catalogue,
            // The two above are the only calls this looks for. A third would
            // want a table rather than another arm.
            _ => return false,
        };
        match &**callee {
            expression::ExpressionInner::Identifier { inner, .. } => {
                locals.contains(inner.name.as_str())
            }
            expression::ExpressionInner::Member { inner, .. } => {
                let expression::ExpressionInner::Identifier { inner: object, .. } = &*inner.object
                else {
                    return false;
                };
                if !self.namespaces.contains(object.name.as_str()) {
                    return false;
                }
                match &inner.property {
                    expression::member::Property::PropertyIdentifier(property) => {
                        property.name.as_str() == name
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The parameter kind `expression` names, however it was imported.
    fn kind_of(&self, value: &expression::Expression<Loc, Loc>) -> Option<ParamKind> {
        match &**value {
            expression::ExpressionInner::Identifier { inner, .. } => {
                self.kinds.get(inner.name.as_str()).copied()
            }
            expression::ExpressionInner::Member { inner, .. } => {
                let expression::ExpressionInner::Identifier { inner: object, .. } = &*inner.object
                else {
                    return None;
                };
                if !self.namespaces.contains(object.name.as_str()) {
                    return None;
                }
                match &inner.property {
                    expression::member::Property::PropertyIdentifier(property) => {
                        ParamKind::from_name(property.name.as_str())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `path:line`, the way every problem and every entry spells a location.
    fn at(&self, loc: &Loc) -> String {
        format!("{}:{}", self.path, loc.start.line)
    }
}

/// The text of a string literal, or of a template literal with no holes.
///
/// A template literal is accepted because it is how the package's own
/// documentation writes a `.match` message: several lines, and no business
/// being on one. One with a substitution in it is not accepted and is not
/// silently truncated — its text is decided while the program runs, so there
/// is no message to put in a file.
fn literal_text(value: &expression::Expression<Loc, Loc>) -> Option<String> {
    match &**value {
        expression::ExpressionInner::StringLiteral { inner, .. } => {
            Some(inner.value.as_str().to_owned())
        }
        expression::ExpressionInner::TemplateLiteral { inner, .. } => {
            if !inner.expressions.is_empty() {
                return None;
            }
            let mut text = String::new();
            for quasi in inner.quasis.iter() {
                text.push_str(quasi.value.cooked.as_str());
            }
            Some(text)
        }
        _ => None,
    }
}

/// The walk itself.
struct Messages<'a> {
    module: &'a Module,
    /// Messages found under a key.
    found: Vec<Found>,
    /// Where each of those calls started, so an unclaimed one can be named.
    claimed: BTreeSet<(i32, i32)>,
    /// Where every `message(…)` call started, claimed or not.
    seen: Vec<(i32, i32, String)>,
    /// What `defineCatalogue` was given as a locale, when it was a literal.
    locales: BTreeSet<String>,
    problems: Vec<ExtractProblem>,
}

impl<'a> Messages<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            found: Vec::new(),
            claimed: BTreeSet::new(),
            seen: Vec::new(),
            locales: BTreeSet::new(),
            problems: Vec::new(),
        }
    }

    /// Take one `message(source, parameters)` under `key`.
    ///
    /// Records the call as claimed whatever happens next, so that a declaration
    /// this could not read is reported once — as the specific thing that was
    /// wrong with it — rather than twice, the second time as "no key".
    fn declare(&mut self, key: String, loc: &Loc, call: &expression::Call<Loc, Loc>) {
        self.claimed.insert((loc.start.line, loc.start.column));
        let at = self.module.at(loc);

        let mut arguments = call.arguments.arguments.iter();
        let (Some(expression::ExpressionOrSpread::Expression(source)), Some(second)) =
            (arguments.next(), arguments.next())
        else {
            self.problems.push(ExtractProblem {
                at,
                kind: ProblemKind::SourceNotALiteral,
                detail: format!(
                    "the message {key:?} is not called as `message(source, parameters)`, so \
                     there is nothing here to put in a catalogue"
                ),
            });
            return;
        };

        let source = match literal_text(source) {
            Some(text) => MessageSource::Text(text),
            None => match &**source {
                expression::ExpressionInner::Identifier { inner, .. } => {
                    MessageSource::Named(inner.name.as_str().to_owned())
                }
                _ => {
                    self.problems.push(ExtractProblem {
                        at,
                        kind: ProblemKind::SourceNotALiteral,
                        detail: format!(
                            "the message {key:?} is written from an expression rather than a \
                             literal, so its text is not decided until the program runs and \
                             there is nothing to send a translator"
                        ),
                    });
                    return;
                }
            },
        };

        let expression::ExpressionOrSpread::Expression(parameters) = second else {
            self.problems.push(ExtractProblem {
                at,
                kind: ProblemKind::ParametersNotReadable,
                detail: format!(
                    "the parameters of {key:?} are spread from another value, so what the \
                     message takes is not decided until the program runs"
                ),
            });
            return;
        };
        let Some(parameters) = self.read_parameters(&key, &at, parameters) else {
            return;
        };

        self.found.push(Found {
            key,
            source,
            parameters,
            at,
        });
    }

    /// The `{ name: string }` beside a message, as names and kinds.
    fn read_parameters(
        &mut self,
        key: &str,
        at: &str,
        value: &expression::Expression<Loc, Loc>,
    ) -> Option<BTreeMap<String, ParamKind>> {
        let expression::ExpressionInner::Object { inner, .. } = &**value else {
            self.problems.push(ExtractProblem {
                at: at.to_owned(),
                kind: ProblemKind::ParametersNotReadable,
                detail: format!(
                    "the parameters of {key:?} are not an object literal, so what the message \
                     takes cannot be read from the source"
                ),
            });
            return None;
        };

        let mut parameters = BTreeMap::new();
        for property in inner.properties.iter() {
            let expression::object::Property::NormalProperty(
                expression::object::NormalProperty::Init {
                    key: name, value, ..
                },
            ) = property
            else {
                self.problems.push(ExtractProblem {
                    at: at.to_owned(),
                    kind: ProblemKind::ParametersNotReadable,
                    detail: format!(
                        "the parameters of {key:?} hold something other than `name: kind` — a \
                         spread, a method or an accessor — so what the message takes cannot be \
                         read from the source"
                    ),
                });
                return None;
            };

            let name = match name {
                expression::object::Key::Identifier(identifier) => {
                    identifier.name.as_str().to_owned()
                }
                expression::object::Key::StringLiteral((_, literal)) => {
                    literal.value.as_str().to_owned()
                }
                _ => {
                    self.problems.push(ExtractProblem {
                        at: at.to_owned(),
                        kind: ProblemKind::ParametersNotReadable,
                        detail: format!(
                            "a parameter of {key:?} is named by an expression rather than \
                             written down, so the message's arguments are not knowable here"
                        ),
                    });
                    return None;
                }
            };

            let Some(kind) = self.module.kind_of(value) else {
                self.problems.push(ExtractProblem {
                    at: at.to_owned(),
                    kind: ProblemKind::ParametersNotReadable,
                    detail: format!(
                        "the parameter `{name}` of {key:?} is not one of the four kinds \
                         `@uniflowed/i18n` exports (string, number, boolean, date)"
                    ),
                });
                return None;
            };
            parameters.insert(name, kind);
        }
        Some(parameters)
    }

    /// Everything this walk found wrong, in source order.
    ///
    /// The unkeyed calls are computed here rather than as they are seen: a
    /// call is claimed by whichever of the two declaration shapes encloses it,
    /// and that is not known until the whole tree has been walked.
    fn take_problems(&mut self) -> Vec<ExtractProblem> {
        let mut problems = self.unkeyed().collect::<Vec<_>>();
        problems.append(&mut self.problems);
        problems.sort_by(|left, right| left.at.cmp(&right.at));
        problems
    }

    /// Every `message(…)` the walk saw and no key claimed.
    fn unkeyed(&self) -> impl Iterator<Item = ExtractProblem> + '_ {
        self.seen
            .iter()
            .filter(|(line, column, _)| !self.claimed.contains(&(*line, *column)))
            .map(|(_, _, at)| ExtractProblem {
                at: at.clone(),
                kind: ProblemKind::NoKey,
                detail: "a message declared under no name: extraction reads `key: message(…)` \
                         in an object or `const key = message(…)`, because the key is what a \
                         translation file is addressed by"
                    .to_owned(),
            })
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Messages<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn object(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast expression::Object<Loc, Loc>,
    ) -> Result<(), ()> {
        for property in expr.properties.iter() {
            let expression::object::Property::NormalProperty(
                expression::object::NormalProperty::Init { key, value, .. },
            ) = property
            else {
                continue;
            };
            let expression::ExpressionInner::Call {
                loc: call_loc,
                inner: call,
            } = &**value
            else {
                continue;
            };
            if !self.module.calls(&call.callee, "message") {
                continue;
            }
            let name = match key {
                expression::object::Key::Identifier(identifier) => {
                    Some(identifier.name.as_str().to_owned())
                }
                expression::object::Key::StringLiteral((_, literal)) => {
                    Some(literal.value.as_str().to_owned())
                }
                _ => None,
            };
            if let Some(name) = name {
                self.declare(name, call_loc, call);
            }
        }
        ast_visitor::object_default(self, loc, expr)
    }

    fn variable_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast statement::VariableDeclaration<Loc, Loc>,
    ) -> Result<(), ()> {
        for declarator in decl.declarations.iter() {
            let ast::pattern::Pattern::Identifier { inner: id, .. } = &declarator.id else {
                continue;
            };
            let Some(init) = &declarator.init else {
                continue;
            };
            let expression::ExpressionInner::Call {
                loc: call_loc,
                inner: call,
            } = &**init
            else {
                continue;
            };
            if self.module.calls(&call.callee, "message") {
                self.declare(id.name.name.as_str().to_owned(), call_loc, call);
            }
        }
        ast_visitor::variable_declaration_default(self, loc, decl)
    }

    fn call(&mut self, loc: &'ast Loc, expr: &'ast expression::Call<Loc, Loc>) -> Result<(), ()> {
        if self.module.calls(&expr.callee, "message") {
            self.seen
                .push((loc.start.line, loc.start.column, self.module.at(loc)));
        } else if self.module.calls(&expr.callee, "defineCatalogue")
            && let Some(expression::ExpressionOrSpread::Expression(first)) =
                expr.arguments.arguments.first()
            && let Some(locale) = literal_text(first)
        {
            // A locale given as a variable is simply not read: `--locale` is
            // the answer, and inventing one would be worse than asking.
            self.locales.insert(locale);
        }
        ast_visitor::call_default(self, loc, expr)
    }
}
