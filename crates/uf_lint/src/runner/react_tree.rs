//! The rules that read the module's tree rather than its lines:
//! `react/no-redundant-memo` and the `react-compiler/*` rules.
//!
//! Every other rule in this crate answers its question from [`FileScan`] — a
//! line, its code, the brace depth it opens at. These cannot, because each asks
//! the **official React Compiler**, which is not a question about the source
//! text at all. The `react-compiler/*` rules report its diagnostics
//! ([`super::react_compiler`]), and `react/no-redundant-memo` reports the
//! memoization it removed ([`uf_transform::memo`]).
//!
//! So both go through `uf_transform`: the official Flow parser, Flow's own
//! lowering rules, and Babel's AST shape — the same three stages `uf build`
//! runs, so the tree the linter reads is the tree the compiler is given.
//!
//! # What this costs, and when it is paid
//!
//! Parsing a module again is far more work than a line scan, and compiling one
//! is more again. Four gates stand in front of it, in order of cheapness:
//!
//! 1. No rule here is enabled — nothing runs.
//! 2. The path is not Flow source — nothing runs.
//! 3. No `react-compiler/*` rule is on and the text holds no call-shaped
//!    `useMemo` or `useCallback` — nothing runs, because a module with no
//!    hand-written memoization gives the memo rule nothing to report. Which
//!    functions the compiler compiles is not guessed from the text: the
//!    compiler's result says.
//! 4. The module is nested or chained past [`uf_flow`]'s parser ceilings —
//!    nothing runs, because `flow/syntax` has already refused it and a linter
//!    must not be the thing that overflows a stack on a minified bundle.
//!
//! Past those, the parse happens on a thread of
//! [`PARSE_STACK_BYTES`](uf_flow::PARSE_STACK_BYTES), for the reason
//! `uf_flow::upstream` spells out: the port's frames are large, the tree is
//! freed where it was built, and a lint worker's own stack is not enough.
//!
//! And past the parse, a fifth: an earlier run already worked this module out.
//! `uf lint` and `uf check` keep [`ReactAnswer`] under `.uf/cache/lint`
//! ([`crate::cache`]), keyed on the path, the text, [`question`] and the `uf`
//! that wrote it, so a module nobody touched is not lowered, rebuilt or handed
//! to the compiler again. ubugeeei-prod/uf#1442.

use serde::{Deserialize, Serialize};
use uf_config::UniflowedConfig;
use uf_profiler::profile_span;
use uf_transform::{ReactCompilerMode, TransformOptions};

use crate::scan::{FileScan, ends_word, next_non_space, prev_non_space, starts_word};
use crate::{Diagnostic, push_at, severity};

/// `react/no-redundant-memo`.
const REDUNDANT_MEMO: &str = "react/no-redundant-memo";

/// The hooks whose hand-written calls `react/no-redundant-memo` reports.
const MEMO_HOOKS: [&str; 2] = ["useMemo", "useCallback"];

/// What these rules want out of a parse, or [`None`] when they want none.
///
/// Split from the analysis so that one parse can serve every runner that
/// needs the module's tree — see [`super::module_tree`], which owns it.
pub(super) struct ReactWork {
    memo: Option<crate::Severity>,
    wants_memo: bool,
    /// The `react-compiler/*` rules, when any of them is on. Whether this
    /// module is compiled for them is not decided here: that is
    /// [`uf_transform::may_contain_react_code`]'s answer, which needs the tree.
    compiler: Option<super::react_compiler::CompilerWork>,
}

/// Whether these rules want this module read at all.
pub(super) fn wanted(scan: &FileScan<'_>, config: &UniflowedConfig) -> Option<ReactWork> {
    // A project that has turned the compiler off gets no report: without it,
    // a hand-written `useMemo` is the only memoization there is.
    let memo = config
        .app
        .builtins
        .react_compiler
        .enabled
        .then(|| severity(config, REDUNDANT_MEMO))
        .flatten();
    // Not tied to `reactCompiler.enabled`: these report the rules of React,
    // which hold whether or not the build memoizes.
    let compiler = super::react_compiler::wanted(config);
    if memo.is_none() && compiler.is_none() {
        return None;
    }
    if !super::flow_syntax::is_flow_syntax_target(&scan.file.path) {
        return None;
    }

    // A hand-written `useMemo` or `useCallback` is what the memo rule reports,
    // so a module without one gives it nothing. Textual on purpose — the point
    // is to decide without parsing. Whether the compiler compiles the function
    // around a call is not decided here: the compiler's result says.
    let wants_memo = memo.is_some() && calls_memo_hook(scan);
    if !wants_memo && compiler.is_none() {
        return None;
    }
    Some(ReactWork {
        memo,
        wants_memo,
        compiler,
    })
}

