//! The formatter's guarantees, on sources that are not fixtures: the
//! `@uniflowed/*` packages, the project templates `uf create` writes, and
//! a corpus of mutated and adversarial inputs.
//!
//! The fixtures pin *what* the printer produces; these tests pin what it
//! may never do to any input at all.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use uf_config::{FmtConfig, QuoteStyle};
use uf_fmt::{FormatError, format_source};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.js` file the repository ships, as `(label, source)`.
///
/// The `@uniflowed/*` packages are real Flow modules written by hand, and
/// the templates are what a new project starts from, so they are the
/// closest thing to a user's code the repository can test against.
fn shipped_sources() -> Vec<(String, String)> {
    let mut sources = Vec::new();
    let packages = repository_root().join("packages");
    let mut modules = Vec::new();
    collect_js(&packages, &mut modules);
    modules.sort();
    for module in modules {
        let source = fs::read_to_string(&module)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", module.display()));
        sources.push((module.display().to_string(), source));
    }
    sources
}

fn collect_js(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            collect_js(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("js") {
            out.push(path);
        }
    }
}

/// The `.js` sources embedded in `uf_project`'s templates, pulled out of
/// the raw string literals that hold them.
///
/// Reading them out of the source rather than depending on `uf_project`
/// keeps the formatter's test suite from pulling in the project crate for
/// one string.
fn template_sources() -> Vec<(String, String)> {
    let path = repository_root().join("crates/uf_project/src/template.rs");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut sources = Vec::new();
    for (index, chunk) in text.split("r#\"").enumerate().skip(1) {
        let Some(source) = chunk.split("\"#").next() else {
            continue;
        };
        // The JSON templates are not JavaScript; the Flow ones all start
        // with a directive or a `// @flow` pragma.
        if source.starts_with("// @flow") || source.starts_with('"') {
            sources.push((format!("template #{index}"), source.to_string()));
        }
    }
    assert!(
        sources.len() >= 8,
        "expected the app and lib templates, found {}",
        sources.len()
    );
    sources
}

/// The configurations every guarantee is checked under.
fn configurations() -> Vec<FmtConfig> {
    let mut narrow = FmtConfig::default();
    narrow.line_width = 40;
    narrow.indent_width = 4;
    narrow.quotes = QuoteStyle::Single;
    narrow.semicolons = false;

    let mut wide = FmtConfig::default();
    wide.line_width = 200;

    vec![FmtConfig::default(), narrow, wide]
}

#[test]
fn shipped_sources_are_formatted_idempotently() {
    for (label, source) in shipped_sources().into_iter().chain(template_sources()) {
        for config in configurations() {
            let once = format_source(&source, &config)
                .unwrap_or_else(|error| panic!("{label} formats: {error}"))
                .output;
            let twice = format_source(&once, &config)
                .unwrap_or_else(|error| panic!("{label} reformats: {error}"))
                .output;
            similar_asserts::assert_eq!(once, twice, "{label} is not idempotent");
        }
    }
}

#[test]
fn shipped_sources_keep_their_tree_and_comments() {
    for (label, source) in shipped_sources().into_iter().chain(template_sources()) {
        let output = format_source(&source, &FmtConfig::default())
            .unwrap_or_else(|error| panic!("{label} formats: {error}"))
            .output;
        similar_asserts::assert_eq!(
            support::structure(&source),
            support::structure(&output),
            "{label} changed the program"
        );
        assert_eq!(
            support::comment_multiset(&source),
            support::comment_multiset(&output),
            "{label} lost, gained or rewrote a comment"
        );
    }
}

/// Sources chosen to exercise the parts of the printer that are easy to
/// get wrong, checked under every configuration.
const CORPUS: &[&str] = &[
    "",
    "\n",
    "// @flow\n",
    "\u{feff}const withBom = 1;\n",
    "const crlf = 1;\r\nconst second = 2;\r\n",
    "#!/usr/bin/env node\nconst shebang = 1;\n",
    "const x = 1;\n",
    "const re = /ab+c/gi;\nif (x) /re/.test(y);\n",
    "const t = `a${b}c${`nested ${d}`}e`;\n",
    "const s = 'it\\'s';\nconst d = \"say \\\"hi\\\"\";\n",
    "/* block */ /** doc */ // line\n",
    "const n = [0x1f, 0b1010, 0o777, 1_000, 1e10, 1n, .5, 1.5e-3];\n",
    "type A = ?string;\ntype B = Array<Map<string, number>>;\n",
    "opaque type Id = string;\n",
    "component Greeting(name: string) renders React.Node { return <p>hi</p>; }\n",
    "const el = <div className=\"a\" data-testid='b'>text {value} more</div>;\n",
    "const frag = <>{items.map((i) => <Item key={i} />)}</>;\n",
    "function f() { switch (x) { case 1: return 2; default: break; } }\n",
    "class A extends B { #x = 1; static y = 2; *gen() { yield* other(); } }\n",
    "const emoji = \"日本語 🎉\";\nconst ident = ünïcödé;\n",
    "async function f() { await g(); for (let i = 0; i < 3; i++) {} }\n",
    "export default function App() {}\n",
    "const o = { a: 1, 'b': 2, [c]: 3, ...rest };\n",
    "label: for (const item of items) { continue label; }\n",
    "declare export function f(): void;\n",
    "\"use client\";\n",
    "const tagged = html`<b>${x}</b>`;\n",
    "const deep = { a: { b: { c: [1, 2, [3, { d: 4 }]] } } };\n",
    "type Exact = {| +read: string, -write?: ?number |};\n",
    "match (x) { 1 => f(), _ => g() }\n",
    "enum E of string { A = \"a\", ... }\n",
    "const guarded = a?.b?.[c]?.(d);\n",
    "const chain = (a?.b)();\n",
];

