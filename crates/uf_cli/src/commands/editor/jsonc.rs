//! Adding keys to a JSON-with-comments settings file without rewriting it.
//!
//! VS Code's `.vscode/settings.json` and Zed's `.zed/settings.json` are JSONC:
//! JSON with `//` and `/* */` comments and trailing commas, written by hand,
//! and committed. `uf editor setup` has to add a handful of keys to such a file
//! and leave the rest of it exactly as it was, comments included. Parsing it
//! into a value and printing that back would lose every comment and reorder
//! nothing useful, so this module never prints the file: it finds where things
//! are, decides which keys are missing, and splices text in at those places.
//!
//! # What it guarantees
//!
//! * **Nothing the file already says is changed.** A key that is present keeps
//!   its value, whatever it is: [`Outcome::Kept`] reports the difference and
//!   nothing is written over it. The only edits are insertions.
//! * **The result is still JSONC the editor reads**: every insertion is a
//!   complete member, followed by a comma when members follow it (JSONC also
//!   accepts a trailing one, but none is left behind).
//! * **Running it twice changes nothing the second time**: every key it adds
//!   is found, with the wanted value, by the next run.
//!
//! What it does not do is understand an editor's semantics. A key the editor
//! would read differently — a language block that is not under `"[javascript]"`
//! but covers it — is not recognised. The keys it adds are exact paths, and a
//! path that is present in any spelling other than exactly that one is not.
//!
//! # Failure
//!
//! Text that is not JSONC, or whose top level is not an object, is an error,
//! and the caller writes nothing: a settings file uf cannot read is a settings
//! file uf must not guess at.

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// One thing a settings file should say.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Want {
    /// The object keys from the top level down to the member, e.g.
    /// `["[javascript]", "editor.defaultFormatter"]`. Keys are matched
    /// exactly; a dotted key is one key, as VS Code writes them.
    pub(crate) path: Vec<String>,
    /// What the member should be.
    pub(crate) kind: WantKind,
}

/// What a [`Want`] asks of the member at its path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WantKind {
    /// The member is this value. A member with any other value is kept.
    Value(Value),
    /// The member is an array containing this value, which is appended to an
    /// existing array that lacks it — how a recommendation is added to
    /// `.vscode/extensions.json` without dropping the project's own.
    Contains(Value),
}

impl Want {
    /// A member that should have `value`.
    pub(crate) fn value(path: &[&str], value: Value) -> Self {
        Self {
            path: path.iter().map(|key| (*key).to_owned()).collect(),
            kind: WantKind::Value(value),
        }
    }

    /// An array member that should contain `value`.
    pub(crate) fn contains(path: &[&str], value: Value) -> Self {
        Self {
            path: path.iter().map(|key| (*key).to_owned()).collect(),
            kind: WantKind::Contains(value),
        }
    }

    /// The value this want would write where nothing is.
    fn fresh(&self) -> Value {
        match &self.kind {
            WantKind::Value(value) => value.clone(),
            WantKind::Contains(value) => Value::Array(vec![value.clone()]),
        }
    }
}

/// What became of one [`Want`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    /// It was missing and is added.
    Added,
    /// The file already says it.
    Already,
    /// The file says something else there, which is kept. `current` is what
    /// it says, for the report.
    Kept { current: Value },
}

/// The result of planning edits to one file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edit {
    /// The file's new text. Equal to the old text when nothing was added.
    pub(crate) text: String,
    /// One outcome per want, in the order the wants were given.
    pub(crate) outcomes: Vec<Outcome>,
    /// The inserted text, one entry per splice, for showing what was added.
    pub(crate) inserted: Vec<String>,
}

impl Edit {
    /// Whether the file changes.
    pub(crate) fn changes(&self) -> bool {
        self.outcomes.contains(&Outcome::Added)
    }
}

