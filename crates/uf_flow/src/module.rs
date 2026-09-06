//! Reading a file as an ES module, which is where `await` may stand alone.
//!
//! Every entry point in `uf` that hands source to the Flow parser hands it
//! [`parse`] rather than calling the port directly, for the reason
//! [`PARSE_OPTIONS`](crate::PARSE_OPTIONS) is one constant: a file that parses
//! for the checker and not for the formatter is worse than one that parses for
//! neither.
//!
//! # What the port cannot be asked
//!
//! ECMAScript has two goal symbols, *Script* and *Module*, and `await` is a
//! reserved word in only one of them. At the top level of a module `await x`
//! is an operator — that is ES2022, and Node has run it for years. In a
//! script `await` is an ordinary identifier and `await x` is two expressions
//! with nothing between them.
//!
//! The port has no way to say which one it is reading.
//! `ParserEnvFlags::allow_await` is initialised `false`
//! (`parser_env.rs`) and turned on only by entering an `async` function, and
//! `ParseOptions` has no member for it — the port's own
//! `flow_parser_wasm` says so where it maps Hermes' `source_type`: "flow_parser
//! has no script/module gate … top-level `await` is rejected". Nor can uf turn
//! it on from outside: `with_allow_await` is `pub(crate)`, and `upstream/flow`
//! is a vendored submodule that `tools/upstream/sync.sh` re-clones with no
//! patch step, so a change there would be deleted rather than reviewed.
//!
//! # What this does instead
//!
//! It asks the port the same question in words the port has an answer for.
//! `await` and `void ` are both five bytes and both an ECMAScript
//! *UnaryExpression*, so replacing one with the other leaves every other byte
//! of the file at the offset it had, and the tree the port builds for
//! `void  load()` is the tree it would build for `await load()` with one field
//! different:
//!
//! ```text
//! await load()  ->  Unary { operator: Await, argument: Call@6..12 }   loc 0..12
//! void  load()  ->  Unary { operator: Void,  argument: Call@6..12 }   loc 0..12
//! ```
//!
//! `for await (… of …)` is the same trick with five spaces: `for      (x of y)`
//! is a `ForOf` whose `loc` starts on the `for`, needing only `await_` set.
//!
//! So the operator is restored on the way out, at the position it was written,
//! and nothing else about the tree is uf's invention. Every location, every
//! comment and every other node came from the port reading the author's own
//! bytes.
//!
//! # The parser decides which `await`s those are, not a heuristic
//!
//! Which `await` is an operator and which is a property name is a question
//! only a parser can answer — `{ await() {} }` and `await (x)` are the same
//! two tokens — and a token-level guess would be the second, worse parser this
//! crate exists to avoid. So the guess is never made. Every `await` in the
//! file is offered to the port, the resulting tree says which of them became a
//! top-level `Unary`, and the ones that did not are put back and the file is
//! read again. The loop only ever *withdraws* candidates, so it settles; see
//! [`MAX_PASSES`].
//!
//! That is also what keeps the two refusals the language still makes:
//!
//! * `await` inside a **non-async function** lands at a function depth this
//!   module will not repair, so the `await` goes back and the parser refuses
//!   it as it always did;
//! * `await` in a **script** never reaches any of this, because [`goal`] reads
//!   the file as a script and [`parse`] stops there.

use flow_parser::ast;
use flow_parser::ast::expression::UnaryOperator;
use flow_parser::ast_visitor::{self, AstVisitor};
use flow_parser::file_key::FileKey;
use flow_parser::loc::{Loc, Position};
use flow_parser::parse_error::ParseError;
use flow_parser::{ParseOptions, ast::statement::ForOf};

use crate::scan::{Token, tokenize};

/// One source file as the port read it: the tree, and the syntax errors beside
/// it.
///
/// The port recovers, so there is always a tree; the errors say whether it is
/// the one the author meant.
pub type Program = (ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>);

/// The `await` keyword, whose five bytes this module trades for another five.
const AWAIT: &str = "await";

