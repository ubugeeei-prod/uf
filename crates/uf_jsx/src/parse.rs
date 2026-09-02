//! JSX structure, read off the token stream as byte spans.
//!
//! [`uf_flow::scan::tokenize_jsx`] has already decided which `<` opens an
//! element and where the text between tags begins and ends, so nothing here
//! guesses: it reads [`TokenKind::JsxTagOpen`], [`TokenKind::JsxTagClose`] and
//! [`TokenKind::JsxText`] and assembles them.
//!
//! Every node is a span into the original source and never a copy of it, which
//! is what lets the renderer rewrite in place and keep the file's line count.
//!
//! Recursion here is bounded twice over: the scanner stops opening JSX frames
//! past its own ceiling, and [`MAX_DEPTH`] refuses a tree deeper than that. A
//! syntax tree has no cycles, so a depth bound is the whole termination
//! argument.

use std::ops::Range;

use uf_flow::scan::{Token, TokenKind, matching_close};

/// Deepest JSX nesting the parser will build a tree for.
pub(crate) const MAX_DEPTH: usize = 128;

/// One JSX element, or a fragment when [`Element::name`] is [`None`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    /// The element name as written, or [`None`] for `<>`.
    pub(crate) name: Option<Range<usize>>,
    /// Attributes in source order.
    pub(crate) attributes: Vec<Attribute>,
    /// The `<` that opened the tag.
    pub(crate) open: Range<usize>,
    /// The `>` that closed the opening tag.
    pub(crate) tag_end: Range<usize>,
    /// The `/` of `<tag />`, when the element is self-closing.
    pub(crate) self_closing: Option<Range<usize>>,
    /// Children in source order.
    pub(crate) children: Vec<Child>,
    /// The whole `</tag>`, when there is one.
    pub(crate) close: Option<Range<usize>>,
}

/// One child between an opening and a closing tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Child {
    /// A nested element.
    Element(Element),
    /// A run of text.
    Text { span: Range<usize> },
    /// `{ … }`, including `{ /* comment */ }` and `{ …spread }`.
    Expression {
        /// The `{`.
        open: Range<usize>,
        /// The `}`.
        close: Range<usize>,
        /// Token indices strictly between the braces.
        inner: Range<usize>,
        /// Whether the braces hold a `...` spread.
        spread: bool,
    },
}

/// One attribute of an opening tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Attribute {
    /// `name`, `name="s"` or `name={e}`.
    Named {
        /// The attribute name as written.
        name: Range<usize>,
        /// The `=`, when there is a value.
        equals: Option<Range<usize>>,
        /// What the value is.
        value: Option<AttributeValue>,
        /// Everything the attribute covers.
        span: Range<usize>,
    },
    /// `{...expr}`.
    Spread {
        /// The `{`.
        open: Range<usize>,
        /// The `}`.
        close: Range<usize>,
        /// Token indices strictly between the braces.
        inner: Range<usize>,
    },
}

/// What an attribute's value is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttributeValue {
    /// `"text"`, whose bytes are raw JSX rather than a JavaScript literal.
    Text { span: Range<usize> },
    /// `{ expr }`.
    Expression {
        /// The `{`.
        open: Range<usize>,
        /// The `}`.
        close: Range<usize>,
        /// Token indices strictly between the braces.
        inner: Range<usize>,
    },
}

