//! Shared machinery for the formatter's guarantees: a location-free view
//! of a parse tree, and the comment multiset of a source.
//!
//! The tree comparison is the interesting one. The port's AST derives
//! `Serialize`, so the cheapest faithful "structural equality ignoring
//! locations" is to serialize both trees to JSON and normalize away
//! everything the formatter is allowed to change:
//!
//! * **locations** — every `Loc` serializes as `[bool, {line, column},
//!   {line, column}]`, which nothing else in the AST looks like, so it is
//!   replaced by a marker string. A marker rather than `null` because
//!   `Option<Loc>` fields carry meaning (`has_unknown_members` is the `...`
//!   of an inexact enum) and `Some`/`None` must stay distinguishable.
//! * **comments** — attachment is the printer's business, and the comment
//!   *multiset* is checked separately.
//! * **`raw` spellings** — requoting a string and normalising a number
//!   rewrite them on purpose; the `value` beside them is what must match.
//! * **trailing commas** — the printer decides those from the layout.
//! * **regex flags** — reordered, so they are compared as a sorted set.
//! * **empty statements** — a lone `;` is dropped, as Prettier drops it.
//! * **the shape of a logical chain** — `a && (b && c)` prints as
//!   `a && b && c`, because the parentheses are redundant and Prettier drops
//!   them too. `&&`, `||` and `??` each evaluate the same operands in the
//!   same order and short-circuit at the same one however they associate, so
//!   a chain of one operator is compared as the flat sequence of its
//!   operands. Only those three: `(a + b) + c` and `a + (b + c)` differ for
//!   strings and for floats, and Prettier keeps those parentheses.
//! * **quoted property keys** — `{ "a": 1 }` is the same property as
//!   `{ a: 1 }`, and the printer drops quotes it does not need.
//! * **a redundant specifier alias** — `export {x as x}` is `export {x}`,
//!   and the printer drops the `as`. React writes the long form, which is
//!   how this was found.
//! * **an absent `new` argument list** — `new Thing` is printed
//!   `new Thing()`, which parses to an empty list rather than to none.
//! * **JSX children** — Prettier moves whitespace between text nodes and
//!   `{" "}` containers, so children are reduced to the sequence React
//!   itself would render: Babel's `cleanJSXElementLiteralChild`, with
//!   `{" "}` read as the text it stands for and adjacent text merged.
//!
//! What survives is everything that decides what the program *does*, which
//! is exactly what a formatter may not change.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use uf_flow::ast::CommentKind;

/// Keys whose values the formatter is allowed to rewrite.
const DROPPED_KEYS: &[&str] = &["comments", "all_comments", "raw", "trailing_comma"];

/// The marker a location is replaced by.
const LOC: &str = "<loc>";

/// A location-free, comment-free rendering of the tree `source` parses to.
///
/// # Panics
///
/// Panics when `source` does not parse; callers pass sources they have
/// already formatted, so a parse failure is a bug in the printer.
pub fn structure(source: &str) -> String {
    // On a thread of its own, with the stack the parser gets. `to_value`
    // and `normalize` both recurse once per AST level, and AST depth is not
    // bracket depth: a `yargs.usage().default().describe()…` chain is one
    // node per link and no brackets at all. fbt's `collectFbt.js` is 10 KB
    // of exactly that, and it overflows a default test thread in a debug
    // build. The formatter has the same shape of problem for real, which is
    // ubugeeei-prod/uf#136; this is only the helper getting out of its way.
    let source = source.to_owned();
    std::thread::Builder::new()
        .name("uf-fmt-structure".to_owned())
        .stack_size(uf_flow::PARSE_STACK_BYTES)
        .spawn(move || structure_here(&source))
        .expect("spawns")
        .join()
        .expect("the tree renders")
}

fn structure_here(source: &str) -> String {
    let parsed = uf_flow::parse(source).unwrap_or_else(|error| panic!("parses: {error}"));
    assert!(
        parsed.is_ok(),
        "structure() needs a clean parse, got {:?}",
        parsed.diagnostics
    );
    let value = serde_json::to_value(&parsed.program).expect("the AST serializes");
    // Pretty-printed so a mismatch shows as a readable line diff rather
    // than one very long line.
    serde_json::to_string_pretty(&normalize(value)).expect("the tree renders")
}

