//! The config object of `uf.config.js`, as far as an editor needs its shape.
//!
//! # Two readings of one document
//!
//! A document an editor asks about is usually being typed, and a document
//! being typed usually does not parse. So it is read one of two ways, and both
//! produce the same [`Object`]:
//!
//! * **The parser's**, when the official Flow parser reads the document without
//!   a single error. It is exact — a brace in a regular expression, a quote in
//!   a template literal or a colon in a comment cannot mislead it — and it is
//!   the only reading used for a document that parses.
//! * **The scanner's**, when it does not. `uf_flow::scan` already knows where
//!   every string, template, regular expression and comment begins and ends,
//!   and fails open on what it cannot lex; this puts brackets, keys and values
//!   on top of those tokens, with the recoveries a half-typed document needs: a
//!   key with no colon yet, a colon with no value, a value with no comma after
//!   it, a string with no closing quote, an object with no closing brace.
//!
//! `the_scanner_reads_every_document_the_way_the_parser_does` holds the second
//! reading to the first on documents both can read. That agreement is what the
//! scanner's answer about a broken document rests on: it is the same reading,
//! carried past the point where the parser has to stop.
//!
//! # What is not read
//!
//! Anything that is not an object, a list, or a single string, name or number
//! is [`Value::Other`]: a template literal, a call, `process.env.X`. uf reads
//! `uf.config.js` without running it, so none of those is a value uf would
//! accept there, and an editor has nothing to offer inside one.

use uf_flow::ast::{expression, pattern, statement};
use uf_flow::scan::{Token, TokenKind};
use uf_flow::{Loc, Position};

/// The call `uf.config.js` wraps its object in.
const DEFINE_CONFIG: &str = "defineConfig";

/// Objects and lists nested deeper than this are read as [`Value::Other`].
///
/// The scanner recurses once per level on the editor server's own thread, so
/// how deep it goes is bounded by a number rather than by the document. No
/// config nests past a dozen; the parser's reading stops at the same depth, so
/// the two stay one reading.
const MAX_DEPTH: usize = 64;

/// A byte range in the document.
///
/// Crate-visible rather than module-visible because it leaves this module in
/// the completion items and hovers the protocol layer encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    /// First byte.
    pub(crate) start: usize,
    /// One past the last byte.
    pub(crate) end: usize,
}

impl Span {
    /// The empty range at `offset`, which is where an insertion goes.
    pub(crate) fn at(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }
}

/// An object literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Object {
    /// The `{`.
    pub(crate) open: usize,
    /// The `}`, or [`None`] when the document ends first.
    pub(crate) close: Option<usize>,
    /// Its members, in source order.
    pub(crate) entries: Vec<Entry>,
}

/// One member of an object literal, however much of it has been written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The key, when it is a name, a string or a number. A spread, a computed
    /// key or a method has none.
    pub(crate) key: Option<Word>,
    /// The `:` after the key, once it has been typed.
    pub(crate) colon: Option<usize>,
    /// The value, once it has been typed.
    pub(crate) value: Option<Value>,
}

/// A list literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Array {
    /// The `[`.
    pub(crate) open: usize,
    /// The `]`, or [`None`] when the document ends first.
    pub(crate) close: Option<usize>,
    /// Its elements, in source order; holes are skipped.
    pub(crate) elements: Vec<Value>,
}

/// A value, as far as completion can tell values apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    /// An object literal.
    Object(Object),
    /// A list literal.
    Array(Array),
    /// One string, name or number: `"single"`, `true`, `5173`.
    Word(Word),
    /// Anything else, as the span it covers.
    Other(Span),
}

/// A key, or a value that is one token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Word {
    /// The token, quotes included.
    pub(crate) span: Span,
    /// The quote a string is written with, or [`None`] for a name or a number.
    pub(crate) quote: Option<u8>,
    /// Whether a string has its closing quote. Always true for anything else.
    pub(crate) terminated: bool,
}

