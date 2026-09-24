//! `DialogRoot` → `Dialog.Root`: the source codemod for `@uniflowed/ui`'s
//! namespace-only surface (ubugeeei-prod/uf#1453).
//!
//! # What changed, and what this rewrites
//!
//! `@uniflowed/ui` used to export every part twice: as a member of a namespace
//! object (`Dialog.Root`) and under a prefixed name of its own (`DialogRoot`).
//! The prefixed names are gone; each namespace is now the module itself,
//! re-exported with `export * as Dialog from "./dialog.js"`. A project that
//! wrote the prefixed names stops compiling, so this rewrites them:
//!
//! * `import { DialogRoot, DialogTrigger as Open } from "@uniflowed/ui"` becomes
//!   `import { Dialog } from "@uniflowed/ui"`, and every reference — JSX tags,
//!   expressions, `typeof DialogRoot`, `renders DialogRoot` — becomes
//!   `Dialog.Root` and `Dialog.Trigger`. An alias is dissolved: `Open` becomes
//!   `Dialog.Trigger` too, because keeping a second name for the part is the
//!   thing the change removed.
//! * `import * as ui from "@uniflowed/ui"` stays, and `ui.DialogRoot` becomes
//!   `ui.Dialog.Root`. This is the shape every component `uf ui add` wrote
//!   before the change imports its parts in.
//! * A file that already binds the namespace's name for something of its own —
//!   a styled wrapper called `Dialog`, most often — gets the import under
//!   `UiDialog` instead, and its references use that.
//!
//! # What it leaves alone, and says so
//!
//! A rewrite this cannot make without guessing is reported in `unmapped` and
//! the file is left exactly as it was — never half-migrated:
//!
//! * a prefixed name in an object shorthand or a local `export { DialogRoot }`,
//!   where the name is also a key or an export of the file's own and renaming
//!   it would change what the file exports;
//! * `export { DialogRoot } from "@uniflowed/ui"`, a re-export that is part of
//!   the file's own surface;
//! * a string-named specifier, and a file whose tokens the scanner could not
//!   read;
//! * a result that no longer parses, which is the backstop for everything the
//!   rules above do not foresee.
//!
//! It is a token rewrite checked by the official Flow parser, not a scope
//! analysis: an inner binding that shadows an imported part's name (a
//! parameter called `DialogRoot`) would be renamed with the import. That is
//! the one gap, and a component name shadowed inside the file that imports it
//! is rare enough to state rather than to build a resolver for.
//!
//! # Why the table is here rather than read from the package
//!
//! [`PARTS`] is the surface the change *removed*. The installed
//! `@uniflowed/ui` no longer says which prefixed names existed — that is the
//! point of the change — so the codemod has to carry the previous release's
//! surface itself, frozen, the way a migration always describes the version
//! it migrates from.

use anyhow::Result;
use camino::Utf8Path;
use std::collections::{BTreeMap, BTreeSet};
use uf_flow::scan::{Token, TokenKind};

use super::Plan;

/// The catalog ID `tools/codemods/catalog.json` registers this migration under.
pub(super) const UI_NAMESPACES: &str = "ui-namespaces-1453";

/// The package whose imports are rewritten.
const PACKAGE: &str = "@uniflowed/ui";