/// Whether `useMemo` or `useCallback` is called where code can call it.
///
/// This is still a cheap textual gate, not binding analysis. It is deliberately
/// narrower than `source.contains`: comments, string prose and imports cannot
/// be hook calls, and those false positives are exactly what make `uf lint`
/// enter the allocation-heavy tree path for modules that cannot report
/// anything here.
fn calls_memo_hook(scan: &FileScan<'_>) -> bool {
    for line in &scan.lines {
        let code = line.code();
        for at in uf_infra::memchr_iter(b'u', code.as_bytes()) {
            let Some(after) = memo_hook_at(code, at) else {
                continue;
            };
            // The call-shape check is cheaper than proving the name is not in
            // a string, and it rejects import-only names before `in_string`
            // has to rescan the line prefix.
            if call_follows_name(code, after)
                && hook_callee_can_match(code, at)
                && !line.in_string(at)
            {
                return true;
            }
        }
    }
    false
}

/// Where the name ends, when one of [`MEMO_HOOKS`] starts at `at`.
fn memo_hook_at(code: &str, at: usize) -> Option<usize> {
    if !starts_word(code, at) {
        return None;
    }
    MEMO_HOOKS
        .into_iter()
        .find(|hook| hook_name_at(code, at, hook))
        .map(|hook| at + hook.len())
}

fn hook_name_at(code: &str, at: usize, hook: &str) -> bool {
    code.as_bytes()[at..].starts_with(hook.as_bytes()) && ends_word(code, at + hook.len())
}

/// Whether a hook-like name is written in a shape the memo rule can report.
///
/// The AST-side rule accepts bare calls and `React.useX(...)`. A method on
/// anything else is just a method that happens to share a React hook's name,
/// and paying for the Babel-shaped tree only to reject it later is the #668
/// path in miniature.
fn hook_callee_can_match(code: &str, at: usize) -> bool {
    let Some((dot, b'.')) = prev_non_space(code, at) else {
        return true;
    };
    react_member_owner_before_dot(code, dot)
}

fn react_member_owner_before_dot(code: &str, dot: usize) -> bool {
    let bytes = code.as_bytes();
    let mut end = dot.min(bytes.len());
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && crate::scan::is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    &code[start..end] == "React"
        && !prev_non_space(code, start).is_some_and(|(_, byte)| byte == b'.')
}

/// Whether the next non-space token after a name can still be the same call.
///
/// `useMemo<T>(...)` is a call too, so `<` is accepted with `(`. The gate stays
/// local to the line: if somebody splits a hook callee from its argument list
/// across lines, this errs toward skipping the expensive optional rule rather
/// than paying the #668 path for import-only modules.
fn call_follows_name(code: &str, after: usize) -> bool {
    match next_non_space(code, after) {
        Some((_, b'(')) => true,
        Some((at, b'<')) if at == after => generic_call_follows_name(code, at),
        _ => false,
    }
}

fn generic_call_follows_name(code: &str, open: usize) -> bool {
    let bytes = code.as_bytes();
    let mut depth = 0u32;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'"' | b'\'' => at = skip_quoted_type_literal(bytes, at),
            b'>' if at > 0 && bytes[at - 1] == b'=' => {}
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return next_non_space(code, at + 1).is_some_and(|(_, byte)| byte == b'(');
                }
            }
            b';' if depth > 0 => return false,
            _ => {}
        }
        at += 1;
    }
    false
}

fn skip_quoted_type_literal(bytes: &[u8], quote: usize) -> usize {
    let mut at = quote + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at = (at + 1).min(bytes.len()),
            byte if byte == bytes[quote] => return at,
            _ => {}
        }
        at += 1;
    }
    bytes.len()
}

/// The validations the compiler is asked for beyond the plugin's defaults.
fn switches(work: &ReactWork) -> uf_transform::LintSwitches {
    uf_transform::LintSwitches {
        effect_dependencies: work
            .compiler
            .as_ref()
            .is_some_and(super::react_compiler::CompilerWork::checks_effect_dependencies),
    }
}