impl Word {
    /// What a completion replaces: the inside of a string's quotes, or the
    /// whole of a name.
    pub(crate) fn contents(&self) -> Span {
        if self.quote.is_none() {
            return self.span;
        }
        let start = (self.span.start + 1).min(self.span.end);
        let end = if self.terminated {
            self.span.end.saturating_sub(1).max(start)
        } else {
            self.span.end
        };
        Span { start, end }
    }

    /// The text of [`Word::contents`].
    pub(crate) fn text<'s>(&self, source: &'s str) -> &'s str {
        let contents = self.contents();
        source.get(contents.start..contents.end).unwrap_or_default()
    }

    /// Whether a cursor at `offset` is on this word.
    ///
    /// A cursor sits *between* two characters, so the end of a name is on it —
    /// that is where the cursor is while the name is being typed — and its
    /// start is not. For a string it is the inside of the quotes, and the end
    /// of one with no closing quote yet.
    pub(crate) fn holds(&self, offset: usize) -> bool {
        let Span { start, end } = self.span;
        match (self.quote, self.terminated) {
            (Some(_), true) => start < offset && offset < end,
            _ => start < offset && offset <= end,
        }
    }
}

impl Entry {
    /// An entry with nothing completion can use in it.
    fn unnamed(span: Span) -> Self {
        Self {
            key: None,
            colon: None,
            value: Some(Value::Other(span)),
        }
    }

    /// Where the entry begins.
    pub(crate) fn start(&self) -> usize {
        self.key
            .map(|key| key.span.start)
            .or(self.colon)
            .or_else(|| self.value.as_ref().map(Value::start))
            .unwrap_or(0)
    }

    /// Where the entry ends, however far it got.
    pub(crate) fn end(&self) -> usize {
        match (&self.value, self.colon, self.key) {
            (Some(value), _, _) => value.end(),
            (None, Some(colon), _) => colon + 1,
            (None, None, Some(key)) => key.span.end,
            (None, None, None) => 0,
        }
    }
}

impl Value {
    /// Where the value begins.
    pub(crate) fn start(&self) -> usize {
        match self {
            Self::Object(object) => object.open,
            Self::Array(array) => array.open,
            Self::Word(word) => word.span.start,
            Self::Other(span) => span.start,
        }
    }

    /// Where the value ends. An object or a list with no closer yet runs to the
    /// end of everything.
    pub(crate) fn end(&self) -> usize {
        match self {
            Self::Object(object) => object.close.map_or(usize::MAX, |close| close + 1),
            Self::Array(array) => array.close.map_or(usize::MAX, |close| close + 1),
            Self::Word(word) => word.span.end,
            Self::Other(span) => span.end,
        }
    }
}

/// The config object `source` exports, read by the parser when the document
/// parses and by the scanner when it does not.
pub(crate) fn read(source: &str) -> Option<Object> {
    match from_tree(source) {
        Parsed::Clean(object) => object,
        Parsed::Broken => from_tokens(source),
    }
}

/// What the parser made of a document.
enum Parsed {
    /// It parsed without an error; this is the config object it exports.
    Clean(Option<Object>),
    /// It did not parse, or the parser could not be run.
    Broken,
}

/// The parser's reading.
///
/// On a thread with the stack `uf_flow::parse` documents, because the thread
/// asking is an editor server's and a document is whatever the editor sent.
/// The tree is read and freed there, and only the [`Object`] comes back.
fn from_tree(source: &str) -> Parsed {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uf-lsp-config".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || read_tree(source))
            .ok()
            .and_then(|worker| worker.join().ok())
            .unwrap_or(Parsed::Broken)
    })
}