/// Every namespace `@uniflowed/ui` exported before ubugeeei-prod/uf#1453, and
/// the parts each one had under a prefixed name of its own: `("Dialog",
/// ["Root", …])` is `DialogRoot` → `Dialog.Root`. Parts another namespace
/// shared (`ContextMenu.Body` is `Menu`'s `MenuBody`) are listed under the
/// namespace whose prefix they carried.
const PARTS: &[(&str, &[&str])] = &[
    (
        "Accordion",
        &["Root", "Item", "Header", "Trigger", "Content"],
    ),
    ("Alert", &["Root", "Title", "Description"]),
    (
        "AlertDialog",
        &[
            "Root",
            "Trigger",
            "Overlay",
            "Body",
            "Header",
            "Footer",
            "Title",
            "Description",
            "Action",
            "Cancel",
        ],
    ),
    ("Avatar", &["Root", "Image", "Fallback"]),
    (
        "Breadcrumb",
        &["Root", "List", "Item", "Link", "Page", "Separator"],
    ),
    ("Calendar", &["Root", "Previous", "Next", "Month", "Day"]),
    (
        "Carousel",
        &["Root", "Content", "Item", "Pause", "Previous", "Next"],
    ),
    ("Collapsible", &["Root", "Trigger", "Content"]),
    (
        "ColorPicker",
        &["Root", "Input", "Field", "Channel", "Swatch"],
    ),
    (
        "Combobox",
        &[
            "Root",
            "Label",
            "Input",
            "List",
            "Option",
            "Group",
            "GroupLabel",
            "Empty",
            "Status",
        ],
    ),
    ("ContextMenu", &["Root", "Trigger"]),
    ("DatePicker", &["Root", "Input", "Trigger", "Calendar"]),
    (
        "DateRangePicker",
        &["Root", "StartField", "EndField", "Trigger", "Calendar"],
    ),
    (
        "Dialog",
        &[
            "Root",
            "Trigger",
            "Overlay",
            "Body",
            "Header",
            "Footer",
            "Title",
            "Description",
            "Close",
        ],
    ),
    (
        "Drawer",
        &[
            "Root",
            "Trigger",
            "Overlay",
            "Body",
            "Handle",
            "Header",
            "Footer",
            "Title",
            "Description",
            "Close",
        ],
    ),
    (
        "Field",
        &["Root", "Label", "Control", "Description", "Status", "Error"],
    ),
    ("HoverCard", &["Root", "Trigger", "Body"]),
    ("InputOtp", &["Root", "Group", "Slot", "Separator"]),
    (
        "Menu",
        &[
            "Root",
            "Trigger",
            "Body",
            "Item",
            "CheckboxItem",
            "RadioGroup",
            "RadioItem",
            "Separator",
            "Group",
            "Label",
            "Sub",
            "SubTrigger",
        ],
    ),
    ("Menubar", &["Root", "Menu", "Trigger"]),
    (
        "NavigationMenu",
        &["Root", "List", "Item", "Trigger", "Body", "Link"],
    ),
    ("NumberField", &["Root", "Input", "Increment", "Decrement"]),
    (
        "Pagination",
        &["Root", "Content", "Item", "Previous", "Next"],
    ),
    ("Popover", &["Root", "Trigger", "Body"]),
    ("RadioGroup", &["Root", "Item", "Indicator"]),
    ("RangeCalendar", &["Root"]),
    ("Resizable", &["PanelGroup", "Panel", "Handle"]),
    ("ScrollArea", &["Root", "Viewport", "Scrollbar"]),
    (
        "Select",
        &[
            "Root",
            "Label",
            "Trigger",
            "Value",
            "List",
            "Option",
            "Group",
            "GroupLabel",
            "Separator",
        ],
    ),
    (
        "Sheet",
        &[
            "Root",
            "Trigger",
            "Overlay",
            "Body",
            "Header",
            "Footer",
            "Title",
            "Description",
            "Close",
        ],
    ),
    (
        "Sidebar",
        &["Root", "Trigger", "Header", "Body", "Footer", "Item"],
    ),
    ("Skeleton", &["Root", "Box"]),
    ("Slider", &["Root", "Track", "Range", "Thumb"]),
    (
        "Table",
        &[
            "Root",
            "Caption",
            "Header",
            "Body",
            "Row",
            "Head",
            "RowHeader",
            "Cell",
            "SelectAll",
            "RowSelect",
        ],
    ),
    ("Tabs", &["Root", "List", "Tab", "Panel"]),
    (
        "Toast",
        &["Region", "Root", "Title", "Description", "Action", "Close"],
    ),
    ("ToggleGroup", &["Root", "Item"]),
    ("Tooltip", &["Provider", "Root", "Trigger", "Body"]),
];

/// The namespace and the member a removed prefixed name is now, or `None` for
/// any other name.
///
/// Exact: `DialogRoot` is `("Dialog", "Root")` and `DialogRooot` is nothing.
/// Where two namespaces are prefixes of one name the longer wins, so
/// `MenubarMenu` is `Menubar.Menu`, not a `Menu` part called `barMenu`.
pub(super) fn part(name: &str) -> Option<(&'static str, &'static str)> {
    PARTS
        .iter()
        .filter_map(|(namespace, parts)| {
            let rest = name.strip_prefix(namespace)?;
            let part = parts.iter().find(|part| **part == rest)?;
            Some((*namespace, *part))
        })
        .max_by_key(|(namespace, _)| namespace.len())
}

/// Whether `name` is one of `@uniflowed/ui`'s namespaces: `Dialog`, `Tabs`.
pub(super) fn is_namespace(name: &str) -> bool {
    PARTS.iter().any(|(namespace, _)| *namespace == name)
}

/// Where an import comes from, as far as this migration is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Origin {
    /// `@uniflowed/ui` itself: `DialogRoot` becomes `Dialog.Root`, and the
    /// namespace is imported by name from the package, `import { Dialog }`.
    Package,
    /// A file `uf ui add` wrote, by component: `DialogTrigger` from
    /// `./components/ui/dialog.js` becomes `Dialog.Trigger`, and the
    /// namespace is the module, `import * as Dialog from "./components/ui/dialog.js"`.
    /// See [`super::ui_copies`].
    Copy(&'static str),
}

impl Origin {
    /// The namespace and the member a removed name from this origin is now.
    fn lookup(self, name: &str) -> Option<(String, &'static str)> {
        match self {
            Self::Package => part(name).map(|(namespace, member)| (namespace.to_owned(), member)),
            Self::Copy(component) => super::ui_copies::old_part(component, name)
                .map(|member| (uf_ui::registry::namespace_name(component), member)),
        }
    }

