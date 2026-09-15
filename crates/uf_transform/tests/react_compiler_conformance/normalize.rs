//! What "the same output" means.
//!
//! uf's printer is not Babel's generator, and the snapshots were also run
//! through Prettier, so two correct outputs almost never agree byte for byte.
//! Both sides are therefore brought to one form before they are compared, and
//! the form is chosen so that nothing uf is being measured on takes part in
//! making it:
//!
//! * **Code** is parsed by the official Flow parser — the same reading for the
//!   snapshot and for uf's output — rendered as ESTree, and has its types
//!   erased by the `StripFlowTypes` port. The trees are then compared with the
//!   things a printer is free to choose removed: positions, comments, the raw
//!   spelling of a literal (`'a'` against `"a"`), whether `{a}` was written
//!   `{a: a}`, and how JSX children are split into text and `{" "}`
//!   containers — each side's children are reduced to what JSX evaluates them
//!   to, following Babel's `cleanJSXElementLiteralChild`.
//!
//!   Types are erased because uf erases them *before* the compiler runs, and
//!   the snapshots keep them. Where that changes what the compiler does, the
//!   difference is still visible in the code around them.
//!
//!   The printer and `babel.rs` are not part of this: the snapshot never
//!   passes through them, so a bug in either shows up as a difference.
//! * **Errors** are compared as text, which the Rust crate formats to match
//!   `formatCompilerError`, with trailing whitespace dropped and the file name
//!   in `name.ts:3:4` lines replaced, because the test runner and this harness
//!   do not spell the path the same way.
//! * **Logs** are compared as JSON values, without `identifierName` and
//!   `fnLoc` — the two keys upstream's own `compiler/scripts/test-e2e.ts`
//!   removes before it compares the Rust crate's events with the TypeScript
//!   compiler's.

use serde_json::{Map, Value, json};

/// Keys a printer or a parser is free to choose.
const PRINTER_CHOICES: [&str; 11] = [
    "range",
    "loc",
    "start",
    "end",
    "comments",
    "leadingComments",
    "trailingComments",
    "innerComments",
    "raw",
    "shorthand",
    // The Flow parser records whether a list ended in `,`; Prettier adds one.
    "trailingComma",
];

/// A module's code in comparable form.
///
/// # Errors
///
/// What the Flow parser or the type erasure said, when the code does not read.
pub fn code(source: &str) -> Result<Value, String> {
    let (program, _) = uf_transform::lowered_ast(source).map_err(|error| error.to_string())?;
    Ok(canonical(program))
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if PRINTER_CHOICES.contains(&key.as_str()) {
                    continue;
                }
                out.insert(key, canonical(child));
            }
            match out.get("type").and_then(Value::as_str) {
                Some("JSXElement" | "JSXFragment") => {
                    if let Some(Value::Array(children)) = out.remove("children") {
                        out.insert("children".to_owned(), Value::Array(jsx_children(children)));
                    }
                }
                // `{"a": b}` and `{a: b}` name the same property; Prettier's
                // `quoteProps: "as-needed"` removes the quotes. Both spellings
                // become one form, whatever fields each parser gave the key.
                Some("Property" | "PropertyDefinition" | "MethodDefinition")
                    if out.get("computed") == Some(&Value::Bool(false)) =>
                {
                    if let Some(key) = out.get_mut("key") {
                        let name = match key["type"].as_str() {
                            Some("Identifier") => key["name"].as_str().map(str::to_owned),
                            Some("Literal") => key["value"]
                                .as_str()
                                .filter(|name| is_identifier_name(name))
                                .map(str::to_owned),
                            _ => None,
                        };
                        if let Some(name) = name {
                            *key = json!({"type": "PropertyName", "name": name});
                        }
                    }
                }
                // Prettier deletes an empty statement from a statement list,
                // and Babel's generator prints one as `;`.
                Some("Program" | "BlockStatement" | "StaticBlock") => {
                    if let Some(Value::Array(body)) = out.get_mut("body") {
                        body.retain(|statement| statement["type"] != "EmptyStatement");
                    }
                }
                Some("SwitchCase") => {
                    if let Some(Value::Array(body)) = out.get_mut("consequent") {
                        body.retain(|statement| statement["type"] != "EmptyStatement");
                    }
                }
                Some("JSXAttribute") => {
                    // `a={"b"}` and `a="b"` are the same attribute.
                    if let Some(value) = out.get_mut("value")
                        && value["type"] == "JSXExpressionContainer"
                        && value["expression"]["type"] == "Literal"
                        && value["expression"]["value"].is_string()
                    {
                        *value = value["expression"].take();
                    }
                }
                _ => {}
            }
            Value::Object(out)
        }
        other => other,
    }
}

/// Whether `name` can be written as a property key without quotes.
fn is_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first == '$' || first.is_alphabetic())
        && chars.all(|next| next == '_' || next == '$' || next.is_alphanumeric())
}

/// JSX children as the text and elements they evaluate to.
fn jsx_children(children: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::new();
    let mut text: Option<String> = None;
    for child in children {
        let piece = match child["type"].as_str() {
            Some("JSXText") => Some(clean_jsx_text(child["value"].as_str().unwrap_or_default())),
            Some("JSXExpressionContainer") => match child["expression"]["type"].as_str() {
                Some("JSXEmptyExpression") => Some(String::new()),
                Some("Literal") => child["expression"]["value"].as_str().map(str::to_owned),
                _ => None,
            },
            _ => None,
        };
        if let Some(piece) = piece {
            text.get_or_insert_with(String::new).push_str(&piece);
        } else {
            flush_text(&mut text, &mut out);
            out.push(child);
        }
    }
    flush_text(&mut text, &mut out);
    out
}