/// Every comment in `source`, in source order, as `(is_line, text)`.
///
/// # Panics
///
/// Panics when `source` does not parse.
pub fn comments(source: &str) -> Vec<(bool, String)> {
    let parsed = uf_flow::parse(source).unwrap_or_else(|error| panic!("parses: {error}"));
    parsed
        .comments()
        .iter()
        .map(|comment| {
            (
                matches!(comment.kind, CommentKind::Line),
                normalize_comment(&comment.text),
            )
        })
        .collect()
}

/// A comment's content, with the whitespace the printer owns taken out.
///
/// Trailing whitespace on any line is not content. Neither is the *leading*
/// whitespace of a continuation line in a block comment: re-indenting
///
/// ```text
///     /* $FlowFixMe[incompatible-type] Error exposed after fixing this
///      * typing unsoundness in flow */
/// ```
///
/// when the code around it moves is what a formatter is for, and Prettier
/// does the same. Relay has eleven of these in one file, which is how the
/// omission was found; the words are identical and only the column moved.
///
/// Only lines that continue a `*` column are touched, so an ASCII diagram
/// or a code sample indented inside a comment still counts as content and
/// is still compared exactly.
///
/// The whitespace before a closing `*/` is the printer's too. A comment
/// whose last line is written flush left
///
/// ```text
///     /* The entry point to start up the debugger CLI
///      * Reads in command line arguments and starts up a UISession
///     */
/// ```
///
/// is re-indented to ` */`, which turns the empty run before the delimiter
/// into a space — content that was not there. Prepack has three of these
/// and each looked like a rewritten comment. A trailing line with nothing
/// on it is dropped rather than compared.
fn normalize_comment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() > 1 && lines.last().is_some_and(|last| last.trim().is_empty()) {
        lines.pop();
    }
    for (index, line) in lines.into_iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let trimmed = line.trim_end();
        if index > 0 && trimmed.trim_start().starts_with('*') {
            out.push_str(trimmed.trim_start());
        } else {
            out.push_str(trimmed);
        }
    }
    out
}

/// The comments of `source` as a multiset, for comparing two files whose
/// comment *order* may legitimately differ (a trailing comment that moves
/// onto the next line).
pub fn comment_multiset(source: &str) -> BTreeMap<(bool, String), usize> {
    let mut counts = BTreeMap::new();
    for comment in comments(source) {
        *counts.entry(comment).or_insert(0) += 1;
    }
    counts
}

