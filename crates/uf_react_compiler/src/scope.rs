//! The scope model every check reads.
//!
//! Two questions decide almost every rule here, and both are answered from this
//! one stack:
//!
//! * *Which function is this token in?* — the nearest [`ScopeKind::is_function`]
//!   frame. A `component`, a `hook` and — in a module [`crate::convention`] has
//!   found React in — a `useX` function may call hooks; every other function
//!   may not, and being inside one of them means a token is no longer in
//!   render.
//! * *Is this token at the top level of that function?* — the frame's recorded
//!   [`Frame::depth`] against the current one. A `{` that opens something the
//!   surrounding statement may run zero or many times raises the depth, so a
//!   hook inside `if`, inside a loop, or inside a callback is exactly the case
//!   where the two disagree.
//!
//! # Which braces nest and which do not
//!
//! Not every `{` conditions what is inside it. Three do not, and
//! [`ScopeKind::nests`] is the list:
//!
//! | Brace | Runs |
//! | --- | --- |
//! | `if (…) { … }`, a loop body, `try { … }`, a callback | zero or many times |
//! | `<p>{ … }</p>` — a JSX expression container | once, where it stands |
//! | `value={ … }` — a JSX attribute | once, where it stands |
//! | `const bag = { … }` — an object literal | once, where it stands |
//!
//! The last two used to raise the depth, and a hook called in either was
//! reported as a conditional call — which it is not: the literal is the
//! initialiser of a top-level `const`, so every call in it runs, in order, on
//! every render, which is the whole of what the rule guarantees. See
//! ubugeeei-prod/uf#477.

use uf_rsc::{Token, TokenKind};

use crate::syntax::{is_assignment, is_jsx_attribute_value};

/// What kind of `{ ... }` a frame on the stack represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// A Flow `component` body.
    Component,
    /// A Flow `hook` body.
    Hook,
    /// A plain function whose name follows the `useSomething` convention, in a
    /// module that has something to do with React.
    ///
    /// The second half is not decoration: the convention is only evidence in a
    /// module that is React at all, which is [`crate::convention`]'s question
    /// and the caller's to ask before it names a frame this.
    UseFunction,
    /// Any other function, arrow, or class body.
    Function,
    /// A JSX expression container or attribute value, which nests neither
    /// scope nor hook depth.
    Jsx,
    /// An object literal, which is an expression the enclosing statement
    /// evaluates exactly once where it stands.
    ObjectLiteral,
    /// A block, or anything else.
    Block,
}

impl ScopeKind {
    /// Whether the frame is a function body of any kind.
    pub const fn is_function(self) -> bool {
        matches!(
            self,
            Self::Component | Self::Hook | Self::UseFunction | Self::Function
        )
    }

    /// Whether the brace raises hook-nesting depth.
    ///
    /// False for the two kinds of brace that open an *expression* the enclosing
    /// statement evaluates exactly once: a JSX container or attribute, and an
    /// object literal. A hook called inside either runs on every render, in the
    /// same order, which is precisely the condition `react/hooks-rules` exists
    /// to check — so counting them would report a call that is not conditional.
    pub const fn nests(self) -> bool {
        !matches!(self, Self::Jsx | Self::ObjectLiteral)
    }

    /// Whether hooks may be called directly in this frame.
    pub const fn allows_hooks(self) -> bool {
        matches!(self, Self::Component | Self::Hook | Self::UseFunction)
    }

    /// Whether the frame's body runs while React renders.
    pub const fn is_render(self) -> bool {
        self.allows_hooks()
    }
}

/// One open `{` during the walk.
#[derive(Debug, Clone)]
pub struct Frame {
    /// What the brace opened.
    pub kind: ScopeKind,
    /// Hook-nesting depth *inside* this frame.
    pub depth: u32,
    /// Set once a `return` belonging to this function frame has finished.
    ///
    /// A hook after it is either unreachable or conditional; both are bugs, and
    /// neither is something a compiler can memoize.
    pub returned: bool,
    /// Set while a `return` statement in this function frame is still open, so
    /// that a hook *inside* the returned expression is not blamed on it.
    pub returning: bool,
}