fn flush_text(text: &mut Option<String>, out: &mut Vec<Value>) {
    if let Some(text) = text.take()
        && !text.is_empty()
    {
        out.push(json!({"type": "JSXText", "value": text}));
    }
}

/// Babel's `cleanJSXElementLiteralChild`: lines trimmed where they meet a line
/// break, blank lines dropped, and the rest joined with one space.
fn clean_jsx_text(value: &str) -> String {
    let lines: Vec<&str> = value
        .split("\r\n")
        .flat_map(|line| line.split(['\n', '\r']))
        .collect();
    let last_non_empty = lines
        .iter()
        .rposition(|line| line.chars().any(|c| c != ' ' && c != '\t'))
        .unwrap_or(0);
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        let mut trimmed = line.replace('\t', " ");
        if index != 0 {
            trimmed = trimmed.trim_start_matches(' ').to_owned();
        }
        if index != lines.len() - 1 {
            trimmed = trimmed.trim_end_matches(' ').to_owned();
        }
        if !trimmed.is_empty() {
            if index != last_non_empty {
                trimmed.push(' ');
            }
            out.push_str(&trimmed);
        }
    }
    out
}

/// An error message in comparable form.
pub fn error(message: &str) -> String {
    let mut out = String::new();
    for line in message.lines() {
        out.push_str(&without_file_name(line.trim_end()));
        out.push('\n');
    }
    out.trim().to_owned()
}

/// `  error.foo.ts:3:4` as `  <file>:3:4`.
fn without_file_name(line: &str) -> String {
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let mut parts = body.rsplitn(3, ':');
    if let (Some(column), Some(row), Some(file)) = (parts.next(), parts.next(), parts.next())
        && is_number(column)
        && is_number(row)
        && !file.is_empty()
        && !file.contains(' ')
    {
        return format!("{indent}<file>:{row}:{column}");
    }
    line.to_owned()
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The `## Logs` section as values.
///
/// # Errors
///
/// When a line is not JSON.
pub fn logs(section: &str) -> Result<Vec<Value>, String> {
    section
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map(without_unstable_keys)
                .map_err(|error| format!("a log line is not JSON: {error}"))
        })
        .collect()
}

/// The crate's events in the form [`logs`] returns.
pub fn events(events: &[Value]) -> Vec<Value> {
    events.iter().cloned().map(without_unstable_keys).collect()
}

fn without_unstable_keys(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(without_unstable_keys).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| key != "identifierName" && key != "fnLoc")
                .map(|(key, child)| (key, without_unstable_keys(child)))
                .collect(),
        ),
        other => other,
    }
}

/// Where two values first differ, short enough for a report line.
pub fn first_difference(expected: &Value, actual: &Value) -> String {
    let mut path = String::new();
    difference(expected, actual, &mut path)
        .unwrap_or_else(|| "values differ but no difference was found".to_owned())
}

fn difference(expected: &Value, actual: &Value, path: &mut String) -> Option<String> {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, left_child) in left {
                let right_child = right.get(key).unwrap_or(&Value::Null);
                if left_child != right_child {
                    let len = path.len();
                    path.push('.');
                    path.push_str(key);
                    if let Some(found) = difference(left_child, right_child, path) {
                        return Some(found);
                    }
                    path.truncate(len);
                }
            }
            right
                .iter()
                .find(|(key, _)| !left.contains_key(*key))
                .map(|(key, child)| format!("{path}.{key}: only uf has {}", short(child)))
        }
        (Value::Array(left), Value::Array(right)) => {
            for (index, (left_child, right_child)) in left.iter().zip(right).enumerate() {
                if left_child != right_child {
                    let len = path.len();
                    path.push_str(&format!("[{index}]"));
                    if let Some(found) = difference(left_child, right_child, path) {
                        return Some(found);
                    }
                    path.truncate(len);
                }
            }
            (left.len() != right.len()).then(|| {
                format!(
                    "{path}: {} items expected, uf has {}",
                    left.len(),
                    right.len()
                )
            })
        }
        _ => (expected != actual).then(|| {
            format!(
                "{path}: expected {}, uf has {}",
                short(expected),
                short(actual)
            )
        }),
    }
}

fn short(value: &Value) -> String {
    let mut text = value.to_string();
    if text.len() > 120 {
        let mut cut = 117;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("...");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{clean_jsx_text, error};

    #[test]
    fn jsx_text_is_trimmed_the_way_babel_evaluates_it() {
        assert_eq!(clean_jsx_text("\n  hello\n  world  \n"), "hello world");
        assert_eq!(clean_jsx_text(" a "), " a ");
        assert_eq!(clean_jsx_text("\n   \n"), "");
    }

    #[test]
    fn an_error_location_loses_its_file_name() {
        assert_eq!(
            error("Error: x\n\nerror.foo.ts:3:4  \n> 3 | a"),
            "Error: x\n\n<file>:3:4\n> 3 | a"
        );
    }
}
