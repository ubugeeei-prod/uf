//! The `security/*` rules about markup that hands control across a trust
//! boundary.
//!
//! The other modules under [`super`] ask whether markup can be *read*. These
//! three ask what it lets somebody else *do*, and they are together because
//! that is the shape they share rather than any machinery: each reads one
//! attribute of one element and answers from its value alone, and in each case
//! the page renders exactly as its author intended. Nothing looks wrong, which
//! is the whole reason a linter has to be the one to say it.
//!
//! * `security/no-script-url` — a URL that **is** code rather than a place.
//! * `security/no-target-blank` — a link that carries the page's own URL to
//!   wherever it goes.
//! * `security/iframe-has-sandbox` — an embedded document with the privileges
//!   of the page that embedded it.
//!
//! # Presence and absence are gated differently, on purpose
//!
//! `security/no-script-url` looks for an attribute's **presence**: the
//! dangerous value is written on the element in front of it. A `{...spread}`
//! can therefore only make it silent, never wrong, so there is no spread gate
//! and `<iframe {...rest} src="javascript:alert(1)" />` is still reported. The
//! other two look for an attribute's **absence** — a missing `rel`, a missing
//! `sandbox` — which a spread may be supplying, so each returns rather than
//! guess. This is the same division [`super::attributes`] draws, for the same
//! reason.
//!
//! # Host elements only
//!
//! These rules read the element the browser will see. `<Link href="…" />`
//! renders markup this module does not show, and what a component does with a
//! prop is the component's to decide. That is a real gap — a `javascript:` URL
//! handed to a link component is as live as one written on an `<a>` — and it
//! is left open deliberately rather than guessed at, the way every other rule
//! in this crate treats a component.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::{Value, spread_may_set};
use super::{Tree, attribute, has_spread, is_javascript_url};
use crate::{Severity, severity};

/// `security/iframe-has-sandbox`.
const IFRAME_HAS_SANDBOX: &str = "security/iframe-has-sandbox";

/// `security/no-script-url`.
const NO_SCRIPT_URL: &str = "security/no-script-url";

/// `security/no-target-blank`.
const NO_TARGET_BLANK: &str = "security/no-target-blank";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    iframe_has_sandbox: Option<Severity>,
    no_script_url: Option<Severity>,
    no_target_blank: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            iframe_has_sandbox: severity(config, IFRAME_HAS_SANDBOX),
            no_script_url: severity(config, NO_SCRIPT_URL),
            no_target_blank: severity(config, NO_TARGET_BLANK),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.iframe_has_sandbox,
            self.no_script_url,
            self.no_target_blank,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            IFRAME_HAS_SANDBOX => self.iframe_has_sandbox,
            NO_SCRIPT_URL => self.no_script_url,
            NO_TARGET_BLANK => self.no_target_blank,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let levels = tree.trust;
    if levels.no_script_url.is_some() {
        no_script_url(tree, name, opening);
    }
    if levels.no_target_blank.is_some() {
        no_target_blank(tree, name, opening);
    }
    if levels.iframe_has_sandbox.is_some() {
        iframe_has_sandbox(tree, name, opening);
    }
}

// --- security/no-script-url -------------------------------------------------

/// The props a browser fetches or navigates to.
///
/// Every one of these is read as a URL, so a `javascript:` value in it is a
/// string the browser turns into code. Props that merely *hold* a URL for the
/// page's own use — `cite`, a `data-*` — are not here: nothing navigates to
/// them, so nothing runs.
const URL_PROPS: [&str; 7] = [
    "href",
    "src",
    "action",
    "formAction",
    "data",
    "poster",
    "xlinkHref",
];

