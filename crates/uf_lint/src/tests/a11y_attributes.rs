//! The `a11y/*` rules that answer from one attribute, or from the tag itself.
//!
//! Each rule is tested first with the examples eslint-plugin-jsx-a11y
//! documents for it, adapted to a Flow component, and then with the shapes uf
//! answers differently — which are marked, with the reason, where they appear.

use super::a11y_content::{accepts, reports};

// --- a11y/no-access-key -----------------------------------------------------

#[test]
fn no_access_key_reports_the_documented_failure() {
    reports(
        "a11y/no-access-key",
        &[
            r#"<div accessKey="h">x</div>"#,
            r#"<button accessKey="s" />"#,
        ],
    );
}

#[test]
fn no_access_key_accepts_the_documented_pass() {
    accepts(
        "a11y/no-access-key",
        &[
            "<div>x</div>",
            // Rendered as no attribute at all, so there is no shortcut taken.
            "<div accessKey={null}>x</div>",
        ],
    );
}

/// A spread cannot make this rule wrong, only silent, so the attribute written
/// here is still reported. See the module documentation.
#[test]
fn no_access_key_still_answers_an_element_with_a_spread() {
    reports(
        "a11y/no-access-key",
        &[r#"<div {...rest} accessKey="h">x</div>"#],
    );
}

// --- a11y/no-autofocus ------------------------------------------------------

#[test]
fn no_autofocus_reports_the_documented_failure() {
    reports(
        "a11y/no-autofocus",
        &["<input autoFocus />", "<input autoFocus={true} />"],
    );
}

#[test]
fn no_autofocus_accepts_the_documented_pass() {
    accepts(
        "a11y/no-autofocus",
        &[
            "<input />",
            // The prop written and turned off: React renders no attribute and
            // no focus is taken.
            "<input autoFocus={false} />",
            "<input autoFocus={null} />",
        ],
    );
}

// --- a11y/no-distracting-elements -------------------------------------------

#[test]
fn no_distracting_elements_reports_the_documented_failure() {
    reports(
        "a11y/no-distracting-elements",
        &["<marquee>x</marquee>", "<blink>x</blink>"],
    );
}

#[test]
fn no_distracting_elements_accepts_the_documented_pass() {
    accepts(
        "a11y/no-distracting-elements",
        &["<div>x</div>", "<Marquee>x</Marquee>"],
    );
}

// --- a11y/scope -------------------------------------------------------------

#[test]
fn scope_reports_the_documented_failure() {
    reports(
        "a11y/scope",
        &[r#"<div scope="col">x</div>"#, r#"<td scope="row">x</td>"#],
    );
}

#[test]
fn scope_accepts_the_documented_pass() {
    accepts(
        "a11y/scope",
        &[
            r#"<th scope="col">x</th>"#,
            r#"<th scope="row">x</th>"#,
            "<div>x</div>",
            // Rendered as no attribute at all.
            "<div scope={null}>x</div>",
            // A component decides its own markup.
            r#"<Cell scope="col" />"#,
        ],
    );
}

// --- a11y/lang --------------------------------------------------------------

#[test]
fn lang_reports_the_documented_failure() {
    reports(
        "a11y/lang",
        &[
            // The mistakes people actually make: an underscore where the tag
            // wants a hyphen, and a subtag left empty.
            r#"<html lang="en_US"></html>"#,
            r#"<html lang="en-"></html>"#,
        ],
    );
}

#[test]
fn lang_accepts_the_documented_pass() {
    accepts(
        "a11y/lang",
        &[
            r#"<html lang="en"></html>"#,
            r#"<html lang="en-GB"></html>"#,
            r#"<html lang="zh-Hans-CN"></html>"#,
            // A value the module does not hold is never wrong.
            "<html lang={locale}></html>",
            // Well formed and not a registered subtag. BCP 47's grammar allows
            // a primary subtag of five to eight letters, so telling this apart
            // from a real language needs the registry rather than the shape —
            // thousands of entries that go out of date — and uf narrows towards
            // silence rather than towards reporting markup that may be right.
            // This is where the rule deliberately answers less than the plugin.
            r#"<html lang="english"></html>"#,
            // Only `<html>` is asked about, which is where the page's language
            // is declared.
            r#"<div lang="en_US">x</div>"#,
        ],
    );
}

/// `a11y/lang` and the shipped `a11y/html-has-lang` divide one element between
/// them, and exactly one answers any `<html>`.
///
/// `a11y/html-has-lang` owns the element with no usable `lang` — absent, empty,
/// or a value that renders nothing. This rule owns the only case left over:
/// present, non-empty text, and not a well-formed tag. A change that lets both
/// speak, or neither, fails here rather than in somebody's editor.
#[test]
fn the_lang_rules_divide_the_html_element_between_them() {
    let missing = "<html></html>";
    reports("a11y/html-has-lang", &[missing]);
    accepts("a11y/lang", &[missing]);

    let empty = r#"<html lang=""></html>"#;
    reports("a11y/html-has-lang", &[empty]);
    accepts("a11y/lang", &[empty]);

    let malformed = r#"<html lang="en_US"></html>"#;
    reports("a11y/lang", &[malformed]);
    accepts("a11y/html-has-lang", &[malformed]);

    let tag = r#"<html lang="en-GB"></html>"#;
    accepts("a11y/lang", &[tag]);
    accepts("a11y/html-has-lang", &[tag]);
}

// --- a11y/autocomplete-valid ------------------------------------------------

#[test]
fn autocomplete_valid_reports_the_documented_failure() {
    reports(
        "a11y/autocomplete-valid",
        &[
            r#"<input autocomplete="birthday" />"#,
            r#"<input autocomplete="shipping nope" />"#,
            // React's own spelling, which is the one a React codebase writes.
            // The rule read only the lowercase attribute before, so every one
            // of these passed unread.
            r#"<input autoComplete="birthday" />"#,
            r#"<select autoComplete="shipping nope"></select>"#,
            // `on` and `off` are the whole value or nothing.
            r#"<input autocomplete="off name" />"#,
        ],
    );
}

#[test]
fn autocomplete_valid_accepts_the_documented_pass() {
    accepts(
        "a11y/autocomplete-valid",
        &[
            r#"<input autocomplete="on" />"#,
            r#"<input autocomplete="off" />"#,
            r#"<input autocomplete="name" />"#,
            r#"<input autocomplete="given-name" />"#,
            r#"<input autocomplete="cc-number" />"#,
            // The grammar's optional parts, in order.
            r#"<input autocomplete="section-blue shipping street-address" />"#,
            r#"<input autocomplete="billing postal-code" />"#,
            r#"<input autocomplete="work email" />"#,
            r#"<input autocomplete="username webauthn" />"#,
            // Tokens are matched the way HTML matches keywords.
            r#"<input autocomplete="GIVEN-NAME" />"#,
            r#"<input autoComplete="email" />"#,
            r#"<textarea autoComplete="street-address"></textarea>"#,
            // A value the module does not hold is never wrong.
            "<input autocomplete={hint} />",
            // Not a control this attribute belongs to.
            r#"<div autocomplete="birthday">x</div>"#,
            // `<select>` and `<textarea>` take it too.
            r#"<select autocomplete="country"></select>"#,
            r#"<textarea autocomplete="street-address"></textarea>"#,
        ],
    );
}
