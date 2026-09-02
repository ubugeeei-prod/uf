//! The `security/*` rules: raw HTML without a sanitizer in the module, and the
//! `eval` family including the string form of the timer functions.

use super::*;

#[test]
fn dangerously_set_inner_html_is_rejected_without_a_sanitizer() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\ncomponent Body(html: string) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
    assert!(diagnostics[0].message.contains("XSS"));
}

#[test]
fn dangerously_set_inner_html_is_accepted_via_a_markdown_helper() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(md: string) {\n  return <div dangerouslySetInnerHTML={{ __html: renderMarkdown(md) }} />;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn dangerously_set_inner_html_allows_the_value_on_the_next_line() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(md: string) {\n  return (\n    <div\n      dangerouslySetInnerHTML={{\n        __html: renderMarkdown(md),\n      }}\n    />\n  );\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn a_markdown_import_does_not_whitelist_an_unrelated_value() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(html: string) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn eval_and_friends_are_rejected() {
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\neval(input);\nconst f = new Function('return 1');\nsetTimeout('tick()', 10);\nsetInterval(`tick()`, 10);\n",
    );

    assert_eq!(diagnostics.len(), 4);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 1));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 11));
    assert_eq!(diagnostics[2].line, 4);
    assert_eq!(diagnostics[3].line, 5);
}

#[test]
fn safe_timers_and_ordinary_identifiers_are_accepted() {
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\nsetTimeout(() => tick(), 10);\nconst evaluate = 1;\nconst f = function () {};\n",
    );

    assert!(diagnostics.is_empty());
}
