//! Single-pass token scanning for `.js` sources.
//!
//! The RSC analyses need four things out of a module: its directive prologue,
//! its import specifiers, its exported bindings, and whether a server module
//! reaches for a client-only API. All four are answered from one token vector
//! produced by [`tokenize`], so a module is scanned exactly once.
//!
//! The scanner is deliberately a lexer and not a parser: it understands string,
//! template, regular-expression and comment boundaries (so a `"use client"`
//! inside a comment or a string is never mistaken for a directive) but it does
//! not build an AST. Every place where that costs precision is documented at the
//! function that pays the cost.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use uf_infra::{InlineVec, LineIndex};

/// Inline list of import specifiers belonging to one module.
pub type ImportList = InlineVec<ImportSpecifier, 8>;

/// Inline list of exported bindings belonging to one module.
pub type ExportList = InlineVec<ModuleExport, 8>;

/// Inline list of client-only API uses found in one module.
pub type ClientApiUseList = InlineVec<ClientApiUse, 4>;

/// Longest source accepted by the scanner, in bytes.
///
/// Guards against unbounded allocation when a hostile or generated file is fed
/// to `uf build`; larger files are reported as unscannable rather than parsed.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// React APIs that only exist inside the client bundle.
///
/// A Server Component that calls one of these fails at render time, which is the
/// error class Next.js surfaces as "This React hook only works in a client
/// component". The list is sorted so lookups can binary search it.
pub const CLIENT_ONLY_APIS: &[&str] = &[
    "createContext",
    "useActionState",
    "useCallback",
    "useContext",
    "useDebugValue",
    "useDeferredValue",
    "useEffect",
    "useFormStatus",
    "useImperativeHandle",
    "useInsertionEffect",
    "useLayoutEffect",
    "useMemo",
    "useOptimistic",
    "useReducer",
    "useRef",
    "useState",
    "useSyncExternalStore",
    "useTransition",
];

/// Browser globals that only exist inside the client bundle.
///
/// Sorted for binary search.
pub const CLIENT_ONLY_GLOBALS: &[&str] = &[
    "alert",
    "document",
    "localStorage",
    "navigator",
    "sessionStorage",
    "window",
];

/// How a module reached another module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportKind {
    /// `import x from "..."` or a bare `import "..."`.
    Static,
    /// `export { x } from "..."` or `export * from "..."`.
    ReExport,
    /// `import("...")`.
    Dynamic,
    /// `require("...")`.
    Require,
}

/// One import specifier as written in the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpecifier {
    /// The specifier text exactly as written, without quotes.
    pub specifier: CompactString,
    /// Syntactic form the specifier appeared in.
    pub kind: ImportKind,
    /// 1-based line the specifier appeared on.
    pub line: u32,
}

/// Shape of an exported binding, as far as a lexer can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportKind {
    /// `export async function f()`, `export const f = async () => {}`.
    AsyncFunction,
    /// `export function f()`, `export const f = () => {}`, `export hook useF()`.
    SyncFunction,
    /// `export class C {}`.
    Class,
    /// Any other initializer: a literal, an object, a call result.
    Value,
    /// `export { x } from "./other.js"`, where the shape lives in another module.
    ReExport,
}

impl ExportKind {
    /// Whether the export is callable at all.
    pub fn is_function(self) -> bool {
        matches!(self, Self::AsyncFunction | Self::SyncFunction)
    }
}

/// One exported binding of a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleExport {
    /// Exported name; `default` for a default export.
    pub name: CompactString,
    /// Shape of the exported binding.
    pub kind: ExportKind,
    /// 1-based line the export was declared on.
    pub line: u32,
}

/// One use of a client-only API inside a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientApiUse {
    /// The API name, borrowed from [`CLIENT_ONLY_APIS`] or [`CLIENT_ONLY_GLOBALS`].
    pub api: &'static str,
    /// 1-based line of the use.
    pub line: u32,
    /// 1-based column of the use.
    pub column: u32,
}

/// Collect the import specifiers of a module.
pub fn scan_imports(source: &str) -> ImportList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    imports_from_tokens(source, &tokens, &index)
}

/// Collect the exported bindings of a module.
pub fn scan_exports(source: &str) -> ExportList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    exports_from_tokens(source, &tokens, &index)
}