/// Plan the edits that make `existing` say every want, or the whole file when
/// there is none yet.
///
/// # Errors
///
/// When `existing` is not JSONC, or its top level is not an object.
pub(crate) fn edit(existing: Option<&str>, wants: &[Want]) -> Result<Edit> {
    let Some(text) = existing.filter(|text| !text.trim().is_empty()) else {
        return Ok(new_file(wants));
    };
    let root = Parser::new(text).document()?;
    let NodeKind::Object(_) = &root.kind else {
        bail!(uf_infra::cstr!("the top level is not an object"));
    };

    let mut outcomes = Vec::with_capacity(wants.len());
    // Insertions keyed by the object they go into, in the order first asked
    // for; wants that create the same missing object share one insertion.
    let mut splices: Vec<Splice> = Vec::new();
    for want in wants {
        outcomes.push(plan_one(text, &root, want, &mut splices)?);
    }

    let mut inserted = Vec::new();
    let mut result = text.to_owned();
    // From the end backwards, so an earlier offset is still right after a
    // later splice.
    splices.sort_by_key(|splice| std::cmp::Reverse(splice.at));
    for splice in &splices {
        let rendered = splice.render(text);
        inserted.push(rendered.shown.clone());
        result.replace_range(splice.at..splice.at, &rendered.text);
    }
    inserted.reverse();
    Ok(Edit {
        text: result,
        outcomes,
        inserted,
    })
}

/// The whole file, for a settings file that does not exist yet.
fn new_file(wants: &[Want]) -> Edit {
    let mut root = Value::Object(serde_json::Map::new());
    for want in wants {
        merge_at(&mut root, &want.path, want.fresh());
    }
    let text = uf_infra::cstr!("{}\n", pretty(&root, "")).into_string();
    Edit {
        inserted: vec![text.trim_end().to_owned()],
        text,
        outcomes: vec![Outcome::Added; wants.len()],
    }
}

/// Put `value` at `path` inside `root`, creating objects on the way and
/// merging into objects already there.
fn merge_at(root: &mut Value, path: &[String], value: Value) {
    let Some((first, rest)) = path.split_first() else {
        *root = value;
        return;
    };
    let Value::Object(map) = root else {
        return;
    };
    let entry = map
        .entry(first.clone())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if rest.is_empty() {
        *entry = value;
    } else {
        merge_at(entry, rest, value);
    }
}

/// Decide one want against the parsed file, recording any insertion it needs.
fn plan_one(text: &str, root: &Node, want: &Want, splices: &mut Vec<Splice>) -> Result<Outcome> {
    let mut node = root;
    for (depth, key) in want.path.iter().enumerate() {
        let NodeKind::Object(members) = &node.kind else {
            // Something other than an object where the path needs one: what
            // is there is the project's, and is kept.
            return Ok(Outcome::Kept {
                current: node.value(text)?,
            });
        };
        let Some(member) = members.iter().find(|member| member.key == *key) else {
            let remaining = &want.path[depth..];
            add_splice(splices, node, remaining, want.fresh());
            return Ok(Outcome::Added);
        };
        node = &member.value;
    }

    match &want.kind {
        WantKind::Value(wanted) => {
            let current = node.value(text)?;
            Ok(if current == *wanted {
                Outcome::Already
            } else {
                Outcome::Kept { current }
            })
        }
        WantKind::Contains(wanted) => {
            let NodeKind::Array(items) = &node.kind else {
                return Ok(Outcome::Kept {
                    current: node.value(text)?,
                });
            };
            for item in items {
                if item.value(text)? == *wanted {
                    return Ok(Outcome::Already);
                }
            }
            splices.push(Splice {
                at: node.start + 1,
                container: Container::Array {
                    empty: items.is_empty(),
                },
                members: vec![(None, wanted.clone())],
            });
            Ok(Outcome::Added)
        }
    }
}

