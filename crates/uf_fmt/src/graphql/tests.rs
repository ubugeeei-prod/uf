//! What the GraphQL parser accepts, what it refuses, and what refusing
//! means for the file the template is in.
//!
//! The layout the printer produces is not tested here — it is tested
//! against Prettier itself, in `tests/fixtures/graphql_*.js`, because
//! Prettier's output is the specification and a hand-written expectation
//! would only record what uf already does. What is tested here is the half
//! no fixture can express: **the templates uf declines come out byte for
//! byte as the author wrote them.** A formatter that half-understands an
//! embedded language and writes back what it half-understood is worse than
//! one that does not read it at all.

use uf_config::FmtConfig;

use super::{Declined, parse, token_signature};
use crate::format_source;

/// The whole file, formatted with the default configuration.
fn format(source: &str) -> String {
    format_source(source, &FmtConfig::default())
        .expect("the file formats")
        .output
}

/// A file holding nothing but `const x = <template>;`.
fn wrap(template: &str) -> String {
    format!("const x = {template};\n")
}

/// Assert that the template inside a one-statement file survives untouched.
fn assert_untouched(template: &str) {
    let source = wrap(template);
    assert_eq!(format(&source), source, "template was rewritten");
}

// ---- what parses ----

#[test]
fn the_executable_grammar_parses() {
    let documents = [
        "{ a }",
        "query { a }",
        "query Q { a }",
        "query Q($x: Int) { a }",
        "query Q($x: [Int!]! = [1, 2] @dir) @other { a }",
        "mutation M { a }",
        "subscription S { a }",
        "fragment F on T { a }",
        "fragment F($x: Int) on T { a }",
        "{ alias: field(a: 1, b: $v) @dir { nested } }",
        "{ ...Spread }",
        "{ ...Spread(a: 1) @dir }",
        "{ ... on T { a } }",
        "{ ... @dir { a } }",
        "{ f(a: 1, b: -2.5e10, c: \"s\", d: true, e: false, g: null, h: ENUM, i: [], j: {}) }",
        "{ f(a: [1, [2], { k: 1 }]) }",
        "{ f(a: \"\"\"block\"\"\") }",
        "{ a } { b }",
        "{ a }\n\n{ b }",
    ];
    for document in documents {
        assert!(parse(document).is_ok(), "{document} did not parse");
    }
}

#[test]
fn the_type_system_grammar_parses() {
    let documents = [
        "schema { query: Q }",
        "schema @dir { query: Q, mutation: M, subscription: S }",
        "extend schema @dir",
        "extend schema { query: Q }",
        "scalar S",
        "\"desc\" scalar S @dir",
        "extend scalar S @dir",
        "type T { a: Int }",
        "type T implements A & B @dir { a(x: Int = 1): [Int!]! @d }",
        "type T implements & A { a: Int }",
        "extend type T { a: Int }",
        "extend type T @dir",
        "interface I { a: Int }",
        "extend interface I implements J { a: Int }",
        "type T",
        "enum E",
        "input I",
        "interface I",
        "type T implements A",
        "union U = A | B",
        "union U = | A | B",
        "union U",
        "extend union U = C",
        "enum E { A B }",
        "enum E @dir { \"one\" A B @deprecated }",
        "extend enum E { C }",
        "input I { a: Int = 1 @dir }",
        "extend input I { b: Int }",
        "directive @d on FIELD",
        "directive @d(a: Int = 1) repeatable on FIELD | QUERY",
        "directive @d on | FIELD",
        "\"\"\"desc\"\"\" directive @d @other on FIELD",
        "extend directive @d @other",
    ];
    for document in documents {
        assert!(parse(document).is_ok(), "{document} did not parse");
    }
}

// ---- what does not ----

#[test]
fn a_comment_declines_the_document() {
    // Prettier places a GraphQL comment with its generic comment-attachment
    // pass, which decides between a leading, a trailing and a dangling
    // comment from the layout around it. uf does not reproduce that pass,
    // so it does not guess: the document is declined whole and the template
    // keeps the author's spelling.
    assert_eq!(parse("# hello\n{ a }"), Err(Declined::Comment(0)));
    assert_eq!(parse("{ a } # trailing"), Err(Declined::Comment(6)));
    assert_eq!(parse("{ a # inside\n}"), Err(Declined::Comment(4)));
    // A `#` inside a string is text, not a comment.
    assert!(parse("{ f(a: \"# not a comment\") }").is_ok());
}

#[test]
fn a_string_value_that_cannot_be_written_back_declines_the_document() {
    // Prettier prints a string value with only `"`, `\` and a newline
    // escaped, so a carriage return goes out raw — and the formatter
    // normalises line endings on the way in, so the next run would read a
    // different string. Refusing keeps `format(format(x)) == format(x)`.
    assert!(matches!(
        parse("{ f(a: \"cr\\rx\") }"),
        Err(Declined::Unprintable(_))
    ));
    assert!(matches!(
        parse("{ f(a: \"bell\\u0007\") }"),
        Err(Declined::Unprintable(_))
    ));
    // A tab and a newline are printable: the newline is escaped, the tab is
    // written raw and reads back the same.
    assert!(parse("{ f(a: \"tab\\tx\") }").is_ok());
    assert!(parse("{ f(a: \"nl\\nx\") }").is_ok());
}