/// What `await` becomes so the port will read it as an operator.
///
/// Four letters and a space: the same five bytes, the same production, the
/// same precedence. See the module documentation.
const VOID: &str = "void ";

/// What the `await` of a `for await` becomes: nothing at all, five spaces
/// wide, leaving an ordinary `for (… of …)` whose `await_` is set afterwards.
const BLANK: &str = "     ";

/// How many times [`parse`] will offer a narrowed set of `await`s to the port.
///
/// Each pass either settles or *withdraws* at least one candidate, so the loop
/// terminates on its own; this bounds the work rather than the answer. Real
/// source settles in one pass, or two when the file also contains an `await`
/// that is a property name or sits in a function — every source in this
/// repository, its packages and its tests does. A file that has not settled by
/// the fourth pass has syntax errors of its own, and for such a file the
/// answer this module falls back to — what the port said about the bytes the
/// author actually wrote — is the right one anyway.
const MAX_PASSES: usize = 4;

/// Which ECMAScript goal symbol a source is read with.
///
/// The difference that matters here is `await`: reserved in a module, an
/// ordinary identifier in a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// A module. `await` may stand at the top level.
    Module,
    /// A script. `await` is an identifier, and `await x` is not an operator.
    Script,
}

/// Whether `source` is a module or a script, decided from its own syntax.
///
/// A file is a module when it contains syntax only a module may contain: an
/// `import` or `export` declaration, or `import.meta`. That is the rule Babel
/// calls `sourceType: "unambiguous"`, and it is the one uf can apply
/// everywhere — `uf fmt` formats source text with no path beside it, the
/// transform answers a request over a pipe, and the checker holds a `FileKey`.
/// A rule any of them could not evaluate would be a rule that gave the same
/// bytes different answers from different commands, which is the thing this
/// module exists to prevent.
///
/// It is also the conservative direction. `await` is reserved only in a
/// module, so reading a file as one can change what its `await` means;
/// requiring the file to have said it is a module first means no file that was
/// not already an ES module changes meaning at all.
///
/// # What it does not cover
///
/// A module with a top-level `await` and no `import`, `export` or
/// `import.meta` reads as a script here and is refused, where Node — which
/// knows the file's extension and its package's `type` — would run it. uf has
/// no such file: every module it generates or ships imports something.
///
/// # Why tokens and not the tree
///
/// `import` and `export` are reserved words, so the only place either can
/// appear other than as the declaration it names is as an *IdentifierName*:
/// after a `.`, or as a property key before a `:` or a method's `(`. Those
/// three exclusions are exact rather than a guess, which a scan over the tree
/// would not be — the tree is the parser's recovered guess precisely when the
/// file has the top-level `await` that brought us here.
#[must_use]
pub fn goal(source: &str) -> Goal {
    goal_of(source, &tokenize(source))
}

/// [`goal`], for a caller that has already tokenized.
fn goal_of(source: &str, tokens: &[Token]) -> Goal {
    for (index, token) in tokens.iter().enumerate() {
        if !token.is_ident(source, "import") && !token.is_ident(source, "export") {
            continue;
        }
        // `x.import` and `x.export`: a property, not a declaration.
        if index
            .checked_sub(1)
            .is_some_and(|previous| tokens[previous].is_punct(b'.'))
        {
            continue;
        }
        match tokens.get(index + 1) {
            // `{ import: 1 }` and `{ export() {} }`: a key, not a declaration.
            // `import(…)` is a dynamic import, which a script may write too.
            Some(next) if next.is_punct(b':') || next.is_punct(b'(') => continue,
            _ => return Goal::Module,
        }
    }
    Goal::Script
}