    /// What a local namespace binding is prefixed with when the file already
    /// uses the namespace's own name: `UiDialog` for the package's, and
    /// `StyledDialog` for a copy's, so the two stay apart in a file that
    /// imports both.
    fn alias_prefix(self) -> &'static str {
        match self {
            Self::Package => "Ui",
            Self::Copy(_) => "Styled",
        }
    }
}

/// Which of the two rewrites a codemod run makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Steps {
    /// [`Origin::Package`]: `@uniflowed/ui`'s prefixed names (0.3.0).
    pub(super) package: bool,
    /// [`Origin::Copy`]: the copies `uf ui add` wrote, and every import of one
    /// (0.3.0). See [`super::ui_copies`].
    pub(super) copies: bool,
}

/// Add every rewrite this project needs to `plan`, one change per file.
///
/// Walks the project's own sources — `.js`, `.jsx`, `.mjs` and `.cjs`, not
/// `node_modules`, not a hidden directory such as `.uf` or `.git`, and not the
/// `dist` a build wrote — and reads only files that mention the package, so a
/// large project costs one read per file and a token scan of the few that
/// import it. Nothing is written here: the plan is applied, or printed by
/// `--dry-run`, by the caller.
///
/// With [`Steps::copies`], a file `uf ui add` wrote is first given the
/// registry's new export shape ([`super::ui_copies::reshape`]), and every
/// import of a copy is rewritten after the package's. The steps are chained per
/// file and yield one change for it, all or nothing. A result is formatted with
/// the project's formatter when the file it replaces was formatted, so a copy
/// matches the registry's text byte for byte where nobody edited it, and
/// `uf ui update` later merges without conflicts.
pub(super) fn plan_project(root: &Utf8Path, plan: &mut Plan, steps: Steps) -> Result<()> {
    let mut files: Vec<(String, String)> = Vec::new();
    let walk = walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0
                || !(name.starts_with('.') || matches!(&*name, "node_modules" | "dist" | "target"))
        });
    for entry in walk {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(extension) = entry.path().extension().and_then(|it| it.to_str()) else {
            continue;
        };
        if !matches!(extension, "js" | "jsx" | "mjs" | "cjs") {
            continue;
        }
        let Ok(before) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let path = entry
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        files.push((path, before));
    }

    let copies = if steps.copies {
        super::ui_copies::copies(&files)
    } else {
        BTreeMap::new()
    };
    let fmt = uf_config::load_config(root)
        .ok()
        .map(|resolved| resolved.config.fmt);
    for (path, before) in files {
        let mut text = before.clone();
        let outcome = (|| -> Result<(), String> {
            if let Some(component) = copies.get(path.as_str())
                && let Some(after) = super::ui_copies::reshape(&text, component)?
            {
                text = after;
            }
            if steps.package
                && text.contains(PACKAGE)
                && let Some(after) = rewrite(&text)?
            {
                text = after;
            }
            if !copies.is_empty() {
                let resolve = |specifier: &str| {
                    super::ui_copies::resolve(&path, specifier, &copies).map(Origin::Copy)
                };
                if let Some(after) = rewrite_with(&text, &resolve)? {
                    text = after;
                }
            }
            Ok(())
        })();
        if let Err(reason) = outcome {
            plan.unmapped.push(format!("{path}: {reason}"));
            continue;
        }
        if text == before {
            continue;
        }
        if let Some(config) = &fmt {
            let was_formatted =
                uf_fmt::format_source(&before, config).is_ok_and(|result| !result.changed);
            if was_formatted && let Ok(result) = uf_fmt::format_source(&text, config) {
                text = result.output;
            }
        }
        plan.write(&path, before, text);
    }
    Ok(())
}

/// `source` with every removed `@uniflowed/ui` name rewritten to its namespace,
/// `Ok(None)` when it names none, or the reason it was left alone.
///
/// The contract the module header describes: all or nothing per file, a
/// result the official Flow grammar accepts whenever the input was accepted,
/// comments, strings and formatting outside the rewritten tokens byte for byte
/// as they were, and a second run over the output a no-op.
pub(super) fn rewrite(source: &str) -> Result<Option<String>, String> {
    rewrite_with(source, &|specifier: &str| {
        (specifier == PACKAGE).then_some(Origin::Package)
    })
}

