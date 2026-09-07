//! The `uniflowed/*` house rules: tabs, trailing whitespace, and package-manager
//! invocations that belong in `uf.config.js`.

use super::*;

#[test]
fn npm_script_invocations_are_rejected() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nspawn('npm run build');\nspawn('pnpm install');\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 8));
}

/// ubugeeei-prod/uf#478: seventeen of this workspace's errors were a package
/// manager's name appearing in text that is not a command.
#[test]
fn a_managers_name_that_does_not_head_a_command_is_not_an_invocation() {
    for source in [
        // A filename, which is what started it.
        "// @flow\nexport const targets: string[] = [\"pnpm-workspace.yaml\", \"pnpm-lock.yaml\"];\n",
        // A test's own title.
        "// @flow\ntest(\"Node and pnpm are pinned once in package.json\", () => {});\n",
        // A pattern that reads `packageManager` out of a manifest.
        "// @flow\nexport const pinned = /^pnpm@(.+)$/;\n",
        // A sentence.
        "// @flow\nexport const note = \"we do not use yarn here\";\n",
    ] {
        let diagnostics = lint_js("uniflowed/no-npm-script-invocation", source);
        assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
    }
}

/// And the invocations it is actually about still fire, including the shape
/// where the program is the whole string.
#[test]
fn a_managers_name_that_heads_a_command_is_still_an_invocation() {
    for source in [
        "// @flow\nspawn(\"pnpm install\");\n",
        "// @flow\nspawn(\"pnpm\", [\"install\"]);\n",
        "// @flow\nexport const command = `npx vite build`;\n",
        "// @flow\nexport const command = \"yarn build\";\n",
    ] {
        let diagnostics = lint_js("uniflowed/no-npm-script-invocation", source);
        assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
    }
}

#[test]
fn uf_task_invocations_are_accepted() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nexport const tasks = { build: 'uf build' };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn npm_mentions_in_comments_are_not_invocations() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\n// migrated away from npm run build\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn reports_tabs_and_trailing_whitespace() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-tabs"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-trailing-whitespace"),
        RuleLevel::Error,
    );

    let report =
        lint_source(&source("// @flow\n\tconst x: number = 1;  \n"), &config).expect("lint");

    assert!(report.has_errors());
    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(report.diagnostics[0].rule, "uniflowed/no-tabs");
    assert_eq!(
        report.diagnostics[1].rule,
        "uniflowed/no-trailing-whitespace"
    );
}

#[test]
fn trailing_whitespace_inside_a_template_literal_is_accepted() {
    // Those spaces are part of the string. `uf fmt` reprints from the syntax
    // tree and keeps them, so reporting them here left the formatter unable to
    // make the linter clean — the two disagreed with no way to converge.
    let diagnostics = lint_js(
        "uniflowed/no-trailing-whitespace",
        "// @flow\nconst t = `a   \n  b`;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn trailing_whitespace_on_a_line_of_code_is_still_reported() {
    let diagnostics = lint_js(
        "uniflowed/no-trailing-whitespace",
        "// @flow\nconst a = 1;   \n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_tab_inside_a_string_is_accepted() {
    // Part of the string, kept by the formatter, and so not something the
    // formatter can be asked to remove.
    let diagnostics = lint_js("uniflowed/no-tabs", "// @flow\nconst t = `a\tb`;\n");

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_tab_in_code_and_a_tab_in_a_comment_are_both_reported() {
    // The comment one is deliberate: the formatter keeps a comment's text, so
    // it is the author's tab to remove — which is exactly the case the LSP's
    // "offer the formatter only where it clears the diagnostic" guard is for.
    let code = lint_js("uniflowed/no-tabs", "// @flow\nconst a\t= 1;\n");
    let comment = lint_js("uniflowed/no-tabs", "// @flow\n// a\tcomment\n");

    assert_eq!(code.len(), 1, "{code:?}");
    assert_eq!(comment.len(), 1, "{comment:?}");
}
