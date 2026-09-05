//! `textDocument/hover`: what uf can honestly say about the thing under the
//! cursor.
//!
//! A hover is asked on mouse-move, so it is answered from data uf already has
//! rather than from work uf would have to start. Three questions are tried, in
//! this order:
//!
//! 1. **A diagnostic covers the position.** The answer is the rule that fired:
//!    its message, plus its entry in `uf_lint`'s catalogue — the same
//!    category, default level and one-line description `uf inspect` prints.
//!    This is the one an editor user asks for most, because the squiggle
//!    already told them *that* something is wrong and never *which rule* nor
//!    why it exists.
//! 2. **An import specifier is under the cursor.** The specifiers come from
//!    `uf_rsc::scan_imports`, the same lexer the RSC module graph and the test
//!    runner's watch graph read, so a hover can never disagree with the build
//!    about what a file imports. What is said about one depends on what it is:
//!    a `@uniflowed/*` module is described from `uf_lib`'s registry (kind,
//!    stability, the Flow names it exports), a relative specifier is resolved
//!    against the file system, and a bare specifier is named as the bundler's
//!    job, which it is.
//! 3. **A rule id is under the cursor.** Hovering the id in a
//!    `// uf-lint-disable-next-line flow/unclear-type` comment says what is
//!    being switched off, which is the moment a reader most wants to know.
//!
//! # What is deliberately not answered
//!
//! **The type at the position.** `uf check` runs Flow's own inference through
//! `uf_check`, and the upstream port it embeds does have the query — the
//! `flow_services_type_info::type_at_pos` that `flow type-at-pos` and the
//! wasm build both call. `uf_check` cannot serve it today: its public surface
//! is `check_sources`, which converts each file's errors into
//! `TypeDiagnostic`s and then drops the `Context`, the file signature and the
//! typed AST that a positional query needs. Serving hover types needs a new
//! entry point that keeps those three alive for the queried file and runs the
//! query on the same oversized stack `on_check_thread` already provides,
//! because the port's state is `Rc`-based and cannot cross a thread. Until
//! that exists this returns no answer for a plain expression rather than an
//! empty popup, which an editor renders as a confident "no type".

use camino::Utf8Path;
use uf_lib::NativeModule;
use uf_lint::{Diagnostic, RuleCategory, RuleDescriptor, RuleRequirement, Severity};
use uf_rsc::is_server_only_specifier;
use uf_test::MODULE_EXTENSIONS;

use crate::commands::lint::identifier_span;

/// Everything an answer can be derived from.
pub(super) struct Request<'a> {
    /// Path the document's URI names; relative specifiers resolve against it.
    pub(super) path: &'a str,
    /// The document as the editor currently holds it.
    pub(super) source: &'a str,
    /// Zero-based line, as the protocol counts lines.
    pub(super) line: usize,
    /// Byte offset within that line's text, already converted from UTF-16.
    pub(super) column: usize,
    /// Diagnostics for this document, from the same lint the editor was sent.
    pub(super) diagnostics: &'a [Diagnostic],
    /// The `@uniflowed/*` registry, resolved once for the session.
    pub(super) modules: &'a [NativeModule],
}

/// An answer, and the span of the line it is about.
pub(super) struct Answer {
    /// Markdown for the popup.
    pub(super) markdown: String,
    /// Byte offset within the hovered line's text where the subject starts.
    pub(super) start: usize,
    /// Byte offset just past it.
    pub(super) end: usize,
}

/// What uf has to say at this position, or [`None`] when it has nothing.
///
/// [`None`] is a real answer and the protocol has a spelling for it (`null`).
/// It is not the same as an empty popup, which reads as "uf looked and there
/// is nothing here".
pub(super) fn hover(request: &Request<'_>) -> Option<Answer> {
    let text = request.source.lines().nth(request.line)?;
    if request.column > text.len() {
        return None;
    }

    diagnostic_answer(request, text)
        .or_else(|| import_answer(request, text))
        .or_else(|| rule_id_answer(text, request.column))
}

