//! The scope model every check reads.
//!
//! Two questions decide almost every rule here, and both are answered from this
//! one stack:
//!
//! * *Which function is this token in?* — the nearest [`ScopeKind::is_function`]
//!   frame. A `component`, a `hook` and a `useX` function may call hooks; every
//!   other function may not, and being inside one of them means a token is no
//!   longer in render.
//! * *Is this token at the top level of that function?* — the frame's recorded
//!   [`Frame::depth`] against the current one. A `{` that is not a JSX
//!   container raises the depth, so a hook inside `if`, inside a loop, or
//!   inside a callback is exactly the case where the two disagree.

use uf_rsc::{Token, TokenKind};

/// What kind of `{ ... }` a frame on the stack represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// A Flow `component` body.
    Component,
    /// A Flow `hook` body.
    Hook,
    /// A plain function whose name follows the `useSomething` convention.
    UseFunction,
    /// Any other function, arrow, or class body.
    Function,
    /// A JSX expression container, which nests neither scope nor hook depth.
    Jsx,
    /// A block, an object literal, or anything else.
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

    /// Forget what the next `{` opens, at the end of a statement.
    pub fn forget(&mut self) {
        self.pending = None;
    }

    /// Open a frame for the `{` at `index`.
    pub fn open(&mut self, source: &str, tokens: &[Token], index: usize) -> ScopeKind {
        let kind = if self.parens == 0 {
            self.pending
                .take()
                .unwrap_or_else(|| classify(source, tokens, index))
        } else {
            classify(source, tokens, index)
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
        if kind != ScopeKind::Jsx {
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
            && frame.kind != ScopeKind::Jsx
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
/// Three cases matter, and the rest is a block. `<div>{…}` is a JSX expression
/// container and nests neither scope nor hook depth. A brace after `=>`, or
/// after the parameter list of a `function` expression, is a function body even
/// when it is an argument to something else: `items.map((item) => { … })` has
/// to be a function, or a `return` inside it would be blamed on the component
/// around it. Everything else is a block, which is the safe answer — an object
/// literal counted as a block only makes the top-level test stricter.
fn classify(source: &str, tokens: &[Token], index: usize) -> ScopeKind {
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
    ScopeKind::Block
}

/// Whether the parameter list opening at `open` belongs to a function
/// expression rather than to `if`, `for`, `while`, `switch` or `catch`.
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

/// Whether an identifier follows the `useSomething` hook naming convention.
pub fn is_hook_name(name: &str) -> bool {
    name.len() > 3 && name.starts_with("use") && name.as_bytes()[3].is_ascii_uppercase()
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
