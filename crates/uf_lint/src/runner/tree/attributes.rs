//! The `a11y/*` rules that answer from one attribute, or from the tag itself.
//!
//! The other `a11y` modules weigh one part of an element against another — a
//! role against its tag, a handler against a tab stop. These six do not: each
//! reads a single attribute (or the element name) and answers from that alone.
//! They are together because that is the shape they share, not because they
//! share machinery.
//!
//! # Presence is the whole question, so a spread changes nothing
//!
//! [`interaction`](super::interaction) returns early on a `{...spread}`,
//! because every rule there looks for an attribute's **absence** and a spread
//! may be supplying it. These rules look for an attribute's **presence**, and a
//! spread can only hide one that was never written here. It can cause this
//! module to say nothing; it can never make it say something wrong. So there is
//! no spread gate, and `<input {...props} accessKey="s" />` is still reported.
//!
//! What the module does respect is a value the source settles as nothing:
//! `accessKey={null}` renders no attribute at all, so there is nothing to
//! report, exactly as [`super::tags`] treats a nullish `role`.
//!
//! # `a11y/no-distracting-elements` writes two element names down
//!
//! Every *role* question in this crate is asked of the generated ARIA table,
//! and this is not one. `<marquee>` and `<blink>` were removed from HTML, so
//! they are not in the table's element mappings and not among its reserved
//! elements either — the table has no opinion to ask for. Naming them here is a
//! new fact about two obsolete tags, not a second copy of table data.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::Value;
use super::{Tree, attribute};
use crate::{Severity, severity};

/// `a11y/autocomplete-valid`.
const AUTOCOMPLETE_VALID: &str = "a11y/autocomplete-valid";

/// `a11y/lang`.
const LANG: &str = "a11y/lang";

/// `a11y/no-access-key`.
const NO_ACCESS_KEY: &str = "a11y/no-access-key";

/// `a11y/no-autofocus`.
const NO_AUTOFOCUS: &str = "a11y/no-autofocus";

/// `a11y/no-distracting-elements`.
const NO_DISTRACTING_ELEMENTS: &str = "a11y/no-distracting-elements";

/// `a11y/scope`.
const SCOPE: &str = "a11y/scope";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    autocomplete_valid: Option<Severity>,
    lang: Option<Severity>,
    no_access_key: Option<Severity>,
    no_autofocus: Option<Severity>,
    no_distracting_elements: Option<Severity>,
    scope: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            autocomplete_valid: severity(config, AUTOCOMPLETE_VALID),
            lang: severity(config, LANG),
            no_access_key: severity(config, NO_ACCESS_KEY),
            no_autofocus: severity(config, NO_AUTOFOCUS),
            no_distracting_elements: severity(config, NO_DISTRACTING_ELEMENTS),
            scope: severity(config, SCOPE),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.autocomplete_valid,
            self.lang,
            self.no_access_key,
            self.no_autofocus,
            self.no_distracting_elements,
            self.scope,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            AUTOCOMPLETE_VALID => self.autocomplete_valid,
            LANG => self.lang,
            NO_ACCESS_KEY => self.no_access_key,
            NO_AUTOFOCUS => self.no_autofocus,
            NO_DISTRACTING_ELEMENTS => self.no_distracting_elements,
            SCOPE => self.scope,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let levels = tree.attributes;
    if levels.no_distracting_elements.is_some() {
        no_distracting_elements(tree, name, opening);
    }
    if levels.no_access_key.is_some() {
        no_access_key(tree, opening);
    }
    if levels.no_autofocus.is_some() {
        no_autofocus(tree, opening);
    }
    if levels.scope.is_some() {
        scope(tree, name, opening);
    }
    if levels.lang.is_some() {
        lang(tree, name, opening);
    }
    if levels.autocomplete_valid.is_some() {
        autocomplete_valid(tree, name, opening);
    }
}

/// Whether the attribute is written *and* renders.
///
/// `accessKey={null}` and `accessKey={undefined}` put no attribute in the
/// document, so there is nothing on the element to report.
fn rendered<'a>(
    tree: &Tree<'_>,
    opening: &'a jsx::Opening<Loc, Loc>,
    name: &str,
) -> Option<&'a jsx::Attribute<Loc, Loc>> {
    let written = attribute(opening, name)?;
    match tree.scope.value(written) {
        Value::Nullish => None,
        _ => Some(written),
    }
}

// --- a11y/no-distracting-elements -------------------------------------------