/// The rule behind the squiggle under the cursor.
fn diagnostic_answer(request: &Request<'_>, text: &str) -> Option<Answer> {
    let number = request.line.checked_add(1)?;
    let found = request.diagnostics.iter().find(|diagnostic| {
        if diagnostic.line != number {
            return false;
        }
        let start = diagnostic.column.saturating_sub(1);
        let end = start + identifier_span(text, diagnostic.column);
        // Half-open, but a zero-width position at the very end of the span is
        // still on the squiggle as far as a reader pointing at it is concerned.
        (start..=end).contains(&request.column)
    })?;

    let start = found.column.saturating_sub(1).min(text.len());
    let end = (start + identifier_span(text, found.column)).min(text.len());
    let severity = match found.severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
    };
    let mut markdown = format!(
        "**`{rule}`** · {severity}\n\n{message}",
        rule = found.rule,
        message = found.message,
    );
    if let Some(descriptor) = uf_lint::rule(found.rule) {
        markdown.push_str("\n\n---\n\n");
        markdown.push_str(&catalogue_entry(descriptor, Some(&found.message)));
    }

    Some(Answer {
        markdown,
        start,
        end,
    })
}

/// What the import specifier under the cursor names.
fn import_answer(request: &Request<'_>, text: &str) -> Option<Answer> {
    let number = u32::try_from(request.line.checked_add(1)?).ok()?;
    for import in uf_rsc::scan_imports(request.source) {
        if import.line != number {
            continue;
        }
        let Some((start, end)) = quoted_span(text, &import.specifier, request.column) else {
            continue;
        };
        return Some(Answer {
            markdown: describe_specifier(&import.specifier, request),
            start,
            end,
        });
    }
    None
}

/// A rule id written out in a suppression comment.
fn rule_id_answer(text: &str, column: usize) -> Option<Answer> {
    let (start, end) = rule_id_span(text, column)?;
    let canonical = uf_lint::canonical_rule_id(&text[start..end])?;
    let descriptor = uf_lint::rule(canonical)?;

    let mut markdown = format!("**`{canonical}`**\n\n");
    if canonical != &text[start..end] {
        markdown.push_str(&format!(
            "`{}` is a deprecated spelling of this rule.\n\n",
            &text[start..end]
        ));
    }
    markdown.push_str(&catalogue_entry(descriptor, None));

    Some(Answer {
        markdown,
        start,
        end,
    })
}

/// A rule's entry in the catalogue `uf inspect` prints, as markdown.
///
/// `said` is text the popup has already shown. Several rules word their
/// diagnostic exactly as the catalogue words the rule, and a hover that prints
/// the same sentence twice reads as a bug in the hover.
fn catalogue_entry(descriptor: &RuleDescriptor, said: Option<&str>) -> String {
    let category = match descriptor.category {
        RuleCategory::Flow => "Flow's own lint set",
        RuleCategory::Uniflowed => "uf house rule",
        RuleCategory::React => "React rule",
        RuleCategory::ReactNative => "React Native rule",
        RuleCategory::Server => "server/client boundary rule",
        RuleCategory::Router => "router rule",
        RuleCategory::Package => "`package.json` rule",
        RuleCategory::Fetch => "`@uniflowed/fetch` rule",
        RuleCategory::Security => "security rule",
    };
    let level = match descriptor.default_level {
        uf_config::RuleLevel::Off => "off",
        uf_config::RuleLevel::Warn => "warn",
        uf_config::RuleLevel::Error => "error",
    };
    let requirement = match descriptor.requirement {
        RuleRequirement::SourceText => "decided from the source text",
        RuleRequirement::TypeChecker => {
            "needs Flow type inference, which `uf lint` does not run yet"
        }
    };
    let summary = format!("{category} · default `{level}` · {requirement}");
    match said {
        Some(said) if said == descriptor.description => summary,
        _ => format!(
            "{description}\n\n{summary}",
            description = descriptor.description
        ),
    }
}

/// What uf knows about one import specifier.
fn describe_specifier(specifier: &str, request: &Request<'_>) -> String {
    let mut markdown = format!("**`{specifier}`**\n\n");

    if let Some(module) = request
        .modules
        .iter()
        .find(|module| module.specifier.as_str() == specifier)
    {
        markdown.push_str(&describe_native_module(module));
    } else if is_relative(specifier) {
        markdown.push_str(&describe_relative(specifier, request.path));
    } else {
        markdown.push_str(
            "A package specifier. uf leaves package resolution to the bundler, so what \
             this names depends on the project's installed dependencies.",
        );
    }

    if is_server_only_specifier(specifier) {
        markdown.push_str(
            "\n\n**Server-only.** A module carrying `\"use client\"` must not import this; \
             `server/no-server-only-import-in-client` reports it if one does.",
        );
    }
    markdown
}