/// [`rewrite`], for every import whose specifier `resolve` places: the package
/// and, for the second step, the copies `uf ui add` wrote.
pub(super) fn rewrite_with(
    source: &str,
    resolve: &dyn Fn(&str) -> Option<Origin>,
) -> Result<Option<String>, String> {
    let tokens = uf_flow::scan::tokenize_jsx(source);
    if tokens.iter().any(|token| token.kind == TokenKind::Invalid) {
        return if names_a_removed_part(source, &tokens) {
            Err(
                "the file has a token uf could not read; rewrite its `@uniflowed/ui` names by hand"
                    .to_owned(),
            )
        } else {
            Ok(None)
        };
    }
    let imports = package_imports(source, &tokens, resolve)?;
    if imports.is_empty() {
        return Ok(None);
    }

    // What each import binds: removed parts by their local name, the
    // namespaces already imported, and the `* as ui` bindings whose members
    // are rewritten in place.
    let mut renamed: BTreeMap<&str, (Origin, String, &'static str)> = BTreeMap::new();
    let mut imported_namespaces: BTreeMap<&str, &str> = BTreeMap::new();
    let mut star_locals: BTreeMap<&str, Origin> = BTreeMap::new();
    for import in &imports {
        if let Some(local) = import.star {
            star_locals.insert(local, import.origin);
        }
        for specifier in &import.named {
            if specifier.type_only {
                continue;
            }
            if let Some((namespace, member)) = import.origin.lookup(specifier.imported) {
                renamed.insert(specifier.local, (import.origin, namespace, member));
            } else if import.origin == Origin::Package
                && PARTS
                    .iter()
                    .any(|(namespace, _)| *namespace == specifier.imported)
            {
                imported_namespaces.insert(specifier.imported, specifier.local);
            }
        }
    }

    // The name each needed namespace is imported as: the binding the file
    // already has, the namespace's own name if the file does not use it for
    // anything else, or `Ui` + the name.
    let inside_imports = |at: usize| imports.iter().any(|import| import.range.contains(&at));
    let bound: BTreeSet<&str> = tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            token.kind == TokenKind::Ident
                && !inside_imports(token.start)
                // A local being renamed away frees its name: every reference
                // to it is rewritten, so the namespace can take it.
                && !renamed.contains_key(token.text(source))
                // `export { DialogRoot as Root }` exports a name; it binds none.
                && !is_exported_alias(source, &tokens, *index)
                && !index
                    .checked_sub(1)
                    .is_some_and(|previous| tokens[previous].is_punct(b'.'))
        })
        .map(|(_, token)| token.text(source))
        .collect();
    // Keyed by origin as well as name: a page may import the package's
    // `Dialog` and its own copy's in one file, and they are two bindings.
    let mut locals: BTreeMap<(Origin, String), String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut added: Vec<String> = Vec::new();
    for (origin, namespace, _) in renamed.values() {
        let key = (*origin, namespace.clone());
        if locals.contains_key(&key) {
            continue;
        }
        if *origin == Origin::Package
            && let Some(local) = imported_namespaces.get(namespace.as_str())
        {
            locals.insert(key, (*local).to_owned());
            continue;
        }
        let mut local = namespace.clone();
        let mut attempt = 1;
        while bound.contains(local.as_str()) || taken.contains(&local) {
            let prefix = origin.alias_prefix();
            local = if attempt == 1 {
                format!("{prefix}{namespace}")
            } else {
                format!("{prefix}{namespace}{attempt}")
            };
            attempt += 1;
        }
        taken.insert(local.clone());
        if *origin == Origin::Package {
            added.push(if local == *namespace {
                local.clone()
            } else {
                format!("{namespace} as {local}")
            });
        }
        locals.insert(key, local);
    }

    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();

    // The imports: the removed specifiers out, the namespaces in, once. The
    // package's namespaces join the first statement that lost a specifier;
    // a copy's is `import * as Dialog from "…"`, a statement of its own after
    // whatever the copy's import keeps.
    let mut pending = Some(added);
    let mut starred: BTreeSet<String> = BTreeSet::new();
    for import in &imports {
        let removed =
            |s: &Specifier<'_>| !s.type_only && import.origin.lookup(s.imported).is_some();
        if !import.named.iter().any(removed) {
            continue;
        }
        let kept: Vec<String> = import
            .named
            .iter()
            .filter(|s| !removed(s))
            .map(|s| source[s.range.clone()].to_owned())
            .collect();
        match import.origin {
            Origin::Package => {
                let mut kept = kept;
                kept.extend(pending.take().unwrap_or_default());
                edits.push((import.braces.clone(), braces(&kept, import.indent(source))));
            }
            Origin::Copy(component) => {
                let namespace = uf_ui::registry::namespace_name(component);
                let local = &locals[&(import.origin, namespace)];
                let indent = import.indent(source);
                let mut statement = String::new();
                if !kept.is_empty() {
                    statement.push_str(&format!(
                        "import {} from {};\n{indent}",
                        braces(&kept, indent),
                        import.specifier
                    ));
                }
                if starred.insert(import.specifier.to_owned()) {
                    statement.push_str(&format!("import * as {local} from {};", import.specifier));
                } else if statement.ends_with(indent) {
                    statement.truncate(statement.len() - indent.len() - 1);
                }
                edits.push((import.range.clone(), statement));
            }
        }
    }

    // Every reference.
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident || inside_imports(token.start) {
            continue;
        }
        let text = token.text(source);
        let previous = index.checked_sub(1).map(|at| &tokens[at]);
        let after_dot = previous.is_some_and(|it| it.is_punct(b'.'));
        if after_dot {
            // `ui.DialogRoot` on a `* as ui` import, and nothing else: a
            // property called `DialogRoot` on any other object is not ours.
            let owner = index.checked_sub(2).map(|at| &tokens[at]);
            let before_owner = index.checked_sub(3).map(|at| &tokens[at]);
            if let Some(owner) = owner
                && owner.kind == TokenKind::Ident
                && let Some(origin) = star_locals.get(owner.text(source))
                && let Some((namespace, member)) = origin.lookup(text)
                && !before_owner.is_some_and(|it| it.is_punct(b'.'))
            {
                // The package's namespace is a level below it; a copy's is the
                // module the star already binds.
                let replacement = match origin {
                    Origin::Package => format!("{namespace}.{member}"),
                    Origin::Copy(_) => member.to_owned(),
                };
                edits.push((token.start..token.end, replacement));
            }
            continue;
        }
        let Some((origin, namespace, member)) = renamed.get(text) else {
            continue;
        };
        let next = tokens.get(index + 1);
        // Keys and shorthands only exist between braces: `[DialogRoot, …]` and
        // `f(DialogRoot, …)` are plain references.
        let opens_group = previous.is_some_and(|it| it.is_punct(b'{') || it.is_punct(b','))
            && enclosing_opener(&tokens, index) == Some(b'{');
        if opens_group && next.is_some_and(|it| it.is_punct(b':')) {
            // An object key, `{ DialogRoot: … }`: a string, not a reference.
            continue;
        }
        if opens_group && next.is_some_and(|it| it.is_punct(b',') || it.is_punct(b'}')) {
            // `{DialogRoot}` is an expression when it is a JSX container and a
            // shorthand property, a pattern or a local export otherwise; only
            // the first is a plain reference.
            if !is_jsx_container(source, &tokens, index) {
                return Err(format!(
                    "`{text}` is written as a shorthand property or a local export; write `{text}: {namespace}.{member}` or rename the export by hand"
                ));
            }
        }
        let local = &locals[&(*origin, namespace.clone())];
        edits.push((token.start..token.end, format!("{local}.{member}")));
    }

    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut after = String::with_capacity(source.len() + edits.len() * 8);
    let mut at = 0;
    for (range, text) in edits {
        after.push_str(&source[at..range.start]);
        after.push_str(&text);
        at = range.end;
    }
    after.push_str(&source[at..]);

    let parses = |text: &str| uf_flow::validate_source(text).is_ok_and(|outcome| outcome.is_ok());
    if parses(source) && !parses(&after) {
        return Err(
            "the rewritten file does not parse; rewrite its `@uniflowed/ui` and `uf ui add` names by hand"
                .to_owned(),
        );
    }
    Ok(Some(after))
}

