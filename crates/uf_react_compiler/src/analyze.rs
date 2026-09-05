//! One walk over one module, producing every finding.
//!
//! The walk is a single pass over the token vector with an explicit scope
//! stack. Nothing here recurses and nothing re-scans the module, so the cost of
//! validating a project is linear in its bytes — which matters, because this
//! runs over every module of every build.

mod declaration;
mod render;

use uf_infra::LineIndex;
use uf_rsc::{Token, TokenKind, tokenize};

use crate::bindings::{Bindings, is_mutating_method};
use crate::error::{MAX_DIAGNOSTICS, MAX_SCOPE_DEPTH, MAX_SOURCE_BYTES, ReactCompilerError};
use crate::rule::{Finding, ReactDiagnostic};
use crate::scope::{ScopeKind, ScopeStack, ident_at, is_hook_name, starts_statement};
use crate::syntax::{
    ParamList, argument_names, compound_assignment, concise_body_end, is_assignment, member_root,
};

/// Everything the walk carries between tokens.
pub(crate) struct Walk<'a> {
    pub source: &'a str,
    pub tokens: &'a [Token],
    pub lines: LineIndex,
    pub stack: ScopeStack,
    pub bindings: Bindings,
    pub diagnostics: Vec<ReactDiagnostic>,
    /// The previous identifier on this line, cleared by any other token.
    pub previous: Option<&'a str>,
    /// Parameters of a `component` whose body has not opened yet.
    pending_props: Option<ParamList>,
    /// Frames whose props go out of scope when they close.
    props_frames: Vec<(usize, ParamList)>,
    /// Token indices at which an arrow's concise body ends, innermost last.
    concise_ends: Vec<usize>,
    /// The body `{` of a declaration whose return type the walk is stepping over.
    ///
    /// Set by the `component`, `hook` and `function` declarations; see
    /// [`crate::syntax::return_type_body`] for why a type must not be walked.
    skip_until: Option<usize>,
}

/// Validate one module.
pub fn validate(source: &str) -> Result<Vec<ReactDiagnostic>, ReactCompilerError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ReactCompilerError::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_SOURCE_BYTES,
        });
    }

    let tokens = tokenize(source);
    let mut walk = Walk {
        source,
        tokens: &tokens,
        lines: LineIndex::new(source),
        stack: ScopeStack::new(),
        bindings: Bindings::new(),
        diagnostics: Vec::new(),
        previous: None,
        pending_props: None,
        props_frames: Vec::new(),
        concise_ends: Vec::new(),
        skip_until: None,
    };

    for index in 0..tokens.len() {
        // A concise arrow body ends where its expression does, not at a brace.
        while walk.concise_ends.last() == Some(&index) {
            walk.concise_ends.pop();
            walk.stack.close();
        }

        // A return type is not code: `hook useX(): [number, () => void] {` holds
        // an `=>` that belongs to a type, and reading it as an arrow opened a
        // frame the real body then sat inside — so every hook call in that body
        // looked like it was outside a hook.
        if let Some(body) = walk.skip_until {
            if index < body {
                continue;
            }
            walk.skip_until = None;
        }

        let token = &tokens[index];
        if token.newline_before {
            walk.previous = None;
        }
        match token.kind {
            TokenKind::Punct(b'(') => walk.stack.parens += 1,
            TokenKind::Punct(b')') => walk.stack.parens = walk.stack.parens.saturating_sub(1),
            TokenKind::Punct(b'{') => {
                walk.open_scope(index)?;
                walk.previous = None;
                continue;
            }
            TokenKind::Punct(b'}') => {
                walk.close_scope();
                walk.previous = None;
                continue;
            }
            TokenKind::Punct(b';') => {
                walk.stack.finish_return();
                walk.stack.forget();
            }
            TokenKind::Arrow => walk.arrow(index),
            TokenKind::Punct(b'=') if is_assignment(&tokens, index) => walk.assignment(index),
            TokenKind::Ident => {
                let word = token.text(source);
                walk.identifier(index, word);
                walk.previous = Some(word);
                continue;
            }
            _ => {}
        }
        walk.previous = None;
    }

    walk.diagnostics.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.finding.cmp(&right.finding))
    });
    Ok(walk.diagnostics)
}

impl<'a> Walk<'a> {
    /// Record a finding at the token at `index`.
    pub(crate) fn report(&mut self, index: usize, finding: Finding, symbol: Option<&str>) {
        if self.diagnostics.len() >= MAX_DIAGNOSTICS {
            return;
        }
        let Some(token) = self.tokens.get(index) else {
            return;
        };
        let position = self.lines.line_col(token.start);
        self.diagnostics.push(ReactDiagnostic {
            finding,
            line: position.line,
            column: position.column,
            span: token.end.saturating_sub(token.start).max(1),
            symbol: symbol.map(Into::into),
        });
    }

