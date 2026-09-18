//! The `a11y/*` rules that ask whether a pointer is the only way in.
//!
//! Each rule is tested first with the examples eslint-plugin-jsx-a11y
//! documents for it, adapted to a Flow component, and then with the shapes uf
//! answers differently — which are marked, with the reason, where they appear.

use super::a11y_content::{accepts, reports};

// --- a11y/tabindex-no-positive ----------------------------------------------

#[test]
fn tabindex_no_positive_reports_the_documented_failure() {
    reports(
        "a11y/tabindex-no-positive",
        &[
            r#"<span tabIndex={1}>foo</span>"#,
            r#"<span tabIndex={5}>foo</span>"#,
        ],
    );
}

#[test]
fn tabindex_no_positive_accepts_the_documented_pass() {
    accepts(
        "a11y/tabindex-no-positive",
        &[
            r#"<span tabIndex={0}>foo</span>"#,
            r#"<span tabIndex={-1}>foo</span>"#,
            r#"<span>foo</span>"#,
            // A spread may be carrying a `tabIndex` in.
            r#"<span tabIndex={order} {...rest}>foo</span>"#,
        ],
    );
}

// --- a11y/no-aria-hidden-on-focusable ---------------------------------------

#[test]
fn no_aria_hidden_on_focusable_reports_the_documented_failure() {
    reports(
        "a11y/no-aria-hidden-on-focusable",
        &[
            r#"<div aria-hidden="true" tabIndex={0}>foo</div>"#,
            // Focusable because HTML focuses it, without a `tabIndex`.
            r#"<button aria-hidden="true">Close</button>"#,
        ],
    );
}

#[test]
fn no_aria_hidden_on_focusable_accepts_the_documented_pass() {
    accepts(
        "a11y/no-aria-hidden-on-focusable",
        &[
            r#"<div aria-hidden="true">foo</div>"#,
            // Script-only focus is not a tab stop, so nobody lands on it.
            r#"<div aria-hidden="true" tabIndex={-1}>foo</div>"#,
            r#"<button aria-hidden="false">Close</button>"#,
            r#"<button>Close</button>"#,
        ],
    );
}

// --- a11y/aria-activedescendant-has-tabindex --------------------------------

