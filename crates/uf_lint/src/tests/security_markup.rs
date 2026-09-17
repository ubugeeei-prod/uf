//! The `security/*` rules that read one attribute of one element: a URL that
//! is really a program, a link that carries the page's URL on, and a frame
//! with no bounds on it.
//!
//! Each rule is tested first with the examples eslint-plugin-react documents
//! for its own version, adapted to a Flow component, and then with the shapes
//! uf answers differently — which are marked, with the reason, where they
//! appear. The boundary with every shipped rule that touches the same markup
//! is pinned at the bottom.

use super::a11y_content::{accepts, reports};

// --- security/no-script-url -------------------------------------------------

#[test]
fn no_script_url_reports_the_documented_failures() {
    reports(
        "security/no-script-url",
        &[
            r#"<iframe src="javascript:alert(1)" title="x" />"#,
            r#"<form action="javascript:alert(1)" />"#,
            r#"<button formAction="javascript:alert(1)" />"#,
            r#"<area href="javascript:alert(1)" alt="x" />"#,
            r#"<object data="javascript:alert(1)" />"#,
            r#"<video poster="javascript:alert(1)" />"#,
        ],
    );
}

#[test]
fn no_script_url_reads_a_url_the_way_the_url_parser_does() {
    // Leading spaces, letter case and a tab inside the scheme all still run,
    // and the answer comes from the same helper `a11y/anchor-is-valid` uses.
    reports(
        "security/no-script-url",
        &[
            r#"<iframe src="  JavaScript:alert(1)" title="x" />"#,
            "<iframe src={\"java\\tscript:alert(1)\"} title=\"x\" />",
            "<iframe src={`javascript:alert(1)`} title=\"x\" />",
        ],
    );
}