fn read_tree(source: &str) -> Parsed {
    let Ok(parsed) = uf_flow::parse(source) else {
        return Parsed::Broken;
    };
    if !parsed.is_ok() {
        return Parsed::Broken;
    }

    let tree = Tree {
        source,
        lines: Lines::new(source),
    };
    for node in parsed.program.statements.iter() {
        match &**node {
            // The first default export is the answer either way: `uf_config`
            // reads nothing after it, and neither does the scanner.
            statement::StatementInner::ExportDefaultDeclaration { inner, .. } => {
                return Parsed::Clean(match &inner.declaration {
                    statement::export_default_declaration::Declaration::Expression(value) => {
                        tree.config_object(value)
                    }
                    statement::export_default_declaration::Declaration::Declaration(_) => None,
                });
            }
            statement::StatementInner::Expression { inner, .. } => {
                if let expression::ExpressionInner::Assignment {
                    inner: assignment, ..
                } = &*inner.expression
                    && is_module_exports(&assignment.left)
                {
                    return Parsed::Clean(tree.config_object(&assignment.right));
                }
            }
            _ => {}
        }
    }
    Parsed::Clean(None)
}

/// Whether an assignment target is `module.exports`.
fn is_module_exports(target: &pattern::Pattern<Loc, Loc>) -> bool {
    let pattern::Pattern::Expression { inner, .. } = target else {
        return false;
    };
    let expression::ExpressionInner::Member { inner: member, .. } = &***inner else {
        return false;
    };
    let expression::ExpressionInner::Identifier { inner: object, .. } = &*member.object else {
        return false;
    };
    matches!(
        &member.property,
        expression::member::Property::PropertyIdentifier(property)
            if object.name.as_str() == "module" && property.name.as_str() == "exports"
    )
}

/// The parser's tree, read into an [`Object`].
struct Tree<'a> {
    source: &'a str,
    lines: Lines,
}

