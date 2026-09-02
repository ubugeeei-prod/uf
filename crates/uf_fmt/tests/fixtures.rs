//! Golden-file tests for the native formatter.
//!
//! Every `<name>.input.js` under `tests/fixtures/` is formatted and compared to
//! `<name>.expected.js`. On top of the golden comparison each fixture is checked
//! against the three invariants the formatter promises: the lexer round-trips the
//! bytes, formatting is idempotent, and the token stream survives untouched.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use uf_config::{FmtConfig, QuoteStyle};
use uf_fmt::format_source;
use uf_fmt::lexer::{Punctuator, Token, TokenKind, tokenize};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A default config with  applied.
///
///  is , because every feature that lands adds a
/// knob to it and a struct literal would break on each one.
fn config_with(mutate: impl FnOnce(&mut FmtConfig)) -> FmtConfig {
    let mut config = FmtConfig::default();
    mutate(&mut config);
    config
}

/// Fixtures whose name carries a non-default configuration.
fn config_for(name: &str) -> FmtConfig {
    match name {
        "config_single_quotes" => config_with(|config| {
            config.quotes = QuoteStyle::Single;
        }),
        "config_no_semicolons" => config_with(|config| {
            config.semicolons = false;
        }),
        "config_wide_indent" => config_with(|config| {
            config.indent_width = 4;
            config.max_blank_lines = 0;
        }),
        "config_narrow_lines" => config_with(|config| {
            config.line_width = 40;
        }),
        _ => FmtConfig::default(),
    }
}

/// Every fixture, keyed by name, as `(input, expected)` pairs.
fn fixtures() -> BTreeMap<String, (String, String)> {
    let dir = fixtures_dir();
    let entries = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()));

    let mut fixtures = BTreeMap::new();
    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let Some(name) = file_name.strip_suffix(".input.js") else {
            continue;
        };
        let expected_path = dir.join(format!("{name}.expected.js"));
        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|error| {
            panic!("missing {}: {error}", expected_path.display());
        });
        let input = fs::read_to_string(&path).expect("readable fixture input");
        fixtures.insert(name.to_string(), (input, expected));
    }

    assert!(
        !fixtures.is_empty(),
        "no fixtures found in {}",
        dir.display()
    );
    fixtures
}

/// Canonical spelling of a token, so a requoted string still compares equal.
fn canonical(token: Token, source: &str) -> String {
    let text = token.text(source);
    match token.kind {
        TokenKind::String | TokenKind::JsxString => {
            let body = text
                .strip_prefix(['\'', '"'])
                .and_then(|rest| rest.strip_suffix(['\'', '"']))
                .unwrap_or(text);
            let mut canonical = String::with_capacity(body.len());
            let mut chars = body.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some(next @ ('"' | '\'')) => canonical.push(next),
                        Some(next) => {
                            canonical.push('\\');
                            canonical.push(next);
                        }
                        None => canonical.push('\\'),
                    }
                } else {
                    canonical.push(ch);
                }
            }
            canonical
        }
        _ => text.to_string(),
    }
}

/// Expand `>>`/`>>>` into the individual `>` characters they stand for.
///
/// Removing the space in `Array<Map<K, V> >` produces `Array<Map<K, V>>`, which
/// the lexer reads back as one shift-shaped token even though a Flow parser
/// splits it again when it closes type arguments. Comparing at the `>` character
/// level keeps that legal rewrite from tripping the token-preservation check
/// without weakening it: the printer never inserts characters inside a token, so
/// the count can only ever be preserved.
fn expand_angles(kind: TokenKind, text: String, out: &mut Vec<(TokenKind, String)>) {
    let repeats = match kind {
        TokenKind::Punctuator(Punctuator::Greater) => 1,
        TokenKind::Punctuator(Punctuator::GreaterGreater) => 2,
        TokenKind::Punctuator(Punctuator::GreaterGreaterGreater) => 3,
        _ => {
            out.push((kind, text));
            return;
        }
    };
    for _ in 0..repeats {
        out.push((TokenKind::Punctuator(Punctuator::Greater), ">".to_string()));
    }
}