/// Everything besides the module's path and text that decides what
/// [`analyse_parsed`] returns, spelled for [`crate::cache::LintCache`]'s key.
///
/// Built from the values the analysis itself reads. The switches are
/// destructured, so that one added later does not compile until it is part of
/// the question: a switch left out would be two questions sharing one answer.
pub(super) fn question(config: &UniflowedConfig, work: &ReactWork) -> String {
    let uf_transform::LintSwitches {
        effect_dependencies,
    } = switches(work);
    // The mode's own serialized name, which covers every mode there is.
    let mode = serde_json::to_string(&compiler_mode(config)).unwrap_or_default();
    uf_infra::into_string(uf_infra::cstr!(
        "memo={}\0compiler={}\0effect-dependencies={effect_dependencies}\0mode={mode}",
        work.wants_memo,
        work.compiler.is_some(),
    ))
}

/// What these rules worked out about one module, before any project decides
/// which of it to report.
///
/// Raw on purpose: this is what [`crate::cache::LintCache`] keeps, and a rule's
/// level is applied by [`report`] on the way out, so changing one reuses the
/// answer instead of invalidating it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ReactAnswer {
    /// What the official React Compiler reported, when it was asked.
    compiler: Vec<uf_transform::LintDiagnostic>,
    /// Hand-written memoization the compiler removed, when that was asked.
    memo: Vec<MemoFound>,
}

/// One `useMemo` or `useCallback` the compiler memoizes already.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MemoFound {
    hook: String,
    /// 1-based line.
    line: u32,
    /// 0-based column, in UTF-16 code units.
    column: u32,
}

/// Run whichever of these rules were asked for, over a tree somebody else
/// parsed.
///
/// # Call this on the thread that built `parsed`
///
/// The lowering recurses over the tree; [`super::module_tree`] is the caller
/// and is on a thread with `uf_flow::PARSE_STACK_BYTES`.
pub(super) fn analyse_parsed(
    parsed: &uf_flow::Parsed,
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    work: &ReactWork,
) -> ReactAnswer {
    profile_span!("run_react_tree_rules");
    let source = &scan.file.source;
    let options = TransformOptions {
        react_compiler: compiler_mode(config),
        ..TransformOptions::new(scan.file.path.clone())
    };

    // The compiler's rules take every module `eslint-plugin-react-hooks` would
    // compile, and only those — the plugin's own test, not one of uf's. A
    // module the compiler has already answered with exactly this text is
    // answered from that, without building the tree again.
    let switches = switches(work);
    let remembered = work
        .compiler
        .as_ref()
        .and_then(|_| uf_transform::lint::cached(&scan.file.path, source, switches));
    let compile = work.compiler.is_some()
        && remembered.is_none()
        && uf_transform::may_contain_react_code(&parsed.program);

    let mut answer = ReactAnswer {
        compiler: remembered.unwrap_or_default(),
        memo: Vec::new(),
    };
    if !work.wants_memo && !compile {
        return answer;
    }

    // Lowered, not raw: `component` and `match` are Flow's own syntax, and the
    // compiler reads functions and calls. After the lowering a component *is*
    // a `FunctionDeclaration`.
    //
    // From the tree the caller already holds rather than from the source:
    // `uf lint` used to hand the text back to `estree::parse` here and have
    // the module parsed a second time. See ubugeeei-prod/uf#668.
    let Ok((program, _)) = uf_transform::lowered_from_parsed(&parsed.program, source) else {
        return answer;
    };
    // An error from here on is a bug in uf rather than in the module — the
    // tree did not fit the compiler's own schema — and `uf build`, which reads
    // the same tree, fails on it loudly. It says nothing about what the other
    // rules already found, so it costs them nothing.
    let Ok(file) = uf_transform::babel_from_lowered(program, source) else {
        return answer;
    };
    // One scope analysis for both questions put to the compiler.
    let scope = uf_transform::scope::analyze(&file);
    if work.wants_memo
        && let Ok(redundant) =
            uf_transform::redundant_memoization_in_scope(&file, &scope, source, &options)
    {
        answer.memo = redundant
            .into_iter()
            .map(|memo| MemoFound {
                hook: memo.hook.to_owned(),
                line: memo.line,
                column: memo.column,
            })
            .collect();
    }
    if compile
        && let Ok(diagnostics) =
            uf_transform::lint::lint(&file, scope, source, &scan.file.path, switches)
    {
        answer.compiler = diagnostics;
    }
    answer
}