impl Tree<'_> {
    fn span(&self, loc: &Loc) -> Span {
        let start = self.lines.offset(self.source, loc.start);
        Span {
            start,
            end: self.lines.offset(self.source, loc.end).max(start),
        }
    }

    /// `{ … }` or `defineConfig({ … })`, which is everything `uf_config`
    /// accepts as a default export.
    fn config_object(&self, value: &expression::Expression<Loc, Loc>) -> Option<Object> {
        match &**value {
            expression::ExpressionInner::Object { loc, inner } => Some(self.object(loc, inner, 0)),
            expression::ExpressionInner::Call { inner, .. } => {
                let expression::ExpressionInner::Identifier { inner: callee, .. } = &*inner.callee
                else {
                    return None;
                };
                if callee.name.as_str() != DEFINE_CONFIG {
                    return None;
                }
                let Some(expression::ExpressionOrSpread::Expression(argument)) =
                    inner.arguments.arguments.first()
                else {
                    return None;
                };
                match &**argument {
                    expression::ExpressionInner::Object { loc, inner } => {
                        Some(self.object(loc, inner, 0))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn object(&self, loc: &Loc, object: &expression::Object<Loc, Loc>, depth: usize) -> Object {
        let span = self.span(loc);
        Object {
            open: span.start,
            close: Some(span.end.saturating_sub(1)),
            entries: object
                .properties
                .iter()
                .map(|property| self.entry(property, depth))
                .collect(),
        }
    }

    fn entry(&self, property: &expression::object::Property<Loc, Loc>, depth: usize) -> Entry {
        use expression::object::{NormalProperty, Property};

        match property {
            Property::NormalProperty(NormalProperty::Init {
                loc,
                key,
                value,
                shorthand,
            }) => {
                let Some(word) = self.key(key) else {
                    return Entry::unnamed(self.span(loc));
                };
                if *shorthand {
                    return Entry {
                        key: Some(word),
                        colon: None,
                        value: None,
                    };
                }
                let value = self.value(value, depth);
                Entry {
                    key: Some(word),
                    colon: gap(self.source, word.span.end, value.start()).and_then(|gap| gap.colon),
                    value: Some(value),
                }
            }
            Property::NormalProperty(method) => Entry::unnamed(self.span(method.loc())),
            Property::SpreadProperty(spread) => Entry::unnamed(self.span(&spread.loc)),
        }
    }

    fn key(&self, key: &expression::object::Key<Loc, Loc>) -> Option<Word> {
        use expression::object::Key;

        let (loc, quoted) = match key {
            Key::Identifier(id) => (&id.loc, false),
            Key::StringLiteral((loc, _)) => (loc, true),
            Key::NumberLiteral((loc, _)) => (loc, false),
            Key::BigIntLiteral((loc, _)) => (loc, false),
            Key::PrivateName(_) | Key::Computed(_) => return None,
        };
        let span = self.span(loc);
        Some(Word {
            span,
            quote: quoted
                .then(|| self.source.as_bytes().get(span.start).copied())
                .flatten(),
            terminated: true,
        })
    }

    fn value(&self, value: &expression::Expression<Loc, Loc>, depth: usize) -> Value {
        let span = self.span(value.loc());
        match &**value {
            expression::ExpressionInner::Object { loc, inner } if depth < MAX_DEPTH => {
                Value::Object(self.object(loc, inner, depth + 1))
            }
            expression::ExpressionInner::Array { inner, .. } if depth < MAX_DEPTH => {
                Value::Array(Array {
                    open: span.start,
                    close: Some(span.end.saturating_sub(1)),
                    elements: inner
                        .elements
                        .iter()
                        .filter_map(|element| match element {
                            expression::ArrayElement::Expression(element) => {
                                Some(self.value(element, depth + 1))
                            }
                            expression::ArrayElement::Spread(spread) => {
                                Some(Value::Other(self.span(&spread.loc)))
                            }
                            expression::ArrayElement::Hole(_) => None,
                        })
                        .collect(),
                })
            }
            expression::ExpressionInner::StringLiteral { .. } => Value::Word(Word {
                span,
                quote: self.source.as_bytes().get(span.start).copied(),
                terminated: true,
            }),
            expression::ExpressionInner::Identifier { .. }
            | expression::ExpressionInner::BooleanLiteral { .. }
            | expression::ExpressionInner::NullLiteral { .. }
            | expression::ExpressionInner::NumberLiteral { .. }
            | expression::ExpressionInner::BigIntLiteral { .. } => Value::Word(Word {
                span,
                quote: None,
                terminated: true,
            }),
            _ => Value::Other(span),
        }
    }
}

/// The scanner's reading.
pub(crate) fn from_tokens(source: &str) -> Option<Object> {
    let tokens = uf_flow::scan::tokenize(source);
    let open = config_open(source, &tokens)?;
    let mut scanner = Scanner {
        source,
        tokens: &tokens,
        at: open,
    };
    Some(scanner.object(0))
}

/// The token the config object opens with: the `{` after `export default` or
/// `module.exports =`, directly or as `defineConfig`'s first argument.
fn config_open(source: &str, tokens: &[Token]) -> Option<usize> {
    let ident = |at: usize, name: &str| {
        tokens
            .get(at)
            .is_some_and(|token| token.is_ident(source, name))
    };
    let punct = |at: usize, byte: u8| tokens.get(at).is_some_and(|token| token.is_punct(byte));

    for at in 0..tokens.len() {
        let mut next = if ident(at, "export") && ident(at + 1, "default") {
            at + 2
        } else if ident(at, "module")
            && punct(at + 1, b'.')
            && ident(at + 2, "exports")
            && punct(at + 3, b'=')
        {
            at + 4
        } else {
            continue;
        };
        if ident(next, DEFINE_CONFIG) && punct(next + 1, b'(') {
            next += 2;
        }
        return punct(next, b'{').then_some(next);
    }
    None
}

/// Tokens, read into an [`Object`] with the recoveries a half-typed document
/// needs.
struct Scanner<'a> {
    source: &'a str,
    tokens: &'a [Token],
    /// The token being read.
    at: usize,
}

impl Scanner<'_> {
    fn token(&self, at: usize) -> Option<Token> {
        self.tokens.get(at).copied()
    }

    /// An object, from the `{` at [`Scanner::at`] to its `}` or the end of the
    /// document.
    fn object(&mut self, depth: usize) -> Object {
        let open = self.tokens[self.at].start;
        self.at += 1;
        let mut entries = Vec::new();

        while let Some(token) = self.token(self.at) {
            match token.kind {
                TokenKind::Punct(b'}') => {
                    self.at += 1;
                    return Object {
                        open,
                        close: Some(token.start),
                        entries,
                    };
                }
                TokenKind::Punct(b',') => self.at += 1,
                // A closer this object did not open belongs to whatever did:
                // `defineConfig({ a: 1 )` is an object with no `}` yet, inside
                // a call that has its `)`.
                TokenKind::Punct(b']' | b')') => break,
                _ if self.is_word(token) => entries.push(self.keyed(depth)),
                _ => {
                    let span = self.other();
                    entries.push(Entry::unnamed(span));
                }
            }
        }

        Object {
            open,
            close: None,
            entries,
        }
    }

    /// An entry that starts with a key, with [`Scanner::at`] on the key.
    fn keyed(&mut self, depth: usize) -> Entry {
        let key = self.word(self.tokens[self.at]);
        match self.token(self.at + 1) {
            Some(next) if next.is_punct(b':') => {
                self.at += 2;
                Entry {
                    key: Some(key),
                    colon: Some(next.start),
                    value: self.value(depth),
                }
            }
            // `name() { … }`: a method. No config has one, and it is read the
            // way the parser's reading reads one, as an entry with nothing in
            // it completion can use.
            Some(next) if next.is_punct(b'(') => Entry::unnamed(self.other()),
            // A key with no colon yet — the one being typed, or a shorthand.
            _ => {
                self.at += 1;
                Entry {
                    key: Some(key),
                    colon: None,
                    value: None,
                }
            }
        }
    }

    /// The value after a colon or in a list, or [`None`] when there is none yet.
    fn value(&mut self, depth: usize) -> Option<Value> {
        let token = self.token(self.at)?;
        match token.kind {
            TokenKind::Punct(b',' | b'}' | b']' | b')') => None,
            // `key:` with the value not typed yet, and the next key already on
            // a line below it.
            _ if token.newline_before && self.starts_key(self.at) => None,
            TokenKind::Punct(b'{') if depth < MAX_DEPTH => {
                Some(Value::Object(self.object(depth + 1)))
            }
            TokenKind::Punct(b'[') if depth < MAX_DEPTH => {
                Some(Value::Array(self.array(depth + 1)))
            }
            _ if self.is_word(token) && self.ends_value(self.at + 1) => {
                self.at += 1;
                Some(Value::Word(self.word(token)))
            }
            _ => Some(Value::Other(self.other())),
        }
    }

    /// A list, from the `[` at [`Scanner::at`] to its `]` or the end of the
    /// document.
    fn array(&mut self, depth: usize) -> Array {
        let open = self.tokens[self.at].start;
        self.at += 1;
        let mut elements = Vec::new();

        while let Some(token) = self.token(self.at) {
            match token.kind {
                TokenKind::Punct(b']') => {
                    self.at += 1;
                    return Array {
                        open,
                        close: Some(token.start),
                        elements,
                    };
                }
                TokenKind::Punct(b',') => self.at += 1,
                TokenKind::Punct(b'}' | b')') => break,
                _ => match self.value(depth) {
                    Some(value) => elements.push(value),
                    // Only a `key:` starting a line gets here, and a key has no
                    // place in a list: step over it.
                    None => self.at += 1,
                },
            }
        }

        Array {
            open,
            close: None,
            elements,
        }
    }

    /// Every token to the end of this value, as one span.
    ///
    /// [`Scanner::at`] is on a token that is not a separator, and it is
    /// consumed whatever it is, so the reading always moves. Brackets are
    /// balanced on the way, so the comma in `f(a, b)` does not end the value.
    /// What does is a comma or a closer at the value's own level, and a key
    /// starting a line, which is a comma that has not been typed yet.
    fn other(&mut self) -> Span {
        let first = self.tokens[self.at];
        let mut last = first;
        let mut depth = usize::from(matches!(first.kind, TokenKind::Punct(b'(' | b'[' | b'{')));
        self.at += 1;

        while let Some(token) = self.token(self.at) {
            match token.kind {
                TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
                TokenKind::Punct(b')' | b']' | b'}') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Punct(b',') if depth == 0 => break,
                _ if depth == 0 && token.newline_before && self.starts_key(self.at) => break,
                _ => {}
            }
            last = token;
            self.at += 1;
        }

        Span {
            start: first.start,
            end: last.end,
        }
    }

    /// A string, a name or a number — including a string with no closing
    /// quote, which is what a string being typed is.
    fn is_word(&self, token: Token) -> bool {
        match token.kind {
            TokenKind::Ident | TokenKind::String | TokenKind::Number => true,
            TokenKind::Invalid => self.quote(token).is_some(),
            _ => false,
        }
    }

    /// Whether a key and its colon start at `at`.
    fn starts_key(&self, at: usize) -> bool {
        self.token(at).is_some_and(|token| self.is_word(token))
            && self.token(at + 1).is_some_and(|token| token.is_punct(b':'))
    }

    /// Whether a one-token value is complete when the next token is at `at`.
    ///
    /// A separator completes it, and so does a name, string or number on a
    /// later line: that is the next key, being typed before the comma that
    /// should precede it — `quotes: "sin` with `semi` on the line below is
    /// two half-typed entries, not one value.
    fn ends_value(&self, at: usize) -> bool {
        match self.token(at) {
            None => true,
            Some(token) => {
                matches!(token.kind, TokenKind::Punct(b',' | b'}' | b']' | b')'))
                    || (token.newline_before && self.is_word(token))
            }
        }
    }

    fn quote(&self, token: Token) -> Option<u8> {
        match self.source.as_bytes().get(token.start) {
            Some(&quote @ (b'"' | b'\'')) => Some(quote),
            _ => None,
        }
    }

    fn word(&self, token: Token) -> Word {
        Word {
            span: Span {
                start: token.start,
                end: token.end,
            },
            quote: self.quote(token),
            terminated: token.kind != TokenKind::Invalid,
        }
    }
}

/// What lies between two tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Gap {
    /// A `,` is in it.
    pub(crate) comma: bool,
    /// A line break is in it, comments included.
    pub(crate) newline: bool,
    /// The first `:` in it.
    pub(crate) colon: Option<usize>,
}

/// The bytes from `from` to `to`, read as the space between two tokens:
/// whitespace, comments and punctuation.
///
/// [`None`] when `to` is inside a comment, which is the one place in a gap
/// where nothing may be offered, and when `from` is past `to`, which means the
/// caller's idea of the gap is wrong.
pub(crate) fn gap(source: &str, from: usize, to: usize) -> Option<Gap> {
    let bytes = source.as_bytes();
    let to = to.min(bytes.len());
    if from > to {
        return None;
    }

    let mut found = Gap::default();
    let mut at = from;
    while at < to {
        match (bytes[at], bytes.get(at + 1)) {
            (b'/', Some(b'/')) => {
                let end = bytes[at..]
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .map_or(bytes.len(), |length| at + length);
                if end >= to {
                    return None;
                }
                at = end;
            }
            (b'/', Some(b'*')) => {
                let end = source[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |length| at + 2 + length + 2);
                if end > to {
                    return None;
                }
                found.newline |= bytes[at..end].contains(&b'\n');
                at = end;
            }
            (b',', _) => {
                found.comma = true;
                at += 1;
            }
            (b'\n', _) => {
                found.newline = true;
                at += 1;
            }
            (b':', _) => {
                found.colon.get_or_insert(at);
                at += 1;
            }
            _ => at += 1,
        }
    }
    Some(found)
}

/// Byte offsets of line starts, for turning a parser position into one.
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

    /// Complete documents, written the ways a config is written: comments with
    /// braces and quotes in them, a template literal with a `}` inside, a
    /// regular expression, both quote styles, trailing commas, CRLF, text that
    /// is not ASCII, `module.exports`, and the member kinds no config uses but
    /// a document can still contain.
    const DOCUMENTS: &[&str] = &[
        r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    targets: ["web", "server"],
  },
  fmt: { quotes: "single", semicolons: false, indentWidth: 2 },
  // A comment with a brace { and a quote " in it.
  lint: {
    rules: { "flow/unclear-type": "off", 'react/component-syntax': 1 },
    /* a block comment: with a colon */
    files: ["src/**/*.js"],
  },
  plugins: ["a", { name: "b", order: "pre" }],
  tasks: {
    build: { command: `echo }`, inputs: [] },
    lint: "uf lint",
  },
  vite: { define: { __DEV__: true, pattern: /[{}]/.source }, base: null },
});
"#,
        "export default {\n  dev: { port: 5173, host: \"localhost\" },\n  ignore: [],\n};\n",
        "module.exports = defineConfig({ test: { coverage: { thresholds: { lines: 80 } } } });\n",
        "export default defineConfig({ app: { react: { strictMode: true, }, }, });\n",
        "export default defineConfig({\r\n  fmt: { quotes: \"double\" },\r\n});\r\n",
        "export default defineConfig({ docs: { app: \"文档\" }, site: { url: \"https://例え.jp\" } });\n",
        "export default defineConfig({});\n",
        "export default defineConfig({ ...base, [computed]: 1, shorthand, 1: \"one\" });\n",
    ];

    #[test]
    fn the_scanner_reads_every_document_the_way_the_parser_does() {
        for document in DOCUMENTS {
            let Parsed::Clean(Some(parsed)) = from_tree(document) else {
                panic!("the parser does not read {document:?}");
            };
            assert_eq!(from_tokens(document), Some(parsed), "{document}");
        }
    }

    /// Typing a document out one character at a time, asking at every step, is
    /// the shape of an editor session. No prefix may panic, and every prefix
    /// that has the config object's `{` in it reads as an object.
    #[test]
    fn every_prefix_of_a_document_reads_as_it_is_typed() {
        for document in DOCUMENTS {
            let open = from_tokens(document)
                .expect("a complete document reads")
                .open;
            for (end, _) in document.char_indices().skip(1) {
                let prefix = &document[..end];
                let object = from_tokens(prefix);
                assert_eq!(
                    object.is_some(),
                    end > open,
                    "{prefix:?} read as {object:?}"
                );
                if let Some(object) = object {
                    let _ = super::super::cursor::locate(prefix, &object, end);
                }
            }
        }
    }

    #[test]
    fn a_document_that_exports_something_else_has_no_config_object() {
        for document in [
            "export default function config() {}\n",
            "export default defineConfig(base);\n",
            "export default other({});\n",
            "const config = {};\n",
        ] {
            assert_eq!(read(document), None, "{document}");
            assert_eq!(from_tokens(document), None, "{document}");
        }
    }

    #[test]
    fn a_half_typed_document_is_read_with_its_gaps() {
        let source = "export default defineConfig({\n  fmt: {\n    quotes: \"sin\n    semi";
        let object = read(source).expect("a config object");
        assert_eq!(object.close, None);

        let Some(Value::Object(fmt)) = &object.entries[0].value else {
            panic!("`fmt` holds an object: {object:?}");
        };
        let quotes = &fmt.entries[0];
        let Some(Value::Word(value)) = quotes.value else {
            panic!("`quotes` holds a word: {fmt:?}");
        };
        assert!(!value.terminated);
        assert_eq!(value.text(source), "sin");
        // The next line's half-typed key is an entry of its own, not the rest
        // of the string: an unterminated string stops at the end of its line.
        assert_eq!(fmt.entries[1].key.map(|key| key.text(source)), Some("semi"));
    }

    #[test]
    fn a_gap_says_what_separates_two_tokens() {
        let source = "a, // note\n /* : */ b";
        assert_eq!(
            gap(source, 1, source.len() - 1),
            Some(Gap {
                comma: true,
                newline: true,
                colon: None
            })
        );
        // Inside either comment is nowhere to offer anything.
        assert_eq!(gap(source, 1, 6), None);
        assert_eq!(gap(source, 11, 16), None);
    }
}
