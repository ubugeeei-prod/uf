//! Where the lines fall: indentation, blank lines, trailing whitespace, the
//! placement of comments, and the groups the printer explodes to fit the
//! configured line width.

use super::*;

#[test]
fn removes_trailing_whitespace_and_adds_a_final_newline() {
    similar_asserts::assert_eq!(
        format("const x = 1;  \nconst y = 2;"),
        "const x = 1;\nconst y = 2;\n"
    );
}

#[test]
fn reindents_using_the_configured_indent_width() {
    let config = config_with(|config| {
        config.indent_width = 4;
    });
    similar_asserts::assert_eq!(
        format_with(
            "function f() {\n\tif (x) {\n\t\treturn 1;\n\t}\n}\n",
            &config
        ),
        "function f() {\n    if (x) {\n        return 1;\n    }\n}\n"
    );
}

#[test]
fn indents_by_bracket_depth_regardless_of_the_input_indentation() {
    similar_asserts::assert_eq!(
        format("const a = [\n1,\n        2,\n];\n"),
        "const a = [\n  1,\n  2,\n];\n"
    );
}

#[test]
fn empty_input_stays_empty() {
    assert_eq!(format(""), "");
    assert_eq!(format("   \n\n"), "");
}

#[test]
fn collapses_runs_of_blank_lines() {
    similar_asserts::assert_eq!(format("a;\n\n\n\n\nb;\n"), "a;\n\nb;\n");
}

#[test]
fn max_blank_lines_is_configurable() {
    let config = config_with(|config| {
        config.max_blank_lines = 0;
    });
    similar_asserts::assert_eq!(format_with("a;\n\n\nb;\n", &config), "a;\nb;\n");
}

#[test]
fn blank_lines_inside_a_delimiter_pair_are_dropped() {
    similar_asserts::assert_eq!(
        format("function f() {\n\n  return 1;\n\n}\n"),
        "function f() {\n  return 1;\n}\n"
    );
}

#[test]
fn leading_blank_lines_are_dropped() {
    similar_asserts::assert_eq!(format("\n\n\nconst x = 1;\n"), "const x = 1;\n");
}

#[test]
fn trailing_blank_lines_collapse_to_one_newline() {
    similar_asserts::assert_eq!(format("const x = 1;\n\n\n\n"), "const x = 1;\n");
}

#[test]
fn comments_are_preserved_verbatim_and_in_place() {
    let source = "/**\n * Doc comment.\n *   Indented continuation.\n */\nfunction f() {\n  // leading\n  return 1; // trailing\n  /* block */\n}\n";
    similar_asserts::assert_eq!(format(source), source);
}

#[test]
fn a_block_comment_between_tokens_keeps_one_space_on_each_side() {
    similar_asserts::assert_eq!(
        format("const x = /* why */ 1;\n"),
        "const x = /* why */ 1;\n"
    );
}

#[test]
fn long_argument_lists_are_exploded_to_fit_the_line_width() {
    let config = config_with(|config| {
        config.line_width = 30;
    });
    similar_asserts::assert_eq!(
        format_with("call(alpha, beta, gamma, delta, epsilon);\n", &config),
        "call(\n  alpha,\n  beta,\n  gamma,\n  delta,\n  epsilon\n);\n"
    );
}

#[test]
fn short_groups_are_left_on_one_line() {
    let config = config_with(|config| {
        config.line_width = 30;
    });
    similar_asserts::assert_eq!(format_with("call(a, b);\n", &config), "call(a, b);\n");
}

#[test]
fn exploding_a_group_never_adds_a_trailing_comma() {
    let config = config_with(|config| {
        config.line_width = 12;
    });
    let output = format_with("const xs = [alpha, beta];\n", &config);
    assert!(!output.contains(",\n]"), "{output}");
    assert_token_preserving("const xs = [alpha, beta];\n", &output);
}

#[test]
fn author_line_breaks_are_preserved() {
    let source = "const xs = [\n  1,\n  2,\n];\n";
    similar_asserts::assert_eq!(format(source), source);
}