fn normalize(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            if is_loc(&items) {
                return Value::String(LOC.to_string());
            }
            if is_jsx_children(&items) {
                return Value::Array(normalize_jsx_children(items));
            }
            // A lone `;` is not a statement anyone wrote on purpose, and
            // the printer drops it, as Prettier does.
            Value::Array(
                items
                    .into_iter()
                    .filter(|item| !is_empty_statement(item))
                    .map(normalize)
                    .collect(),
            )
        }
        Value::Object(fields) if is_comment_container(&fields) => {
            // A node whose *only* content is its comments — a `null`
            // literal is one — carries the container inline rather than
            // under a `comments` key.
            Value::Null
        }
        Value::Object(fields) if is_logical(&fields) => {
            // A chain of one logical operator, as the flat list of its
            // operands. `a && (b && c)` and `(a && b) && c` are the same
            // program and print the same, so they compare equal here.
            let mut operator = String::new();
            let mut operands = Vec::new();
            flatten_logical(Value::Object(fields), &mut operator, &mut operands);
            let mut chain = Map::new();
            chain.insert("operator".to_owned(), Value::String(operator));
            chain.insert("operands".to_owned(), Value::Array(operands));
            let mut out = Map::new();
            out.insert("LogicalChain".to_owned(), Value::Object(chain));
            Value::Object(out)
        }
        Value::Object(fields) => {
            // `export {x as x}` and `export {x}` are the same specifier, and
            // so are the two spellings of an import. The parser records the
            // alias as `Some` in one and `None` in the other; the printer
            // writes the short form, which is not a change to the program.
            let fields = normalize_specifier(fields);
            let mut out = Map::with_capacity(fields.len());
            for (key, field) in fields {
                if DROPPED_KEYS.contains(&key.as_str()) {
                    continue;
                }
                // `new Thing` and `new Thing()` are the same expression;
                // the printer always writes the parentheses, as Prettier
                // does.
                if key == "arguments" && field.is_null() {
                    let mut empty = Map::new();
                    empty.insert("loc".to_string(), Value::String(LOC.to_string()));
                    empty.insert("arguments".to_string(), Value::Array(Vec::new()));
                    out.insert(key, Value::Object(empty));
                    continue;
                }
                // `{ "a": 1 }` and `{ a: 1 }` are the same property, and
                // the printer drops quotes it does not need, so an
                // identifier-shaped key is compared by its name.
                if key == "key"
                    && let Some(name) = identifier_key_name(&field)
                {
                    out.insert(key, Value::String(format!("key:{name}")));
                    continue;
                }
                let field = if key == "flags" {
                    match field.as_str() {
                        Some(flags) => {
                            let mut flags: Vec<char> = flags.chars().collect();
                            flags.sort_unstable();
                            Value::String(flags.into_iter().collect())
                        }
                        None => normalize(field),
                    }
                } else {
                    normalize(field)
                };
                out.insert(key, field);
            }
            Value::Object(out)
        }
        other => other,
    }
}

/// Whether `fields` is a serialized `Syntax`: the comment container.
fn is_comment_container(fields: &Map<String, Value>) -> bool {
    fields.len() == 3
        && fields.contains_key("leading")
        && fields.contains_key("trailing")
        && fields.contains_key("internal")
}

/// Whether `value` is an empty statement: a lone `;`.
fn is_empty_statement(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|fields| fields.len() == 1 && fields.contains_key("Empty"))
}

/// The name of a property key that is an identifier, or a string literal
/// spelling one.
fn identifier_key_name(value: &Value) -> Option<&str> {
    let fields = value.as_object()?;
    if fields.len() != 1 {
        return None;
    }
    let name = match fields.iter().next()? {
        (tag, value) if tag == "Identifier" => value.get("name")?.as_str()?,
        (tag, value) if tag == "StringLiteral" => value.get(1)?.get("value")?.as_str()?,
        _ => return None,
    };
    is_identifier_name(name).then_some(name)
}

/// Whether `name` can be written without quotes.
fn is_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first == '$' || first == '_' || first.is_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '$' || ch == '_' || ch.is_alphanumeric())
}

/// Whether `items` is a serialized `Loc`: `[bool, position, position]`.
fn is_loc(items: &[Value]) -> bool {
    items.len() == 3 && items[0].is_boolean() && is_position(&items[1]) && is_position(&items[2])
}

fn is_position(value: &Value) -> bool {
    match value.as_object() {
        Some(fields) => {
            fields.len() == 2 && fields.contains_key("line") && fields.contains_key("column")
        }
        None => false,
    }
}

/// Whether `items` is a list of JSX children: every entry is an
/// externally tagged enum with one of the child variant names.
fn is_jsx_children(items: &[Value]) -> bool {
    !items.is_empty()
        && items.iter().all(|item| {
            item.as_object().is_some_and(|fields| {
                fields.len() == 1
                    && fields.keys().next().is_some_and(|key| {
                        matches!(
                            key.as_str(),
                            "Text" | "Element" | "Fragment" | "ExpressionContainer" | "SpreadChild"
                        )
                    })
            })
        })
        && items.iter().any(|item| {
            item.as_object()
                .is_some_and(|fields| fields.contains_key("Text"))
        })
}