/// One named specifier of an import from [`PACKAGE`].
struct Specifier<'a> {
    /// The name the package exports.
    imported: &'a str,
    /// The name this file binds it to.
    local: &'a str,
    /// `type X` or `typeof X` inside the braces.
    type_only: bool,
    /// The specifier as written, `DialogTrigger as Open` included.
    range: std::ops::Range<usize>,
}

/// One value import from [`PACKAGE`].
struct Import<'a> {
    /// The whole statement, `import` to its `;`.
    range: std::ops::Range<usize>,
    /// The braces and everything between them, or an empty range where the
    /// statement has none.
    braces: std::ops::Range<usize>,
    named: Vec<Specifier<'a>>,
    /// The local name of `* as ui`, when the statement is that form.
    star: Option<&'a str>,
    /// Where the specifier leads.
    origin: Origin,
    /// The specifier as written, quotes included.
    specifier: &'a str,
}

impl Import<'_> {
    /// The indentation a multi-line specifier list continues with.
    fn indent<'s>(&self, source: &'s str) -> &'s str {
        let line = source[..self.range.start]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let text = &source[line..self.range.start];
        &text[..text.len() - text.trim_start().len()]
    }
}

/// A specifier list, on one line while it fits in the hundred columns uf's
/// formatter wraps at and one per line after that.
fn braces(specifiers: &[String], indent: &str) -> String {
    let line = format!("{{ {} }}", specifiers.join(", "));
    if line.len() + indent.len() + 30 <= 100 {
        return line;
    }
    let mut out = String::from("{\n");
    for specifier in specifiers {
        out.push_str(indent);
        out.push_str("  ");
        out.push_str(specifier);
        out.push_str(",\n");
    }
    out.push_str(indent);
    out.push('}');
    out
}

