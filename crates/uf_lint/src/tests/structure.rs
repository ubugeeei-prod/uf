//! The scope-sensitive rules: nested `component` and `hook` declarations, which
//! is where a lexer-free scan has to be most careful.

use super::*;

#[test]
fn nested_component_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Outer() {\n  component Inner() { return null; }\n  return <Inner />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 3));
}

#[test]
fn sibling_component_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Inner() { return null; }\ncomponent Outer() { return <Inner />; }\n",
    );

    assert!(diagnostics.is_empty());
}

/// ubugeeei-prod/uf#476: one JSX comment made the rest of the file unlintable.
///
/// The line scanner stopped a line's code at the first `/*`, so `{/* … */}` was
/// a `{` with no `}`; the brace stack never came back down and every
/// `component` after it read as nested inside something. The comment is blanked
/// before any line is read now — see `scan::mask_inline_comments`.
#[test]
fn a_jsx_comment_does_not_nest_the_components_after_it() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent A() {\n  return (\n    <>\n      {/* a comment inside JSX */}\n      <div />\n    </>\n  );\n}\n\ncomponent B() {\n  return <div />;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// And the same for a comment beside ordinary code, which used to swallow
/// everything after it on the line.
#[test]
fn an_inline_comment_does_not_swallow_the_brace_after_it() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\nfunction wrap() { /* why */ }\ncomponent A() { return null; }\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn nested_hook_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\ncomponent Outer() {\n  hook useInner(): number { return 1; }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn top_level_hook_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\nhook useInner(): number { return 1; }\ncomponent Outer() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}