/// Collect client-only API uses inside a module.
pub fn scan_client_api_uses(source: &str) -> ClientApiUseList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    client_api_uses_from_tokens(source, &tokens, &index)
}

/// Clamp a `usize` position into the `u32` used by diagnostics.
pub(crate) fn clamp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    String,
    Template,
    Number,
    Regex,
    /// `=>`, lexed as one token so it is never confused with `=` followed by `>`.
    Arrow,
    /// A single punctuation byte.
    Punct(u8),
    /// An unterminated string, template or comment.
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
    /// Whether a line terminator separates this token from the previous one.
    pub newline_before: bool,
}

impl Token {
    pub(crate) fn text<'a>(&self, source: &'a str) -> &'a str {
        source.get(self.start..self.end).unwrap_or_default()
    }

    /// Content of a string or template token, without the surrounding quotes.
    pub(crate) fn quoted_content<'a>(&self, source: &'a str) -> &'a str {
        if self.end < self.start + 2 {
            return "";
        }
        source.get(self.start + 1..self.end - 1).unwrap_or_default()
    }

    pub(crate) fn is_punct(&self, byte: u8) -> bool {
        self.kind == TokenKind::Punct(byte)
    }
}

/// Tokenize a module, skipping a BOM and a leading `#!` line.
pub(crate) fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    if bytes.len() > MAX_SOURCE_BYTES {
        return Vec::new();
    }

    let mut tokens: Vec<Token> = Vec::with_capacity(bytes.len() / 6 + 8);
    let mut cursor = skip_bom(bytes);
    let mut newline_before = false;

    if bytes[cursor..].starts_with(b"#!") {
        cursor = line_end(bytes, cursor);
    }

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(next) = line_terminator_width(bytes, cursor) {
            newline_before = true;
            cursor += next;
            continue;
        }
        if let Some(width) = whitespace_width(bytes, cursor) {
            cursor += width;
            continue;
        }
        if byte == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
            cursor = line_end(bytes, cursor);
            continue;
        }
        if byte == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            let (end, saw_newline) = block_comment_end(bytes, cursor);
            newline_before |= saw_newline;
            cursor = end;
            continue;
        }

        let start = cursor;
        let kind = lex_token(bytes, &mut cursor, tokens.last(), source);
        tokens.push(Token {
            kind,
            start,
            end: cursor,
            newline_before,
        });
        newline_before = false;
    }

    tokens
}

fn skip_bom(bytes: &[u8]) -> usize {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    }
}

/// Width in bytes of a line terminator at `at`, if there is one.
///
/// Handles LF, CR, CRLF and the two non-ASCII terminators U+2028 and U+2029.
fn line_terminator_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b'\n' => Some(1),
        b'\r' => {
            if bytes.get(at + 1) == Some(&b'\n') {
                Some(2)
            } else {
                Some(1)
            }
        }
        0xe2 if bytes.get(at + 1) == Some(&0x80)
            && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
        {
            Some(3)
        }
        _ => None,
    }
}

/// Width in bytes of non-line-terminator whitespace at `at`, if there is any.
fn whitespace_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b' ' | b'\t' | 0x0b | 0x0c => Some(1),
        // U+00A0 NO-BREAK SPACE.
        0xc2 if bytes.get(at + 1) == Some(&0xa0) => Some(2),
        // U+FEFF ZERO WIDTH NO-BREAK SPACE, legal whitespace away from the BOM.
        0xef if bytes.get(at + 1) == Some(&0xbb) && bytes.get(at + 2) == Some(&0xbf) => Some(3),
        _ => None,
    }
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut cursor = from;
    while cursor < bytes.len() && line_terminator_width(bytes, cursor).is_none() {
        cursor += 1;
    }
    cursor
}

fn block_comment_end(bytes: &[u8], from: usize) -> (usize, bool) {
    let mut cursor = from + 2;
    let mut saw_newline = false;
    while cursor < bytes.len() {
        if line_terminator_width(bytes, cursor).is_some() {
            saw_newline = true;
        }
        if bytes[cursor] == b'*' && bytes.get(cursor + 1) == Some(&b'/') {
            return (cursor + 2, saw_newline);
        }
        cursor += 1;
    }
    (bytes.len(), saw_newline)
}

