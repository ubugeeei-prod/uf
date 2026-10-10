//! Hoist an inline `"use server"` closure into a generated export.
//!
//! The closure's captured variables are not arguments the caller chose, so
//! they must not travel in the clear. The rewrite leaves each one as a name
//! in `bindServerClosure(...)`. The server seals the values when the component
//! renders; the hoisted export unseals them before the body runs. The build id
//! never appears in this file.
//!
//! A closure written in a `"use client"` module is not rewritten. Sealing it
//! would mean shipping the key. The registry records that closure as
//! unreachable, and the manifest never gives it an endpoint.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::Serialize;
use uf_infra::LineIndex;

use crate::action::{ServerActionKind, ServerActionRegistry};
use crate::scan::owner::{FunctionHead, function_head, function_name};
use crate::scan::{
    ImportKind, Token, TokenKind, clamp_u32, matching_close, matching_open, scan_imports,
    starts_statement, tokenize,
};
use crate::{RscError, scan_directives};

/// File name of the rewrite, beside `uf-rsc-manifest.json`.
pub const INLINE_CLOSURES_FILE_NAME: &str = "inline-closures.json";

const SIDECAR_VERSION: u32 = 1;
const IMPORT_LINE: &str =
    "import { bindServerClosure, unsealServerCaptures } from \"@uniflowed/router/rsc\";\n";
const SEALED_PARAM: &str = "$$ufSealed";

/// One inline closure the registry has already named.
#[derive(Debug, Clone)]
pub(crate) struct ClosureId {
    /// `N` in `$$ufActionN`, which is the closure's scan order in its module.
    pub(crate) index: usize,
    /// Action id, lowercase hex.
    pub(crate) id: CompactString,
}

/// Why a module could not be rewritten.
///
/// The detail names a shape in the source. It never includes a capture value
/// or the build id.
#[derive(Debug)]
pub(crate) struct RewriteFailure {
    /// Static reason, safe to surface.
    pub(crate) detail: &'static str,
}

#[derive(Serialize)]
struct Sidecar {
    version: u32,
    modules: std::collections::BTreeMap<String, String>,
}

