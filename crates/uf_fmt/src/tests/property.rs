//! The invariants that must hold for every source in the corpus at once:
//! formatting is idempotent, token-preserving and comment-preserving, under
//! every configuration rather than only the default one.

use super::*;

#[test]
fn formatting_is_idempotent_over_the_corpus() {
    for source in SOURCE_CORPUS {
        let once = format(source);
        let twice = format(&once);
        similar_asserts::assert_eq!(once, twice, "not idempotent for {source:?}");
    }
}

#[test]
fn formatting_preserves_the_token_stream_over_the_corpus() {
    for source in SOURCE_CORPUS {
        assert_token_preserving(source, &format(source));
    }
}

#[test]
fn formatting_preserves_comments_verbatim() {
    for source in SOURCE_CORPUS {
        assert_comments_preserved(source, &format(source));
    }
}

#[test]
fn formatting_is_idempotent_for_every_configuration() {
    let configs = [
        FmtConfig::default(),
        config_with(|config| {
            config.indent_width = 4;
            config.quotes = QuoteStyle::Single;
            config.semicolons = false;
            config.line_width = 40;
            config.max_blank_lines = 0;
        }),
        config_with(|config| {
            config.indent_width = 1;
            config.quotes = QuoteStyle::Single;
            config.semicolons = true;
            config.line_width = 20;
            config.max_blank_lines = 2;
        }),
    ];
    for config in &configs {
        for source in SOURCE_CORPUS {
            let once = format_with(source, config);
            let twice = format_with(&once, config);
            similar_asserts::assert_eq!(once, twice, "not idempotent for {source:?}");
            assert_token_preserving(source, &once);
        }
    }
}