/// Flow's contextual keywords, at the start of a statement and everywhere
/// else.
///
/// Not in {@link CORPUS}: that list is fuzzed by
/// `no_mutated_input_panics`, and adding an entry reshuffles which mutation
/// every other entry gets. A regression test should not decide what an
/// unrelated fuzzer does.
#[test]
fn a_contextual_keyword_is_parenthesized_only_where_it_has_to_be() {
    // Needs them: the parser commits to a type alias and wants a `=`.
    // React's devtools writes the first.
    for source in [
        "(type) as empty;\n",
        "(component) as empty;\n",
        "(interface) as empty;\n",
        "(hook) as const;\n",
    ] {
        let output = format_source(source, &FmtConfig::default())
            .unwrap_or_else(|error| panic!("{source:?} formats: {error}"))
            .output;
        assert_eq!(output, source, "lost the parentheses it needs");
    }

    // Does not: every one of these puts punctuation after the identifier,
    // so the parser gives up on the declaration and reads an expression.
    // An earlier version of the rule asked only whether the identifier was
    // the first token, and turned the first of these into
    // `(hook).renderers.forEach(…)` — in this repository's own
    // `refresh-runtime.js`.
    for source in [
        "hook.renderers.forEach((injected, id) => injected.id === id);\n",
        "type(argument);\n",
        "component[0] = 1;\n",
        "type;\n",
    ] {
        let output = format_source(source, &FmtConfig::default())
            .unwrap_or_else(|error| panic!("{source:?} formats: {error}"))
            .output;
        assert!(
            !output.starts_with('('),
            "grew parentheses it does not need:\n{output}"
        );
    }
}

#[test]
fn the_corpus_is_idempotent_under_every_configuration() {
    for config in configurations() {
        for source in CORPUS {
            let once = format_source(source, &config)
                .unwrap_or_else(|error| panic!("{source:?} formats: {error}"))
                .output;
            let twice = format_source(&once, &config)
                .unwrap_or_else(|error| panic!("{source:?} reformats: {error}"))
                .output;
            similar_asserts::assert_eq!(once, twice, "not idempotent for {source:?}");
        }
    }
}

#[test]
fn the_corpus_keeps_its_tree_and_comments() {
    for source in CORPUS {
        let output = format_source(source, &FmtConfig::default())
            .unwrap_or_else(|error| panic!("{source:?} formats: {error}"))
            .output;
        similar_asserts::assert_eq!(
            support::structure(source),
            support::structure(&output),
            "{source:?} changed the program"
        );
        assert_eq!(
            support::comment_multiset(source),
            support::comment_multiset(&output),
            "{source:?} changed a comment"
        );
    }
}

/// Invalid syntax is an error, never a rewrite: the caller leaves the file
/// alone rather than saving the parser's guess at what was meant.
#[test]
fn invalid_syntax_is_refused() {
    for source in [
        "const = ;\n",
        "function () {}\n",
        "class {\n",
        "const x = ;\n",
        "type = ;\n",
        "<div>\n",
        "`unterminated ${\n",
        "/* unterminated\n",
        "'unterminated\n",
        "}}}}\n",
        "((((\n",
        "import from;\n",
    ] {
        let error = format_source(source, &FmtConfig::default())
            .expect_err(&format!("{source:?} must be refused"));
        assert!(
            matches!(error, FormatError::Flow(_)),
            "{source:?} gave {error:?}"
        );
    }
}