/// Reduce JSX children to what React renders: text cleaned the way Babel
/// cleans it, `{" "}` read as a space, adjacent text merged, and empty
/// text dropped.
/// Drops a specifier's alias when it names the same thing as the local
/// binding.
///
/// Both halves are `Identifier` nodes, so they are compared by `name` —
/// their locations differ and are about to be replaced anyway.
fn normalize_specifier(mut fields: Map<String, Value>) -> Map<String, Value> {
    for alias in ["exported", "imported", "remote"] {
        let Some(local) = fields.get("local").and_then(identifier_name) else {
            continue;
        };
        let Some(other) = fields.get(alias).and_then(identifier_name) else {
            continue;
        };
        if local == other {
            fields.insert(alias.to_string(), Value::Null);
        }
    }
    fields
}

/// The `name` of a node, when it is an identifier.
fn identifier_name(value: &Value) -> Option<&str> {
    value.get("name").and_then(Value::as_str)
}

fn normalize_jsx_children(items: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        let text = jsx_text_of(&item);
        match text {
            Some(text) => {
                let cleaned = clean_jsx_text(&text);
                if cleaned.is_empty() {
                    continue;
                }
                if let Some(Value::String(previous)) = out.last_mut() {
                    previous.push_str(&cleaned);
                    continue;
                }
                out.push(Value::String(cleaned));
            }
            None => out.push(normalize(item)),
        }
    }
    out
}

/// The text a child stands for: its own, or the `" "` of a `{" "}`.
fn jsx_text_of(item: &Value) -> Option<String> {
    let fields = item.as_object()?;
    if let Some(text) = fields.get("Text") {
        return Some(text.get("inner")?.get("value")?.as_str()?.to_string());
    }
    let container = fields.get("ExpressionContainer")?;
    let expression = container
        .get("inner")?
        .get("expression")?
        .get("Expression")?;
    let literal = expression.get("StringLiteral")?.get("inner")?;
    let value = literal.get("value")?.as_str()?;
    if value == " " {
        Some(" ".to_string())
    } else {
        None
    }
}

/// Babel's `cleanJSXElementLiteralChild`: drop whitespace-only lines, trim
/// the ends of every line that touches a newline, and join what is left
/// with single spaces.
fn clean_jsx_text(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut line = *line;
        if index > 0 {
            line = line.trim_start();
        }
        if index < last {
            line = line.trim_end();
        }
        if !line.is_empty() {
            kept.push(line);
        }
    }
    kept.join(" ")
}

/// Whether a serialized node is a `Logical`.
fn is_logical(fields: &Map<String, Value>) -> bool {
    fields.len() == 1 && fields.contains_key("Logical")
}

/// The operator and operands of a logical chain, normalised.
///
/// Recurses through both sides for as long as the operator is the same, so
/// `a && (b && c)`, `(a && b) && c` and `a && b && c` all reduce to
/// `And [a, b, c]`. A different operator ends the chain and is normalised as
/// an operand in its own right, which keeps `a && (b || c)` distinct from
/// `(a && b) || c`.
fn flatten_logical(value: Value, operator: &mut String, operands: &mut Vec<Value>) {
    let Value::Object(fields) = value else {
        operands.push(normalize(value));
        return;
    };
    if !is_logical(&fields) {
        operands.push(normalize(Value::Object(fields)));
        return;
    }
    let Some(Value::Object(node)) = fields.get("Logical") else {
        operands.push(normalize(Value::Object(fields)));
        return;
    };
    let here = match node.get("inner").and_then(|inner| inner.get("operator")) {
        Some(Value::String(name)) => name.clone(),
        _ => {
            operands.push(normalize(Value::Object(fields)));
            return;
        }
    };
    if operator.is_empty() {
        *operator = here;
    } else if *operator != here {
        operands.push(normalize(Value::Object(fields)));
        return;
    }
    let Some(Value::Object(inner)) = node.get("inner") else {
        operands.push(normalize(Value::Object(fields)));
        return;
    };
    for side in ["left", "right"] {
        match inner.get(side) {
            Some(value) => flatten_logical(value.clone(), operator, operands),
            None => operands.push(Value::Null),
        }
    }
}
