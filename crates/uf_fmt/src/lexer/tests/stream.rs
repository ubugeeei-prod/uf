//! The lossless invariants: the tokens tile the source, trivia is never dropped,
//! and pathological or unbalanced input still round-trips.

use super::{kinds, significant};
use crate::SOURCE_CORPUS as CORPUS;
use crate::lexer::{TokenKind, Unterminated, tokenize};

#[test]
fn tokens_reproduce_the_source_byte_for_byte() {
    for source in CORPUS {
        let mut rebuilt = String::with_capacity(source.len());
        for token in tokenize(source) {
            rebuilt.push_str(token.text(source));
        }
        assert_eq!(&rebuilt, source, "round trip failed for {source:?}");
    }
}

#[test]
fn token_spans_tile_the_source_without_gaps() {
    for source in CORPUS {
        let mut cursor = 0;
        for token in tokenize(source) {
            assert_eq!(token.span.start, cursor, "gap in {source:?}");
            assert!(
                token.span.end > token.span.start,
                "empty token in {source:?}"
            );
            cursor = token.span.end;
        }
        assert_eq!(cursor, source.len(), "trailing gap in {source:?}");
    }
}

#[test]
fn empty_source_produces_no_tokens() {
    assert!(tokenize("").is_empty());
}

#[test]
fn line_comments_stop_before_the_newline() {
    let tokens = kinds("// hi\nx");
    assert_eq!(
        tokens,
        vec![
            TokenKind::LineComment,
            TokenKind::Newline,
            TokenKind::Identifier
        ]
    );
}

#[test]
fn doc_comments_are_distinguished_from_block_comments() {
    assert_eq!(kinds("/** doc */"), vec![TokenKind::DocComment]);
    assert_eq!(kinds("/* plain */"), vec![TokenKind::BlockComment]);
    assert_eq!(kinds("/**/"), vec![TokenKind::BlockComment]);
}

#[test]
fn unterminated_block_comment_runs_to_end_of_input() {
    assert_eq!(
        kinds("/* nope"),
        vec![TokenKind::Unterminated(Unterminated::BlockComment)]
    );
}
#[test]
fn shebang_is_only_recognized_at_offset_zero() {
    assert_eq!(kinds("#!/usr/bin/env uf\n")[0], TokenKind::Shebang);
    assert_eq!(significant("x\n#!y")[1], TokenKind::Unknown);
}
#[test]
fn deeply_nested_braces_do_not_overflow_the_stack() {
    let depth = 10_000;
    let mut source = String::with_capacity(depth * 2);
    for _ in 0..depth {
        source.push('{');
    }
    for _ in 0..depth {
        source.push('}');
    }
    assert_eq!(tokenize(&source).len(), depth * 2);
}

#[test]
fn unpaired_closing_delimiters_do_not_panic() {
    for source in [
        ")",
        "]",
        "}",
        "))))",
        "`${}}`",
        "<div></p></div>",
        "\\",
        "\\\\",
    ] {
        let tokens = tokenize(source);
        let mut rebuilt = String::new();
        for token in &tokens {
            rebuilt.push_str(token.text(source));
        }
        assert_eq!(rebuilt, source);
    }
}

#[test]
fn crlf_is_a_single_newline_token() {
    assert_eq!(
        kinds("a\r\nb"),
        vec![
            TokenKind::Identifier,
            TokenKind::Newline,
            TokenKind::Identifier
        ]
    );
}