/// Write `inline-closures.json` into `dir`, including when nothing was rewritten.
///
/// An empty file is the record of "this build has no inline closure". Leaving
/// the previous build's file in place would keep serving a deleted closure.
pub fn write_inline_closures(
    dir: &Utf8Path,
    root: &Utf8Path,
    registry: &ServerActionRegistry,
) -> Result<Utf8PathBuf, RscError> {
    let mut grouped: std::collections::BTreeMap<Utf8PathBuf, Vec<ClosureId>> =
        std::collections::BTreeMap::new();
    for action in registry.actions() {
        if action.kind != ServerActionKind::InlineClosure || !action.exposure.is_callable() {
            continue;
        }
        let Some(index) = export_index(&action.export) else {
            return Err(invalid(
                &action.module,
                "inline closure export is not $$ufActionN",
            ));
        };
        grouped
            .entry(action.module.clone())
            .or_default()
            .push(ClosureId {
                index,
                id: action.id.to_hex(),
            });
    }

    let mut modules = std::collections::BTreeMap::new();
    for (module, mut closures) in grouped {
        closures.sort_by_key(|closure| closure.index);
        let absolute = root.join(&module);
        let bytes = fs::read(&absolute).map_err(|source| RscError::Read {
            path: module.clone(),
            source,
        })?;
        let source = uf_infra::validate_utf8(&bytes).map_err(|_| RscError::NonUtf8Source {
            path: module.clone(),
        })?;
        let rewritten = rewrite_module(source, &closures)
            .map_err(|failure| invalid(&module, failure.detail))?;
        let key = module.as_str().replace('\\', "/");
        modules.insert(key, rewritten);
    }

    fs::create_dir_all(dir).map_err(|source| RscError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(INLINE_CLOSURES_FILE_NAME);
    let sidecar = Sidecar {
        version: SIDECAR_VERSION,
        modules,
    };
    let mut json =
        serde_json::to_string_pretty(&sidecar).map_err(|source| RscError::Serialize { source })?;
    json.push('\n');
    // A body-only edit does not change the manifest, so the dev server tells
    // the two files apart by bytes. Rewriting identical bytes would move the
    // modification time and reload every page on every save.
    if fs::read_to_string(&path).is_ok_and(|current| current == json) {
        return Ok(path);
    }
    fs::write(&path, json).map_err(|source| RscError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub(crate) fn rewrite_module(
    source: &str,
    closures: &[ClosureId],
) -> Result<String, RewriteFailure> {
    let tokens = tokenize(source);
    let scan = scan_directives(source);
    if scan.function_directives.len() != closures.len() {
        return Err(fail("scanned closures do not match the registry"));
    }
    for (position, closure) in closures.iter().enumerate() {
        if closure.index != position {
            return Err(fail("closure indexes are not the scan order"));
        }
    }
    if closures.is_empty() {
        return Ok(source.to_owned());
    }

    let index = LineIndex::new(source);
    let mut located = locate(source, &tokens, &index, &scan.function_directives)?;
    let scopes = analyze(source, &tokens)?;
    for place in &mut located {
        let Some(site) = scopes
            .functions
            .iter()
            .find(|site| site.open == place.brace)
        else {
            return Err(fail("a closure body is not a function scope"));
        };
        place.scope = site.scope;
    }
    let mut rendered = Vec::with_capacity(located.len());
    for place in &located {
        rendered.push(render_one(
            source, &tokens, place, closures, &scopes, &located,
        )?);
    }

    let mut edits = Vec::new();
    for (place, one) in located.iter().zip(rendered.iter()) {
        if !is_nested(place, &located) {
            edits.push((
                place.replace_start,
                place.replace_end,
                one.replacement.clone(),
            ));
        }
    }
    let mut output = apply(source, 0, &edits);
    let import_at = import_byte(source, &tokens, &scan);
    // Edits sit in function bodies, after the prologue, so this offset still
    // points at the same byte.
    if import_at <= output.len() {
        output.insert_str(import_at, IMPORT_LINE);
    }
    for one in &rendered {
        output.push_str(&one.hoisted);
    }
    Ok(output)
}

struct Place {
    index: usize,
    /// Token index of the `{` that opens this function.
    brace: usize,
    replace_start: usize,
    replace_end: usize,
    body_start: usize,
    body_end: usize,
    scope: usize,
    use_start: usize,
    params: String,
    statement: bool,
    name: Option<CompactString>,
    /// Set when the closure is an object-literal method. The call site keeps
    /// `name:` so the object stays an object. Class methods are refused.
    object_key: Option<CompactString>,
}

struct Rendered {
    replacement: String,
    hoisted: String,
}

struct Scope {
    parent: Option<usize>,
    module: bool,
    bindings: Vec<CompactString>,
}

struct Analysis {
    scopes: Vec<Scope>,
    scope_of: Vec<usize>,
    binding: Vec<u8>,
    typed: Vec<u8>,
    functions: Vec<FunctionSite>,
}

struct FunctionSite {
    open: usize,
    scope: usize,
}

fn render_one(
    source: &str,
    tokens: &[Token],
    place: &Place,
    closures: &[ClosureId],
    scopes: &Analysis,
    all: &[Place],
) -> Result<Rendered, RewriteFailure> {
    let Some(closure) = closures.iter().find(|closure| closure.index == place.index) else {
        return Err(fail("a closure has no action id"));
    };
    let replacement = render_replacement(source, tokens, place, closure, scopes)?;
    let captures = captures_of(source, tokens, place, scopes);
    let child_edits = child_edits_from(source, tokens, place, closures, all, scopes)?;
    let body = apply(
        &source[place.body_start..place.body_end],
        place.body_start,
        &child_edits,
    );
    let (params, prelude) = signature_and_prelude(source, tokens, place, &captures, scopes)?;
    let hoisted = hoist(place, closure, &captures, &params, &prelude, &body)?;
    Ok(Rendered {
        replacement,
        hoisted,
    })
}

fn hoist(
    place: &Place,
    closure: &ClosureId,
    captures: &[CompactString],
    params: &str,
    prelude: &str,
    body: &str,
) -> Result<String, RewriteFailure> {
    let export = export_name(place.index);
    let id = json_hex(&closure.id)?;
    let mut hoisted = String::from("\nexport async function ");
    hoisted.push_str(&export);
    hoisted.push('(');
    hoisted.push_str(SEALED_PARAM);
    if !params.is_empty() {
        hoisted.push_str(", ");
        hoisted.push_str(params.trim());
    }
    hoisted.push_str(") {\n  ");
    if captures.is_empty() {
        hoisted.push_str("unsealServerCaptures(");
        hoisted.push_str(SEALED_PARAM);
        hoisted.push_str(", ");
        hoisted.push_str(&id);
        hoisted.push_str(");\n");
    } else {
        hoisted.push_str("const [");
        for (position, name) in captures.iter().enumerate() {
            if position > 0 {
                hoisted.push_str(", ");
            }
            hoisted.push_str(name);
        }
        hoisted.push_str("] = unsealServerCaptures(");
        hoisted.push_str(SEALED_PARAM);
        hoisted.push_str(", ");
        hoisted.push_str(&id);
        hoisted.push_str(");\n");
    }
    hoisted.push_str(prelude);
    hoisted.push_str(body);
    if !body.ends_with('\n') {
        hoisted.push('\n');
    }
    hoisted.push_str("}\n");
    Ok(hoisted)
}

/// Replacements for the closures directly inside `parent`, applied to its body.
fn child_edits_from(
    source: &str,
    tokens: &[Token],
    parent: &Place,
    closures: &[ClosureId],
    all: &[Place],
    scopes: &Analysis,
) -> Result<Vec<(usize, usize, String)>, RewriteFailure> {
    let mut edits = Vec::new();
    for child in all {
        if !is_direct_child(parent, child, all) {
            continue;
        }
        let Some(closure) = closures.iter().find(|closure| closure.index == child.index) else {
            return Err(fail("a nested closure has no action id"));
        };
        let rendered = render_replacement(source, tokens, child, closure, scopes)?;
        edits.push((child.replace_start, child.replace_end, rendered));
    }
    Ok(edits)
}

fn render_replacement(
    source: &str,
    tokens: &[Token],
    place: &Place,
    closure: &ClosureId,
    scopes: &Analysis,
) -> Result<String, RewriteFailure> {
    if place.params.contains(SEALED_PARAM) || place.name.as_deref() == Some(SEALED_PARAM) {
        return Err(fail("a closure parameter is named $$ufSealed"));
    }
    let captures = captures_of(source, tokens, place, scopes);
    if captures.iter().any(|name| name == SEALED_PARAM) {
        return Err(fail("a captured binding is named $$ufSealed"));
    }
    let export = export_name(place.index);
    let id = json_hex(&closure.id)?;
    let mut replacement = String::new();
    if let Some(name) = place.object_key.as_deref() {
        replacement.push_str(name);
        replacement.push_str(": ");
    } else if place.statement {
        let Some(name) = place.name.as_deref() else {
            return Err(fail("a statement closure has no binding to keep"));
        };
        if source[place.replace_start..].starts_with("export ") {
            replacement.push_str("export ");
        }
        replacement.push_str("const ");
        replacement.push_str(name);
        replacement.push_str(" = ");
    }
    replacement.push_str("bindServerClosure(");
    replacement.push_str(&export);
    replacement.push_str(", ");
    replacement.push_str(&id);
    replacement.push_str(", ");
    replacement.push_str(&capture_list(&captures));
    replacement.push(')');
    if place.statement && !source[place.replace_end..].starts_with(';') {
        replacement.push(';');
    }
    Ok(replacement)
}

fn locate(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    directives: &[crate::FunctionDirective],
) -> Result<Vec<Place>, RewriteFailure> {
    let mut places = Vec::with_capacity(directives.len());
    for (ordinal, directive) in directives.iter().enumerate() {
        let Some(directive_at) = tokens.iter().position(|token| {
            if token.kind != TokenKind::String {
                return false;
            }
            let at = index.line_col(token.start);
            clamp_u32(at.line) == directive.line && clamp_u32(at.column) == directive.column
        }) else {
            return Err(fail("a use server directive is not in the token stream"));
        };
        let Some(brace) = directive_at.checked_sub(1) else {
            return Err(fail("a use server directive does not open a function"));
        };
        if !tokens[brace].is_punct(b'{') {
            return Err(fail("a use server directive does not open a function"));
        }
        let Some(head) = function_head(source, tokens, brace) else {
            return Err(fail("a use server directive is not a function body"));
        };
        let Some(close) = matching_close(tokens, brace, b'{', b'}') else {
            return Err(fail("a function body does not close"));
        };
        let expr = expression_start(source, tokens, head)?;
        let (stmt, export_default) = statement_start(source, tokens, expr);
        let name = function_name(source, tokens, head);
        let object_key = object_method_key(source, tokens, expr, name.as_ref())?;
        let expression =
            object_key.is_some() || export_default || is_expression(source, tokens, expr);
        let replace_token = if expression { expr } else { stmt };
        let params = param_source(source, tokens, head);
        let body_start = directive_end(tokens, directive_at);
        places.push(Place {
            index: ordinal,
            brace,
            replace_start: tokens[replace_token].start,
            replace_end: tokens[close].end,
            body_start,
            body_end: tokens[close].start,
            scope: usize::MAX,
            use_start: use_start(tokens, head),
            params,
            statement: !expression,
            name,
            object_key,
        });
    }
    Ok(places)
}

fn analyze(source: &str, tokens: &[Token]) -> Result<Analysis, RewriteFailure> {
    let mut scopes = vec![Scope {
        parent: None,
        module: true,
        bindings: Vec::new(),
    }];
    for import in scan_imports(source) {
        if import.kind == ImportKind::ReExport {
            continue;
        }
        for binding in &import.bindings {
            scopes[0].bindings.push(binding.local.clone());
        }
    }

    let len = tokens.len();
    let mut scope_of = vec![0usize; len];
    let mut binding = vec![0u8; len];
    let mut typed = vec![0u8; len];
    let mut functions = Vec::new();
    let mut stack = vec![0usize];
    let mut frames: Vec<(usize, bool)> = Vec::new();

    for position in 0..len {
        while frames.last().is_some_and(|frame| frame.0 < position) {
            let frame = frames.pop().expect("frame");
            if frame.1 {
                stack.pop();
            }
        }
        scope_of[position] = *stack.last().expect("scope");

        if tokens[position].is_punct(b'{') {
            if let Some(head) = function_head(source, tokens, position) {
                let parent = *stack.last().expect("scope");
                let scope = scopes.len();
                scopes.push(Scope {
                    parent: Some(parent),
                    module: false,
                    bindings: Vec::new(),
                });
                mark_params(
                    source,
                    tokens,
                    head,
                    position,
                    scope,
                    &mut scopes,
                    &mut binding,
                    &mut typed,
                );
                stack.push(scope);
                let close = matching_close(tokens, position, b'{', b'}').unwrap_or(position);
                frames.push((close, true));
                functions.push(FunctionSite {
                    open: position,
                    scope,
                });
                // Parameter defaults are tokenised before `{` pushes this
                // scope, so a sibling parameter would otherwise resolve in the
                // enclosing scope and be sealed. The header belongs to the
                // function.
                let header = use_start(tokens, head);
                for slot in scope_of.iter_mut().take(position).skip(header) {
                    *slot = scope;
                }
            } else if is_block(source, tokens, position) {
                let parent = *stack.last().expect("scope");
                let scope = scopes.len();
                let mut bindings = Vec::new();
                mark_header(source, tokens, position, &mut bindings, &mut binding);
                scopes.push(Scope {
                    parent: Some(parent),
                    module: false,
                    bindings,
                });
                stack.push(scope);
                let close = matching_close(tokens, position, b'{', b'}').unwrap_or(position);
                frames.push((close, true));
            } else if let Some(close) = matching_close(tokens, position, b'{', b'}') {
                frames.push((close, false));
            }
        }

        if is_lexical_declaration(source, tokens, position) {
            let scope = *stack.last().expect("scope");
            mark_declaration(
                source,
                tokens,
                position + 1,
                scope,
                &mut scopes,
                &mut binding,
                &mut typed,
            );
        }
        if is_word(source, tokens, position, "function")
            || is_word(source, tokens, position, "class")
        {
            mark_declared_name(
                source,
                tokens,
                position,
                *stack.last().expect("scope"),
                &mut scopes,
                &mut binding,
            );
        }
    }

    Ok(Analysis {
        scopes,
        scope_of,
        binding,
        typed,
        functions,
    })
}

fn captures_of(
    source: &str,
    tokens: &[Token],
    place: &Place,
    analysis: &Analysis,
) -> Vec<CompactString> {
    let mut names = Vec::new();
    let start = place.use_start;
    let end = tokens
        .iter()
        .position(|token| token.start >= place.body_end)
        .unwrap_or(tokens.len());
    // `use_start` is a token index. `end` is the first token at or past the
    // closing brace. Params sit before the body, so the range covers defaults.
    let from = start.min(tokens.len());
    let to = end.max(from);
    for position in from..to {
        if !is_use(source, tokens, analysis, position) {
            continue;
        }
        let name = CompactString::from(tokens[position].text(source));
        if names.contains(&name) {
            continue;
        }
        let Some(declared) = resolve(analysis, analysis.scope_of[position], &name) else {
            continue;
        };
        if analysis.scopes[declared].module || within(analysis, declared, place.scope) {
            continue;
        }
        names.push(name);
    }
    names
}

fn is_use(source: &str, tokens: &[Token], analysis: &Analysis, position: usize) -> bool {
    if tokens[position].kind != TokenKind::Ident {
        return false;
    }
    if analysis.binding[position] == 1 || analysis.typed[position] == 1 {
        return false;
    }
    if position > 0 && tokens[position - 1].is_punct(b'.') {
        return false;
    }
    // An identifier followed by `:` is an object key or a label. A ternary, a
    // `case`, and a Flow cast also write `:`, and those are reads.
    if is_object_key(tokens, position) {
        return false;
    }
    let text = tokens[position].text(source);
    !is_keyword(text)
}

fn resolve(analysis: &Analysis, mut scope: usize, name: &str) -> Option<usize> {
    loop {
        if analysis.scopes[scope]
            .bindings
            .iter()
            .rev()
            .any(|binding| binding == name)
        {
            return Some(scope);
        }
        scope = analysis.scopes[scope].parent?;
    }
}

fn within(analysis: &Analysis, mut scope: usize, ancestor: usize) -> bool {
    if ancestor == usize::MAX {
        return false;
    }
    loop {
        if scope == ancestor {
            return true;
        }
        match analysis.scopes[scope].parent {
            Some(parent) => scope = parent,
            None => return false,
        }
    }
}

fn is_nested(place: &Place, all: &[Place]) -> bool {
    all.iter().any(|other| {
        other.index != place.index
            && place.replace_start >= other.body_start
            && place.replace_end <= other.body_end
    })
}

fn is_direct_child(parent: &Place, child: &Place, all: &[Place]) -> bool {
    if child.index == parent.index {
        return false;
    }
    if child.replace_start < parent.body_start || child.replace_end > parent.body_end {
        return false;
    }
    !all.iter().any(|other| {
        other.index != child.index
            && other.index != parent.index
            && other.replace_start >= parent.body_start
            && other.replace_end <= parent.body_end
            && other.replace_start <= child.replace_start
            && other.replace_end >= child.replace_end
            && (other.replace_start < child.replace_start || other.replace_end > child.replace_end)
    })
}

fn expression_start(
    source: &str,
    tokens: &[Token],
    head: FunctionHead,
) -> Result<usize, RewriteFailure> {
    let mut at = match head {
        FunctionHead::Arrow(arrow) => {
            let Some(before) = arrow.checked_sub(1) else {
                return Err(fail("an arrow has no parameter"));
            };
            if tokens[before].is_punct(b')') {
                matching_open(tokens, before, b'(', b')').unwrap_or(before)
            } else {
                before
            }
        }
        FunctionHead::Params(close) => matching_open(tokens, close, b'(', b')')
            .ok_or(fail("a parameter list does not open"))?,
    };
    let mut saw_name = false;
    while let Some(previous) = at.checked_sub(1) {
        let token = &tokens[previous];
        if token.kind == TokenKind::Ident {
            let text = token.text(source);
            if matches!(text, "function" | "async" | "hook" | "component") {
                at = previous;
                continue;
            }
            if matches!(
                text,
                "export"
                    | "default"
                    | "return"
                    | "throw"
                    | "new"
                    | "void"
                    | "typeof"
                    | "await"
                    | "case"
                    | "else"
                    | "yield"
                    | "delete"
                    | "get"
                    | "set"
            ) {
                break;
            }
            if !saw_name {
                saw_name = true;
                at = previous;
                continue;
            }
            break;
        }
        if token.is_punct(b'*') {
            at = previous;
            continue;
        }
        if token.is_punct(b'>')
            && let Some(open) = matching_open(tokens, previous, b'<', b'>')
        {
            at = open;
            continue;
        }
        break;
    }
    Ok(at)
}

fn statement_start(source: &str, tokens: &[Token], expr: usize) -> (usize, bool) {
    let Some(previous) = expr.checked_sub(1) else {
        return (expr, false);
    };
    if is_word(source, tokens, previous, "default")
        && let Some(export_at) = previous.checked_sub(1)
        && is_word(source, tokens, export_at, "export")
    {
        return (export_at, true);
    }
    if is_word(source, tokens, previous, "export") {
        return (previous, false);
    }
    (expr, false)
}

fn is_object_key(tokens: &[Token], position: usize) -> bool {
    position > 0
        && position + 1 < tokens.len()
        && tokens[position + 1].is_punct(b':')
        && (tokens[position - 1].is_punct(b'{') || tokens[position - 1].is_punct(b','))
}

/// `name:` for an object method. A class method and a getter or setter are
/// refused: neither stays valid source if only the function is replaced.
fn object_method_key(
    source: &str,
    tokens: &[Token],
    start: usize,
    name: Option<&CompactString>,
) -> Result<Option<CompactString>, RewriteFailure> {
    let Some(previous) = start.checked_sub(1) else {
        return Ok(None);
    };
    let at = if is_word(source, tokens, previous, "get") || is_word(source, tokens, previous, "set")
    {
        let Some(before) = previous.checked_sub(1) else {
            return Ok(None);
        };
        if !method_separator(tokens, before) {
            return Ok(None);
        }
        return Err(fail(
            "a getter or setter cannot be an inline server closure",
        ));
    } else {
        previous
    };
    if !method_separator(tokens, at) {
        return Ok(None);
    }
    let Some(opener) = enclosing_brace(tokens, at) else {
        return Ok(None);
    };
    if is_class_body(source, tokens, opener) {
        return Err(fail("a class method cannot be an inline server closure"));
    }
    let Some(name) = name else {
        return Err(fail("an object method has no name"));
    };
    Ok(Some(name.clone()))
}

fn method_separator(tokens: &[Token], position: usize) -> bool {
    tokens
        .get(position)
        .is_some_and(|token| token.is_punct(b'{') || token.is_punct(b',') || token.is_punct(b'}'))
}

fn enclosing_brace(tokens: &[Token], from: usize) -> Option<usize> {
    if tokens.get(from).is_some_and(|token| token.is_punct(b'{')) {
        return Some(from);
    }
    let mut depth = i32::from(tokens.get(from).is_some_and(|token| token.is_punct(b'}')));
    let mut at = from;
    while let Some(previous) = at.checked_sub(1) {
        at = previous;
        let token = tokens.get(at)?;
        if token.is_punct(b'}') || token.is_punct(b']') || token.is_punct(b')') {
            depth += 1;
            continue;
        }
        if token.is_punct(b'{') || token.is_punct(b'[') || token.is_punct(b'(') {
            if depth == 0 && token.is_punct(b'{') {
                return Some(at);
            }
            depth -= 1;
        }
    }
    None
}

fn is_class_body(source: &str, tokens: &[Token], brace: usize) -> bool {
    let Some(mut at) = brace.checked_sub(1) else {
        return false;
    };
    let mut depth = 0i32;
    for _ in 0..12 {
        if is_word(source, tokens, at, "class") || is_word(source, tokens, at, "extends") {
            return depth == 0;
        }
        if depth == 0
            && (tokens[at].is_punct(b'=') || tokens[at].is_punct(b';') || tokens[at].is_punct(b'{'))
        {
            return false;
        }
        if tokens[at].is_punct(b'>') || tokens[at].is_punct(b')') {
            depth += 1;
        } else if tokens[at].is_punct(b'<') || tokens[at].is_punct(b'(') {
            depth -= 1;
        }
        at = match at.checked_sub(1) {
            Some(previous) => previous,
            None => return false,
        };
    }
    false
}

/// Parameter text for the hoisted signature, and assignments that have to run
/// after the captures are unsealed.
///
/// A default in the signature runs before the body, so it cannot see a capture
/// unsealed there. Defaults from the first one that reads a capture onward move
/// into the body. Earlier defaults stay, because they do not depend on one.
fn signature_and_prelude(
    source: &str,
    tokens: &[Token],
    place: &Place,
    captures: &[CompactString],
    analysis: &Analysis,
) -> Result<(String, String), RewriteFailure> {
    let open = place.use_start;
    if open >= tokens.len() || !tokens[open].is_punct(b'(') {
        return Ok((place.params.clone(), String::new()));
    }
    let Some(close) = matching_close(tokens, open, b'(', b')') else {
        return Ok((place.params.clone(), String::new()));
    };
    let segments = split_params(tokens, open + 1, close);
    let mut first_moved = None;
    for (index, (start, end)) in segments.iter().copied().enumerate() {
        if let Some(eq) = top_level_eq(tokens, start, end)
            && default_reads_capture(source, tokens, analysis, eq + 1, end, captures)
        {
            first_moved = Some(index);
            break;
        }
    }
    let Some(first_moved) = first_moved else {
        return Ok((place.params.clone(), String::new()));
    };
    let mut signature = String::new();
    let mut prelude = String::new();
    for (index, (start, end)) in segments.iter().copied().enumerate() {
        if !signature.is_empty() {
            signature.push_str(", ");
        }
        let eq = top_level_eq(tokens, start, end);
        if index >= first_moved
            && let Some(eq) = eq
        {
            let name = simple_param_name(source, tokens, start, eq)?;
            signature.push_str(source[tokens[start].start..tokens[eq].start].trim());
            prelude.push_str("if (");
            prelude.push_str(&name);
            prelude.push_str(" === undefined) ");
            prelude.push_str(&name);
            prelude.push_str(" = ");
            prelude.push_str(source[tokens[eq].end..tokens[end].start].trim());
            prelude.push_str(";\n  ");
        } else {
            signature.push_str(source[tokens[start].start..tokens[end].start].trim());
        }
    }
    Ok((signature, prelude))
}

fn split_params(tokens: &[Token], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut segments = Vec::new();
    let mut from = start;
    let mut depth = 0i32;
    let mut position = start;
    while position < end {
        let token = &tokens[position];
        if token.is_punct(b'(')
            || token.is_punct(b'{')
            || token.is_punct(b'[')
            || token.is_punct(b'<')
        {
            depth += 1;
        } else if token.is_punct(b')')
            || token.is_punct(b'}')
            || token.is_punct(b']')
            || token.is_punct(b'>')
        {
            depth -= 1;
        } else if depth == 0 && token.is_punct(b',') {
            if from < position {
                segments.push((from, position));
            }
            from = position + 1;
        }
        position += 1;
    }
    if from < end {
        segments.push((from, end));
    }
    segments
}

fn top_level_eq(tokens: &[Token], start: usize, end: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut position = start;
    while position < end {
        let token = &tokens[position];
        if token.is_punct(b'(')
            || token.is_punct(b'{')
            || token.is_punct(b'[')
            || token.is_punct(b'<')
        {
            depth += 1;
        } else if token.is_punct(b')')
            || token.is_punct(b'}')
            || token.is_punct(b']')
            || token.is_punct(b'>')
        {
            depth -= 1;
        } else if depth == 0 && token.is_punct(b'=') {
            return Some(position);
        }
        position += 1;
    }
    None
}

fn default_reads_capture(
    source: &str,
    tokens: &[Token],
    analysis: &Analysis,
    start: usize,
    end: usize,
    captures: &[CompactString],
) -> bool {
    (start..end).any(|position| {
        is_use(source, tokens, analysis, position)
            && captures
                .iter()
                .any(|name| name == tokens[position].text(source))
    })
}

fn simple_param_name(
    source: &str,
    tokens: &[Token],
    start: usize,
    eq: usize,
) -> Result<String, RewriteFailure> {
    if tokens[start].is_punct(b'.')
        || tokens[start].is_punct(b'{')
        || tokens[start].is_punct(b'[')
        || tokens[start].kind != TokenKind::Ident
    {
        return Err(fail(
            "a parameter default that reads a capture must name one binding",
        ));
    }
    let name = tokens[start].text(source).to_owned();
    let mut position = start + 1;
    if tokens
        .get(position)
        .is_some_and(|token| token.is_punct(b'?'))
    {
        position += 1;
    }
    if position == eq || (position < eq && tokens[position].is_punct(b':')) {
        return Ok(name);
    }
    Err(fail(
        "a parameter default that reads a capture must name one binding",
    ))
}

fn is_expression(source: &str, tokens: &[Token], start: usize) -> bool {
    if starts_statement(tokens, start) {
        return false;
    }
    let Some(previous) = start.checked_sub(1) else {
        return false;
    };
    let token = &tokens[previous];
    if token.kind == TokenKind::Ident {
        return matches!(
            token.text(source),
            "return"
                | "throw"
                | "yield"
                | "await"
                | "void"
                | "typeof"
                | "delete"
                | "new"
                | "in"
                | "of"
                | "instanceof"
                | "default"
                | "case"
                | "else"
        );
    }
    token.kind == TokenKind::Arrow
        || matches!(
            token.kind,
            TokenKind::Punct(
                b'=' | b'('
                    | b','
                    | b'['
                    | b':'
                    | b'?'
                    | b'&'
                    | b'|'
                    | b'!'
                    | b'+'
                    | b'-'
                    | b'*'
                    | b'/'
                    | b'%'
                    | b'~'
                    | b'{'
                    | b'}'
            )
        )
}

fn param_source(source: &str, tokens: &[Token], head: FunctionHead) -> String {
    match head {
        FunctionHead::Arrow(arrow) => {
            let Some(before) = arrow.checked_sub(1) else {
                return String::new();
            };
            if tokens[before].is_punct(b')') {
                let Some(open) = matching_open(tokens, before, b'(', b')') else {
                    return String::new();
                };
                source[tokens[open].end..tokens[before].start]
                    .trim()
                    .to_owned()
            } else if tokens[before].kind == TokenKind::Ident {
                tokens[before].text(source).to_owned()
            } else {
                String::new()
            }
        }
        FunctionHead::Params(close) => {
            let Some(open) = matching_open(tokens, close, b'(', b')') else {
                return String::new();
            };
            source[tokens[open].end..tokens[close].start]
                .trim()
                .to_owned()
        }
    }
}

fn use_start(tokens: &[Token], head: FunctionHead) -> usize {
    match head {
        FunctionHead::Arrow(arrow) => {
            let Some(before) = arrow.checked_sub(1) else {
                return arrow;
            };
            if tokens[before].is_punct(b')') {
                matching_open(tokens, before, b'(', b')').unwrap_or(before)
            } else {
                before
            }
        }
        FunctionHead::Params(close) => matching_open(tokens, close, b'(', b')').unwrap_or(close),
    }
}

fn directive_end(tokens: &[Token], directive_at: usize) -> usize {
    if tokens
        .get(directive_at + 1)
        .is_some_and(|token| token.is_punct(b';'))
    {
        tokens[directive_at + 1].end
    } else {
        tokens[directive_at].end
    }
}

fn import_byte(source: &str, tokens: &[Token], scan: &crate::DirectiveScan) -> usize {
    let Some(file) = scan.file_directive else {
        return tokens.first().map(|token| token.start).unwrap_or(0);
    };
    let index = LineIndex::new(source);
    let Some(at) = tokens.iter().position(|token| {
        if token.kind != TokenKind::String {
            return false;
        }
        let position = index.line_col(token.start);
        clamp_u32(position.line) == file.line && clamp_u32(position.column) == file.column
    }) else {
        return tokens.first().map(|token| token.start).unwrap_or(0);
    };
    directive_end(tokens, at)
}

fn is_block(source: &str, tokens: &[Token], brace: usize) -> bool {
    if starts_statement(tokens, brace) {
        return true;
    }
    let Some(previous) = brace.checked_sub(1) else {
        return false;
    };
    if tokens[previous].is_punct(b')')
        && let Some(open) = matching_open(tokens, previous, b'(', b')')
        && header_keyword(source, tokens, open).is_some()
    {
        return true;
    }
    is_word(source, tokens, previous, "else")
        || is_word(source, tokens, previous, "try")
        || is_word(source, tokens, previous, "do")
        || is_word(source, tokens, previous, "finally")
}

fn header_keyword(source: &str, tokens: &[Token], open: usize) -> Option<&'static str> {
    let mut at = open;
    for _ in 0..3 {
        at = at.checked_sub(1)?;
        let text = tokens[at].text(source);
        if text == "for" {
            return Some("for");
        }
        if text == "catch" {
            return Some("catch");
        }
        if text == "await" {
            continue;
        }
        return None;
    }
    None
}

fn mark_header(
    source: &str,
    tokens: &[Token],
    brace: usize,
    names: &mut Vec<CompactString>,
    binding: &mut [u8],
) {
    let Some(previous) = brace.checked_sub(1) else {
        return;
    };
    if !tokens[previous].is_punct(b')') {
        return;
    }
    let Some(open) = matching_open(tokens, previous, b'(', b')') else {
        return;
    };
    let Some(keyword) = header_keyword(source, tokens, open) else {
        return;
    };
    if keyword == "catch" {
        let _ = collect_pattern(
            source,
            tokens,
            open + 1,
            previous,
            names,
            binding,
            &mut vec![0; tokens.len()],
            false,
        );
        return;
    }
    let mut position = open + 1;
    while position < previous {
        if is_word(source, tokens, position, "const")
            || is_word(source, tokens, position, "let")
            || is_word(source, tokens, position, "var")
        {
            position = collect_pattern(
                source,
                tokens,
                position + 1,
                previous,
                names,
                binding,
                &mut vec![0; tokens.len()],
                false,
            );
            continue;
        }
        position += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn mark_params(
    source: &str,
    tokens: &[Token],
    head: FunctionHead,
    brace: usize,
    scope: usize,
    scopes: &mut [Scope],
    binding: &mut [u8],
    typed: &mut [u8],
) {
    let (open, close) = match head {
        FunctionHead::Arrow(arrow) => {
            let Some(before) = arrow.checked_sub(1) else {
                return;
            };
            if tokens[before].kind == TokenKind::Ident && !tokens[before].is_punct(b')') {
                bind_token(source, tokens, before, scope, scopes, binding);
                mark_annotation(tokens, before + 1, arrow, typed);
                return;
            }
            if !tokens[before].is_punct(b')') {
                return;
            }
            let Some(open) = matching_open(tokens, before, b'(', b')') else {
                return;
            };
            (open, before)
        }
        FunctionHead::Params(close) => {
            let Some(open) = matching_open(tokens, close, b'(', b')') else {
                return;
            };
            (open, close)
        }
    };
    let _ = collect_pattern(
        source,
        tokens,
        open + 1,
        close,
        &mut scopes[scope].bindings,
        binding,
        typed,
        true,
    );
    mark_annotation(tokens, close + 1, brace, typed);
}

fn mark_annotation(tokens: &[Token], start: usize, end: usize, typed: &mut [u8]) {
    if !tokens.get(start).is_some_and(|token| token.is_punct(b':')) {
        return;
    }
    let mut position = start;
    while position < end && position < tokens.len() {
        typed[position] = 1;
        position += 1;
    }
}

fn mark_declaration(
    source: &str,
    tokens: &[Token],
    start: usize,
    scope: usize,
    scopes: &mut [Scope],
    binding: &mut [u8],
    typed: &mut [u8],
) {
    let _ = collect_pattern(
        source,
        tokens,
        start,
        tokens.len(),
        &mut scopes[scope].bindings,
        binding,
        typed,
        false,
    );
}

fn mark_declared_name(
    source: &str,
    tokens: &[Token],
    keyword: usize,
    scope: usize,
    scopes: &mut [Scope],
    binding: &mut [u8],
) {
    let mut position = keyword + 1;
    if tokens
        .get(position)
        .is_some_and(|token| token.is_punct(b'*'))
    {
        position += 1;
    }
    if tokens
        .get(position)
        .is_some_and(|token| token.kind == TokenKind::Ident)
        && !is_keyword(tokens[position].text(source))
    {
        bind_token(source, tokens, position, scope, scopes, binding);
    }
}

/// Record the binding names of one pattern and return the token index it stopped at.
#[allow(clippy::too_many_arguments)]
fn collect_pattern(
    source: &str,
    tokens: &[Token],
    mut position: usize,
    end: usize,
    names: &mut Vec<CompactString>,
    binding: &mut [u8],
    typed: &mut [u8],
    in_params: bool,
) -> usize {
    let mut depth = 0i32;
    while position < end && position < tokens.len() {
        let token = &tokens[position];
        // A default expression is not a binding. Stopping at `=` would also
        // drop every parameter that follows one.
        if in_params && depth == 0 && token.is_punct(b'=') {
            position = skip_default(tokens, position + 1, end);
            continue;
        }
        if depth == 0 && stops_pattern(source, token) {
            break;
        }
        if token.is_punct(b'{') {
            position = object_pattern(source, tokens, position, names, binding, typed);
            continue;
        }
        if token.is_punct(b'[') {
            position = array_pattern(source, tokens, position, names, binding, typed);
            continue;
        }
        if token.is_punct(b'(')
            || token.is_punct(b')')
            || token.is_punct(b']')
            || token.is_punct(b'}')
        {
            depth += i32::from(token.is_punct(b'('));
            depth -=
                i32::from(token.is_punct(b')') || token.is_punct(b']') || token.is_punct(b'}'));
            position += 1;
            continue;
        }
        if token.kind == TokenKind::Ident
            && !is_keyword(token.text(source))
            && binding[position] == 0
        {
            let next_colon = tokens
                .get(position + 1)
                .is_some_and(|next| next.is_punct(b':'));
            bind_name(token.text(source), position, names, binding);
            position += 1;
            if next_colon && position < tokens.len() && tokens[position].is_punct(b':') {
                position += 1;
                position = mark_type(tokens, position, end, typed);
            }
            continue;
        }
        if token.is_punct(b':') {
            position += 1;
            position = mark_type(tokens, position, end, typed);
            continue;
        }
        position += 1;
    }
    position
}

fn object_pattern(
    source: &str,
    tokens: &[Token],
    open: usize,
    names: &mut Vec<CompactString>,
    binding: &mut [u8],
    typed: &mut [u8],
) -> usize {
    let Some(close) = matching_close(tokens, open, b'{', b'}') else {
        return open + 1;
    };
    let mut position = open + 1;
    while position < close {
        if tokens[position].is_punct(b'.') {
            position += 1;
            continue;
        }
        if tokens[position].kind == TokenKind::Ident {
            let renamed = tokens
                .get(position + 1)
                .is_some_and(|token| token.is_punct(b':'));
            if renamed {
                position += 2;
                position = if tokens
                    .get(position)
                    .is_some_and(|token| token.is_punct(b'{'))
                {
                    object_pattern(source, tokens, position, names, binding, typed)
                } else if tokens
                    .get(position)
                    .is_some_and(|token| token.is_punct(b'['))
                {
                    array_pattern(source, tokens, position, names, binding, typed)
                } else if tokens
                    .get(position)
                    .is_some_and(|token| token.kind == TokenKind::Ident)
                {
                    bind_name(tokens[position].text(source), position, names, binding);
                    position + 1
                } else {
                    position + 1
                };
            } else {
                bind_name(tokens[position].text(source), position, names, binding);
                position += 1;
            }
            if tokens
                .get(position)
                .is_some_and(|token| token.is_punct(b'='))
            {
                position = skip_default(tokens, position + 1, close);
            }
            continue;
        }
        position += 1;
    }
    close + 1
}

fn array_pattern(
    source: &str,
    tokens: &[Token],
    open: usize,
    names: &mut Vec<CompactString>,
    binding: &mut [u8],
    typed: &mut [u8],
) -> usize {
    let Some(close) = matching_close(tokens, open, b'[', b']') else {
        return open + 1;
    };
    let mut position = open + 1;
    while position < close {
        if tokens[position].is_punct(b'{') {
            position = object_pattern(source, tokens, position, names, binding, typed);
            continue;
        }
        if tokens[position].is_punct(b'[') {
            position = array_pattern(source, tokens, position, names, binding, typed);
            continue;
        }
        if tokens[position].kind == TokenKind::Ident && !is_keyword(tokens[position].text(source)) {
            bind_name(tokens[position].text(source), position, names, binding);
            position += 1;
            if tokens
                .get(position)
                .is_some_and(|token| token.is_punct(b'='))
            {
                position = skip_default(tokens, position + 1, close);
            }
            continue;
        }
        position += 1;
    }
    close + 1
}

fn mark_type(tokens: &[Token], start: usize, end: usize, typed: &mut [u8]) -> usize {
    let mut position = start;
    let mut depth = 0i32;
    while position < end && position < tokens.len() {
        let token = &tokens[position];
        if depth == 0
            && (token.is_punct(b'=')
                || token.is_punct(b',')
                || token.is_punct(b')')
                || token.is_punct(b';')
                || token.kind == TokenKind::Arrow)
        {
            break;
        }
        typed[position] = 1;
        if token.is_punct(b'(')
            || token.is_punct(b'{')
            || token.is_punct(b'[')
            || token.is_punct(b'<')
        {
            depth += 1;
        } else if token.is_punct(b')')
            || token.is_punct(b'}')
            || token.is_punct(b']')
            || token.is_punct(b'>')
        {
            depth -= 1;
        }
        position += 1;
    }
    position
}

fn skip_default(tokens: &[Token], start: usize, end: usize) -> usize {
    let mut position = start;
    let mut depth = 0i32;
    while position < end && position < tokens.len() {
        let token = &tokens[position];
        if depth == 0 && (token.is_punct(b',') || token.is_punct(b'}') || token.is_punct(b']')) {
            break;
        }
        if token.is_punct(b'(') || token.is_punct(b'{') || token.is_punct(b'[') {
            depth += 1;
        } else if token.is_punct(b')') || token.is_punct(b'}') || token.is_punct(b']') {
            depth -= 1;
        }
        position += 1;
    }
    position
}

fn stops_pattern(source: &str, token: &Token) -> bool {
    token.is_punct(b'=')
        || token.is_punct(b';')
        || token.is_punct(b')')
        || is_word_token(source, token, "of")
        || is_word_token(source, token, "in")
}

fn bind_token(
    source: &str,
    tokens: &[Token],
    position: usize,
    scope: usize,
    scopes: &mut [Scope],
    binding: &mut [u8],
) {
    bind_name(
        tokens[position].text(source),
        position,
        &mut scopes[scope].bindings,
        binding,
    );
}

fn bind_name(name: &str, position: usize, names: &mut Vec<CompactString>, binding: &mut [u8]) {
    if is_keyword(name) {
        return;
    }
    binding[position] = 1;
    names.push(CompactString::from(name));
}

fn is_lexical_declaration(source: &str, tokens: &[Token], position: usize) -> bool {
    if !(is_word(source, tokens, position, "const")
        || is_word(source, tokens, position, "let")
        || is_word(source, tokens, position, "var"))
    {
        return false;
    }
    starts_statement(tokens, position)
}

fn is_word(source: &str, tokens: &[Token], position: usize, word: &str) -> bool {
    tokens
        .get(position)
        .is_some_and(|token| is_word_token(source, token, word))
}

fn is_word_token(source: &str, token: &Token, word: &str) -> bool {
    token.kind == TokenKind::Ident && token.text(source) == word
}

fn is_keyword(text: &str) -> bool {
    matches!(
        text,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "of"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "undefined"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "async"
            | "from"
            | "as"
            | "get"
            | "set"
            | "hook"
            | "component"
            | "type"
            | "declare"
    )
}

fn export_index(export: &str) -> Option<usize> {
    let rest = export.strip_prefix("$$ufAction")?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn export_name(index: usize) -> String {
    uf_infra::into_string(uf_infra::cstr!("$$ufAction{index}"))
}

fn json_hex(id: &str) -> Result<String, RewriteFailure> {
    if id.len() != crate::ACTION_ID_HEX_LEN || !id.bytes().all(is_lower_hex) {
        return Err(fail("an action id is not lowercase hex"));
    }
    let mut out = String::with_capacity(id.len() + 2);
    out.push('"');
    out.push_str(id);
    out.push('"');
    Ok(out)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn capture_list(names: &[CompactString]) -> String {
    let mut out = String::from("[");
    for (position, name) in names.iter().enumerate() {
        if position > 0 {
            out.push_str(", ");
        }
        out.push_str(name);
    }
    out.push(']');
    out
}

fn apply(source: &str, base: usize, edits: &[(usize, usize, String)]) -> String {
    let mut order: Vec<usize> = (0..edits.len()).collect();
    order.sort_by(|&left, &right| edits[right].0.cmp(&edits[left].0));
    let mut output = source.to_owned();
    for index in order {
        let (start, end, text) = &edits[index];
        if *start < base
            || *end < *start
            || *end - base > output.len()
            || *start - base > output.len()
        {
            continue;
        }
        output.replace_range(*start - base..*end - base, text);
    }
    output
}

fn fail(detail: &'static str) -> RewriteFailure {
    RewriteFailure { detail }
}

fn invalid(path: &Utf8Path, detail: &str) -> RscError {
    RscError::Read {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, detail.to_owned()),
    }
}