/// A spread cannot make this rule wrong, only silent: the dangerous value is
/// written on the element in front of it. See the module documentation.
#[test]
fn no_script_url_still_answers_an_element_with_a_spread() {
    reports(
        "security/no-script-url",
        &[r#"<iframe {...rest} src="javascript:alert(1)" title="x" />"#],
    );
}

#[test]
fn no_script_url_accepts_the_documented_passes() {
    accepts(
        "security/no-script-url",
        &[
            r#"<iframe src="/embed" title="x" />"#,
            r#"<form action="/submit" />"#,
            r#"<a href="https://example.com" />"#,
            // A value the module does not hold is never wrong.
            "<iframe src={url} title=\"x\" />",
            // Not a prop anything navigates to, so nothing runs.
            r#"<blockquote cite="javascript:alert(1)">x</blockquote>"#,
            r#"<div data-src="javascript:alert(1)">x</div>"#,
            // A component renders markup this module does not show, and what it
            // does with a prop is its own decision. A real gap, left open
            // deliberately rather than guessed at.
            r#"<Embed src="javascript:alert(1)" />"#,
        ],
    );
}

// --- security/no-target-blank -----------------------------------------------

#[test]
fn no_target_blank_reports_the_documented_failures() {
    reports(
        "security/no-target-blank",
        &[
            r#"<a href="https://example.com" target="_blank" />"#,
            r#"<area href="https://example.com" target="_blank" alt="x" />"#,
            r#"<form action="https://example.com" target="_blank" />"#,
            // `noopener` is what the browser already does for `target="_blank"`,
            // so it is not an answer to the header this rule is about.
            r#"<a href="https://example.com" target="_blank" rel="noopener" />"#,
            // A token is a whole word, so this one is not `noreferrer`.
            r#"<a href="https://example.com" target="_blank" rel="noreferrers" />"#,
        ],
    );
}

#[test]
fn no_target_blank_accepts_the_documented_passes() {
    accepts(
        "security/no-target-blank",
        &[
            r#"<a href="https://example.com" target="_blank" rel="noreferrer" />"#,
            r#"<a href="https://example.com" target="_blank" rel="noopener noreferrer" />"#,
            // `rel` is a set of keywords, matched without regard to case.
            r#"<a href="https://example.com" target="_blank" rel="NoReferrer" />"#,
            r#"<a href="/about" />"#,
            r#"<a href="/about" target="_self" />"#,
            // Nothing opens a context, so nothing carries the header on.
            r#"<a href="/about" target="_parent" />"#,
            // A value the module does not hold is never wrong.
            "<a href=\"/about\" target={frame} />",
            r#"<a href="https://example.com" target="_blank" rel={rel} />"#,
            // A spread may be carrying the `rel` in: this half is an absence
            // check, so it stops rather than guess.
            r#"<a href="https://example.com" target="_blank" {...rest} />"#,
            // A component decides its own markup.
            r#"<Link href="https://example.com" target="_blank" />"#,
        ],
    );
}

// --- security/iframe-has-sandbox --------------------------------------------

#[test]
fn iframe_has_sandbox_reports_the_documented_failures() {
    reports(
        "security/iframe-has-sandbox",
        &[
            r#"<iframe src="https://example.com" title="x" />"#,
            // Written and turned off renders no attribute, so the frame is not
            // sandboxed and the markup says it meant to be.
            r#"<iframe src="https://example.com" title="x" sandbox={null} />"#,
            r#"<iframe src="https://example.com" title="x" sandbox={false} />"#,
        ],
    );
}

#[test]
fn iframe_has_sandbox_accepts_the_documented_passes() {
    accepts(
        "security/iframe-has-sandbox",
        &[
            // A bare `sandbox` and an empty one are the *strongest* form: every
            // restriction applied, nothing named back.
            r#"<iframe src="https://example.com" title="x" sandbox />"#,
            r#"<iframe src="https://example.com" title="x" sandbox="" />"#,
            r#"<iframe src="https://example.com" title="x" sandbox="allow-scripts" />"#,
            // Deliberately accepted. The pair defeats a sandbox only when the
            // framed document is same-origin with the page, and where `src`
            // resolves to is not something the markup says — around a genuinely
            // third-party URL this is safe and common. See the module docs.
            r#"<iframe src="https://example.com" title="x" sandbox="allow-scripts allow-same-origin" />"#,
            // A value the module does not hold is never a missing one.
            "<iframe src=\"https://example.com\" title=\"x\" sandbox={tokens} />",
            // A spread may be carrying the `sandbox` in.
            r#"<iframe {...rest} src="https://example.com" title="x" />"#,
            // Not a frame, and a component decides its own markup.
            "<div>x</div>",
            r#"<Frame src="https://example.com" />"#,
        ],
    );
}

// --- the boundaries with the shipped rules ----------------------------------

/// `security/no-script-url` and the shipped `a11y/anchor-is-valid` divide the
/// URL props between them, and exactly one answers any given markup.
///
/// `a11y/anchor-is-valid` owns `href` on an `<a>` — it reports a `javascript:`
/// URL there already, at `error`, with a message that says React 19 blocks it.
/// This rule owns every other pair, `<area href>` included, since that rule
/// only ever looks at `<a>`. A change that lets both speak, or neither, fails
/// here rather than in somebody's editor.
#[test]
fn the_script_url_rules_divide_the_url_props_between_them() {
    let anchor = r#"<a href="javascript:void(0)" />"#;
    reports("a11y/anchor-is-valid", &[anchor]);
    accepts("security/no-script-url", &[anchor]);

    let frame = r#"<iframe src="javascript:alert(1)" title="x" />"#;
    reports("security/no-script-url", &[frame]);
    accepts("a11y/anchor-is-valid", &[frame]);

    let area = r#"<area href="javascript:alert(1)" alt="x" />"#;
    reports("security/no-script-url", &[area]);
    accepts("a11y/anchor-is-valid", &[area]);
}

/// `security/iframe-has-sandbox` and the shipped `a11y/iframe-has-title` sit on
/// one element and ask different questions of it, so each answers its own
/// attribute and neither is a second opinion about the other's.
///
/// A frame missing both is two defects and draws one finding from each; a frame
/// with both is silent from both.
#[test]
fn the_two_iframe_rules_ask_different_questions_of_one_element() {
    let untitled = r#"<iframe src="/embed" sandbox />"#;
    reports("a11y/iframe-has-title", &[untitled]);
    accepts("security/iframe-has-sandbox", &[untitled]);

    let unsandboxed = r#"<iframe src="/embed" title="Map of the venue" />"#;
    reports("security/iframe-has-sandbox", &[unsandboxed]);
    accepts("a11y/iframe-has-title", &[unsandboxed]);

    let neither = r#"<iframe src="/embed" />"#;
    reports("a11y/iframe-has-title", &[neither]);
    reports("security/iframe-has-sandbox", &[neither]);

    let both = r#"<iframe src="/embed" title="Map of the venue" sandbox />"#;
    accepts("a11y/iframe-has-title", &[both]);
    accepts("security/iframe-has-sandbox", &[both]);
}

/// `security/no-target-blank` reads an `<a>` that `a11y/anchor-is-valid` has
/// already accepted, and the two never report the same markup: one is about
/// where the link goes, the other about what it takes with it.
#[test]
fn no_target_blank_and_anchor_is_valid_never_answer_together() {
    let outbound = r#"<a href="https://example.com" target="_blank" />"#;
    reports("security/no-target-blank", &[outbound]);
    accepts("a11y/anchor-is-valid", &[outbound]);

    let dead = r##"<a href="#" />"##;
    reports("a11y/anchor-is-valid", &[dead]);
    accepts("security/no-target-blank", &[dead]);
}
