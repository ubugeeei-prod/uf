//! Children: text trimming, containers, spreads, comments, and whitespace.
//!
//! Whitespace between children is where a JSX transform is most often subtly
//! wrong, so the rules are pinned down here one at a time: what survives a
//! line break, what a blank line contributes, and where the single spaces that
//! hold `<b>a</b> <i>b</i>` apart come from.

use super::{call, expression, lower};
use crate::text;

#[test]
fn a_text_child_becomes_a_string() {
    assert_eq!(
        expression("<p>hello</p>"),
        "_jsx(\"p\", {children: \"hello\"})"
    );
}

#[test]
fn text_around_a_container_is_kept() {
    assert_eq!(
        expression("<p>tone: {tone}</p>"),
        "_jsxs(\"p\", {children: [\"tone: \", tone,]})"
    );
}

#[test]
fn a_container_child_keeps_its_expression() {
    assert_eq!(
        expression("<p>{greeting.value ?? name}</p>"),
        "_jsx(\"p\", {children: greeting.value ?? name})"
    );
}

#[test]
fn a_comment_only_container_is_not_a_child() {
    assert_eq!(expression("<p>{/* nothing here */}</p>"), "_jsx(\"p\", {})");
}

#[test]
fn a_comment_container_beside_a_real_child_is_dropped() {
    assert_eq!(
        expression("<p>{/* note */}{value}</p>"),
        "_jsx(\"p\", {children: value})"
    );
}

#[test]
fn an_empty_container_is_not_a_child() {
    assert_eq!(expression("<p>{}</p>"), "_jsx(\"p\", {})");
}

#[test]
fn a_spread_child_becomes_a_spread_in_the_list() {
    assert_eq!(
        expression("<ul>{...items}</ul>"),
        "_jsxs(\"ul\", {children: [...items,]})"
    );
}

#[test]
fn a_spread_child_forces_the_list_form_on_its_own() {
    let out = expression("<ul>{...items}</ul>");

    assert!(out.starts_with("_jsxs("), "{out}");
}

#[test]
fn a_spread_child_mixes_with_ordinary_children() {
    assert_eq!(
        expression("<ul>{first}{...rest}</ul>"),
        "_jsxs(\"ul\", {children: [first, ...rest,]})"
    );
}

#[test]
fn a_conditional_child_survives_as_written() {
    assert_eq!(
        expression("<div>{ok && <span />}</div>"),
        "_jsx(\"div\", {children: ok && _jsx(\"span\", {})})"
    );
}

#[test]
fn whitespace_only_children_are_dropped() {
    let source = "const a = (\n  <div>\n    <span />\n  </div>\n);\n";

    let out = call(source);

    assert!(
        out.contains("_jsx(\"div\", {children: _jsx(\"span\", {})})"),
        "{out}"
    );
}

#[test]
fn a_blank_line_between_children_contributes_nothing() {
    let source = "const a = (\n  <div>\n    <a />\n\n    <b />\n  </div>\n);\n";

    let out = call(source);

    assert!(
        out.contains("children: [_jsx(\"a\", {}), _jsx(\"b\", {}),]"),
        "{out}"
    );
}

#[test]
fn a_space_between_two_elements_on_one_line_is_kept() {
    assert_eq!(
        expression("<p><b>a</b> <i>c</i></p>"),
        "_jsxs(\"p\", {children: [_jsx(\"b\", {children: \"a\"}), \" \", _jsx(\"i\", {children: \"c\"}),]})"
    );
}

#[test]
fn text_wrapped_over_lines_is_joined_with_single_spaces() {
    let source = "const a = (\n  <p>\n    one\n    two\n  </p>\n);\n";

    let out = call(source);

    assert!(out.contains("children: \"one two\""), "{out}");
}