/// Parse `source` with the official Flow port, reading a module as a module.
///
/// This is the call every entry point in uf makes — [`parse`](crate::parse),
/// [`validate_source`](crate::validate_source), `uf_transform`'s transform and
/// `uf_check`'s checker — so that one file gets one answer whichever command
/// asked. `file` is the port's `FileKey` where the caller has one; it decides
/// nothing here and is passed through so locations carry it.
///
/// Syntax errors are returned, not raised: the port recovers and the caller
/// decides what a recovered tree is worth.
#[must_use]
pub fn parse(source: &str, options: &ParseOptions, file: Option<&FileKey>) -> Program {
    let script = read(source, options, file);

    // Nothing to do for a file with no `await` in it, which is nearly every
    // file: one substring search, and the tokenizer below never runs.
    if !source.contains(AWAIT) {
        return script;
    }

    let tokens = tokenize(source);
    if goal_of(source, &tokens) == Goal::Script {
        return script;
    }

    let mut wanted = candidates(source, &tokens);
    if wanted.is_empty() {
        return script;
    }

    // Every `await` the port already read as an operator — which is every
    // `await` in an `async` function — is left exactly as it was. That is what
    // keeps ordinary `async`/`await` code, which is most code that has an
    // `await` at all, to the single parse above.
    let mut resolved = Resolved { at: Vec::new() };
    // The default traversal never fails; `()` is only the error type this
    // visitor declines to have.
    let _ = resolved.program(&script.0);
    resolved.at.sort_unstable();
    wanted.retain(|candidate| resolved.at.binary_search(&candidate.at).is_err());
    if wanted.is_empty() {
        return script;
    }

    for _ in 0..MAX_PASSES {
        let (program, errors) = read(&rewrite(source, &wanted), options, file);
        let mut repair = Repair {
            wanted: &wanted,
            kept: Vec::new(),
            depth: 0,
        };
        let repaired = repair.map_program(&program);
        repair.kept.sort_unstable_by_key(|candidate| candidate.at);

        if repair.kept == wanted {
            return (repaired, errors);
        }
        if repair.kept.is_empty() {
            return script;
        }
        wanted = repair.kept;
    }

    script
}

/// Hand `source` to the port, with or without a file name.
fn read(source: &str, options: &ParseOptions, file: Option<&FileKey>) -> Program {
    match file {
        Some(file) => flow_parser::parse_program_file::<()>(
            false,
            None,
            Some(options.clone()),
            file.clone(),
            Ok(source),
        ),
        None => {
            flow_parser::parse_program_without_file(false, None, Some(options.clone()), Ok(source))
        }
    }
}

/// An `await` that might be a module's, and the shape it would take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Candidate {
    /// Byte offset of the `await` token itself, which is what gets replaced.
    await_at: usize,
    /// Where the node the port will build starts: the `await` for an operator,
    /// the `for` for a `for await`. This is what the tree is matched on.
    at: Position,
    /// Which of the two shapes it is.
    kind: Kind,
}

impl Candidate {
    /// The five bytes that stand in for this candidate's `await`.
    const fn replacement(self) -> &'static str {
        match self.kind {
            Kind::Operator => VOID,
            Kind::ForAwait => BLANK,
        }
    }
}

/// The two places `await` is a keyword rather than an operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `await x` — a `Unary` whose operator it is.
    Operator,
    /// `for await (… of …)` — a `ForOf` whose `await_` it sets.
    ForAwait,
}

/// Every `await` in `source` that could be a module's, in source order.
///
/// Deliberately generous: this decides what to *offer* the port, and the tree
/// the port hands back decides what it was. The two exclusions below are not
/// part of that decision — an `await` after a `.` or before a `:` is a
/// property name, the port would say so, and skipping the obvious ones only
/// saves the loop in [`parse`] a pass.
fn candidates(source: &str, tokens: &[Token]) -> Vec<Candidate> {
    let mut found: Vec<(usize, usize, Kind)> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if !token.is_ident(source, AWAIT) {
            continue;
        }
        let previous = index.checked_sub(1).map(|previous| tokens[previous]);
        // `x.await` and `this.#await`.
        if previous.is_some_and(|previous| previous.is_punct(b'.') || previous.is_punct(b'#')) {
            continue;
        }
        // `{ await: 1 }`.
        if tokens
            .get(index + 1)
            .is_some_and(|next| next.is_punct(b':'))
        {
            continue;
        }
        match previous {
            Some(previous) if previous.is_ident(source, "for") => {
                found.push((token.start, previous.start, Kind::ForAwait));
            }
            _ => found.push((token.start, token.start, Kind::Operator)),
        }
    }

    let node_starts: Vec<usize> = found.iter().map(|(_, at, _)| *at).collect();
    positions(source, &node_starts)
        .into_iter()
        .zip(found)
        .map(|(at, (await_at, _, kind))| Candidate { await_at, at, kind })
        .collect()
}