/// A `javascript:` URL written into a prop the browser navigates to.
///
/// The URL is not a place, it is a program, and it runs with the full
/// privileges of the page that wrote it. It is also the shape an interpolated
/// value turns into an injection in: the moment any part of the string comes
/// from outside, `javascript:` is the prefix that makes the rest execute.
///
/// # The boundary with `a11y/anchor-is-valid`
///
/// `href` on an `<a>` is **not** reported here, and that is not an oversight.
/// The shipped `a11y/anchor-is-valid` owns exactly that pair — it reports a
/// `javascript:` `href` at `error` already, with a message that says React 19
/// blocks it — so answering it again would put two rules on one element for
/// one mistake. This rule owns every other pair: `<iframe src>`,
/// `<form action>`, `<button formAction>`, `<area href>`, and the rest of
/// [`URL_PROPS`]. Pinned in both directions by a test.
///
/// The seam that leaves: `a11y/anchor-is-valid` is silent on an `<a>` carrying
/// a `role` or a `{...spread}`, so such an anchor is answered by neither rule.
/// That is the a11y rule's own narrowing, and widening this one to cover it
/// would mean keeping a copy of when that rule stays quiet — two rules coupled
/// to each other's silence is worse than the gap.
fn no_script_url(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    for prop in URL_PROPS {
        // `a11y/anchor-is-valid`'s pair, not this rule's.
        if name == "a" && prop == "href" {
            continue;
        }
        let Some(written) = attribute(opening, prop) else {
            continue;
        };
        let Value::Text(text) = tree.scope.value(written) else {
            continue;
        };
        if !is_javascript_url(text) {
            continue;
        }
        tree.report(
            &written.loc,
            NO_SCRIPT_URL,
            uf_infra::into_string(uf_infra::cstr!(
                "`{prop}` on this `<{name}>` is a `javascript:` URL, which is not a place to go \
                 but a program to run, with everything the page can do; browsers and React both \
                 block it in more and more positions, and the moment any part of the string comes \
                 from outside the module it is an injection rather than a quirk — attach an \
                 `onClick` handler and give the element a real destination, or none at all"
            )),
        );
    }
}

// --- security/no-target-blank -----------------------------------------------

/// The elements that open a named browsing context.
const TARGETS: [&str; 3] = ["a", "area", "form"];

/// `target="_blank"` with no `rel` that stops the referrer.
///
/// # What this rule is, now that browsers closed the other half
///
/// The famous half of this defect is gone: setting `target="_blank"` on an
/// `<a>`, `<area>` or `<form>` now gives the same behaviour as
/// `rel="noopener"` on its own, so the opened page gets a null
/// `window.opener` and cannot reach back to navigate the page that opened it.
/// A rule that asked for `noopener` today would be asking for what the browser
/// already does, and `rel="noopener"` is therefore **not** accepted here: it
/// adds nothing to the markup it is written on.
///
/// What is left is the half nothing closed. The `Referer` header still goes
/// out, so the destination is told the full URL the reader came from — and in
/// an application that is a path with an order number, a document id or a
/// search in it. `rel="noreferrer"` is the one value that stops it, and it
/// implies `noopener` as well, so it is the whole answer rather than half of
/// one.
///
/// `error`, with the rest of the `security/*` namespace, which does not warn —
/// `uf_lint::rules::tests::security_rules_are_errors_by_default` pins that.
/// Sending a referrer on purpose is legitimate in plenty of code, an affiliate
/// link and an analytics-bearing outbound link both want it, and the answer to
/// those is a suppression on the line or a level in `uf.config.js`: a decision
/// the file records, rather than a default under which every such link passes
/// unnoticed. The fix when it is not deliberate is one exact token.
fn no_target_blank(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if !TARGETS.contains(&name) {
        return;
    }
    let Some(target) = attribute(opening, "target") else {
        return;
    };
    let Value::Text(text) = tree.scope.value(target) else {
        return;
    };
    // The keyword is matched the way HTML matches it, without regard to case.
    if !text.trim().eq_ignore_ascii_case("_blank") {
        return;
    }
    // An absence check, so a spread that may be carrying the `rel` in settles
    // it: see the module documentation.
    if spread_may_set(opening, "rel") {
        return;
    }
    let stopped = match attribute(opening, "rel") {
        Some(rel) => match tree.scope.value(rel) {
            Value::Text(tokens) => has_token(tokens, "noreferrer"),
            // A `rel` this module does not hold is not a missing one.
            Value::Unknown => true,
            _ => false,
        },
        None => false,
    };
    if stopped {
        return;
    }
    tree.report(
        &target.loc,
        NO_TARGET_BLANK,
        uf_infra::into_string(uf_infra::cstr!(
            "`<{name} target=\"_blank\">` sends the `Referer` header on, so wherever this goes is \
             told the full URL the reader is coming from — which in an application is a path with \
             an order number, a document id or a search in it; add `rel=\"noreferrer\"`, which \
             stops it and covers `noopener` too (`noopener` on its own adds nothing, because \
             `target=\"_blank\"` already gives the opened page a null `window.opener`)"
        )),
    );
}