fn lex_token(bytes: &[u8], cursor: &mut usize, prev: Option<&Token>, source: &str) -> TokenKind {
    let byte = bytes[*cursor];
    match byte {
        b'"' | b'\'' => lex_quoted(bytes, cursor, byte),
        b'`' => lex_template(bytes, cursor),
        b'0'..=b'9' => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'.' if matches!(bytes.get(*cursor + 1), Some(b'0'..=b'9')) => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'/' if regex_allowed(prev, source) => match lex_regex(bytes, cursor) {
            true => TokenKind::Regex,
            false => {
                *cursor += 1;
                TokenKind::Punct(b'/')
            }
        },
        b'=' if bytes.get(*cursor + 1) == Some(&b'>') => {
            *cursor += 2;
            TokenKind::Arrow
        }
        _ if is_ident_start(byte) => {
            *cursor += 1;
            while *cursor < bytes.len() && is_ident_part(bytes[*cursor]) {
                *cursor += 1;
            }
            TokenKind::Ident
        }
        _ => {
            *cursor += 1;
            TokenKind::Punct(byte)
        }
    }
}

fn lex_quoted(bytes: &[u8], cursor: &mut usize, quote: u8) -> TokenKind {
    let mut at = *cursor + 1;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if byte == quote {
            *cursor = at + 1;
            return TokenKind::String;
        }
        if line_terminator_width(bytes, at).is_some() {
            break;
        }
        at += 1;
    }
    *cursor = at.min(bytes.len());
    TokenKind::Invalid
}

fn lex_template(bytes: &[u8], cursor: &mut usize) -> TokenKind {
    let mut at = *cursor + 1;
    let mut depth = 0usize;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if depth == 0 && byte == b'`' {
            *cursor = at + 1;
            return TokenKind::Template;
        }
        if byte == b'$' && bytes.get(at + 1) == Some(&b'{') {
            depth += 1;
            at += 2;
            continue;
        }
        if depth > 0 && byte == b'}' {
            depth -= 1;
        }
        at += 1;
    }
    *cursor = bytes.len();
    TokenKind::Invalid
}

fn lex_number(bytes: &[u8], cursor: &mut usize) {
    let mut at = *cursor;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
            at += 1;
            continue;
        }
        if (byte == b'+' || byte == b'-') && at > *cursor {
            let previous = bytes[at - 1];
            if previous == b'e' || previous == b'E' {
                at += 1;
                continue;
            }
        }
        break;
    }
    *cursor = at;
}

/// Consume a regular-expression literal, returning whether one was found.
fn lex_regex(bytes: &[u8], cursor: &mut usize) -> bool {
    let mut at = *cursor + 1;
    let mut in_class = false;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if line_terminator_width(bytes, at).is_some() {
            return false;
        }
        match byte {
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => {
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                *cursor = at;
                return true;
            }
            _ => {}
        }
        at += 1;
    }
    false
}

