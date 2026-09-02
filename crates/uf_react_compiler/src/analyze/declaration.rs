//! Declarations: what a name is, and what it is an alias of.
//!
//! Everything the mutation checks know starts here. A `component`'s parameters
//! are its props by construction — that is what the Flow `component` syntax
//! means — and a `const` whose initializer is a bare member chain is an alias
//! of whatever that chain is rooted in. Anything else builds a new value, and a
//! new value is the author's to write to.

use uf_rsc::TokenKind;

use crate::scope::ScopeKind;
use crate::scope::is_hook_name;
use crate::syntax::{
    ParamList, alias_root, parameter_list, parameters, return_type_body, statement_end,
};

use super::Walk;

impl<'a> Walk<'a> {
    /// `component Page(flag: boolean) { ... }`: the parameters are props.
    pub(super) fn declare_component(&mut self, index: usize) {
        self.stack.expect(ScopeKind::Component);
        let Some(open) = parameter_list(self.tokens, index + 1) else {
            return;
        };
        self.skip_return_type(open);
        let params = parameters(self.source, self.tokens, open);
        for name in &params {
            self.bindings.declare(name, false);
        }
        self.pending_props = Some(params);
    }

    /// `hook useThing(value) { ... }`: parameters are values, not props.
    pub(super) fn declare_hook(&mut self, index: usize) {
        self.stack.expect(ScopeKind::Hook);
        self.declare_parameters(index);
    }

    /// `function f() {}`, and the `useX` naming convention that makes one a hook.
    pub(super) fn declare_function(&mut self, index: usize) {
        let kind = match self.ident(index + 1) {
            Some(name) if is_hook_name(name) => ScopeKind::UseFunction,
            _ => ScopeKind::Function,
        };
        self.stack.expect(kind);
        self.declare_parameters(index);
    }

    /// Declare a declaration's parameters without giving them any other meaning.
    fn declare_parameters(&mut self, index: usize) {
        let Some(open) = parameter_list(self.tokens, index + 1) else {
            return;
        };
        self.skip_return_type(open);
        for name in parameters(self.source, self.tokens, open) {
            self.bindings.declare(&name, false);
        }
    }

    /// Step the walk over this declaration's return type, if it has one.
    ///
    /// A `renders` clause is covered too: `component P() renders [Node] {` puts
    /// the same brackets in the same place.
    fn skip_return_type(&mut self, open: usize) {
        if let Some(body) = return_type_body(self.tokens, open) {
            self.skip_until = Some(body);
        }
    }

    /// `const`, `let` and `var`, in both the plain and the destructuring form.
    pub(super) fn declare_variable(&mut self, index: usize) {
        let module_scope = !self.stack.in_function();
        let at = index + 1;

        let (names, equals) = match self.tokens.get(at).map(|token| token.kind) {
            Some(TokenKind::Punct(b'{' | b'[')) => {
                let names = self.pattern_names(at);
                (names, self.equals_after_pattern(at))
            }
            Some(TokenKind::Ident) => {
                let mut names = ParamList::new();
                if let Some(name) = self.ident(at) {
                    names.push(name.into());
                    if is_hook_name(name) {
                        self.stack.expect(ScopeKind::UseFunction);
                    }
                }
                (names, self.punct(at + 1, b'=').then_some(at + 1))
            }
            _ => return,
        };

        for name in &names {
            self.bindings.declare(name, module_scope);
        }

        let Some(equals) = equals else {
            return;
        };
        let end = statement_end(self.tokens, equals + 1);

        // `const ref = useRef(...)` is the only initializer whose *shape* the
        // ref check can trust; a ref passed around under another name is not
        // something a lexer can follow.
        if self.ident(equals + 1) == Some("useRef") && self.punct(equals + 2, b'(') {
            for name in &names {
                self.bindings.mark_ref(name);
            }
            return;
        }

        // An alias of a prop is a prop; a fresh object built from one is not.
        if let Some(root) = alias_root(self.tokens, equals, end)
            && let Some(source_name) = self.ident(root)
            && self.bindings.get(source_name).props
        {
            for name in &names {
                self.bindings.mark_props(name);
            }
        }
    }

    /// The names bound by a destructuring pattern opening at `open`.
    fn pattern_names(&self, open: usize) -> ParamList {
        /// Longest pattern uf will read, in tokens.
        const BUDGET: usize = 256;

        let mut names = ParamList::new();
        let mut depth = 0i32;
        let limit = (open + BUDGET).min(self.tokens.len());
        for at in open..limit {
            match self.tokens[at].kind {
                TokenKind::Punct(b'{' | b'[') => depth += 1,
                TokenKind::Punct(b'}' | b']') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::Ident => {
                    let previous = at.checked_sub(1).map(|index| self.tokens[index].kind);
                    // `{ a }` and `{ a: b }` both bind the last name in the
                    // group, which is the one a `:` or a `,` precedes.
                    let binds =
                        matches!(previous, Some(TokenKind::Punct(b'{' | b'[' | b',' | b':')));
                    if binds
                        && let Some(name) = self.ident(at)
                        && !self.punct(at + 1, b':')
                    {
                        names.push(name.into());
                    }
                }
                _ => {}
            }
        }
        names
    }

    /// The `=` that follows a destructuring pattern opening at `open`.
    fn equals_after_pattern(&self, open: usize) -> Option<usize> {
        /// Longest pattern uf will scan past, in tokens.
        const BUDGET: usize = 256;

        let mut depth = 0i32;
        let limit = (open + BUDGET).min(self.tokens.len());
        for at in open..limit {
            match self.tokens[at].kind {
                TokenKind::Punct(b'{' | b'[') => depth += 1,
                TokenKind::Punct(b'}' | b']') => {
                    depth -= 1;
                    if depth == 0 {
                        return self.punct(at + 1, b'=').then_some(at + 1);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// `import { a, b as c } from "..."`: every local name is module state.
    pub(super) fn declare_imports(&mut self, index: usize) {
        /// Longest import clause uf will read, in tokens.
        const BUDGET: usize = 512;

        let limit = (index + BUDGET).min(self.tokens.len());
        let mut at = index + 1;
        while at < limit {
            let token = &self.tokens[at];
            if token.kind == TokenKind::String || token.is_punct(b';') {
                return;
            }
            if token.kind == TokenKind::Ident {
                let name = token.text(self.source);
                if !matches!(name, "from" | "as" | "type" | "typeof") {
                    // `x as y` binds `y`; a bare `x` binds itself.
                    let local = match (self.ident(at + 1), self.ident(at + 2)) {
                        (Some("as"), Some(alias)) => alias,
                        _ => name,
                    };
                    self.bindings.declare(local, true);
                }
            }
            at += 1;
        }
    }
}