    /// The text of the token at `index`, if it is an identifier.
    pub(crate) fn ident(&self, index: usize) -> Option<&'a str> {
        ident_at(self.source, self.tokens, index)
    }

    /// Whether the token at `index` is exactly the punctuation `byte`.
    pub(crate) fn punct(&self, index: usize, byte: u8) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.is_punct(byte))
    }

    fn open_scope(&mut self, index: usize) -> Result<(), ReactCompilerError> {
        if self.stack.len() >= MAX_SCOPE_DEPTH {
            return Err(ReactCompilerError::ScopeTooDeep {
                limit: MAX_SCOPE_DEPTH,
                offset: self.tokens[index].start,
            });
        }
        let kind = self.stack.open(self.source, self.tokens, index);
        if kind.is_function()
            && let Some(params) = self.pending_props.take()
        {
            for name in &params {
                self.bindings.mark_props(name);
            }
            self.props_frames.push((self.stack.len(), params));
        }
        Ok(())
    }

    fn close_scope(&mut self) {
        if self
            .props_frames
            .last()
            .is_some_and(|(at, _)| *at == self.stack.len())
            && let Some((_, params)) = self.props_frames.pop()
        {
            for name in &params {
                self.bindings.clear_props(name);
            }
        }
        self.stack.close();
    }

    /// Whether the contextual keyword at `index` is followed by a declaration.
    ///
    /// `component` and `hook` are contextual keywords: both are ordinary
    /// identifiers everywhere they are not immediately followed by a name and
    /// a parameter list. React's own Fast Refresh runtime holds the DevTools
    /// global in a variable called `hook` and writes to it at the start of a
    /// line — where there is no preceding identifier to say otherwise — and
    /// the walk read `hook.inject = …` as a hook declaration. The body it then
    /// opened swallowed the rest of the enclosing function, so every write to
    /// module state inside it was reported as a write during render.
    ///
    /// A name and then `(`, or `<` for a generic. Nothing else is either
    /// keyword.
    fn names_a_declaration(&self, index: usize) -> bool {
        self.ident(index + 1).is_some()
            && (self.punct(index + 2, b'(') || self.punct(index + 2, b'<'))
    }

    /// Handle one identifier token.
    fn identifier(&mut self, index: usize, word: &'a str) {
        let declaration = self.previous.is_none()
            || matches!(self.previous, Some("export" | "declare" | "default"));

        match word {
            "component" if declaration && self.names_a_declaration(index) => {
                return self.declare_component(index);
            }
            "hook" if declaration && self.names_a_declaration(index) => {
                return self.declare_hook(index);
            }
            "function" => return self.declare_function(index),
            "class" => return self.stack.expect(ScopeKind::Function),
            "return" => return self.stack.start_return(),
            "const" | "let" | "var" => return self.declare_variable(index),
            "import" if starts_statement(self.tokens, index) => {
                return self.declare_imports(index);
            }
            "delete" => return,
            _ => {}
        }

        // A name being bound is not a use of it.
        if matches!(
            self.previous,
            Some("function" | "const" | "let" | "var" | "component" | "hook" | "class")
        ) {
            return;
        }

        let after_dot = index.checked_sub(1).is_some_and(|at| self.punct(at, b'.'));

        if after_dot {
            if is_mutating_method(word) && self.punct(index + 1, b'(') {
                self.mutating_call(index);
            }
            return;
        }

        if is_hook_name(word) && self.punct(index + 1, b'(') {
            self.hook_call(index, word);
            return;
        }

        if self.previous == Some("delete") {
            self.written(index, index, word);
            return;
        }

        if self.update_expression(index) {
            self.written(index, index, word);
            return;
        }

        self.render_read(index, word);
    }

    /// Handle a hook call: where it sits, and what it was handed.
    fn hook_call(&mut self, index: usize, word: &'a str) {
        let finding = match self.stack.function() {
            None => Some(Finding::HookOutsideComponent),
            Some(frame) if !frame.kind.allows_hooks() => Some(Finding::HookOutsideComponent),
            Some(frame) if frame.depth != self.stack.depth() => Some(Finding::HookNotAtTopLevel),
            Some(frame) if frame.returned => Some(Finding::HookAfterEarlyReturn),
            Some(_) => None,
        };
        if let Some(finding) = finding {
            self.report(index, finding, Some(word));
        }

        for name in argument_names(self.source, self.tokens, index + 1) {
            self.bindings.mark_passed_to_hook(&name);
        }
    }

    /// Handle `receiver.push(...)` and the other methods that write.
    fn mutating_call(&mut self, index: usize) {
        let Some(root) = index
            .checked_sub(2)
            .and_then(|end| member_root(self.tokens, end))
        else {
            return;
        };
        let Some(name) = self.ident(root) else {
            return;
        };
        self.written(index, root, name);
    }

    /// Handle an `=>`, opening a frame when its body has no brace.
    fn arrow(&mut self, index: usize) {
        self.stack.expect(ScopeKind::Function);
        if self.punct(index + 1, b'{') {
            return;
        }
        self.stack.open_concise();
        self.concise_ends
            .push(concise_body_end(self.source, self.tokens, index + 1));
    }

    /// Handle the `=` of an assignment.
    ///
    /// `total += 1` puts the operator between the target and the `=`, so the
    /// target's member chain ends one token further back than it does for a
    /// plain `=`.
    fn assignment(&mut self, index: usize) {
        // `const mode: Mode = "onSubmit"` also ends in a name and an `=`, and
        // the name is a type rather than the thing being written.
        if self.initializes_an_annotated_declarator(index) {
            return;
        }
        let skip = usize::from(compound_assignment(self.tokens, index)) + 1;
        let Some(root) = index
            .checked_sub(skip)
            .and_then(|end| member_root(self.tokens, end))
        else {
            return;
        };
        let Some(name) = self.ident(root) else {
            return;
        };
        // `const x = ...` is a declaration, and was handled at the keyword.
        if root
            .checked_sub(1)
            .and_then(|at| self.ident(at))
            .is_some_and(|word| matches!(word, "const" | "let" | "var"))
        {
            return;
        }
        self.written(index, root, name);
    }

    /// Whether the `=` at `index` initializes a declarator with a type
    /// annotation.
    ///
    /// `const mode: Mode = …` reads, backwards from the `=`, as `Mode =`,
    /// which is the shape of a write to `Mode`. The annotation is not code:
    /// it names a type, and a type is never written to. So the declaration
    /// this `=` belongs to has to be looked for in front of the annotation
    /// rather than immediately in front of the `=`.
    ///
    /// The scan gives up on anything it cannot read — an unbalanced bracket,
    /// an annotation longer than the budget — and giving up means falling
    /// through to the ordinary write handling, which is what happened before
    /// this existed.
    fn initializes_an_annotated_declarator(&self, index: usize) -> bool {
        /// Longest annotation uf will scan back over, in tokens.
        const BUDGET: usize = 256;

        let mut depth = 0i32;
        let floor = index.saturating_sub(BUDGET);
        let mut at = index;
        while at > floor {
            at -= 1;
            match self.tokens[at].kind {
                TokenKind::Punct(b')' | b']' | b'}') => depth += 1,
                TokenKind::Punct(b'(' | b'[' | b'{') => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                // The `=` of an arrow in a function type — `const f: (a: A) =>
                // B = g` — is part of the annotation rather than the end of
                // one, and it is the only `=` that can be.
                TokenKind::Punct(b'=') if depth == 0 && !self.arrow_at(at) => return false,
                TokenKind::Punct(b';') if depth == 0 => return false,
                TokenKind::Punct(b':') if depth == 0 => {
                    // In front of the annotation stands the declarator, and in
                    // front of that the keyword — or the comma of a second
                    // declarator, as in `let a = 1, b: B = 2`.
                    if at
                        .checked_sub(1)
                        .and_then(|name| self.ident(name))
                        .is_none()
                    {
                        return false;
                    }
                    return at.checked_sub(2).is_some_and(|before| {
                        self.punct(before, b',')
                            || self
                                .ident(before)
                                .is_some_and(|word| matches!(word, "const" | "let" | "var"))
                    });
                }
                _ => {}
            }
        }
        false
    }

    /// Whether the `=` at `index` is the first half of an `=>`.
    fn arrow_at(&self, index: usize) -> bool {
        match (self.tokens.get(index), self.tokens.get(index + 1)) {
            (Some(equals), Some(greater)) => equals.end == greater.start && greater.is_punct(b'>'),
            _ => false,
        }
    }

    /// Report a write to `name`, whichever rule owns it.
    ///
    /// The order is deliberate: writing to a prop is wrong wherever it happens,
    /// writing to a value a hook has seen is wrong wherever it happens, and
    /// writing to module state is only wrong while rendering. Reporting the
    /// most specific one keeps a single mistake from producing three
    /// diagnostics that all point at the same line.
    fn written(&mut self, at: usize, root: usize, name: &str) {
        let facts = self.bindings.get(name);
        let finding = if facts.props {
            Finding::PropsMutated
        } else if facts.passed_to_hook {
            Finding::MutationAfterHook
        } else if facts.module_scope && self.stack.in_render() {
            Finding::ModuleBindingAssigned
        } else {
            return;
        };
        self.report(root.min(at), finding, Some(name));
    }

    /// Whether the identifier at `index` is the operand of `++` or `--`.
    ///
    /// The two punctuation tokens have to be adjacent in the source, so `a + +b`
    /// is not mistaken for `a++`.
    fn update_expression(&self, index: usize) -> bool {
        let adjacent =
            |first: usize, second: usize| match (self.tokens.get(first), self.tokens.get(second)) {
                (Some(left), Some(right)) => {
                    left.end == right.start
                        && matches!(left.kind, TokenKind::Punct(b'+' | b'-'))
                        && left.kind == right.kind
                }
                _ => false,
            };
        adjacent(index + 1, index + 2)
            || index
                .checked_sub(2)
                .is_some_and(|first| adjacent(first, first + 1))
    }
}