#[test]
fn a_multi_line_text_child_keeps_the_line_count() {
    let source = "const a = (\n  <p>\n    hello\n  </p>\n);\n";

    let out = lower(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(out.contains("\"hello\""), "{out}");
}

#[test]
fn entities_in_text_are_decoded() {
    assert_eq!(
        expression("<p>a &amp; b</p>"),
        "_jsx(\"p\", {children: \"a & b\"})"
    );
}

#[test]
fn a_numeric_entity_in_text_is_decoded() {
    assert_eq!(
        expression("<p>&#8212;</p>"),
        "_jsx(\"p\", {children: \"\u{2014}\"})"
    );
}

#[test]
fn an_apostrophe_in_text_survives() {
    assert_eq!(
        expression("<p>it's fine</p>"),
        "_jsx(\"p\", {children: \"it's fine\"})"
    );
}

#[test]
fn a_quote_in_text_is_escaped() {
    let out = expression("<p>say \"hi\"</p>");

    assert_eq!(out, "_jsx(\"p\", {children: \"say \\\"hi\\\"\"})");
}

#[test]
fn a_slash_in_text_survives() {
    assert_eq!(
        expression("<p>and / or</p>"),
        "_jsx(\"p\", {children: \"and / or\"})"
    );
}

#[test]
fn text_cleaning_trims_each_line() {
    assert_eq!(text::clean("\n    hello\n  ").as_deref(), Some("hello"));
}

#[test]
fn text_cleaning_joins_lines_with_one_space() {
    assert_eq!(text::clean("\n  one\n  two\n").as_deref(), Some("one two"));
}

#[test]
fn text_cleaning_keeps_leading_space_on_the_first_line() {
    assert_eq!(text::clean(" lead\n").as_deref(), Some(" lead"));
}

#[test]
fn text_cleaning_keeps_trailing_space_on_the_last_line() {
    assert_eq!(text::clean("tail ").as_deref(), Some("tail "));
}

#[test]
fn text_cleaning_drops_a_whitespace_only_run_that_spans_lines() {
    assert_eq!(text::clean("\n   \n  "), None);
    assert_eq!(text::clean("\n\n"), None);
    assert_eq!(text::clean(""), None);
}

#[test]
fn text_cleaning_keeps_a_single_space_run_on_one_line() {
    assert_eq!(text::clean(" ").as_deref(), Some(" "));
}

#[test]
fn text_cleaning_turns_tabs_into_spaces() {
    assert_eq!(text::clean("a\tb").as_deref(), Some("a b"));
}

#[test]
fn text_cleaning_handles_carriage_returns() {
    assert_eq!(
        text::clean("\r\n  one\r\n  two\r\n").as_deref(),
        Some("one two")
    );
}

#[test]
fn entity_decoding_leaves_an_unterminated_entity_alone() {
    assert_eq!(text::decode_entities("a & b").as_str(), "a & b");
    assert_eq!(text::decode_entities("&amp").as_str(), "&amp");
}

#[test]
fn entity_decoding_handles_the_named_set() {
    assert_eq!(
        text::decode_entities("&amp;&lt;&gt;&quot;&apos;&nbsp;").as_str(),
        "&<>\"'\u{a0}"
    );
}

#[test]
fn entity_decoding_ignores_an_over_long_entity_name() {
    let long = format!("&{};", "x".repeat(64));

    assert_eq!(text::decode_entities(&long).as_str(), long);
}

#[test]
fn deeply_nested_children_lower_correctly() {
    let out = expression("<a><b><c><d>x</d></c></b></a>");

    assert_eq!(
        out,
        "_jsx(\"a\", {children: _jsx(\"b\", {children: _jsx(\"c\", {children: _jsx(\"d\", {children: \"x\"})})})})"
    );
}

#[test]
fn a_child_list_of_mixed_kinds_keeps_its_order() {
    assert_eq!(
        expression("<p>one{two}<b>three</b></p>"),
        "_jsxs(\"p\", {children: [\"one\", two, _jsx(\"b\", {children: \"three\"}),]})"
    );
}

#[test]
fn the_scaffolded_page_body_lowers() {
    let source = "const a = (\n  <main {...stylex.props(styles.shell)}>\n    <h1>{greeting.value ?? viewerName}</h1>\n    <p>tone: {selectedTone.get()}</p>\n    <Counter initial={1} />\n  </main>\n);\n";

    assert_eq!(lower(source).lines().count(), source.lines().count());

    let out = call(source);
    assert!(
        out.contains("_jsxs(\"main\", {...stylex.props(styles.shell), children: ["),
        "{out}"
    );
    assert!(
        out.contains("_jsx(\"h1\", {children: greeting.value ?? viewerName})"),
        "{out}"
    );
    assert!(
        out.contains("_jsxs(\"p\", {children: [\"tone: \", selectedTone.get(),]})"),
        "{out}"
    );
    assert!(out.contains("_jsx(Counter, {initial: 1,})"), "{out}");
}