/// Each React Compiler diagnostic, filed under its `react-compiler/*` rule.
///
/// A diagnostic whose category `uf lint` files under no rule, or whose rule
/// this project has switched off, is dropped here rather than carried to
/// [`report`].
fn compiler_findings(
    compiler: &super::react_compiler::CompilerWork,
    diagnostics: impl IntoIterator<Item = uf_transform::LintDiagnostic>,
) -> impl Iterator<Item = TreeFinding> {
    diagnostics.into_iter().filter_map(move |diagnostic| {
        let rule = super::react_compiler::rule_for(diagnostic.category)?;
        compiler.level(rule)?;
        Some(TreeFinding {
            kind: FindingKind::Compiler(rule),
            line: diagnostic.line,
            column: diagnostic.column,
            message: diagnostic.message,
        })
    })
}

/// Each finding in `answer` this project reports, filed under its rule.
fn findings(work: &ReactWork, answer: ReactAnswer) -> Vec<TreeFinding> {
    let mut found: Vec<TreeFinding> = Vec::new();
    if work.wants_memo {
        found.extend(answer.memo.into_iter().map(|memo| TreeFinding {
            kind: FindingKind::RedundantMemo,
            line: memo.line,
            column: memo.column,
            message: uf_infra::into_string(uf_infra::cstr!(
                "the React Compiler memoizes this already; `{}` here is a second dependency array to keep correct",
                memo.hook
            )),
        }));
    }
    if let Some(compiler) = &work.compiler {
        found.extend(compiler_findings(compiler, answer.compiler));
    }
    found
}

/// Turn what the analysis found into diagnostics.
pub(super) fn report(
    scan: &FileScan<'_>,
    work: &ReactWork,
    answer: ReactAnswer,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source = &scan.file.source;
    for finding in findings(work, answer) {
        let rule = match finding.kind {
            FindingKind::RedundantMemo => REDUNDANT_MEMO,
            FindingKind::Compiler(rule) => rule,
        };
        let level = match finding.kind {
            FindingKind::RedundantMemo => work.memo,
            FindingKind::Compiler(rule) => work
                .compiler
                .as_ref()
                .and_then(|compiler| compiler.level(rule)),
        };
        let Some(index) = usize::try_from(finding.line)
            .ok()
            .and_then(|line| line.checked_sub(1))
        else {
            continue;
        };
        let (Some(level), Some(line)) = (level, scan.lines.get(index)) else {
            continue;
        };
        // A diagnostic carries bytes, and the conversion reads the line out of
        // the *unmasked* source: `mask_inline_comments` replaces a comment byte
        // for byte, which keeps every offset but not every character.
        let text = source
            .get(line.offset..line.offset + line.text.len())
            .unwrap_or(line.text);
        let column = byte_column(text, finding.column);
        push_at(
            diagnostics,
            scan,
            rule,
            level,
            index,
            column,
            finding.message,
        );
    }
}

/// Which rule a finding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindingKind {
    RedundantMemo,
    /// A React Compiler diagnostic, filed under this `react-compiler/*` rule.
    Compiler(&'static str),
}

/// One finding, positioned the way the tree positions things.
struct TreeFinding {
    kind: FindingKind,
    /// 1-based line.
    line: u32,
    /// 0-based column, in UTF-16 code units: see [`byte_column`].
    column: u32,
    message: String,
}

/// The mode uf would compile this project's modules in.
///
/// A `match` rather than a default, so that a mode added to `uf.config.js`
/// cannot reach the compiler here under the wrong name: `uf lint` and
/// `uf build` have to ask the compiler the same question.
fn compiler_mode(config: &UniflowedConfig) -> ReactCompilerMode {
    match config.app.builtins.react_compiler.mode {
        uf_config::ReactCompilerMode::Syntax => ReactCompilerMode::Syntax,
    }
}

