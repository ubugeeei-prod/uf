//! Lowering elements to `_jsx` calls, one span at a time.
//!
//! Every rewrite is local to the bytes the element already occupied:
//!
//! ```text
//! <div className="a">text</div>
//! ───┬──────────────┬────┬─────
//!    │              │    └── `})`
//!    │              └── `children: `
//!    └── `_jsx("div", {`
//! ```
//!
//! Nothing is reflowed and nothing is copied, which is why an element written
//! over eight lines still occupies eight lines afterwards. The one value that
//! does move is `key`, because the automatic runtime takes it as an argument
//! *after* the props rather than inside them; the span it came from is
//! squashed so the line count nets out. See [`crate::edit`].
//!
//! The object literal is built with a comma after every attribute and none
//! before `children`, which is what makes `<div>` and `<div a>` both come out
//! valid without the renderer tracking whether it has emitted anything yet.

use std::ops::Range;

use crate::edit::{Edit, newlines};
use crate::parse::{Attribute, AttributeValue, Child, Element, Parser};
use crate::text;
use uf_flow::scan::{Token, TokenKind};

/// Local name the `jsx` helper is imported under.
pub const JSX_LOCAL: &str = "_jsx";
/// Local name the `jsxs` helper is imported under.
pub const JSXS_LOCAL: &str = "_jsxs";
/// Local name the `Fragment` helper is imported under.
pub const FRAGMENT_LOCAL: &str = "_Fragment";

/// Which runtime helpers a module turned out to need.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Helpers {
    /// `_jsx`, for an element with zero or one child.
    pub jsx: bool,
    /// `_jsxs`, for an element with a list of children.
    pub jsxs: bool,
    /// `_Fragment`, for `<>…</>`.
    pub fragment: bool,
}

impl Helpers {
    /// Whether any helper is needed, and so whether an import is.
    #[must_use]
    pub const fn any(self) -> bool {
        self.jsx || self.jsxs || self.fragment
    }
}

/// Turns parsed elements into edits.
pub(crate) struct Renderer<'a> {
    parser: Parser<'a>,
    tokens: &'a [Token],
    source: &'a str,
    edits: Vec<Edit>,
    helpers: Helpers,
    elements: usize,
    limit: usize,
}

impl<'a> Renderer<'a> {
    pub(crate) fn new(tokens: &'a [Token], source: &'a str, limit: usize) -> Self {
        Self {
            parser: Parser::new(tokens, source),
            tokens,
            source,
            edits: Vec::new(),
            helpers: Helpers::default(),
            elements: 0,
            limit,
        }
    }

    /// Everything the renderer produced.
    pub(crate) fn finish(self) -> (Vec<Edit>, Helpers, usize) {
        (self.edits, self.helpers, self.elements)
    }

    /// Whether the element ceiling was passed.
    pub(crate) fn overflowed(&self) -> bool {
        self.elements > self.limit
    }

    /// Render every element opening in a range of token indices.
    ///
    /// Iterative over siblings, recursive over nesting, and the nesting is
    /// bounded by [`crate::parse::MAX_DEPTH`].
    pub(crate) fn collect(&mut self, from: usize, to: usize, depth: usize) {
        let mut cursor = from;
        while cursor < to.min(self.tokens.len()) {
            if self.tokens[cursor].kind != TokenKind::JsxTagOpen || self.kind_is_slash(cursor + 1) {
                cursor += 1;
                continue;
            }
            match self.parser.element(cursor, depth) {
                Some((element, next)) => {
                    self.element(&element, depth);
                    cursor = next;
                }
                // Not a shape the renderer knows how to lower. Leaving it is
                // the only safe answer: the module keeps its JSX, and the
                // build's own "no JSX survived" check reports it rather than
                // this pass guessing.
                None => cursor += 1,
            }
        }
    }