/// Record that `object` needs `remaining[0]`, holding the rest of the path
/// down to `value`, merging with an insertion already planned there.
fn add_splice(splices: &mut Vec<Splice>, object: &Node, remaining: &[String], value: Value) {
    let NodeKind::Object(members) = &object.kind else {
        return;
    };
    let at = object.start + 1;
    let (key, rest) = remaining.split_first().expect("a missing member has a key");
    let mut nested = value;
    for inner in rest.iter().rev() {
        let mut map = serde_json::Map::new();
        map.insert(inner.clone(), nested);
        nested = Value::Object(map);
    }

    if let Some(splice) = splices.iter_mut().find(|splice| splice.at == at) {
        if let Some((_, existing)) = splice
            .members
            .iter_mut()
            .find(|(existing, _)| existing.as_deref() == Some(key.as_str()))
        {
            merge_values(existing, nested);
        } else {
            splice.members.push((Some(key.clone()), nested));
        }
        return;
    }
    splices.push(Splice {
        at,
        container: Container::Object {
            empty: members.is_empty(),
        },
        members: vec![(Some(key.clone()), nested)],
    });
}

/// Merge `other` into `into`, object keys recursively; anything else replaces.
fn merge_values(into: &mut Value, other: Value) {
    match (into, other) {
        (Value::Object(into), Value::Object(other)) => {
            for (key, value) in other {
                match into.get_mut(&key) {
                    Some(existing) => merge_values(existing, value),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (into, other) => *into = other,
    }
}

/// Text to insert just inside an object's `{` or an array's `[`.
#[derive(Debug)]
struct Splice {
    /// The byte offset just after the opening bracket.
    at: usize,
    container: Container,
    /// `(key, value)` for an object member, `(None, value)` for an array item.
    members: Vec<(Option<String>, Value)>,
}

#[derive(Debug, Clone, Copy)]
enum Container {
    /// `empty` when the object has no members, so no comma follows ours.
    Object {
        empty: bool,
    },
    Array {
        empty: bool,
    },
}

/// A splice as text: what goes into the file, and what to show a reader.
struct Rendered {
    text: String,
    shown: String,
}

impl Splice {
    fn render(&self, source: &str) -> Rendered {
        let outer = line_indent(source, self.at - 1);
        let inner = uf_infra::cstr!("{outer}  ").into_string();
        let empty = match self.container {
            Container::Object { empty } | Container::Array { empty } => empty,
        };
        let items = self
            .members
            .iter()
            .map(|(key, value)| {
                let value = pretty(value, &inner);
                match key {
                    Some(key) => {
                        uf_infra::cstr!("{}: {value}", Value::String(key.clone())).into_string()
                    }
                    None => value,
                }
            })
            .collect::<Vec<_>>();

        if let Container::Array { .. } = self.container {
            // An array item goes inline: `["uniflowed.uf", "other"]`.
            let joined = items.join(", ");
            let text = if empty {
                joined.clone()
            } else {
                uf_infra::cstr!("{joined}, ").into_string()
            };
            return Rendered {
                text,
                shown: joined,
            };
        }

        let mut text = String::new();
        for (index, item) in items.iter().enumerate() {
            text.push('\n');
            text.push_str(&inner);
            text.push_str(item);
            if !empty || index + 1 < items.len() {
                text.push(',');
            }
        }
        if empty {
            text.push('\n');
            text.push_str(&outer);
        }
        // Exactly what goes into the file, commas included, less the line
        // breaks around it.
        let shown = text.trim_start_matches('\n').trim_end().to_owned();
        Rendered { text, shown }
    }
}

/// The leading whitespace of the line `offset` is on.
fn line_indent(text: &str, offset: usize) -> String {
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    text[line_start..]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// `value` as two-space-indented JSON, every line after the first prefixed
/// with `indent` so it lines up where it is inserted.
fn pretty(value: &Value, indent: &str) -> String {
    let printed = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let mut lines = printed.lines();
    let mut out = lines.next().unwrap_or_default().to_owned();
    for line in lines {
        out.push('\n');
        out.push_str(indent);
        out.push_str(line);
    }
    out
}

/// A value in the file and where it is.
#[derive(Debug)]
struct Node {
    kind: NodeKind,
    /// Byte offset of the value's first character.
    start: usize,
    /// Byte offset just past its last character.
    end: usize,
}

#[derive(Debug)]
enum NodeKind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    Scalar,
}

#[derive(Debug)]
struct Member {
    key: String,
    value: Node,
}

impl Node {
    /// The value's JSON, read from its text (comments inside it allowed).
    fn value(&self, text: &str) -> Result<Value> {
        json5::from_str(&text[self.start..self.end]).map_err(|error| {
            anyhow!(uf_infra::cstr!(
                "cannot read `{}`: {error}",
                &text[self.start..self.end]
            ))
        })
    }
}

/// A recursive-descent reader of JSONC that keeps offsets and nothing else.
struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            at: 0,
        }
    }

    fn document(mut self) -> Result<Node> {
        let node = self.value()?;
        self.skip_trivia()?;
        if self.at != self.bytes.len() {
            bail!(uf_infra::cstr!(
                "unexpected text after the top-level value at byte {}",
                self.at
            ));
        }
        Ok(node)
    }

    /// Skip whitespace and comments.
    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.bytes.get(self.at) {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.at += 1,
                Some(b'/') if self.bytes.get(self.at + 1) == Some(&b'/') => {
                    while let Some(byte) = self.bytes.get(self.at) {
                        if *byte == b'\n' {
                            break;
                        }
                        self.at += 1;
                    }
                }
                Some(b'/') if self.bytes.get(self.at + 1) == Some(&b'*') => {
                    let Some(close) = self.text[self.at + 2..].find("*/") else {
                        bail!(uf_infra::cstr!("a /* comment is not closed"));
                    };
                    self.at += 2 + close + 2;
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self) -> Result<Node> {
        self.skip_trivia()?;
        let start = self.at;
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(start),
            Some(b'[') => self.array(start),
            Some(b'"') => {
                self.string()?;
                Ok(Node {
                    kind: NodeKind::Scalar,
                    start,
                    end: self.at,
                })
            }
            Some(_) => {
                while let Some(byte) = self.bytes.get(self.at) {
                    if matches!(
                        byte,
                        b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r' | b'/'
                    ) {
                        break;
                    }
                    self.at += 1;
                }
                if self.at == start {
                    bail!(uf_infra::cstr!("expected a value at byte {start}"));
                }
                Ok(Node {
                    kind: NodeKind::Scalar,
                    start,
                    end: self.at,
                })
            }
            None => bail!(uf_infra::cstr!("the file ends where a value was expected")),
        }
    }

    /// A string literal; returns its decoded contents.
    fn string(&mut self) -> Result<String> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.bytes.get(self.at) {
                Some(b'\\') => self.at += 2,
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(_) => self.at += 1,
                None => bail!(uf_infra::cstr!(
                    "a string starting at byte {start} is not closed"
                )),
            }
        }
        serde_json::from_str(&self.text[start..self.at]).map_err(|error| {
            anyhow!(uf_infra::cstr!(
                "cannot read the string at byte {start}: {error}"
            ))
        })
    }

    fn object(&mut self, start: usize) -> Result<Node> {
        self.at += 1;
        let mut members = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.bytes.get(self.at) {
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                Some(b'"') => {
                    let key = self.string()?;
                    self.skip_trivia()?;
                    if self.bytes.get(self.at) != Some(&b':') {
                        bail!(uf_infra::cstr!(
                            "expected `:` after the key {key:?} at byte {}",
                            self.at
                        ));
                    }
                    self.at += 1;
                    let value = self.value()?;
                    members.push(Member { key, value });
                    self.skip_trivia()?;
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {}
                        _ => bail!(uf_infra::cstr!("expected `,` or `}}` at byte {}", self.at)),
                    }
                }
                _ => bail!(uf_infra::cstr!(
                    "expected a key or `}}` at byte {}",
                    self.at
                )),
            }
        }
        Ok(Node {
            kind: NodeKind::Object(members),
            start,
            end: self.at,
        })
    }

    fn array(&mut self, start: usize) -> Result<Node> {
        self.at += 1;
        let mut items = Vec::new();
        loop {
            self.skip_trivia()?;
            if self.bytes.get(self.at) == Some(&b']') {
                self.at += 1;
                break;
            }
            items.push(self.value()?);
            self.skip_trivia()?;
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {}
                _ => bail!(uf_infra::cstr!("expected `,` or `]` at byte {}", self.at)),
            }
        }
        Ok(Node {
            kind: NodeKind::Array(items),
            start,
            end: self.at,
        })
    }
}