/// Byte offset within `line` of the `column`-th UTF-16 code unit.
///
/// Every finding here comes from the Babel tree, whose `loc` `babel::finalize`
/// recomputed in UTF-16 units. A line holding an astral character is where
/// that differs from a code point count, and from a byte count everywhere past
/// the first non-ASCII character.
fn byte_column(line: &str, column: u32) -> usize {
    let mut units = 0u32;
    for (offset, character) in line.char_indices() {
        if units >= column {
            return offset;
        }
        units += u32::try_from(character.len_utf16()).unwrap_or(u32::MAX);
    }
    line.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceFile;
    use crate::scan::mask_inline_comments;
    use uf_config::{RuleLevel, UniflowedConfig};
    use uf_infra::CompactString;

    fn only(rule: &str) -> UniflowedConfig {
        let mut config = UniflowedConfig::default();
        config.lint.rules.clear();
        config
            .lint
            .rules
            .insert(CompactString::from(rule), RuleLevel::Error);
        config
    }

    fn wants(rule: &str, source: &str) -> bool {
        let file = SourceFile {
            path: "app/page.js".to_owned(),
            source: source.to_owned(),
        };
        let masked = mask_inline_comments(&file.source);
        let scan = FileScan::new(&file, &masked);
        wanted(&scan, &only(rule)).is_some()
    }

    #[test]
    fn comments_and_strings_do_not_request_the_react_tree_path() {
        let source = r#"// @flow
// useEffect useState useMemo useCallback
const prose = "useEffect useState useMemo useCallback";
component Page() {
  return <main />;
}
"#;

        assert!(!wants(REDUNDANT_MEMO, source));
    }

    #[test]
    fn import_only_hook_names_do_not_request_the_react_tree_path() {
        let source = r#"// @flow
import { useEffect, useMemo, useState } from "react";
component Page() {
  return <main />;
}
"#;

        assert!(!wants(REDUNDANT_MEMO, source));
    }

    #[test]
    fn memoization_in_a_plain_function_still_requests_the_react_tree_path() {
        // Whether the compiler compiles `Page` is the compiler's answer, read
        // from its result, and not a guess this gate makes from the text.
        let source = r#"// @flow
import { useMemo } from "react";
export function Page(items: Array<string>) {
  const sorted = useMemo(() => items.slice(), [items]);
  return <main>{sorted}</main>;
}
"#;

        assert!(wants(REDUNDANT_MEMO, source));
    }

    #[test]
    fn methods_that_share_hook_names_do_not_request_the_react_tree_path() {
        let memo = r#"// @flow
component Page(cache: Cache) {
  const value = cache.useMemo(() => 1, []);
  const handler = window.React.useCallback(() => value, [value]);
  return <main>{value}</main>;
}
"#;

        assert!(!wants(REDUNDANT_MEMO, memo));
    }

    #[test]
    fn hook_calls_still_request_the_react_tree_path() {
        let memo = r#"// @flow
import { useMemo } from "react";
component Page() {
  const value = useMemo(() => 1, []);
  return <main />;
}
"#;
        let react_member = r#"// @flow
import * as React from "react";
component Page() {
  const value = React.useMemo(() => 1, []);
  return <main />;
}
"#;
        let hook_declaration = r#"// @flow
import { useCallback } from "react";
export hook useHandler(id: string): () => void {
  return useCallback(() => send(id), [id]);
}
"#;

        assert!(wants(REDUNDANT_MEMO, memo));
        assert!(wants(REDUNDANT_MEMO, react_member));
        assert!(wants(REDUNDANT_MEMO, hook_declaration));
    }

    #[test]
    fn generic_hook_calls_still_request_the_react_tree_path() {
        let simple = r#"// @flow
import { useMemo } from "react";
component Page() {
  const value = useMemo<number>(() => 1, []);
  return <main>{value}</main>;
}
"#;
        let object_type = r#"// @flow
import { useMemo } from "react";
component Page() {
  const value = useMemo<{kind: "ready"}>(() => ({kind: "ready"}), []);
  return <main>{value.kind}</main>;
}
"#;

        assert!(wants(REDUNDANT_MEMO, simple));
        assert!(wants(REDUNDANT_MEMO, object_type));
    }

    #[test]
    fn less_than_expressions_do_not_request_the_memo_tree_path() {
        let source = r#"// @flow
component Page(useMemo: number, limit: number) {
  const value = useMemo < limit ? useMemo : limit;
  return <main>{value}</main>;
}
"#;

        assert!(!wants(REDUNDANT_MEMO, source));
    }
}