/// `source` with each candidate's `await` replaced by its stand-in.
///
/// Byte for byte: every replacement is exactly as long as the `await` it
/// covers, so every other offset, line and column in the file is the one the
/// author wrote and every location the port reports is a location in the
/// original source.
fn rewrite(source: &str, candidates: &[Candidate]) -> String {
    let mut rewritten = String::with_capacity(source.len());
    let mut cursor = 0;
    for candidate in candidates {
        rewritten.push_str(&source[cursor..candidate.await_at]);
        rewritten.push_str(candidate.replacement());
        cursor = candidate.await_at + AWAIT.len();
    }
    rewritten.push_str(&source[cursor..]);
    rewritten
}

/// The parser positions of `offsets`, which must be sorted.
///
/// The port counts lines from one and columns in *bytes* from zero — "the
/// column offset are measured by bytes", `flow_parser::loc`. Not code points,
/// and not the UTF-16 code units the transform converts to for source maps, so
/// this is arithmetic on line breaks and nothing else.
///
/// A line break here is `\n`, which is what the rest of this crate counts and
/// what `uf_fmt` normalises its input to. A source that ends its lines some
/// other way puts [`parse`]'s candidates at positions no node in the tree has,
/// so every one of them is withdrawn and the file is read exactly as the port
/// read it — the wrong answer would be a wrong tree, and this is a missing
/// improvement instead.
pub(crate) fn positions(source: &str, offsets: &[usize]) -> Vec<Position> {
    let mut out = Vec::with_capacity(offsets.len());
    let mut line = 1usize;
    let mut line_start = 0usize;
    let mut cursor = 0usize;

    for &offset in offsets {
        let end = offset.clamp(cursor, source.len());
        while let Some(newline) = memchr(b'\n', &source.as_bytes()[cursor..end]) {
            cursor += newline + 1;
            line_start = cursor;
            line += 1;
        }
        cursor = end;
        out.push(Position {
            line: i32::try_from(line).unwrap_or(i32::MAX),
            column: i32::try_from(offset - line_start).unwrap_or(i32::MAX),
        });
    }

    out
}

/// The first `needle` in `haystack`.
///
/// Spelled out rather than pulled in: `uf_flow` depends on the Flow port and
/// on nothing else, and this is the whole of what a search crate would be used
/// for.
fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|byte| *byte == needle)
}

/// Where the port already read an `await` as an operator.
///
/// Every `Unary` with `UnaryOperator::Await` and every `for await` in a tree,
/// by the position the node starts at — which is the position a [`Candidate`]
/// records, so the two compare directly.
struct Resolved {
    at: Vec<Position>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Resolved {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn unary_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Unary<Loc, Loc>,
    ) -> Result<(), ()> {
        if expr.operator == UnaryOperator::Await {
            self.at.push(loc.start);
        }
        ast_visitor::unary_expression_default(self, loc, expr)
    }

    fn for_of_statement(&mut self, loc: &'ast Loc, stmt: &'ast ForOf<Loc, Loc>) -> Result<(), ()> {
        if stmt.await_ {
            self.at.push(loc.start);
        }
        ast_visitor::for_of_statement_default(self, loc, stmt)
    }
}