/// Whether a `/` at this position starts a regular expression rather than a division.
fn regex_allowed(prev: Option<&Token>, source: &str) -> bool {
    let Some(prev) = prev else {
        return true;
    };
    match prev.kind {
        TokenKind::Arrow => true,
        TokenKind::Ident => matches!(
            prev.text(source),
            "await"
                | "case"
                | "delete"
                | "do"
                | "else"
                | "in"
                | "instanceof"
                | "new"
                | "of"
                | "return"
                | "throw"
                | "typeof"
                | "void"
                | "yield"
        ),
        TokenKind::Punct(byte) => !matches!(byte, b')' | b']' | b'}'),
        _ => false,
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

fn is_ident_part(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

pub(crate) fn imports_from_tokens(source: &str, tokens: &[Token], index: &LineIndex) -> ImportList {
    let mut imports = ImportList::new();

    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        match token.text(source) {
            "import" => {
                if !starts_statement(tokens, position) && !is_dynamic_import(tokens, position) {
                    continue;
                }
                if let Some(next) = tokens.get(position + 1) {
                    if next.kind == TokenKind::String {
                        push_specifier(&mut imports, source, next, ImportKind::Static, index);
                        continue;
                    }
                    if next.is_punct(b'(') {
                        if let Some(literal) = tokens.get(position + 2)
                            && literal.kind == TokenKind::String
                        {
                            push_specifier(
                                &mut imports,
                                source,
                                literal,
                                ImportKind::Dynamic,
                                index,
                            );
                        }
                        continue;
                    }
                }
                if let Some(literal) = find_from_clause(source, tokens, position) {
                    push_specifier(&mut imports, source, literal, ImportKind::Static, index);
                }
            }
            "export" => {
                if !starts_statement(tokens, position) {
                    continue;
                }
                if let Some(literal) = find_from_clause(source, tokens, position) {
                    push_specifier(&mut imports, source, literal, ImportKind::ReExport, index);
                }
            }
            "require" => {
                let Some(open) = tokens.get(position + 1) else {
                    continue;
                };
                if !open.is_punct(b'(') {
                    continue;
                }
                if let Some(literal) = tokens.get(position + 2)
                    && literal.kind == TokenKind::String
                {
                    push_specifier(&mut imports, source, literal, ImportKind::Require, index);
                }
            }
            _ => {}
        }
    }

    imports
}

fn push_specifier(
    imports: &mut ImportList,
    source: &str,
    literal: &Token,
    kind: ImportKind,
    index: &LineIndex,
) {
    let position = index.line_col(literal.start);
    imports.push(ImportSpecifier {
        specifier: CompactString::from(literal.quoted_content(source)),
        kind,
        line: clamp_u32(position.line),
    });
}

/// Find the string literal of a `from "..."` clause started at `position`.
fn find_from_clause<'a>(source: &str, tokens: &'a [Token], position: usize) -> Option<&'a Token> {
    let mut at = position + 1;
    let limit = (position + 512).min(tokens.len());
    while at < limit {
        let token = &tokens[at];
        if token.is_punct(b';') {
            return None;
        }
        if token.kind == TokenKind::Ident && token.text(source) == "from" {
            let literal = tokens.get(at + 1)?;
            return (literal.kind == TokenKind::String).then_some(literal);
        }
        at += 1;
    }
    None
}

fn is_dynamic_import(tokens: &[Token], position: usize) -> bool {
    tokens
        .get(position + 1)
        .is_some_and(|token| token.is_punct(b'('))
}

/// Whether the token at `position` begins a statement.
pub(crate) fn starts_statement(tokens: &[Token], position: usize) -> bool {
    match position.checked_sub(1) {
        None => true,
        Some(previous) => {
            let token = &tokens[previous];
            token.is_punct(b';') || token.is_punct(b'{') || token.is_punct(b'}')
        }
    }
}

pub(crate) fn exports_from_tokens(source: &str, tokens: &[Token], index: &LineIndex) -> ExportList {
    let mut exports = ExportList::new();
    let locals = local_bindings(source, tokens);

    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident || token.text(source) != "export" {
            continue;
        }
        if !starts_statement(tokens, position) {
            continue;
        }
        let line = clamp_u32(index.line_col(token.start).line);
        collect_export(source, tokens, position, line, &locals, &mut exports);
    }

    exports
}

fn collect_export(
    source: &str,
    tokens: &[Token],
    position: usize,
    line: u32,
    locals: &[(CompactString, ExportKind)],
    exports: &mut ExportList,
) {
    let Some(next) = tokens.get(position + 1) else {
        return;
    };

    if next.is_punct(b'*') {
        return;
    }

    if next.is_punct(b'{') {
        let re_export = find_from_clause(source, tokens, position).is_some();
        let mut at = position + 2;
        let mut pending: Option<&Token> = None;
        while at < tokens.len() && !tokens[at].is_punct(b'}') {
            let token = &tokens[at];
            if token.kind == TokenKind::Ident {
                if token.text(source) == "as" {
                    pending = None;
                    at += 1;
                    continue;
                }
                pending = Some(token);
            }
            if token.is_punct(b',')
                && let Some(name) = pending.take()
            {
                push_named_export(source, name, line, re_export, locals, exports);
            }
            at += 1;
        }
        if let Some(name) = pending.take() {
            push_named_export(source, name, line, re_export, locals, exports);
        }
        return;
    }

    if next.kind != TokenKind::Ident {
        return;
    }

    match next.text(source) {
        "default" => {
            let kind = initializer_kind(source, tokens, position + 2);
            exports.push(ModuleExport {
                name: CompactString::const_new("default"),
                kind,
                line,
            });
        }
        "type" | "interface" | "opaque" | "declare" | "enum" => {}
        "async" => {
            if let Some(name) = declaration_name(source, tokens, position + 3) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::AsyncFunction,
                    line,
                });
            }
        }
        "function" | "hook" | "component" => {
            if let Some(name) = declaration_name(source, tokens, position + 2) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::SyncFunction,
                    line,
                });
            }
        }
        "class" => {
            if let Some(name) = declaration_name(source, tokens, position + 2) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::Class,
                    line,
                });
            }
        }
        "const" | "let" | "var" => {
            collect_declarators(source, tokens, position + 2, line, exports);
        }
        _ => {}
    }
}