/// Every value import whose specifier `resolve` places, or the reason the file
/// is left alone.
fn package_imports<'a>(
    source: &'a str,
    tokens: &[Token],
    resolve: &dyn Fn(&str) -> Option<Origin>,
) -> Result<Vec<Import<'a>>, String> {
    let mut imports = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let keyword = token.text(source);
        if token.kind != TokenKind::Ident || !matches!(keyword, "import" | "export") {
            continue;
        }
        if !(uf_flow::scan::starts_statement(tokens, index) || token.newline_before) {
            continue;
        }
        // `import(…)` and `import.meta` are expressions, not declarations.
        if tokens
            .get(index + 1)
            .is_some_and(|next| next.is_punct(b'(') || next.is_punct(b'.'))
        {
            continue;
        }
        // A clause is names, braces, commas, `*` and string names and nothing
        // else, so the search for its `from` stops at the first token that
        // cannot be in one: `export component Page() {…}` is not read to its end.
        let Some(from) = (index + 1..tokens.len())
            .take_while(|at| {
                let it = &tokens[*at];
                matches!(it.kind, TokenKind::Ident | TokenKind::String)
                    || it.is_punct(b'{')
                    || it.is_punct(b'}')
                    || it.is_punct(b',')
                    || it.is_punct(b'*')
            })
            .find(|at| {
                tokens[*at].is_ident(source, "from")
                    && tokens
                        .get(at + 1)
                        .is_some_and(|next| next.kind == TokenKind::String)
            })
        else {
            continue;
        };
        let literal = &tokens[from + 1];
        let specifier = literal.text(source);
        if specifier.len() < 2 {
            continue;
        }
        let Some(origin) = resolve(&specifier[1..specifier.len() - 1]) else {
            continue;
        };
        let clause = &tokens[index + 1..from];
        if clause
            .first()
            .is_some_and(|first| matches!(first.text(source), "type" | "typeof"))
            && clause.len() > 1
        {
            // `import type { … }` / `export type { … }`: types are unchanged.
            continue;
        }
        let end = tokens
            .get(from + 2)
            .filter(|next| next.is_punct(b';'))
            .map_or(literal.end, |semicolon| semicolon.end);
        let open = clause.iter().position(|it| it.is_punct(b'{'));
        let close = clause.iter().position(|it| it.is_punct(b'}'));
        let named = match (open, close) {
            (Some(open), Some(close)) => specifiers(source, &clause[open + 1..close])?,
            _ => Vec::new(),
        };
        if keyword == "export" {
            if let Some(found) = named.iter().find(|s| origin.lookup(s.imported).is_some()) {
                return Err(format!(
                    "re-exports `{}` from {specifier}; a re-export is this file's own surface, so rename it by hand",
                    found.imported
                ));
            }
            continue;
        }
        let star = clause
            .windows(3)
            .find(|it| it[0].is_punct(b'*') && it[1].is_ident(source, "as"))
            .map(|it| it[2].text(source));
        imports.push(Import {
            range: token.start..end,
            braces: match (open, close) {
                (Some(open), Some(close)) => clause[open].start..clause[close].end,
                _ => 0..0,
            },
            named,
            star,
            origin,
            specifier,
        });
    }
    Ok(imports)
}

/// The comma-separated specifiers between an import's braces.
fn specifiers<'a>(source: &'a str, inner: &[Token]) -> Result<Vec<Specifier<'a>>, String> {
    let mut out = Vec::new();
    for entry in inner.split(|token| token.is_punct(b',')) {
        let Some(first) = entry.first() else {
            continue;
        };
        let type_only = entry.len() > 1 && matches!(first.text(source), "type" | "typeof");
        let words = if type_only { &entry[1..] } else { entry };
        if words.iter().any(|it| it.kind != TokenKind::Ident) {
            if words
                .iter()
                .any(|it| part(it.text(source).trim_matches(['"', '\''])).is_some())
            {
                return Err(
                    "imports a removed name with a string specifier; rewrite it by hand".to_owned(),
                );
            }
            continue;
        }
        let (imported, local) = match words {
            [name] => (name.text(source), name.text(source)),
            [name, keyword, local] if keyword.text(source) == "as" => {
                (name.text(source), local.text(source))
            }
            _ => continue,
        };
        let last = entry.last().unwrap_or(first);
        out.push(Specifier {
            imported,
            local,
            type_only,
            range: first.start..last.end,
        });
    }
    Ok(out)
}

/// Whether `{name}` at `index` is a JSX expression container: a child
/// (`<p>{DialogRoot}</p>`) or an attribute value (`as={DialogRoot}`).
///
/// Conservative in the direction that matters: an answer of `false` makes the
/// caller leave the file for a person, never rewrite it wrongly.
fn is_jsx_container(source: &str, tokens: &[Token], index: usize) -> bool {
    let Some(open) = index.checked_sub(1) else {
        return false;
    };
    if !tokens[open].is_punct(b'{') || !tokens.get(index + 1).is_some_and(|it| it.is_punct(b'}')) {
        return false;
    }
    let Some(before) = open.checked_sub(1).map(|at| &tokens[at]) else {
        return false;
    };
    if matches!(before.kind, TokenKind::JsxTagClose | TokenKind::JsxText) {
        return true;
    }
    if !before.is_punct(b'=') {
        return false;
    }
    // An attribute's `=`: the nearest tag token behind it opens a tag.
    tokens[..open]
        .iter()
        .rev()
        .take(256)
        .find(|it| matches!(it.kind, TokenKind::JsxTagOpen | TokenKind::JsxTagClose))
        .is_some_and(|it| it.kind == TokenKind::JsxTagOpen && !source[it.start..].starts_with("</"))
}