/// Reads elements out of one token stream.
pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: &'a [Token], source: &'a str) -> Self {
        Self { tokens, source }
    }

    /// The element opening at `at`, and the token index just past it.
    ///
    /// Returns [`None`] when the tokens are not an element the renderer knows
    /// how to lower — an unterminated tag, or an attribute shape outside the
    /// grammar — so that source is left exactly as written rather than
    /// rewritten into a guess.
    pub(crate) fn element(&self, at: usize, depth: usize) -> Option<(Element, usize)> {
        if depth > MAX_DEPTH || self.kind(at)? != TokenKind::JsxTagOpen {
            return None;
        }

        let open = self.span(at)?;
        let mut cursor = at + 1;
        let name = if self.kind(cursor)? == TokenKind::JsxTagClose {
            None
        } else {
            let (span, next) = self.name(cursor)?;
            cursor = next;
            Some(span)
        };

        let mut attributes = Vec::new();
        let mut self_closing = None;
        loop {
            match self.kind(cursor)? {
                TokenKind::JsxTagClose => break,
                TokenKind::Punct(b'/') => {
                    self_closing = Some(self.span(cursor)?);
                    cursor += 1;
                }
                _ => {
                    let (attribute, next) = self.attribute(cursor)?;
                    attributes.push(attribute);
                    cursor = next;
                }
            }
        }

        let tag_end = self.span(cursor)?;
        cursor += 1;

        if self_closing.is_some() {
            return Some((
                Element {
                    name,
                    attributes,
                    open,
                    tag_end,
                    self_closing,
                    children: Vec::new(),
                    close: None,
                },
                cursor,
            ));
        }

        let (children, close, next) = self.children(cursor, depth)?;
        Some((
            Element {
                name,
                attributes,
                open,
                tag_end,
                self_closing: None,
                children,
                close: Some(close),
            },
            next,
        ))
    }

    /// A tag or attribute name: `a`, `a.b`, `a-b`, `a:b`, adjacency required.
    fn name(&self, at: usize) -> Option<(Range<usize>, usize)> {
        if self.kind(at)? != TokenKind::Ident {
            return None;
        }
        let start = self.tokens[at].start;
        let mut end = self.tokens[at].end;
        let mut cursor = at + 1;

        while let Some(TokenKind::Punct(b'.' | b'-' | b':')) = self.kind(cursor) {
            let separator = &self.tokens[cursor];
            let Some(following) = self.tokens.get(cursor + 1) else {
                break;
            };
            if separator.start != end
                || following.start != separator.end
                || following.kind != TokenKind::Ident
            {
                break;
            }
            end = following.end;
            cursor += 2;
        }

        Some((start..end, cursor))
    }

    fn attribute(&self, at: usize) -> Option<(Attribute, usize)> {
        if self.kind(at)? == TokenKind::Punct(b'{') {
            let close = matching_close(self.tokens, at, b'{', b'}')?;
            return Some((
                Attribute::Spread {
                    open: self.span(at)?,
                    close: self.span(close)?,
                    inner: at + 1..close,
                },
                close + 1,
            ));
        }

        let (name, mut cursor) = self.name(at)?;
        let span_start = name.start;
        if self.kind(cursor) != Some(TokenKind::Punct(b'=')) {
            let span = span_start..name.end;
            return Some((
                Attribute::Named {
                    name,
                    equals: None,
                    value: None,
                    span,
                },
                cursor,
            ));
        }

        let equals = self.span(cursor)?;
        cursor += 1;
        let (value, next) = match self.kind(cursor)? {
            TokenKind::String => (
                AttributeValue::Text {
                    span: self.span(cursor)?,
                },
                cursor + 1,
            ),
            TokenKind::Punct(b'{') => {
                let close = matching_close(self.tokens, cursor, b'{', b'}')?;
                (
                    AttributeValue::Expression {
                        open: self.span(cursor)?,
                        close: self.span(close)?,
                        inner: cursor + 1..close,
                    },
                    close + 1,
                )
            }
            _ => return None,
        };

        let span = span_start..self.tokens.get(next - 1)?.end;
        Some((
            Attribute::Named {
                name,
                equals: Some(equals),
                value: Some(value),
                span,
            },
            next,
        ))
    }

    /// Children up to the closing tag, which is returned with them.
    fn children(&self, at: usize, depth: usize) -> Option<(Vec<Child>, Range<usize>, usize)> {
        let mut children = Vec::new();
        let mut cursor = at;

        loop {
            match self.kind(cursor)? {
                TokenKind::JsxText => {
                    children.push(Child::Text {
                        span: self.span(cursor)?,
                    });
                    cursor += 1;
                }
                TokenKind::Punct(b'{') => {
                    let close = matching_close(self.tokens, cursor, b'{', b'}')?;
                    children.push(Child::Expression {
                        open: self.span(cursor)?,
                        close: self.span(close)?,
                        inner: cursor + 1..close,
                        spread: self.starts_spread(cursor + 1, close),
                    });
                    cursor = close + 1;
                }
                TokenKind::JsxTagOpen => {
                    if self.kind(cursor + 1) == Some(TokenKind::Punct(b'/')) {
                        let end = self.closing_tag(cursor)?;
                        let close = self.tokens[cursor].start..self.tokens[end].end;
                        return Some((children, close, end + 1));
                    }
                    let (element, next) = self.element(cursor, depth + 1)?;
                    children.push(Child::Element(element));
                    cursor = next;
                }
                _ => return None,
            }
        }
    }

    /// Index of the `>` closing a `</name>` that starts at `at`.
    fn closing_tag(&self, at: usize) -> Option<usize> {
        let mut cursor = at + 2;
        if self.kind(cursor)? != TokenKind::JsxTagClose {
            let (_, next) = self.name(cursor)?;
            cursor = next;
        }
        (self.kind(cursor)? == TokenKind::JsxTagClose).then_some(cursor)
    }

    /// Whether the tokens in `start..end` begin with a `...` spread.
    fn starts_spread(&self, start: usize, end: usize) -> bool {
        if end < start + 3 {
            return false;
        }
        (0..3).all(|offset| {
            self.tokens
                .get(start + offset)
                .is_some_and(|token| token.is_punct(b'.'))
        })
    }

    /// Whether the tokens in `range` hold anything at all.
    ///
    /// A container that lexes to nothing held only a comment or whitespace,
    /// which is not a child: `{/* note */}` renders nothing.
    pub(crate) fn is_empty_range(&self, range: &Range<usize>) -> bool {
        range.start >= range.end
    }

    /// The source text of a token range.
    pub(crate) fn text(&self, range: &Range<usize>) -> &'a str {
        let Some(first) = self.tokens.get(range.start) else {
            return "";
        };
        let Some(last) = range.end.checked_sub(1).and_then(|at| self.tokens.get(at)) else {
            return "";
        };
        self.source.get(first.start..last.end).unwrap_or_default()
    }

    fn kind(&self, at: usize) -> Option<TokenKind> {
        self.tokens.get(at).map(|token| token.kind)
    }

    fn span(&self, at: usize) -> Option<Range<usize>> {
        self.tokens.get(at).map(|token| token.start..token.end)
    }
}