/// `<marquee>` and `<blink>`.
///
/// Both animate their content forever and neither can be stopped by the
/// reader. WCAG 2.2.2 requires that anything moving for more than five seconds
/// can be paused, and these two elements offer no way to do it — which is why
/// HTML removed them. They still render in every browser, so the markup keeps
/// working and keeps being a problem.
fn no_distracting_elements(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name != "marquee" && name != "blink" {
        return;
    }
    tree.report(
        &opening.loc,
        NO_DISTRACTING_ELEMENTS,
        uf_infra::cstr!(
            "`<{name}>` animates on its own and gives the reader no way to stop it, which WCAG \
             2.2.2 requires for anything that moves for more than five seconds; HTML removed the \
             element and browsers still render it, so this keeps working and keeps being a \
             problem — drop it, or animate with CSS the reader can turn off through \
             `prefers-reduced-motion`"
        )
        .into_string(),
    );
}

// --- a11y/no-access-key -----------------------------------------------------

/// An `accessKey` on anything.
///
/// The shortcut it asks for is the browser's or the screen reader's to give.
/// Every combination worth having is already taken by one of them — and which
/// modifier reaches an access key differs per browser, per platform and per
/// assistive technology — so the attribute either does nothing or takes a key
/// away from somebody who was relying on it.
fn no_access_key(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = rendered(tree, opening, "accessKey") else {
        return;
    };
    tree.report(
        &written.loc,
        NO_ACCESS_KEY,
        "`accessKey` asks for a keyboard shortcut the browser and the screen reader have already \
         given out, and the modifier that reaches it differs per browser, per platform and per \
         assistive technology; it either does nothing or takes a shortcut away from somebody \
         relying on it, so drop it and give the control a visible, documented way in"
            .to_owned(),
    );
}

// --- a11y/no-autofocus ------------------------------------------------------

/// `autoFocus` on anything.
///
/// It moves focus before the reader has been told where they are. A screen
/// reader starts announcing from the focused control rather than from the top,
/// so everything above it — the heading that says what the page is — is simply
/// never read, and somebody using magnification is moved somewhere they did not
/// ask to go.
fn no_autofocus(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = rendered(tree, opening, "autoFocus") else {
        return;
    };
    // `autoFocus={false}` is the prop written and turned off, which React does
    // not render: there is no focus being taken.
    if tree.scope.value(written) == Value::Bool(false) {
        return;
    }
    tree.report(
        &written.loc,
        NO_AUTOFOCUS,
        "`autoFocus` moves focus before the reader has been told where they are: a screen reader \
         begins at this control instead of the top of the page, so the heading that says what the \
         page is never gets read, and somebody using magnification is moved somewhere they did \
         not ask to go; focus it from an event the reader caused instead"
            .to_owned(),
    );
}

// --- a11y/scope -------------------------------------------------------------

/// `scope` anywhere but on a `<th>`.
///
/// `scope` is what says whether a header names its column or its row, and HTML
/// defines it on `<th>` alone. On anything else it is ignored — so a table
/// whose headers are `<td scope="col">` has no headers at all as far as a
/// screen reader is concerned, which is the one thing that makes a data table
/// navigable.
fn scope(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name == "th" {
        return;
    }
    let Some(written) = rendered(tree, opening, "scope") else {
        return;
    };
    tree.report(
        &written.loc,
        SCOPE,
        uf_infra::cstr!(
            "HTML defines `scope` on `<th>` and nowhere else, so on a `<{name}>` it is ignored and \
             the header it was meant to describe is not announced as a header at all; make this a \
             `<th scope=\"col\">` or `<th scope=\"row\">`, which is what lets a screen reader read \
             a data table cell by cell"
        )
        .into_string(),
    );
}

// --- a11y/lang --------------------------------------------------------------

/// A `lang` that is not a language tag.
///
/// The exact boundary with [`super::content`]'s `a11y/html-has-lang`:
/// that rule owns the `<html>` with **no** usable `lang` — absent, empty,
/// whitespace, or a value the source settles as a boolean or as nothing. This
/// one owns the remaining case, the only one left: present, non-empty text,
/// and not a well-formed tag. Disjoint by construction, and pinned by a test in
/// both directions.
///
/// A tag the module cannot read ([`Value::Unknown`]) is never wrong.
fn lang(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name != "html" {
        return;
    }
    let Some(written) = attribute(opening, "lang") else {
        return;
    };
    let Value::Text(text) = tree.scope.value(written) else {
        return;
    };
    // `a11y/html-has-lang`'s case, not this one's.
    if text.trim().is_empty() || well_formed_language_tag(text) {
        return;
    }
    tree.report(
        &written.loc,
        LANG,
        uf_infra::cstr!(
            "`lang=\"{text}\"` is not a language tag, so a screen reader cannot tell what to \
             pronounce this page as and carries on in whatever voice it was already using; write \
             a BCP 47 tag such as `en`, `en-GB` or `ja`"
        )
        .into_string(),
    );
}