/// The scope stack, plus the two counters the classification needs.
#[derive(Debug, Default)]
pub struct ScopeStack {
    frames: Vec<Frame>,
    depth: u32,
    /// How many `(` are open. A `{` inside parentheses is an expression.
    pub parens: u32,
    /// What the next `{` at parenthesis depth zero opens, when it is known.
    pending: Option<ScopeKind>,
    /// The exact `{` [`Self::pending`] describes, when the declaration knew it.
    ///
    /// A `component`, a `hook` and a `function` can all say which brace is
    /// their body — the caller finds it with `return_type_body` — and a brace
    /// named that way is the body wherever it stands. Without this, a
    /// declaration inside a call's arguments lost its answer to the
    /// parenthesis test below, and every `it("…", () => { component Probe() {
    /// … } })` in a test file had its hooks reported as being called outside a
    /// component.
    pending_body: Option<usize>,
}

impl ScopeStack {
    /// An empty stack, at module scope.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many frames are open.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the walk is at module scope.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// The current hook-nesting depth.
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    /// The nearest enclosing function frame.
    pub fn function(&self) -> Option<&Frame> {
        self.frames
            .iter()
            .rev()
            .find(|frame| frame.kind.is_function())
    }

    /// The nearest enclosing function frame, mutably.
    pub fn function_mut(&mut self) -> Option<&mut Frame> {
        self.frames
            .iter_mut()
            .rev()
            .find(|frame| frame.kind.is_function())
    }

    /// Whether the walk is inside a function body at all.
    pub fn in_function(&self) -> bool {
        self.function().is_some()
    }

    /// Whether the walk is positioned in code that runs during render.
    ///
    /// True inside a `component`, `hook` or `useX` body — blocks and JSX
    /// containers included — and false inside any function nested in one, since
    /// uf cannot tell an event handler from a callback that runs during render
    /// without knowing what it is passed to.
    pub fn in_render(&self) -> bool {
        self.function().is_some_and(|frame| frame.kind.is_render())
    }

    /// Remember what the next `{` opens.
    ///
    /// A trailing `=>` must not downgrade a hook-eligible declaration:
    /// `const useThing = (): number => {` sets `UseFunction` at the `const` and
    /// then `Function` at the arrow, and the first one is the true answer.
    pub fn expect(&mut self, kind: ScopeKind) {
        match (self.pending, kind) {
            (Some(existing), ScopeKind::Function) if existing.allows_hooks() => {}
            _ => self.pending = Some(kind),
        }
    }

    /// Say which `{` the pending kind belongs to.
    ///
    /// Stronger than [`Self::expect`] alone, because a named brace survives the
    /// parenthesis test in [`Self::open`]. A declaration knows which brace is
    /// its own body; being written inside a call's arguments does not change
    /// that, and a test file is nothing but declarations written there.
    pub fn name_body(&mut self, body: usize) {
        self.pending_body = Some(body);
    }

    /// Forget what the next `{` opens, at the end of a statement.
    pub fn forget(&mut self) {
        self.pending = None;
        self.pending_body = None;
    }

    /// Open a frame for the `{` at `index`.
    pub fn open(&mut self, source: &str, tokens: &[Token], index: usize) -> ScopeKind {
        let named = self.pending_body == Some(index);
        if named {
            self.pending_body = None;
        }
        // Read before `self.pending` is borrowed, and passed down rather than
        // looked up inside [`classify`]: a `{` after a `:` is a property value
        // when — and only when — it stands inside an object literal, and a free
        // function has no way to know that.
        let in_object_literal = self
            .frames
            .last()
            .is_some_and(|frame| frame.kind == ScopeKind::ObjectLiteral);
        let kind = if named || self.parens == 0 {
            self.pending
                .take()
                .unwrap_or_else(|| classify(source, tokens, index, in_object_literal))
        } else {
            classify(source, tokens, index, in_object_literal)
        };
        self.push(kind);
        kind
    }

    /// Open a frame for an arrow with a concise body, which has no brace.
    ///
    /// `() => box.current` is a function even though nothing in the token
    /// stream closes it, and treating it as one is what keeps a value read
    /// inside an event handler from being blamed on the render around it.
    pub fn open_concise(&mut self) -> ScopeKind {
        let kind = match self.parens {
            0 => self.pending.take().unwrap_or(ScopeKind::Function),
            _ => ScopeKind::Function,
        };
        self.push(kind);
        kind
    }