/// A `@uniflowed/*` module, from the registry `uf inspect` enumerates.
fn describe_native_module(module: &NativeModule) -> String {
    let kind = serde_json::to_value(module.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| String::from("module"));
    let stability = serde_json::to_value(module.stability)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| String::from("unknown"));

    let mut markdown = format!("A uf `{kind}` module, {stability}.");
    if !module.flow_exports.is_empty() {
        let exports = module
            .flow_exports
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        markdown.push_str(&format!("\n\nFlow exports: {exports}"));
    }
    markdown
}

/// Where a relative specifier lands, resolved against the file system.
///
/// The candidate order is `uf_test`'s: the path as written, then each of
/// [`MODULE_EXTENSIONS`], then a directory index. That crate resolves against
/// the set of modules it was handed because its graph must stay a pure
/// function of its inputs; an editor is asking about a file that exists or
/// does not, so this asks the file system the same question in the same order.
fn describe_relative(specifier: &str, path: &str) -> String {
    let Some(directory) = Utf8Path::new(path).parent() else {
        return String::from(
            "A relative import. uf could not tell which directory it is relative to, because the document's URI names no parent directory.",
        );
    };
    let target = uf_rsc::normalize_module_path(&directory.join(specifier));

    for candidate in candidates(&target) {
        if std::fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
            return format!("A relative import. Resolves to `{candidate}`.");
        }
    }
    format!(
        "A relative import. Nothing on disk at `{target}`, with or without uf's module \
         extensions ({extensions}).",
        extensions = MODULE_EXTENSIONS
            .iter()
            .map(|extension| format!("`.{extension}`"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// Every path a relative specifier could name, in resolution order.
fn candidates(target: &Utf8Path) -> Vec<String> {
    let mut paths = Vec::with_capacity(1 + MODULE_EXTENSIONS.len() * 2);
    paths.push(target.to_string());
    paths.extend(MODULE_EXTENSIONS.map(|extension| format!("{target}.{extension}")));
    paths.extend(MODULE_EXTENSIONS.map(|extension| format!("{target}/index.{extension}")));
    paths
}

fn is_relative(specifier: &str) -> bool {
    specifier == "." || specifier.starts_with("./") || specifier.starts_with("../")
}

/// Byte range of the quoted `specifier` on `text` that contains `column`.
///
/// The lexer reports the line an import is on but not its column, and one line
/// can hold several imports, so the occurrence is picked by which one the
/// cursor is inside. A specifier written with an escape does not appear
/// literally in the line and gets no answer, which is the right way to be
/// wrong here.
fn quoted_span(text: &str, specifier: &str, column: usize) -> Option<(usize, usize)> {
    for quote in ['"', '\''] {
        let needle = format!("{quote}{specifier}{quote}");
        let mut from = 0usize;
        while let Some(offset) = text.get(from..)?.find(&needle) {
            let start = from + offset;
            let end = start + needle.len();
            if (start..=end).contains(&column) {
                return Some((start, end));
            }
            from = end;
        }
    }
    None
}

/// Byte range of the rule-id-shaped word `column` is inside.
///
/// Rule ids are `namespace/name`, with `-` inside either half, so the word is
/// taken over exactly those bytes. All of them are ASCII, so extending byte by
/// byte can never split a character.
fn rule_id_span(text: &str, column: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let is_part = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/');

    let mut start = column.min(bytes.len());
    while start > 0 && is_part(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = column.min(bytes.len());
    while end < bytes.len() && is_part(bytes[end]) {
        end += 1;
    }
    (start < end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(rule: &'static str, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            rule,
            severity: Severity::Error,
            path: Some("app/index.js".to_owned()),
            line,
            column,
            message: String::from("the `bool` type alias is deprecated; write `boolean`"),
        }
    }

    fn request<'a>(
        source: &'a str,
        line: usize,
        column: usize,
        diagnostics: &'a [Diagnostic],
        modules: &'a [NativeModule],
    ) -> Request<'a> {
        Request {
            path: "/project/app/index.js",
            source,
            line,
            column,
            diagnostics,
            modules,
        }
    }

    #[test]
    fn a_diagnostic_under_the_cursor_explains_its_rule() {
        let source = "// @flow\ntype B = bool;\n";
        let diagnostics = [diagnostic("flow/deprecated-type", 2, 10)];
        let answer = hover(&request(source, 1, 10, &diagnostics, &[])).expect("an answer");

        assert!(
            answer.markdown.contains("flow/deprecated-type"),
            "{}",
            answer.markdown
        );
        assert!(
            answer.markdown.contains("deprecated"),
            "{}",
            answer.markdown
        );
        // The catalogue entry, not just the message.
        assert!(
            answer.markdown.contains("Flow's own lint set"),
            "{}",
            answer.markdown
        );
        assert_eq!((answer.start, answer.end), (9, 13));
    }

    /// Off the squiggle, on a line that has one, is not an answer.
    #[test]
    fn a_position_away_from_the_diagnostic_gets_nothing() {
        let source = "// @flow\ntype B = bool;\n";
        let diagnostics = [diagnostic("flow/deprecated-type", 2, 10)];

        assert!(hover(&request(source, 1, 2, &diagnostics, &[])).is_none());
    }

    /// A plain expression is where the missing type checker shows; it must say
    /// nothing rather than say nothing *confidently*.
    #[test]
    fn an_ordinary_expression_gets_no_answer() {
        let source = "// @flow\nconst total = 1 + 2;\n";

        assert!(hover(&request(source, 1, 8, &[], &[])).is_none());
        // Past the end of a line, and past the end of the file.
        assert!(hover(&request(source, 1, 999, &[], &[])).is_none());
        assert!(hover(&request(source, 40, 0, &[], &[])).is_none());
    }

    #[test]
    fn a_uf_module_specifier_is_described_from_the_registry() {
        let modules = uf_lib::builtin_modules();
        let source = "import { Node } from \"@uniflowed/react\";\n";
        let answer = hover(&request(source, 0, 30, &[], &modules)).expect("an answer");

        assert!(
            answer.markdown.contains("@uniflowed/react"),
            "{}",
            answer.markdown
        );
        assert!(answer.markdown.contains("framework"), "{}", answer.markdown);
        assert!(answer.markdown.contains("`Node`"), "{}", answer.markdown);
    }

    #[test]
    fn a_bare_specifier_says_the_bundler_resolves_it() {
        let source = "import React from \"react\";\n";
        let answer = hover(&request(source, 0, 20, &[], &[])).expect("an answer");

        assert!(answer.markdown.contains("bundler"), "{}", answer.markdown);
    }

    #[test]
    fn a_server_only_specifier_says_so() {
        let source = "import { db } from \"@uniflowed/server\";\n";
        let answer = hover(&request(source, 0, 25, &[], &[])).expect("an answer");

        assert!(
            answer.markdown.contains("Server-only"),
            "{}",
            answer.markdown
        );
    }

    #[test]
    fn a_relative_specifier_that_names_nothing_says_nothing_is_there() {
        let source = "import { x } from \"./nowhere\";\n";
        let answer = hover(&request(source, 0, 22, &[], &[])).expect("an answer");

        assert!(
            answer.markdown.contains("Nothing on disk"),
            "{}",
            answer.markdown
        );
        assert!(
            answer.markdown.contains("/project/app/nowhere"),
            "{}",
            answer.markdown
        );
    }

    /// Two imports on one line: the cursor picks which one is answered.
    #[test]
    fn the_specifier_under_the_cursor_is_the_one_answered() {
        let source = "import a from \"react\"; import b from \"@uniflowed/react\";\n";

        let first = hover(&request(source, 0, 17, &[], &[])).expect("an answer");
        assert!(first.markdown.contains("**`react`**"), "{}", first.markdown);

        let second = hover(&request(source, 0, 45, &[], &[])).expect("an answer");
        assert!(
            second.markdown.contains("**`@uniflowed/react`**"),
            "{}",
            second.markdown
        );
    }

    #[test]
    fn a_rule_id_in_a_suppression_comment_explains_itself() {
        let source = "// uf-lint-disable-next-line flow/unclear-type\nlet a: any;\n";
        let answer = hover(&request(source, 0, 34, &[], &[])).expect("an answer");

        assert!(
            answer.markdown.contains("flow/unclear-type"),
            "{}",
            answer.markdown
        );
        assert!(answer.markdown.contains("default"), "{}", answer.markdown);
        assert_eq!((answer.start, answer.end), (29, 46));
    }

    #[test]
    fn a_deprecated_rule_id_names_its_replacement() {
        let source = "// uf-lint-disable-next-line flow/type-aware/no-explicit-any\nlet a: any;\n";
        let answer = hover(&request(source, 0, 40, &[], &[])).expect("an answer");

        assert!(
            answer.markdown.contains("flow/unclear-type"),
            "{}",
            answer.markdown
        );
        assert!(
            answer.markdown.contains("deprecated spelling"),
            "{}",
            answer.markdown
        );
    }

    #[test]
    fn a_word_that_is_not_a_rule_id_gets_no_answer() {
        let source = "const ratio = width/height;\n";

        assert!(hover(&request(source, 0, 16, &[], &[])).is_none());
    }
}