/// JSX that runs out at end of input is refused, not closed for the author.
///
/// ubugeeei-prod/uf#128. Flow's port recovers a truncated element by
/// inventing the closing tag, and for this shape — end of input with no
/// trailing newline — it reports no diagnostic while doing so. The printer
/// then wrote out a tree with no `</div>` in it, breaking both of the
/// guarantees above it in this file at once.
///
/// The same sources *with* a trailing newline are refused by the parser and
/// always were, which is why `invalid_syntax_is_refused` did not catch this:
/// every entry in that list ends in one.
#[test]
fn jsx_truncated_at_end_of_input_is_refused() {
    for source in [
        // The 58 bytes from the issue.
        "const el = <div className=\"a\" data-testid='b'>text {value}",
        // The same without attributes, and nested.
        "const el = <div>text {value}",
        "const el = <a><b>{x}",
        // Truncated after a complete child element.
        "const el = <ul><li /></ul",
        // Inside a function body, so the element is not the last thing the
        // parser sees.
        "const f = () => <div>{x}",
    ] {
        let error = format_source(source, &FmtConfig::default())
            .expect_err(&format!("{source:?} must be refused"));
        assert!(
            matches!(error, FormatError::Flow(_)),
            "{source:?} gave {error:?}"
        );
    }
}

/// Self-closing elements are not truncated ones.
///
/// The check behind {@link jsx_truncated_at_end_of_input_is_refused} reads
/// "no closing tag", and `<br />` has no closing tag either — the tree
/// stores `None` for both. Telling them apart is the whole difficulty, so
/// the case that must keep working gets a test of its own rather than
/// relying on the corpus to notice.
#[test]
fn self_closing_jsx_still_formats() {
    for source in [
        "const el = <br />;\n",
        "const el = <Foo bar={1} />;\n",
        "const el = <div><br /><hr /></div>;\n",
        "const el = <Foo.Bar.Baz />;\n",
        "const el = <svg:rect />;\n",
        "const el = <></>;\n",
        "const el = <>{x}</>;\n",
        // No trailing newline, which is the axis the bug turned on.
        "const el = <br />;",
    ] {
        format_source(source, &FmtConfig::default())
            .unwrap_or_else(|error| panic!("{source:?} must format: {error:?}"));
    }
}

/// Nesting past the parser's ceiling is a typed error rather than a stack
/// overflow, which is the failure mode this whole design exists to avoid.
#[test]
fn nesting_past_the_ceiling_is_refused() {
    let depth = uf_flow::MAX_NESTING_DEPTH + 1;
    for (open, close) in [("[", "]"), ("(", ")"), ("{a:", "}"), ("`${", "}`")] {
        let source = format!("x = {}1{};\n", open.repeat(depth), close.repeat(depth));
        let error = format_source(&source, &FmtConfig::default()).expect_err("refused");
        assert!(matches!(error, FormatError::Flow(_)), "{open}: {error:?}");
    }
    let braces = "{".repeat(10_000) + &"}".repeat(10_000);
    assert!(format_source(&braces, &FmtConfig::default()).is_err());
}

/// Nesting *at* the ceiling still formats, so the limit is a real one
/// rather than a number no input ever reaches.
#[test]
fn nesting_at_the_ceiling_is_formatted() {
    let depth = uf_flow::MAX_NESTING_DEPTH;
    let source = format!("x = {}1{};\n", "[".repeat(depth), "]".repeat(depth));
    let config = FmtConfig::default();
    let formatted = format_source(&source, &config).expect("formats");
    assert_eq!(formatted.output.matches('[').count(), depth);
    let again = format_source(&formatted.output, &config).expect("reformats");
    similar_asserts::assert_eq!(formatted.output, again.output);
}

/// Nesting does not cost exponentially.
///
/// ubugeeei-prod/uf#125. `print_arguments` prints the argument it is
/// considering hugging a second time, and so does every level below it, so
/// the document was 2^depth: about 4x per level, sixteen seconds at twelve,
/// and React Native's `react-native-compatibility-check` — nineteen deep —
/// did not finish at all.
///
/// Forty rather than twelve, because twelve is now too fast to distinguish
/// from nothing and forty is decisive: at 4x per level it would be longer
/// than the age of the universe.
#[test]
fn deep_call_arguments_are_not_exponential() {
    let mut source = "x".to_owned();
    for _ in 0..40 {
        source = format!("expect.objectContaining({{ fault: {source} }})");
    }
    let source = format!("// @flow\nconst result = {source};\n");

    let config = FmtConfig::default();
    let started = Instant::now();
    let once = format_source(&source, &config).expect("formats").output;
    let took = started.elapsed();

    assert!(took < Duration::from_secs(2), "forty levels took {took:?}");

    // And the answer is a real one: it settles, and it is still the same
    // program. A cache that returned the wrong document would be fast.
    let twice = format_source(&once, &config).expect("reformats").output;
    similar_asserts::assert_eq!(once, twice);
    assert_eq!(
        support::structure(&source),
        support::structure(&once),
        "the program changed"
    );
}