/// Puts `await` back where the port read the stand-in as a module's.
///
/// The tree is rebuilt rather than edited because the port's nodes are behind
/// `Arc`s; the port's own mapper does the rebuilding, so every node this does
/// not name comes through untouched and a node added upstream needs nothing
/// here.
///
/// [`Repair::kept`] is the other half of the answer: a candidate that did not
/// land where a module's `await` may stand — inside a function, or as a
/// property name — is not repaired and not kept, and [`parse`] offers the
/// narrowed set again.
struct Repair<'a> {
    /// What [`parse`] offered the port, in source order.
    wanted: &'a [Candidate],
    /// Which of them the port put where a module's `await` may stand.
    kept: Vec<Candidate>,
    /// How many function-like scopes enclose the node being mapped.
    ///
    /// Zero is the top level of the module, and the only depth at which
    /// `await` needs no `async` around it. A class field initialiser and a
    /// static block count as scopes of their own for the same reason a
    /// function does: neither is the module's top level, and `await` in one is
    /// a syntax error in every goal symbol.
    depth: usize,
}

impl Repair<'_> {
    /// The candidate that starts at `at`, if this is one of them.
    fn wanted_at(&self, at: Position, kind: Kind) -> Option<Candidate> {
        self.wanted
            .iter()
            .copied()
            .find(|candidate| candidate.at == at && candidate.kind == kind)
    }

    /// Run `map` one function-like scope deeper.
    fn nested<T>(&mut self, map: impl FnOnce(&mut Self) -> T) -> T {
        self.depth += 1;
        let mapped = map(self);
        self.depth -= 1;
        mapped
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Repair<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_unary_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Unary<Loc, Loc>,
    ) -> ast::expression::Unary<Loc, Loc> {
        let mapped = ast_visitor::map_unary_expression_default(self, loc, expr);
        if self.depth > 0 || mapped.operator != UnaryOperator::Void {
            return mapped;
        }
        match self.wanted_at(loc.start, Kind::Operator) {
            Some(candidate) => {
                self.kept.push(candidate);
                ast::expression::Unary {
                    operator: UnaryOperator::Await,
                    ..mapped
                }
            }
            None => mapped,
        }
    }

    fn map_for_of_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ForOf<Loc, Loc>,
    ) -> ForOf<Loc, Loc> {
        let mapped = ast_visitor::map_for_of_statement_default(self, loc, stmt);
        if self.depth > 0 || mapped.await_ {
            return mapped;
        }
        match self.wanted_at(loc.start, Kind::ForAwait) {
            Some(candidate) => {
                self.kept.push(candidate);
                ForOf {
                    await_: true,
                    ..mapped
                }
            }
            None => mapped,
        }
    }

    fn map_function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc> {
        self.nested(|repair| ast_visitor::map_function_default(repair, loc, function))
    }

    fn map_component_declaration(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> ast::statement::ComponentDeclaration<Loc, Loc> {
        self.nested(|repair| ast_visitor::map_component_declaration_default(repair, loc, component))
    }

    fn map_class_static_block(
        &mut self,
        block: &'ast ast::class::StaticBlock<Loc, Loc>,
    ) -> ast::class::StaticBlock<Loc, Loc> {
        self.nested(|repair| ast_visitor::map_class_static_block_default(repair, block))
    }

    fn map_class_property_value(
        &mut self,
        value: &'ast ast::class::property::Value<Loc, Loc>,
    ) -> ast::class::property::Value<Loc, Loc> {
        self.nested(|repair| ast_visitor::map_class_property_value_default(repair, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, validate_source};

    /// A module that awaits: the issue's own reproduction, which is a module
    /// because it exports.
    const AWAITED_CONFIG: &str =
        "// @flow\nconst value: number = await Promise.resolve(1);\nexport { value };\n";

    /// Every `await` operator in the tree `source` parses to, as the
    /// `(line, column)` the port reports for the node it starts.
    fn awaits(source: &str) -> Vec<(i32, i32)> {
        let parsed = parse(source).expect("the source is inside every ceiling");
        let mut resolved = Resolved { at: Vec::new() };
        let _ = resolved.program(&parsed.program);
        resolved
            .at
            .iter()
            .map(|position| (position.line, position.column))
            .collect()
    }

    /// The diagnostics `source` produces, as messages.
    fn refusals(source: &str) -> Vec<String> {
        validate_source(source)
            .expect("the parser backend is always available")
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn a_module_awaits_at_its_top_level() {
        assert!(refusals(AWAITED_CONFIG).is_empty(), "{AWAITED_CONFIG}");
        // Line 2, column 22: on the `await`, where the author wrote it. The
        // whole point of trading five bytes for five is that this is the
        // position in the file rather than a position in a rewrite.
        assert_eq!(awaits(AWAITED_CONFIG), [(2, 22)]);
    }

    #[test]
    fn a_script_still_reads_await_as_an_identifier() {
        // The same module without the `export`, which is the only thing that
        // made it a module. `await` is not reserved in a script, so this is
        // the identifier `await` followed by a call, and the parser says so.
        let script = "// @flow\nconst value: number = await Promise.resolve(1);\n";
        assert_eq!(goal(script), Goal::Script);
        assert_eq!(refusals(script).len(), 1, "{script}");
        assert!(awaits(script).is_empty());
    }

    #[test]
    fn a_script_may_still_name_a_variable_await() {
        // The half that must not regress: `await` is a perfectly good
        // identifier outside a module, and a script that uses one keeps
        // parsing.
        let script = "// @flow\nconst await = 1;\nconst b = await;\n";
        assert!(refusals(script).is_empty(), "{:?}", refusals(script));
    }

    #[test]
    fn a_module_may_still_name_a_property_await() {
        // `x.await`, `{ await: 1 }` and `{ await() {} }` are IdentifierNames,
        // which every goal symbol allows. The third is the one no token scan
        // could tell from `await (x)`, so it is the one the parser decides.
        let module = concat!(
            "// @flow\n",
            "export const keys = { await: 1, async await() {} };\n",
            "export const read = (source) => source.await;\n",
        );
        assert!(refusals(module).is_empty(), "{:?}", refusals(module));
        assert!(awaits(module).is_empty());
    }

    #[test]
    fn a_module_awaits_a_parenthesized_operand() {
        // `await (x)` is a call to a function named `await` in a script and an
        // await expression in a module, and the two are the same two tokens.
        // Reading the file as a module is what decides it.
        let module = "// @flow\nexport const value = await (load());\n";
        assert_eq!(awaits(module), [(2, 21)]);

        let script = "// @flow\nconst value = await (load());\n";
        assert!(refusals(script).is_empty(), "a script calls `await`");
        assert!(awaits(script).is_empty());
    }

    #[test]
    fn a_module_awaits_in_a_for_of_head() {
        let module = "// @flow\nexport const rows = [];\nfor await (const row of stream()) {\n  rows.push(row);\n}\n";
        assert!(refusals(module).is_empty(), "{:?}", refusals(module));
        // The `ForOf` starts on its `for`, which is where the candidate is
        // recorded and where the flag is put back.
        assert_eq!(awaits(module), [(3, 0)]);
    }

    #[test]
    fn a_non_async_function_in_a_module_still_refuses_await() {
        // A module's top level is not every line of the module. Inside a
        // function that is not `async`, `await` is the error it always was,
        // and it is described in the same words.
        let module = "// @flow\nexport function read() {\n  return await load();\n}\n";
        assert_eq!(refusals(module), [crate::explain::AWAIT_OUTSIDE_ASYNC]);
    }

    #[test]
    fn an_await_inside_an_async_function_is_left_exactly_alone() {
        // It already parsed, so nothing here touches it: the fast path in
        // `parse` withdraws every `await` the port had already read.
        let module = "// @flow\nexport async function read() {\n  return await load();\n}\n";
        assert!(refusals(module).is_empty());
        assert_eq!(awaits(module), [(3, 9)]);
    }

    #[test]
    fn a_module_awaits_where_a_module_may_and_not_where_it_may_not() {
        // Both in one file, because the two answers are decided in the same
        // pass and a module with one of each is what shakes that loose.
        let module = concat!(
            "// @flow\n",
            "export const value = await load();\n",
            "function read() {\n",
            "  return await load();\n",
            "}\n",
        );
        assert_eq!(refusals(module), [crate::explain::AWAIT_OUTSIDE_ASYNC]);
        assert_eq!(awaits(module), [(2, 21)]);
    }

    #[test]
    fn a_module_is_a_file_that_says_it_is_one() {
        for module in [
            "import { load } from \"./load.js\";\n",
            "export const value = 1;\n",
            "export default 1;\n",
            "export * from \"./load.js\";\n",
            "const here = import.meta.url;\n",
        ] {
            assert_eq!(goal(module), Goal::Module, "{module}");
        }

        for script in [
            "const load = require(\"./load.js\");\n",
            "module.exports = { load };\n",
            "const later = import(\"./load.js\");\n",
            "const keys = { import: 1, export: 2 };\n",
            "const name = value.export;\n",
        ] {
            assert_eq!(goal(script), Goal::Script, "{script}");
        }
    }

    #[test]
    fn every_other_byte_keeps_the_position_it_had() {
        // The whole argument for trading `await` for five other bytes: an
        // error anywhere else in the file is still reported where it is. Four
        // bytes for the wave, one column each, because that is what the port
        // means by a column.
        let module =
            "// @flow\nexport const sea = \"🌊\";\nconst value = await load();\nconst broken = ;\n";
        let diagnostics = validate_source(module)
            .expect("the parser backend is always available")
            .diagnostics;

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            (diagnostics[0].line, diagnostics[0].column),
            (Some(4), Some(15))
        );
        assert_eq!(awaits(module), [(3, 14)]);
    }

    #[test]
    fn a_module_awaits_across_the_line_break_the_way_node_does() {
        // `await` is reserved in a module, so it cannot be the identifier that
        // automatic semicolon insertion would end the statement after: this is
        // one `await load()` and not two statements. A script reads the same
        // three lines the other way, and that is the difference between the
        // goal symbols rather than a choice made here.
        let module = "// @flow\nexport const value = await\n  load();\n";
        assert_eq!(awaits(module), [(2, 21)]);

        let script = "// @flow\nconst value = await\n  load();\n";
        assert!(awaits(script).is_empty());
        assert!(refusals(script).is_empty());
    }

    #[test]
    fn a_file_with_no_await_never_reaches_any_of_this() {
        // The fast path is a substring search, so a file without the word
        // parses exactly once. Nothing here can observe that directly; what it
        // can observe is that such a file is unchanged by the module, which is
        // the property the search is allowed to assume.
        let module = "// @flow\nexport const value = load();\n";
        assert!(refusals(module).is_empty());
        assert!(awaits(module).is_empty());
    }

    #[test]
    fn a_module_at_the_chain_ceiling_still_parses_with_an_await_in_it() {
        // On a thread the size `parse` documents. The parser was never the
        // only thing that recurses here: this module walks the tree once to
        // see which `await`s the port had already read, and rebuilds it once
        // to put the operator back, and both of those recurse per level too.
        std::thread::Builder::new()
            .stack_size(crate::PARSE_STACK_BYTES)
            .spawn(|| {
                let source = format!(
                    "export const value = await load();\nconst chain = {};\n",
                    vec!["1"; crate::MAX_CHAIN_DEPTH].join(" + ")
                );
                let parsed = parse(&source).expect("parses");
                assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
            })
            .expect("spawns")
            .join()
            .expect("no overflow at the ceiling");
    }

    #[test]
    fn a_module_whose_awaits_are_all_property_names_settles_and_changes_nothing() {
        // The loop withdraws a candidate the port declined to read as an
        // operator, and this is a module made of nothing else: every `await`
        // in it is an IdentifierName. It has to come out exactly as the port
        // read it the first time.
        let module = concat!(
            "// @flow\n",
            "import { io } from \"./io.js\";\n",
            "export const keys = { await: 1, async await() {}, get await2() { return io.await; } };\n",
        );
        assert!(refusals(module).is_empty(), "{:?}", refusals(module));
        assert!(awaits(module).is_empty());
    }
}