#[test]
fn what_is_not_graphql_is_refused() {
    let refused = [
        "",
        "   ",
        ",",
        "{",
        "{ }",
        "{ a",
        "query",
        "query Q",
        "fragment on T { a }",
        "fragment F { a }",
        "{ f() }",
        "{ f(a) }",
        "query Q() { a }",
        "{ a: }",
        "{ f(a: ) }",
        "{ f(a: 01) }",
        "{ f(a: 1.) }",
        "{ f(a: 1e) }",
        "{ f(a: \"unterminated) }",
        "{ f(a: \"line\nbreak\") }",
        "type T {}",
        "enum E { true }",
        "extend type T",
        "extend directive @d",
        "directive @d",
        "directive @d on",
        "schema",
        "..Spread",
        "SELECT * FROM t",
    ];
    for document in refused {
        assert!(
            matches!(parse(document), Err(Declined::Syntax(_))),
            "{document:?} parsed and should not have"
        );
    }
}

#[test]
fn nesting_past_the_ceiling_is_refused() {
    let deep = format!("{}a{}", "{ f ".repeat(200), " }".repeat(200));
    assert_eq!(parse(&deep), Err(Declined::TooDeep));
    // And the file still formats; the template is simply left alone.
    let source = wrap(&format!("graphql`{deep}`"));
    assert_eq!(format(&source), source);
}

// ---- what declining means for the file ----

#[test]
fn a_declined_template_keeps_every_byte() {
    // Each of these is a template uf recognises as GraphQL and then refuses
    // to reprint. Nothing about the text may move — not the author's
    // indentation, not the whitespace, not a comment.
    assert_untouched("graphql`\n    query Q { a } # why\n  `");
    assert_untouched("graphql`\n      # only a heading\n      query Q { a }\n`");
    assert_untouched("graphql`\n  query Q { f(a: \"cr\\rx\") }\n    `");
    assert_untouched("graphql`\n      this is not graphql at all\n`");
    assert_untouched("gql`\n  query {\n    user { ...${name} }\n  }\n`");
    assert_untouched("gql`query Q { a } # hides ${Fragment}`");
}

#[test]
fn a_tag_that_is_not_graphql_is_left_alone() {
    // The match is exact and case sensitive; Prettier's is too.
    assert_untouched("Relay.QL`query Q { a }`");
    assert_untouched("gql.experimental`query Q { a }`");
    assert_untouched("GraphQL`query Q { a }`");
    assert_untouched("Gql`query Q { a }`");
    assert_untouched("graphql.other`query Q { a }`");
    assert_untouched("someGql`query Q { a }`");
    assert_untouched("notGraphql(`query Q { a }`)");
    // A block comment that is not exactly `/* GraphQL */`.
    assert_untouched("/* graphql */ `query Q { a }`");
    assert_untouched("/*GraphQL*/ `query Q { a }`");
}

#[test]
fn a_recognised_tag_is_formatted() {
    // The other half of the rule above: these do move, so the assertions
    // there are about the tag and not about some accident of the input.
    for template in [
        "graphql`query Q { a }`",
        "gql`query Q { a }`",
        "graphql.experimental`query Q { a }`",
        "/* GraphQL */ `query Q { a }`",
        "graphql(`query Q { a }`)",
    ] {
        let source = wrap(template);
        assert_ne!(format(&source), source, "{template} was not formatted");
    }
}

// ---- the signature the tree guarantee compares ----

#[test]
fn the_signature_ignores_what_the_formatter_may_change() {
    // Whitespace, commas, an optional leading `|`, and the spelling of a
    // string's escapes are all things Prettier rewrites.
    let same = [
        ("{ a, b }", "{\n  a\n  b\n}"),
        ("{ f(a: 1, b: 2) }", "{ f(a: 1 b: 2) }"),
        ("union U = A | B", "union U =\n  | A\n  | B"),
        ("directive @d on FIELD", "directive @d on | FIELD"),
        ("type T implements A & B", "type T implements & A & B"),
        ("{ f(a: \"\"\"  x  \"\"\") }", "{ f(a: \"\"\"x\"\"\") }"),
        (r#"{ f(a: "A") }"#, r#"{ f(a: "A") }"#),
        ("{ a } # note  ", "{ a } # note"),
    ];
    for (left, right) in same {
        assert_eq!(
            token_signature(left),
            token_signature(right),
            "{left:?} and {right:?} should share a signature"
        );
        assert!(token_signature(left).is_some());
    }
}

#[test]
fn the_signature_keeps_what_the_formatter_may_not() {
    let different = [
        ("{ a }", "{ b }"),
        ("{ a @dir }", "{ a }"),
        ("{ a b }", "{ b a }"),
        ("{ f(a: 1) }", "{ f(a: 1.0) }"),
        ("{ f(a: \"x\") }", "{ f(a: \"\"\"x\"\"\") }"),
        ("union U = A | B", "union U = A B"),
        ("type T implements A & B", "type T implements A B"),
        ("{ f(a: \"\"\"a\nb\"\"\") }", "{ f(a: \"\"\"a\nb \"\"\") }"),
        ("{ a }", "{ a } { a }"),
    ];
    for (left, right) in different {
        assert_ne!(
            token_signature(left),
            token_signature(right),
            "{left:?} and {right:?} should not share a signature"
        );
    }
    // Text that is not GraphQL has no signature and is compared as text.
    // `*` starts no token, and neither does `.` on its own.
    assert_eq!(token_signature("SELECT * FROM t"), None);
    assert_eq!(token_signature("a.b { c }"), None);
}