/// No input panics or hangs. The mutations are deterministic — a fixed
/// pseudo-random walk over the corpus — so a failure can be reproduced
/// from the seed printed in the message.
#[test]
fn no_mutated_input_panics() {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let config = FmtConfig::default();
    for seed in 0..2_000u32 {
        let source = CORPUS[(next() % CORPUS.len() as u64) as usize];
        let bytes = source.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        let mut mutated = bytes.to_vec();
        match next() % 4 {
            // Truncate.
            0 => mutated.truncate((next() as usize) % bytes.len()),
            // Delete one byte.
            1 => {
                let at = (next() as usize) % bytes.len();
                mutated.remove(at);
            }
            // Duplicate a byte.
            2 => {
                let at = (next() as usize) % bytes.len();
                mutated.insert(at, bytes[at]);
            }
            // Replace a byte with a delimiter.
            _ => {
                let at = (next() as usize) % bytes.len();
                mutated[at] = *b"{}()[]<>`'\"/\\*"
                    .get((next() as usize) % 14)
                    .unwrap_or(&b'{');
            }
        }
        let Ok(mutated) = String::from_utf8(mutated) else {
            continue;
        };
        // The result may be an error; what matters is that it returns.
        let outcome = format_source(&mutated, &config);
        if let Ok(result) = outcome {
            let again = format_source(&result.output, &config).unwrap_or_else(|error| {
                panic!(
                    "seed {seed}: reformat failed: {error}\n\
                     --- in\n{mutated:?}\n--- out\n{:?}",
                    result.output
                )
            });
            similar_asserts::assert_eq!(
                result.output,
                again.output,
                "seed {seed} is not idempotent for {mutated:?}"
            );
        }
    }
}

/// A megabyte-long line is formatted rather than refused, and a source
/// past the ceiling is refused rather than formatted.
#[test]
fn size_limits_hold() {
    let long = format!("const x = \"{}\";\n", "a".repeat(1_000_000));
    let config = FmtConfig::default();
    let formatted = format_source(&long, &config).expect("formats");
    // The string itself cannot break, so only the assignment moves.
    similar_asserts::assert_eq!(
        formatted.output,
        format!("const x =\n  \"{}\";\n", "a".repeat(1_000_000))
    );
    let again = format_source(&formatted.output, &config).expect("reformats");
    similar_asserts::assert_eq!(formatted.output, again.output);

    let huge = format!("const x = \"{}\";\n", "a".repeat(uf_flow::MAX_PARSE_BYTES));
    assert!(format_source(&huge, &FmtConfig::default()).is_err());
}

/// The configuration knobs the formatter rejects outright.
#[test]
fn invalid_configuration_is_rejected() {
    let mut zero_indent = FmtConfig::default();
    zero_indent.indent_width = 0;
    assert_eq!(
        format_source("x;\n", &zero_indent),
        Err(FormatError::InvalidIndentWidth)
    );

    let mut wide_indent = FmtConfig::default();
    wide_indent.indent_width = 200;
    assert_eq!(
        format_source("x;\n", &wide_indent),
        Err(FormatError::InvalidIndentWidth)
    );

    let mut zero_width = FmtConfig::default();
    zero_width.line_width = 0;
    assert_eq!(
        format_source("x;\n", &zero_width),
        Err(FormatError::InvalidLineWidth)
    );
}

/// Bytes rather than syntax: a byte order mark survives, CRLF and lone CR
/// become LF, and the file ends with exactly one newline.
#[test]
fn encoding_is_normalised() {
    let config = FmtConfig::default();

    let bom = format_source("\u{feff}const x = 1;\n", &config).expect("formats");
    assert!(bom.output.starts_with('\u{feff}'));
    similar_asserts::assert_eq!(bom.output, "\u{feff}const x = 1;\n");

    let crlf = format_source("const x = 1;\r\nconst y = 2;\r\n", &config).expect("formats");
    similar_asserts::assert_eq!(crlf.output, "const x = 1;\nconst y = 2;\n");

    let cr = format_source("a;\rb;\r", &config).expect("formats");
    similar_asserts::assert_eq!(cr.output, "a;\nb;\n");

    let trailing = format_source("const x = 1;\n\n\n\n", &config).expect("formats");
    similar_asserts::assert_eq!(trailing.output, "const x = 1;\n");

    let empty = format_source("", &config).expect("formats");
    assert_eq!(empty.output, "");

    let blank = format_source("   \n\n", &config).expect("formats");
    assert_eq!(blank.output, "");
}

/// `changed` says whether the file needs writing, which is what
/// `uf fmt --check` reports.
#[test]
fn changed_reports_whether_the_output_differs() {
    let config = FmtConfig::default();
    assert!(!format_source("const x = 1;\n", &config).unwrap().changed);
    assert!(format_source("const x = 1;  \n", &config).unwrap().changed);
}