/// Whether the token at `index` is the `Y` of `X as Y` in an `export { … }`
/// list, which names an export rather than binding anything in the file.
fn is_exported_alias(source: &str, tokens: &[Token], index: usize) -> bool {
    if !index
        .checked_sub(1)
        .is_some_and(|previous| tokens[previous].is_ident(source, "as"))
    {
        return false;
    }
    for at in (0..index).rev() {
        let token = &tokens[at];
        if token.is_punct(b';') || token.is_punct(b'}') {
            return false;
        }
        if token.is_punct(b'{') {
            return at
                .checked_sub(1)
                .is_some_and(|previous| tokens[previous].is_ident(source, "export"));
        }
    }
    false
}

/// The bracket — `(`, `[` or `{` — the token at `index` sits directly inside,
/// or `None` at the top level of the file.
fn enclosing_opener(tokens: &[Token], index: usize) -> Option<u8> {
    let mut depth = 0usize;
    for token in tokens[..index].iter().rev() {
        let TokenKind::Punct(byte) = token.kind else {
            continue;
        };
        match byte {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth == 0 => return Some(byte),
            b'(' | b'[' | b'{' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Whether a file whose tokens could not all be read still names a removed
/// part, so leaving it alone has to be reported rather than silent.
fn names_a_removed_part(source: &str, tokens: &[Token]) -> bool {
    tokens
        .iter()
        .any(|token| token.kind == TokenKind::Ident && part(token.text(source)).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewritten(source: &str) -> String {
        rewrite(source)
            .expect("rewritable")
            .expect("something to rewrite")
    }

    #[test]
    fn every_removed_name_maps_to_exactly_one_part() {
        assert_eq!(part("DialogRoot"), Some(("Dialog", "Root")));
        assert_eq!(part("MenubarMenu"), Some(("Menubar", "Menu")));
        assert_eq!(part("MenuSubTrigger"), Some(("Menu", "SubTrigger")));
        assert_eq!(part("AlertDialogCancel"), Some(("AlertDialog", "Cancel")));
        assert_eq!(part("AlertRoot"), Some(("Alert", "Root")));
        for kept in [
            "Dialog",
            "Checkbox",
            "DialogRooot",
            "toast",
            "DialogRole",
            "Root",
        ] {
            assert_eq!(part(kept), None, "{kept}");
        }
        let count: usize = PARTS.iter().map(|(_, parts)| parts.len()).sum();
        assert_eq!(count, 197, "the removed surface is 197 names");
    }

    #[test]
    fn named_imports_become_one_namespace_and_every_reference_follows() {
        let before = r#"// @flow
import * as React from "react";
import { DialogRoot, DialogTrigger as Open, Checkbox } from "@uniflowed/ui";
import type { DialogRole } from "@uniflowed/ui";

// DialogRoot in a comment stays, and so does "DialogRoot" in a string.
const label = "DialogRoot";

export component Page(role: DialogRole) renders DialogRoot {
  const Kind: typeof DialogRoot = DialogRoot;
  return (
    <DialogRoot>
      <Open>Open</Open>
      <Checkbox />
    </DialogRoot>
  );
}
"#;
        let after = rewritten(before);
        assert_eq!(
            after,
            r#"// @flow
import * as React from "react";
import { Checkbox, Dialog } from "@uniflowed/ui";
import type { DialogRole } from "@uniflowed/ui";

// DialogRoot in a comment stays, and so does "DialogRoot" in a string.
const label = "DialogRoot";

export component Page(role: DialogRole) renders Dialog.Root {
  const Kind: typeof Dialog.Root = Dialog.Root;
  return (
    <Dialog.Root>
      <Dialog.Trigger>Open</Dialog.Trigger>
      <Checkbox />
    </Dialog.Root>
  );
}
"#
        );
        // And a second run has nothing left to do.
        assert_eq!(rewrite(&after).unwrap(), None);
    }

    #[test]
    fn a_namespace_import_has_its_members_rewritten_in_place() {
        let before = "import * as Primitive from \"@uniflowed/ui\";\n\nexport const a = <Primitive.DialogRoot><Primitive.Dialog.Body /></Primitive.DialogRoot>;\nexport const b = other.DialogRoot;\n";
        assert_eq!(
            rewritten(before),
            "import * as Primitive from \"@uniflowed/ui\";\n\nexport const a = <Primitive.Dialog.Root><Primitive.Dialog.Body /></Primitive.Dialog.Root>;\nexport const b = other.DialogRoot;\n"
        );
    }

    #[test]
    fn a_name_the_file_already_uses_is_not_taken_twice() {
        let before = "import { DialogRoot, DialogTitle } from \"@uniflowed/ui\";\n\nexport component Dialog() {\n  return <DialogRoot><DialogTitle>Hi</DialogTitle></DialogRoot>;\n}\n";
        assert_eq!(
            rewritten(before),
            "import { Dialog as UiDialog } from \"@uniflowed/ui\";\n\nexport component Dialog() {\n  return <UiDialog.Root><UiDialog.Title>Hi</UiDialog.Title></UiDialog.Root>;\n}\n"
        );
    }

    #[test]
    fn an_existing_namespace_import_is_reused() {
        let before = "import { Dialog, DialogRoot } from \"@uniflowed/ui\";\nexport const a = [Dialog.Body, DialogRoot];\n";
        assert_eq!(
            rewritten(before),
            "import { Dialog } from \"@uniflowed/ui\";\nexport const a = [Dialog.Body, Dialog.Root];\n"
        );
    }

    #[test]
    fn a_long_specifier_list_wraps_the_way_the_formatter_does() {
        let before = "import {\n  AccordionRoot,\n  BreadcrumbRoot,\n  CarouselRoot,\n  CollapsibleRoot,\n  ComboboxRoot,\n  DatePickerRoot,\n  NavigationMenuRoot,\n} from \"@uniflowed/ui\";\nexport const all = [AccordionRoot, BreadcrumbRoot, CarouselRoot, CollapsibleRoot, ComboboxRoot, DatePickerRoot, NavigationMenuRoot];\n";
        let after = rewritten(before);
        assert!(
            after.starts_with("import {\n  Accordion,\n  Breadcrumb,\n  Carousel,\n  Collapsible,\n  Combobox,\n  DatePicker,\n  NavigationMenu,\n} from \"@uniflowed/ui\";\n"),
            "{after}"
        );
        assert!(
            after.contains("[Accordion.Root, Breadcrumb.Root,"),
            "{after}"
        );
    }

    #[test]
    fn keys_members_of_other_objects_and_other_packages_are_left_alone() {
        let before = "import { DialogRoot } from \"@uniflowed/ui\";\nimport { TabsList } from \"./mine.js\";\nexport const map = { DialogRoot: 1, other: DialogRoot };\nexport const tabs = TabsList;\n";
        assert_eq!(
            rewritten(before),
            "import { Dialog } from \"@uniflowed/ui\";\nimport { TabsList } from \"./mine.js\";\nexport const map = { DialogRoot: 1, other: Dialog.Root };\nexport const tabs = TabsList;\n"
        );
    }

    #[test]
    fn a_jsx_container_is_a_reference_and_a_shorthand_is_left_for_a_person() {
        let container = "import { TabsTab } from \"@uniflowed/ui\";\nexport const a = <Menu as={TabsTab}>{TabsTab}</Menu>;\n";
        assert_eq!(
            rewritten(container),
            "import { Tabs } from \"@uniflowed/ui\";\nexport const a = <Menu as={Tabs.Tab}>{Tabs.Tab}</Menu>;\n"
        );

        for shorthand in [
            "import { TabsTab } from \"@uniflowed/ui\";\nexport const parts = { TabsTab };\n",
            "import { TabsTab } from \"@uniflowed/ui\";\nexport { TabsTab };\n",
            "export { TabsTab } from \"@uniflowed/ui\";\n",
        ] {
            let reason = rewrite(shorthand).expect_err(shorthand);
            assert!(reason.contains("TabsTab"), "{reason}");
        }
    }

    #[test]
    fn a_file_that_names_nothing_removed_is_untouched() {
        for source in [
            "import { Dialog, Checkbox } from \"@uniflowed/ui\";\nexport const a = <Dialog.Root />;\n",
            "import type { DialogRole } from \"@uniflowed/ui\";\n",
            "export const DialogRoot = 1;\n",
            "import \"@uniflowed/ui\";\n",
        ] {
            assert_eq!(rewrite(source).unwrap(), None, "{source}");
        }
    }

    #[test]
    fn a_project_is_planned_file_by_file_and_leaves_what_it_skips_in_the_report() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "app/$page.js",
            "import { SwitchThumb, DialogRoot } from \"@uniflowed/ui\";\nexport const a = DialogRoot;\n",
        );
        write(
            "app/parts.js",
            "import { TabsTab } from \"@uniflowed/ui\";\nexport { TabsTab };\n",
        );
        write(
            "node_modules/x/index.js",
            "import { DialogRoot } from \"@uniflowed/ui\";\n",
        );
        write(
            ".uf/cache/y.js",
            "import { DialogRoot } from \"@uniflowed/ui\";\n",
        );

        let mut plan = Plan::new("codemod");
        plan_project(
            root,
            &mut plan,
            Steps {
                package: true,
                copies: true,
            },
        )
        .unwrap();

        let paths: Vec<&str> = plan.changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["app/$page.js"]);
        assert_eq!(
            plan.changes[0].after.as_deref(),
            Some(
                "import { SwitchThumb, Dialog } from \"@uniflowed/ui\";\nexport const a = Dialog.Root;\n"
            )
        );
        assert_eq!(plan.unmapped.len(), 1, "{:?}", plan.unmapped);
        assert!(plan.unmapped[0].starts_with("app/parts.js: "));
    }
}