/// The tokens the formatter is not allowed to touch: everything except trivia
/// and statement-terminating semicolons.
fn program_tokens(source: &str) -> Vec<(TokenKind, String)> {
    let mut tokens = Vec::new();
    for token in tokenize(source) {
        if token.kind.is_trivia() || token.kind == TokenKind::Punctuator(Punctuator::Semicolon) {
            continue;
        }
        expand_angles(token.kind, canonical(token, source), &mut tokens);
    }
    tokens
}

fn comments(source: &str) -> Vec<String> {
    tokenize(source)
        .into_iter()
        .filter(|token| token.kind.is_comment())
        .map(|token| token.text(source).to_string())
        .collect()
}

/// Set `UF_FMT_BLESS=1` to rewrite the `.expected.js` files from the current
/// formatter output. Review the diff before committing it.
fn blessing() -> bool {
    std::env::var_os("UF_FMT_BLESS").is_some()
}

#[test]
fn fixtures_match_their_expected_output() {
    let dir = fixtures_dir();
    for (name, (input, expected)) in fixtures() {
        let result = format_source(&input, &config_for(&name)).expect("fixture formats");
        if blessing() {
            fs::write(dir.join(format!("{name}.expected.js")), &result.output)
                .expect("writable fixture");
            continue;
        }
        similar_asserts::assert_eq!(result.output, expected, "fixture {name} formatted wrong");
    }
    assert!(
        !blessing(),
        "fixtures were rewritten; rerun without UF_FMT_BLESS"
    );
}

#[test]
fn fixtures_are_idempotent() {
    for (name, (input, _)) in fixtures() {
        let config = config_for(&name);
        let once = format_source(&input, &config)
            .expect("fixture formats")
            .output;
        let twice = format_source(&once, &config)
            .expect("fixture formats")
            .output;
        similar_asserts::assert_eq!(once, twice, "fixture {name} is not idempotent");
    }
}

#[test]
fn expected_output_is_already_formatted() {
    for (name, (_, expected)) in fixtures() {
        let result = format_source(&expected, &config_for(&name)).expect("fixture formats");
        assert!(
            !result.changed,
            "fixture {name} expected output is not a fixed point"
        );
    }
}

#[test]
fn fixtures_preserve_the_token_stream() {
    for (name, (input, _)) in fixtures() {
        let output = format_source(&input, &config_for(&name))
            .expect("fixture formats")
            .output;
        similar_asserts::assert_eq!(
            program_tokens(&input),
            program_tokens(&output),
            "fixture {name} changed the token stream"
        );
    }
}

#[test]
fn fixtures_preserve_comments_verbatim() {
    for (name, (input, _)) in fixtures() {
        let output = format_source(&input, &config_for(&name))
            .expect("fixture formats")
            .output;
        assert_eq!(
            comments(&input),
            comments(&output),
            "fixture {name} rewrote a comment"
        );
    }
}

#[test]
fn the_lexer_round_trips_every_fixture() {
    for (name, (input, expected)) in fixtures() {
        for (label, source) in [("input", &input), ("expected", &expected)] {
            let mut rebuilt = String::with_capacity(source.len());
            let mut cursor = 0;
            for token in tokenize(source) {
                assert_eq!(
                    token.span.start, cursor,
                    "fixture {name} {label} has a gap in its token spans"
                );
                cursor = token.span.end;
                rebuilt.push_str(token.text(source));
            }
            assert_eq!(cursor, source.len(), "fixture {name} {label} ends early");
            similar_asserts::assert_eq!(
                &rebuilt,
                source,
                "fixture {name} {label} did not round trip"
            );
        }
    }
}

#[test]
fn every_expected_file_has_an_input() {
    let dir = fixtures_dir();
    for entry in fs::read_dir(&dir).expect("readable fixture directory") {
        let path = entry.expect("readable directory entry").path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if let Some(name) = file_name.strip_suffix(".expected.js") {
            let input = dir.join(format!("{name}.input.js"));
            assert!(input.exists(), "orphan fixture {}", path.display());
        }
    }
}