    fn kind_is_slash(&self, at: usize) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.is_punct(b'/'))
    }

    fn element(&mut self, element: &Element, depth: usize) {
        self.elements += 1;
        if self.overflowed() {
            return;
        }

        let children = self.live_children(element);
        let array = children.len() != 1
            || children
                .iter()
                .any(|child| matches!(child, Child::Expression { spread: true, .. }));
        let helper = if array && !children.is_empty() {
            self.helpers.jsxs = true;
            JSXS_LOCAL
        } else {
            self.helpers.jsx = true;
            JSX_LOCAL
        };

        let type_expression = match &element.name {
            Some(name) => element_type(&self.source[name.clone()]),
            None => {
                self.helpers.fragment = true;
                String::from(FRAGMENT_LOCAL)
            }
        };

        let head = match &element.name {
            Some(name) => element.open.start..name.end,
            None => element.open.clone(),
        };
        self.edits.push(Edit::replace(
            head,
            format!("{helper}({type_expression}, {{"),
        ));

        let key = self.attributes(element, depth);

        if let Some(slash) = &element.self_closing {
            self.edits.push(Edit::replace(
                slash.start..element.tag_end.end,
                close(&key, false),
            ));
            return;
        }

        if children.is_empty() {
            self.edits.push(Edit::blank(element.tag_end.clone()));
        } else if array {
            self.edits
                .push(Edit::replace(element.tag_end.clone(), "children: ["));
        } else {
            self.edits
                .push(Edit::replace(element.tag_end.clone(), "children: "));
        }

        self.children(element, &children, array, depth);

        if let Some(span) = &element.close {
            self.edits.push(Edit::replace(
                span.clone(),
                close(&key, array && !children.is_empty()),
            ));
        }
    }

    /// Children that survive: text that is not only whitespace, containers
    /// that hold something, and every nested element.
    fn live_children(&self, element: &Element) -> Vec<Child> {
        element
            .children
            .iter()
            .filter(|child| match child {
                Child::Text { span } => text::clean(&self.source[span.clone()]).is_some(),
                Child::Expression { inner, .. } => !self.parser.is_empty_range(inner),
                Child::Element(_) => true,
            })
            .cloned()
            .collect()
    }

    /// Rewrite the attributes, returning the extracted `key` expression and
    /// the line terminators its span gave up.
    fn attributes(&mut self, element: &Element, depth: usize) -> Option<Key> {
        let mut key = None;

        for attribute in &element.attributes {
            match attribute {
                Attribute::Spread { open, close, inner } => {
                    self.edits.push(Edit::blank(open.clone()));
                    self.edits.push(Edit::blank(close.clone()));
                    self.edits.push(Edit::insert(close.end, ", "));
                    self.collect(inner.start, inner.end, depth + 1);
                }
                Attribute::Named {
                    name,
                    equals,
                    value,
                    span,
                } => {
                    // `key` is an argument of the call, not a member of the
                    // props object, so its bytes move out of the object
                    // entirely. Squashing rather than blanking keeps the file's
                    // line count when the expression spanned lines.
                    if &self.source[name.clone()] == "key" && element.name.is_some() {
                        let written = &self.source[span.clone()];
                        key = Some((
                            self.value_text(value),
                            newlines(written),
                            written.contains("\r\n"),
                        ));
                        self.edits.push(Edit::squash(span.clone()));
                        continue;
                    }
                    self.attribute_name(name);
                    match (equals, value) {
                        (Some(equals), Some(AttributeValue::Text { span })) => {
                            self.edits.push(Edit::replace(equals.clone(), ": "));
                            let raw = &self.source[span.start + 1..span.end - 1];
                            self.edits.push(Edit::replace(
                                span.clone(),
                                text::quote(&text::decode_entities(raw)),
                            ));
                        }
                        (Some(equals), Some(AttributeValue::Expression { open, close, inner })) => {
                            // The blanked `{` supplies the space here, where a
                            // string value has none to spare.
                            self.edits.push(Edit::replace(equals.clone(), ":"));
                            self.edits.push(Edit::blank(open.clone()));
                            self.edits.push(Edit::blank(close.clone()));
                            self.collect(inner.start, inner.end, depth + 1);
                        }
                        _ => self.edits.push(Edit::insert(name.end, ": true")),
                    }
                    self.edits.push(Edit::insert(span.end, ", "));
                }
            }
        }

        key
    }

    /// Quote an attribute name that is not a bare identifier.
    fn attribute_name(&mut self, name: &Range<usize>) {
        let written = &self.source[name.clone()];
        if !text::is_identifier(written) {
            self.edits
                .push(Edit::replace(name.clone(), text::quote(written)));
        }
    }

    /// The source of an attribute value, as an expression.
    fn value_text(&self, value: &Option<AttributeValue>) -> String {
        match value {
            Some(AttributeValue::Expression { inner, .. }) => self.parser.text(inner).to_string(),
            Some(AttributeValue::Text { span }) => {
                let raw = &self.source[span.start + 1..span.end - 1];
                text::quote(&text::decode_entities(raw))
            }
            None => String::from("true"),
        }
    }

    fn children(&mut self, element: &Element, live: &[Child], array: bool, depth: usize) {
        for child in &element.children {
            let kept = live.contains(child);
            match child {
                Child::Text { span } => match text::clean(&self.source[span.clone()]) {
                    Some(value) if kept => {
                        let mut rendered = text::quote(&value);
                        if array {
                            rendered.push_str(", ");
                        }
                        self.edits.push(Edit::replace(span.clone(), rendered));
                    }
                    _ => self.edits.push(Edit::blank(span.clone())),
                },
                Child::Expression {
                    open, close, inner, ..
                } => {
                    if !kept {
                        self.edits.push(Edit::blank(open.start..close.end));
                        continue;
                    }
                    self.edits.push(Edit::blank(open.clone()));
                    self.edits.push(Edit::blank(close.clone()));
                    if array {
                        self.edits.push(Edit::insert(close.end, ", "));
                    }
                    self.collect(inner.start, inner.end, depth + 1);
                }
                Child::Element(nested) => {
                    self.element(nested, depth + 1);
                    if array {
                        let end = nested
                            .close
                            .as_ref()
                            .map_or(nested.tag_end.end, |span| span.end);
                        self.edits.push(Edit::insert(end, ", "));
                    }
                }
            }
        }
    }
}