/// Whether `tag` is shaped like a BCP 47 language tag.
///
/// Deliberately a shape check rather than a registry lookup. The registry has
/// thousands of entries and changes, and this rule is for the mistake people
/// actually make — `en_US` with an underscore, a trailing `-`, a sentence in
/// the attribute — not for adjudicating whether a real subtag is registered. A
/// tag that is merely unusual is left alone, which is the direction this crate
/// errs in everywhere.
fn well_formed_language_tag(tag: &str) -> bool {
    let mut subtags = tag.split('-');
    let Some(primary) = subtags.next() else {
        return false;
    };
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    subtags.all(|subtag| {
        (1..=8).contains(&subtag.len()) && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

// --- a11y/autocomplete-valid ------------------------------------------------

/// An `autocomplete` token the browser does not know.
///
/// A token it recognises is what lets it fill a field from what the reader has
/// already told it once — which is WCAG 1.3.5, and which matters most to
/// somebody for whom typing an address again is the hard part. A token it does
/// not recognise fills nothing, and nothing in the page says so.
///
/// **Both spellings are read.** React's prop is `autoComplete`, and it is the
/// one a React codebase writes; `autocomplete` is the HTML attribute, which
/// React passes through with a warning. Reading only the lowercase name left
/// every correctly spelled React field unchecked. When both are written,
/// `autoComplete` is the one React sends, so it is the one judged.
///
/// **Only the certainly-wrong shape is reported.** The grammar allows a
/// `section-*` name, a `shipping`/`billing` group, a recipient type and a
/// trailing `webauthn` around the field token, and uf does not police the
/// order of those or whether the token suits this particular control. An
/// unknown *word* is the mistake this rule is for.
fn autocomplete_valid(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if !matches!(name, "input" | "select" | "textarea") {
        return;
    }
    let Some(written) =
        attribute(opening, "autoComplete").or_else(|| attribute(opening, "autocomplete"))
    else {
        return;
    };
    let Value::Text(text) = tree.scope.value(written) else {
        return;
    };
    let lowered = text.to_ascii_lowercase();
    let mut tokens = lowered.split_ascii_whitespace();
    let Some(first) = tokens.next() else {
        return;
    };
    // `on` and `off` are the whole value when they are used at all.
    if (first == "on" || first == "off") && tokens.next().is_none() {
        return;
    }
    let unknown = lowered
        .split_ascii_whitespace()
        .find(|token| !is_autofill_token(token));
    let Some(unknown) = unknown else {
        return;
    };
    tree.report(
        &written.loc,
        AUTOCOMPLETE_VALID,
        uf_infra::cstr!(
            "`{unknown}` is not an autofill token, so the browser cannot tell what this field is \
             for and fills nothing — which WCAG 1.3.5 asks for, and which matters most to somebody \
             for whom typing an address again is the hard part; use a token from the HTML autofill \
             list, such as `name`, `email`, `street-address` or `cc-number`, or `off` on its own"
        )
        .into_string(),
    );
}

/// Whether `token` is one the autofill grammar defines.
///
/// `section-*` is a name somebody chooses, so only its eight-character prefix
/// is checked; everything else is a fixed word from the HTML standard's
/// autofill field list.
fn is_autofill_token(token: &str) -> bool {
    if let Some(name) = token.strip_prefix("section-") {
        return !name.is_empty();
    }
    AUTOFILL_TOKENS.contains(token)
}

/// The autofill tokens, from the HTML standard's autofill field list.
///
/// `on` and `off` are not here: they are the entire value when used, never one
/// token among several, and [`autocomplete_valid`] answers them before this is
/// asked.
static AUTOFILL_TOKENS: phf::Set<&'static str> = phf::phf_set! {
    // Grouping identifiers and the web authorization token.
    "shipping", "billing", "webauthn",
    // Recipient types, which precede a digital contact token.
    "home", "work", "mobile", "fax", "pager",
    // Digital contact tokens.
    "tel", "tel-country-code", "tel-national", "tel-area-code", "tel-local",
    "tel-local-prefix", "tel-local-suffix", "tel-extension", "email", "impp",
    // Names.
    "name", "honorific-prefix", "given-name", "additional-name", "family-name",
    "honorific-suffix", "nickname", "username",
    // Credentials.
    "new-password", "current-password", "one-time-code",
    // Organisation and address.
    "organization-title", "organization", "street-address", "address-line1",
    "address-line2", "address-line3", "address-level1", "address-level2",
    "address-level3", "address-level4", "country", "country-name", "postal-code",
    // Payment instruments.
    "cc-name", "cc-given-name", "cc-additional-name", "cc-family-name", "cc-number",
    "cc-exp", "cc-exp-month", "cc-exp-year", "cc-csc", "cc-type",
    "transaction-currency", "transaction-amount",
    // The rest of the field list.
    "language", "bday", "bday-day", "bday-month", "bday-year", "sex", "url", "photo",
};