fn push_named_export(
    source: &str,
    name: &Token,
    line: u32,
    re_export: bool,
    locals: &[(CompactString, ExportKind)],
    exports: &mut ExportList,
) {
    let text = name.text(source);
    let kind = if re_export {
        ExportKind::ReExport
    } else {
        locals
            .iter()
            .find(|(local, _)| local == text)
            .map_or(ExportKind::Value, |(_, kind)| *kind)
    };
    exports.push(ModuleExport {
        name: CompactString::from(text),
        kind,
        line,
    });
}

fn declaration_name(source: &str, tokens: &[Token], position: usize) -> Option<CompactString> {
    let token = tokens.get(position)?;
    (token.kind == TokenKind::Ident).then(|| CompactString::from(token.text(source)))
}

/// Walk the declarators of a `const a = 1, b = 2;` statement.
///
/// Destructuring patterns bind names the lexer cannot attribute to an
/// initializer, so they are skipped rather than guessed at.
fn collect_declarators(
    source: &str,
    tokens: &[Token],
    position: usize,
    line: u32,
    exports: &mut ExportList,
) {
    let mut at = position;
    loop {
        let Some(name_token) = tokens.get(at) else {
            return;
        };
        if name_token.kind != TokenKind::Ident {
            return;
        }
        let name = CompactString::from(name_token.text(source));

        let mut cursor = at + 1;
        let mut depth = 0usize;
        let mut kind = ExportKind::Value;
        let mut assigned = false;
        while cursor < tokens.len() {
            let token = &tokens[cursor];
            match token.kind {
                TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
                TokenKind::Punct(b')' | b']' | b'}') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Punct(b'=') if depth == 0 && !assigned => {
                    assigned = true;
                    kind = initializer_kind(source, tokens, cursor + 1);
                }
                TokenKind::Punct(b',' | b';') if depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }

        exports.push(ModuleExport { name, kind, line });

        match tokens.get(cursor) {
            Some(token) if token.is_punct(b',') => at = cursor + 1,
            _ => return,
        }
    }
}

/// Classify the initializer expression starting at `position`.
///
/// Server-action factories are transparent: `serverAction(async () => {})` is an
/// async function for the purposes of the React server-action contract, so a
/// call expression is classified by its first argument.
fn initializer_kind(source: &str, tokens: &[Token], position: usize) -> ExportKind {
    initializer_kind_inner(source, tokens, position, 0)
}

fn initializer_kind_inner(
    source: &str,
    tokens: &[Token],
    position: usize,
    depth: u32,
) -> ExportKind {
    if depth > 4 {
        return ExportKind::Value;
    }
    let Some(token) = tokens.get(position) else {
        return ExportKind::Value;
    };

    if token.kind == TokenKind::Ident {
        match token.text(source) {
            "async" => {
                return ExportKind::AsyncFunction;
            }
            "function" | "hook" | "component" => return ExportKind::SyncFunction,
            "class" => return ExportKind::Class,
            _ => {}
        }
        if tokens
            .get(position + 1)
            .is_some_and(|next| next.is_punct(b'('))
        {
            return initializer_kind_inner(source, tokens, position + 2, depth + 1);
        }
        if tokens
            .get(position + 1)
            .is_some_and(|next| next.kind == TokenKind::Arrow)
        {
            return ExportKind::SyncFunction;
        }
        return ExportKind::Value;
    }

    if token.is_punct(b'(')
        && let Some(close) = matching_close(tokens, position, b'(', b')')
    {
        // A Flow return type may sit between `)` and `=>`.
        let mut at = close + 1;
        let limit = (close + 64).min(tokens.len());
        while at < limit {
            if tokens[at].kind == TokenKind::Arrow {
                return ExportKind::SyncFunction;
            }
            if tokens[at].is_punct(b';') || tokens[at].is_punct(b',') {
                break;
            }
            at += 1;
        }
    }

    ExportKind::Value
}

/// Index of the token closing the group opened at `position`.
pub(crate) fn matching_close(
    tokens: &[Token],
    position: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 0usize;
    for (at, token) in tokens.iter().enumerate().skip(position) {
        if token.is_punct(open) {
            depth += 1;
        } else if token.is_punct(close) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// Index of the token opening the group closed at `position`.
pub(crate) fn matching_open(
    tokens: &[Token],
    position: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = position;
    loop {
        let token = tokens.get(at)?;
        if token.is_punct(close) {
            depth += 1;
        } else if token.is_punct(open) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
        at = at.checked_sub(1)?;
    }
}

/// Collect top-level `function`/`class`/`const` bindings so `export { foo }` can
/// be classified without a second pass over the source.
fn local_bindings(source: &str, tokens: &[Token]) -> Vec<(CompactString, ExportKind)> {
    let mut locals = Vec::new();
    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident || !starts_statement(tokens, position) {
            continue;
        }
        match token.text(source) {
            "async" => {
                if tokens
                    .get(position + 1)
                    .is_some_and(|next| next.kind == TokenKind::Ident)
                    && let Some(name) = declaration_name(source, tokens, position + 2)
                {
                    locals.push((name, ExportKind::AsyncFunction));
                }
            }
            "function" | "hook" | "component" => {
                if let Some(name) = declaration_name(source, tokens, position + 1) {
                    locals.push((name, ExportKind::SyncFunction));
                }
            }
            "class" => {
                if let Some(name) = declaration_name(source, tokens, position + 1) {
                    locals.push((name, ExportKind::Class));
                }
            }
            "const" | "let" | "var" => {
                let mut declarators = ExportList::new();
                collect_declarators(source, tokens, position + 1, 0, &mut declarators);
                locals.extend(
                    declarators
                        .into_iter()
                        .map(|export| (export.name, export.kind)),
                );
            }
            _ => {}
        }
    }
    locals
}

pub(crate) fn client_api_uses_from_tokens(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
) -> ClientApiUseList {
    let mut uses = ClientApiUseList::new();
    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let text = token.text(source);
        if is_declaration_site(source, tokens, position) {
            continue;
        }

        let matched = if tokens
            .get(position + 1)
            .is_some_and(|next| next.is_punct(b'('))
        {
            CLIENT_ONLY_APIS
                .binary_search(&text)
                .ok()
                .map(|found| CLIENT_ONLY_APIS[found])
        } else {
            None
        };

        let matched = matched.or_else(|| {
            if position
                .checked_sub(1)
                .is_some_and(|previous| tokens[previous].is_punct(b'.'))
            {
                return None;
            }
            CLIENT_ONLY_GLOBALS
                .binary_search(&text)
                .ok()
                .map(|found| CLIENT_ONLY_GLOBALS[found])
        });

        if let Some(api) = matched {
            let position = index.line_col(token.start);
            uses.push(ClientApiUse {
                api,
                line: clamp_u32(position.line),
                column: clamp_u32(position.column),
            });
        }
    }
    uses
}

/// Whether the identifier at `position` is being declared rather than used.
fn is_declaration_site(source: &str, tokens: &[Token], position: usize) -> bool {
    let Some(previous) = position.checked_sub(1) else {
        return false;
    };
    let token = &tokens[previous];
    token.kind == TokenKind::Ident
        && matches!(
            token.text(source),
            "function" | "hook" | "component" | "class" | "const" | "let" | "var" | "import"
        )
}

#[cfg(test)]
mod tests;