    fn push(&mut self, kind: ScopeKind) {
        if kind.nests() {
            self.depth += 1;
        }
        self.frames.push(Frame {
            kind,
            depth: self.depth,
            returned: false,
            returning: false,
        });
    }

    /// Close the innermost frame.
    pub fn close(&mut self) {
        if let Some(frame) = self.frames.pop()
            && frame.kind.nests()
        {
            self.depth = self.depth.saturating_sub(1);
        }
        self.pending = None;
    }

    /// Record that a `return` statement has started in the current function.
    pub fn start_return(&mut self) {
        if let Some(frame) = self.function_mut() {
            frame.returning = true;
        }
    }

    /// Record that the open `return` statement has finished.
    pub fn finish_return(&mut self) {
        if let Some(frame) = self.function_mut()
            && frame.returning
        {
            frame.returning = false;
            frame.returned = true;
        }
    }
}

/// What a `{` opens, judged from the token before it.
///
/// Used when a declaration has not already said what the brace opens — which is
/// every brace inside a parameter list or an argument list, where the pending
/// answer belongs to the declaration still being read.
///
/// Four cases matter, and the rest is a block. `<div>{…}` is a JSX expression
/// container and nests neither scope nor hook depth. A brace after `=>`, or
/// after the parameter list of a `function` expression, is a function body even
/// when it is an argument to something else: `items.map((item) => { … })` has
/// to be a function, or a `return` inside it would be blamed on the component
/// around it. An object literal — see [`opens_an_object_literal`] — is an
/// expression the enclosing statement evaluates once. Everything else is a
/// block.
///
/// `in_object_literal` says whether the innermost open frame is an object
/// literal, which is the one thing the caller knows and the token stream does
/// not; it decides the `:` row of [`opens_an_object_literal`].
fn classify(source: &str, tokens: &[Token], index: usize, in_object_literal: bool) -> ScopeKind {
    let Some(previous) = index.checked_sub(1).and_then(|at| tokens.get(at)) else {
        return ScopeKind::Block;
    };
    if previous.kind == TokenKind::Arrow {
        return ScopeKind::Function;
    }
    if previous.is_punct(b'>') {
        let before = index
            .checked_sub(2)
            .and_then(|at| tokens.get(at))
            .map(|token| token.text(source));
        // `=>` lexes as one token, so a `>` here that follows `-` is an arrow
        // written the old way and a bare `>` is the end of a JSX opening tag.
        if before == Some("-") {
            return ScopeKind::Block;
        }
        return ScopeKind::Jsx;
    }
    if previous.is_punct(b')')
        && let Some(open) = uf_rsc::matching_open(tokens, index - 1, b'(', b')')
        && is_function_head(source, tokens, open)
    {
        return ScopeKind::Function;
    }
    // `value={…}` on a JSX element, and `<T = {…}>` in a type parameter list.
    // Both are read once where they stand, and the second is not code at all.
    if is_jsx_attribute_value(tokens, index) {
        return ScopeKind::Jsx;
    }
    if opens_an_object_literal(source, tokens, index, in_object_literal) {
        return ScopeKind::ObjectLiteral;
    }
    // A container the JSX text runs into: `<p>hello {name}</p>`. The brace
    // follows a word rather than the `>` that ended the opening tag, and it
    // still nests nothing — so a hook called there was reported as being
    // called somewhere conditional, which a JSX container never is.
    if !starts_statement(tokens, index) && !opens_a_block_after(source, tokens, index - 1) {
        return ScopeKind::Jsx;
    }
    ScopeKind::Block
}