/// A `key` that moved out of the props: its text, and the line terminators
/// its span gave up so that the module's line count nets out.
type Key = (String, usize, bool);

/// The text that closes a call: the children list, the props, and the key.
fn close(key: &Option<Key>, array: bool) -> String {
    let mut out = String::with_capacity(16);
    if array {
        out.push(']');
    }
    out.push_str("})");
    if let Some((key, lines, crlf)) = key {
        // `_jsx(type, props, key)`: the key sits after the props, which is why
        // it had to leave the object literal. The line terminators its span
        // held travel with it, so the module's line count is unchanged.
        out.pop();
        out.push_str(", ");
        out.push_str(key.trim());
        for _ in 0..*lines {
            out.push_str(if *crlf { "\r\n" } else { "\n" });
        }
        out.push(')');
    }
    out
}

/// The expression an element name lowers to.
///
/// A lowercase name with no dot is a host element and becomes a string; a
/// capitalised name, or one holding a dot, is the component the module already
/// has in scope. A name holding a `-` or `:` is a custom element, which is a
/// string however it is capitalised.
#[must_use]
pub fn element_type(name: &str) -> String {
    if name.contains('-') || name.contains(':') {
        return text::quote(name);
    }
    if name.contains('.') {
        return name.to_string();
    }
    match name.as_bytes().first() {
        Some(byte) if byte.is_ascii_lowercase() => text::quote(name),
        _ => name.to_string(),
    }
}

/// Where a runtime import may be inserted without displacing a line.
///
/// After a byte-order mark and after a `#!` line, and never past the first
/// line: the import goes *in front of* whatever line one holds, so the module
/// gains a statement without gaining a line and the source map stays a
/// per-line table.
#[must_use]
pub fn import_offset(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut at = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if bytes[at..].starts_with(b"#!") {
        while at < bytes.len() && bytes[at] != b'\n' {
            at += 1;
        }
        if at < bytes.len() {
            at += 1;
        }
    }
    at
}

/// Whether applying `edits` to `source` would change its line count.
#[must_use]
pub(crate) fn preserves_lines(source: &str, result: &str) -> bool {
    newlines(source) == newlines(result)
}
