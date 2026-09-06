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
fn naming_the_sink_in_a_string_is_not_reaching_for_it() {
    // Every page that documents the rule, and the rule's own message, contain
    // the word.
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nexport const advice = 'never use dangerouslySetInnerHTML on untrusted input';\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn a_sanitizer_named_in_a_string_does_not_excuse_a_sink() {
    // The dangerous direction: this answer *suppresses* a finding, so reading
    // a helper's name out of a message would hide a real sink.
    // The mention has to be on the line the sink is on, or on the one after
    // it, because those are the two lines the suppression reads.
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(html: string) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} title='call renderMarkdown(md) first' />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].line, 4);
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
fn the_eval_family_named_inside_strings_is_accepted() {
    // A message about the rule, a task that shells out, documentation prose:
    // all three contain the words and none of them executes anything.
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\nconst why = 'eval(x) executes arbitrary code';\nconst also = 'new Function(body) compiles it';\nconst timer = `setTimeout(\"tick()\", 10) is the string form`;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn safe_timers_and_ordinary_identifiers_are_accepted() {
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\nsetTimeout(() => tick(), 10);\nconst evaluate = 1;\nconst f = function () {};\n",
    );

    assert!(diagnostics.is_empty());
}