#[test]
fn activedescendant_has_tabindex_reports_the_documented_failure() {
    reports(
        "a11y/aria-activedescendant-has-tabindex",
        &[r#"<div aria-activedescendant="option-1">foo</div>"#],
    );
}

#[test]
fn activedescendant_has_tabindex_accepts_the_documented_pass() {
    accepts(
        "a11y/aria-activedescendant-has-tabindex",
        &[
            r#"<div aria-activedescendant="option-1" tabIndex={0}>foo</div>"#,
            // An `<input>` takes focus without being told to.
            r#"<input aria-activedescendant="option-1" />"#,
            r#"<div>foo</div>"#,
        ],
    );
}

// --- a11y/mouse-events-have-key-events --------------------------------------

#[test]
fn mouse_events_have_key_events_reports_the_documented_failure() {
    // One at a time: an element missing both `onFocus` and `onBlur` is two
    // findings, and each of these asks about one of them.
    reports(
        "a11y/mouse-events-have-key-events",
        &[
            r#"<div onMouseOver={show}>Menu</div>"#,
            r#"<div onMouseOut={hide}>Menu</div>"#,
        ],
    );
}

#[test]
fn mouse_events_have_key_events_accepts_the_documented_pass() {
    accepts(
        "a11y/mouse-events-have-key-events",
        &[
            r#"<div onMouseOver={show} onFocus={show}>Menu</div>"#,
            r#"<div onMouseOut={hide} onBlur={hide}>Menu</div>"#,
            r#"<div onClick={open}>Menu</div>"#,
            r#"<div>Menu</div>"#,
        ],
    );
}

// --- a11y/interactive-supports-focus ----------------------------------------

#[test]
fn interactive_supports_focus_reports_the_documented_failure() {
    reports(
        "a11y/interactive-supports-focus",
        &[r#"<div role="button" onClick={open}>Open</div>"#],
    );
}

#[test]
fn interactive_supports_focus_accepts_the_documented_pass() {
    accepts(
        "a11y/interactive-supports-focus",
        &[
            r#"<div role="button" tabIndex={0} onClick={open}>Open</div>"#,
            // Not a widget role: there is no control here to reach.
            r#"<div role="article" onClick={open}>Open</div>"#,
            // No handler, so nothing a keyboard is missing out on.
            r#"<div role="button">Open</div>"#,
            // The element is already the control, and HTML focuses it.
            r#"<button onClick={open}>Open</button>"#,
            // Roving tabindex: one widget item is `0`, the rest are `-1`, and
            // arrow keys move focus between them.
            r#"<div role="treeitem" tabIndex={selected ? 0 : -1} onClick={open} onKeyDown={open}>Node</div>"#,
            r#"<div role="treeitem" tabIndex={-1} onClick={open} onKeyDown={open}>Node</div>"#,
        ],
    );
}

// --- a11y/no-noninteractive-tabindex ----------------------------------------

#[test]
fn no_noninteractive_tabindex_reports_the_documented_failure() {
    reports(
        "a11y/no-noninteractive-tabindex",
        &[
            r#"<div tabIndex={0}>foo</div>"#,
            r#"<article role="article" tabIndex={0}>foo</article>"#,
        ],
    );
}

#[test]
fn no_noninteractive_tabindex_accepts_the_documented_pass() {
    accepts(
        "a11y/no-noninteractive-tabindex",
        &[
            r#"<button tabIndex={0}>Open</button>"#,
            // A widget role is a control somebody is building.
            r#"<div role="button" tabIndex={0}>Open</div>"#,
            // Script-only focus: how a dialog takes focus when it opens.
            r#"<div tabIndex={-1}>foo</div>"#,
            r#"<div>foo</div>"#,
        ],
    );
}

// --- a11y/click-events-have-key-events --------------------------------------

#[test]
fn click_events_have_key_events_reports_the_documented_failure() {
    reports(
        "a11y/click-events-have-key-events",
        &[
            r#"<div role="button" tabIndex={0} onClick={open}>Open</div>"#,
            r#"<div role="presentation" tabIndex={0} onClick={open}>Open</div>"#,
            r#"<div role="button" aria-hidden="true" tabIndex={0} onClick={open}>Open</div>"#,
        ],
    );
}

#[test]
fn click_events_have_key_events_accepts_the_documented_pass() {
    accepts(
        "a11y/click-events-have-key-events",
        &[
            // The handler this rule asks for.
            r#"<div role="button" tabIndex={0} onClick={open} onKeyDown={open}>Open</div>"#,
            // A `<button>` answers Enter and Space without being told, and its
            // `onClick` fires for both.
            r#"<button onClick={open}>Open</button>"#,
            // A spread may be carrying a key handler in.
            r#"<div role="button" onClick={open} {...rest}>Open</div>"#,
            r#"<div role="button" tabIndex={0}>Open</div>"#,
            // Pointer-only dismissal layers are not interactive content.
            r#"<div role="presentation" onClick={open} />"#,
            r#"<div role="none" onClick={open} />"#,
            r#"<div role="button" aria-hidden="true" onClick={open} />"#,
        ],
    );
}

// --- the boundary with a11y/no-static-element-interactions ------------------

/// The two rules that split one defect answer exactly one markup each.
///
/// `a11y/no-static-element-interactions` takes the element with **no** `role`:
/// nobody has said what it is, so the advice is to use a `<button>` or to give
/// it a role and a handler. `a11y/click-events-have-key-events` takes the
/// element that **has** one, where the missing handler is the whole of the
/// advice.
///
/// A change to either that makes both fire on one markup — or neither — fails
/// here rather than in somebody's editor, where a defect reported twice reads
/// as two defects and teaches people to stop reading.
#[test]
fn the_click_handler_rules_divide_the_markup_between_them() {
    // No role: the shipped rule answers, and this module stays out of it.
    let no_role = r#"<div onClick={open}>Open</div>"#;
    reports("a11y/no-static-element-interactions", &[no_role]);
    accepts("a11y/click-events-have-key-events", &[no_role]);

    // A role: this module answers, and the shipped rule stays out of it.
    let with_role = r#"<div role="button" tabIndex={0} onClick={open}>Open</div>"#;
    reports("a11y/click-events-have-key-events", &[with_role]);
    accepts("a11y/no-static-element-interactions", &[with_role]);
}

// --- the pair that must not send anybody in a circle ------------------------

/// `a11y/no-noninteractive-tabindex` and
/// `a11y/no-noninteractive-element-interactions` must agree about one element.
///
/// A named, focusable container that answers the arrow keys is a legitimate
/// pattern — uf's own clips example is one, with real buttons beside it
/// running the same handler. The interactions rule stays silent because a
/// keyboard can reach it, and this rule has to stay silent because there is
/// something to do at the stop.
///
/// If it did not, the two would contradict each other: obeying this one by
/// dropping the `tabIndex` makes the handlers unreachable and brings the other
/// one back, and nothing the author writes satisfies both. Two rules that send
/// somebody in a circle are worse than either being absent, so this is pinned
/// rather than described.
#[test]
fn the_focusable_container_rules_do_not_contradict_each_other() {
    let region = r#"<div role="region" tabIndex={0} aria-label="Clips" onKeyDown={move}>x</div>"#;
    accepts("a11y/no-noninteractive-tabindex", &[region]);
    accepts("a11y/no-noninteractive-element-interactions", &[region]);

    // Each still answers the markup it is for: a tab stop with nothing to do
    // at it, and handlers on something a keyboard cannot reach.
    reports(
        "a11y/no-noninteractive-tabindex",
        &[r#"<div role="region" tabIndex={0} aria-label="Clips">x</div>"#],
    );
    reports(
        "a11y/no-noninteractive-element-interactions",
        &[r#"<div role="region" aria-label="Clips" onKeyDown={move}>x</div>"#],
    );
}

/// The pair stays non-contradictory when the handler is statically nothing.
///
/// `onKeyDown={null}` installs no listener, so there is nothing to do at the
/// stop and this rule reports it — while the interactions rule stays silent,
/// because there are no handlers to be unreachable. The two move together
/// because they ask one shared question; if they ever stopped, obeying one
/// would bring the other back.
#[test]
fn a_nullish_handler_gives_a_tab_stop_nothing_to_do() {
    let nullish = r#"<article tabIndex={0} onKeyDown={null}>x</article>"#;
    reports("a11y/no-noninteractive-tabindex", &[nullish]);
    accepts("a11y/no-noninteractive-element-interactions", &[nullish]);

    // A handler that is really installed still answers for the stop.
    let wired = r#"<article tabIndex={0} onKeyDown={move}>x</article>"#;
    accepts("a11y/no-noninteractive-tabindex", &[wired]);
    accepts("a11y/no-noninteractive-element-interactions", &[wired]);
}

/// An `<a>` with no `href` is not a control, so a tab stop on one is a stop
/// with nothing to do at it.
///
/// Whether the anchor is a link is asked of the generated ARIA table — with an
/// `href` it is a `link`, without one it is `generic` — which is what keeps
/// this rule and the two role rules from disagreeing about one anchor.
#[test]
fn an_anchor_that_is_not_a_link_is_not_a_control() {
    reports(
        "a11y/no-noninteractive-tabindex",
        &[r#"<a tabIndex={0}>Docs</a>"#],
    );
    accepts(
        "a11y/no-noninteractive-tabindex",
        &[r#"<a href="/docs" tabIndex={0}>Docs</a>"#],
    );
}