/// Whether a space-separated attribute holds `wanted`.
///
/// `rel` is a set of keywords, matched without regard to case, so
/// `rel="NOREFERRER noopener"` carries the token and `rel="noreferrers"` does
/// not.
fn has_token(tokens: &str, wanted: &str) -> bool {
    tokens
        .split_ascii_whitespace()
        .any(|token| token.eq_ignore_ascii_case(wanted))
}

// --- security/iframe-has-sandbox --------------------------------------------

/// An `<iframe>` with no `sandbox`.
///
/// Without it the embedded document runs with everything a document gets: its
/// own scripts, forms, popups, and — when it is served from this origin —
/// full access to the embedding page. `sandbox` is the attribute that takes
/// those away and hands back only the ones named, and the empty value,
/// `sandbox=""` or a bare `sandbox`, is the strongest form rather than a
/// missing one: it applies every restriction there is.
///
/// # What this rule deliberately does not check
///
/// `sandbox="allow-scripts allow-same-origin"` is the documented way to defeat
/// the attribute: together the two let the framed document reach up and remove
/// the `sandbox` from its own frame. That escape needs the framed document to
/// be **same-origin with the page**, and whether it is depends on where `src`
/// resolves to — which a module does not say. Reporting the pair regardless
/// would fire on `<iframe sandbox="allow-scripts allow-same-origin">` around a
/// genuinely third-party URL, where the combination is safe and common, so the
/// rule asks the one question it can answer from the markup: is this frame
/// sandboxed at all.
///
/// `error`, and this is where uf departs from `eslint-plugin-react`, which
/// leaves `iframe-missing-sandbox` out of its `recommended` preset. uf's
/// `security/*` namespace is error-by-default by construction — see
/// `uf_lint::rules::tests::security_rules_are_errors_by_default` — and an
/// unsandboxed frame is a privilege question rather than a style one. A frame
/// around first-party content that genuinely needs no bounds is a decision a
/// project records, with a suppression or a level, rather than one every frame
/// in the codebase gets by default.
///
/// **Deliberately silent** on a `{...spread}`, which may be carrying the
/// `sandbox` in — the same narrowing `a11y/iframe-has-title` makes on the same
/// element.
fn iframe_has_sandbox(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name != "iframe" || has_spread(opening) {
        return;
    }
    let loc = match attribute(opening, "sandbox") {
        // A bare `sandbox`, `sandbox=""` and any token list are all a sandbox.
        Some(sandbox) => match tree.scope.value(sandbox) {
            Value::Bool(true) | Value::Text(_) | Value::Unknown => return,
            // `sandbox={null}` and `sandbox={false}` render no attribute, so
            // the frame is not sandboxed and the markup says it meant to be.
            _ => &sandbox.loc,
        },
        None => &opening.loc,
    };
    tree.report(
        loc,
        IFRAME_HAS_SANDBOX,
        String::from(
            "this `<iframe>` has no `sandbox`, so the document inside it runs with everything a \
             document gets — its own scripts, forms and popups, and the whole of this page if it \
             is served from this origin; add `sandbox` to take all of that away, then name back \
             only what the frame needs, such as `sandbox=\"allow-scripts\"`",
        ),
    );
}