/// Whether the `{` at `index` opens an object literal.
///
/// Answered from the token in front of it, and the list is closed because the
/// grammar's is: an object literal stands where an *expression* may stand, and
/// the places a brace can follow and still be one are an initialiser or
/// assignment, an argument list, an array, a `return`, a comma, and the `:` of
/// a property whose value is another object.
///
/// | | before | opens |
/// | --- | --- | --- |
/// | `const bag = { … }` | `=` | an object literal |
/// | `f({ … })` | `(` | an object literal |
/// | `[{ … }]` | `[` | an object literal |
/// | `f(a, { … })` | `,` | an object literal |
/// | `return { … }` | `return` | an object literal |
/// | `{ outer: { … } }` | `:` | an object literal, inside one |
/// | `case 1: { … }` | `:` | a block, outside one |
/// | `if (flag) { … }` | `)` | a block |
/// | `{ method() { … } }` | `)` | a block, which under-describes a method body
///   and still keeps it nesting |
///
/// [`is_assignment`] is what the `=` row asks, because two other `=`s can
/// stand there and neither introduces a value: a comparison, and the `=` of a
/// JSX attribute — which [`classify`] has already answered above this.
///
/// The `:` row is why `in_object_literal` is a parameter: a label and a `case`
/// clause both put a block after a colon, and neither can appear inside an
/// object literal, so the enclosing frame settles it.
fn opens_an_object_literal(
    source: &str,
    tokens: &[Token],
    index: usize,
    in_object_literal: bool,
) -> bool {
    let Some(at) = index.checked_sub(1) else {
        return false;
    };
    match tokens[at].kind {
        TokenKind::Punct(b'=') => is_assignment(tokens, at),
        TokenKind::Punct(b'(' | b'[' | b',') => true,
        TokenKind::Punct(b':') => in_object_literal,
        TokenKind::Ident => ident_at(source, tokens, at) == Some("return"),
        _ => false,
    }
}

/// Whether a `{` following the token at `at` can open a block.
///
/// Deliberately answered from what a block may follow rather than from what
/// JSX looks like, because the first list is short and closed: `else`, `do`,
/// `try` and `finally`, the `)` of an `if`, `for`, `while`, `switch` or
/// `catch`, and a statement boundary. A brace anywhere else is an expression
/// — a JSX container or an object literal — and neither conditions what is
/// inside it.
///
/// Conservative where it cannot tell: an unrecognised token answers yes, so a
/// brace uf cannot read keeps raising hook depth instead of quietly lowering
/// it. That direction matters, because the wrong answer here would hide a
/// hook that really is called conditionally.
fn opens_a_block_after(source: &str, tokens: &[Token], at: usize) -> bool {
    match tokens[at].kind {
        TokenKind::Ident => {
            let word = tokens[at].text(source);
            matches!(word, "else" | "do" | "try" | "finally")
                // `class Name {` is a body, and it is named by the word in
                // front of the name rather than by the brace.
                || at
                    .checked_sub(1)
                    .and_then(|before| ident_at(source, tokens, before))
                    .is_some_and(|before| matches!(before, "class" | "function"))
        }
        // Text and punctuation a block can never follow. `{` after a comma is
        // an object literal in an argument list or an array, which is an
        // expression as much as a container is.
        TokenKind::String | TokenKind::Template | TokenKind::Number => false,
        TokenKind::Punct(b',' | b'!' | b'?' | b'.') => false,
        _ => true,
    }
}

/// Whether the parameter list opening at `open` belongs to a function
/// expression rather than to `if`, `for`, `while`, `switch` or `catch`.
///
/// Only `function` is recognised here, and deliberately: a `component`, a
/// `hook` and a named `function` all name their own body brace through
/// [`ScopeStack::expect_body`], which is exact. This is the fallback for a
/// function expression that named nothing.
fn is_function_head(source: &str, tokens: &[Token], open: usize) -> bool {
    let word = |at: usize| ident_at(source, tokens, at);
    match open.checked_sub(1).and_then(word) {
        // `function (…) {`
        Some("function") => true,
        // `function name(…) {`
        Some(_) => open.checked_sub(2).and_then(word) == Some("function"),
        None => false,
    }
}

/// Whether the token at `index` starts a statement.
///
/// A newline counts as well as a `;`, because a module written without
/// semicolons still starts a statement on every line.
pub fn starts_statement(tokens: &[Token], index: usize) -> bool {
    match index.checked_sub(1) {
        None => true,
        Some(previous) => {
            let token = &tokens[previous];
            token.is_punct(b';')
                || token.is_punct(b'{')
                || token.is_punct(b'}')
                || tokens[index].newline_before
        }
    }
}

/// The identifier text at `index`, if the token there is one.
pub fn ident_at<'a>(source: &'a str, tokens: &[Token], index: usize) -> Option<&'a str> {
    tokens
        .get(index)
        .filter(|token| token.kind == TokenKind::Ident)
        .map(|token| token.text(source))
}
