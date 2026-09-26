//! The `a11y/*` rules that ask whether an element has something to say.
//!
//! A link, a heading and a frame are each announced by name, and these rules
//! ask the question a screen reader asks on reaching one: what is it called?
//! `a11y/anchor-has-content` and `a11y/heading-has-content` ask whether there
//! is any answer at all; `a11y/anchor-ambiguous-text` and
//! `a11y/img-redundant-alt` whether the answer is worth hearing;
//! `a11y/iframe-has-title`, `a11y/html-has-lang` and `a11y/media-has-caption`
//! ask for the one attribute or child that carries it; and
//! `a11y/anchor-is-valid` asks whether an `<a>` is a link at all.
//!
//! # Content is looked for, not assumed
//!
//! eslint-plugin-jsx-a11y counts any child element as content, so
//! `<a href="/"><svg>…</svg></a>` passes it. These rules look inside host
//! elements the way the accessible-name computation does: text counts, an
//! `aria-label` or a `title` counts, an `<img>` counts by its `alt`, and an
//! element counts only when something inside it does. An icon with no title and
//! an image with `alt=""` are the two ways a link most often ends up with no
//! name, and both are found.
//!
//! What cannot be seen is never counted as missing. A component renders
//! something this module does not show, an expression is a value the source
//! does not hold, and a `{...spread}` may carry the very attribute a rule is
//! looking for. Each of those is [`Content::Unknown`], and a rule that reaches
//! one says nothing.

use uf_config::UniflowedConfig;
use uf_flow::ast::expression::ExpressionInner;
use uf_flow::ast::jsx;
use uf_flow::{Loc, ast};

use super::value::{Scope, Value, spread_may_set};
use super::{
    Tree, attribute, cleaned_url, has_javascript_scheme, has_spread, heading_level, host_name,
    string_attribute,
};
use crate::{Severity, severity};

/// `a11y/anchor-ambiguous-text`.
const ANCHOR_AMBIGUOUS_TEXT: &str = "a11y/anchor-ambiguous-text";

/// `a11y/anchor-has-content`.
const ANCHOR_HAS_CONTENT: &str = "a11y/anchor-has-content";

/// `a11y/anchor-is-valid`.
const ANCHOR_IS_VALID: &str = "a11y/anchor-is-valid";

/// `a11y/control-has-associated-label`.
const CONTROL_HAS_LABEL: &str = "a11y/control-has-associated-label";

/// `a11y/heading-has-content`.
const HEADING_HAS_CONTENT: &str = "a11y/heading-has-content";

/// `a11y/html-has-lang`.
const HTML_HAS_LANG: &str = "a11y/html-has-lang";

/// `a11y/iframe-has-title`.
const IFRAME_HAS_TITLE: &str = "a11y/iframe-has-title";

/// `a11y/img-redundant-alt`.
const IMG_REDUNDANT_ALT: &str = "a11y/img-redundant-alt";

/// `a11y/media-has-caption`.
const MEDIA_HAS_CAPTION: &str = "a11y/media-has-caption";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    anchor_ambiguous_text: Option<Severity>,
    anchor_has_content: Option<Severity>,
    anchor_is_valid: Option<Severity>,
    control_has_label: Option<Severity>,
    heading_has_content: Option<Severity>,
    html_has_lang: Option<Severity>,
    iframe_has_title: Option<Severity>,
    img_redundant_alt: Option<Severity>,
    media_has_caption: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            anchor_ambiguous_text: severity(config, ANCHOR_AMBIGUOUS_TEXT),
            anchor_has_content: severity(config, ANCHOR_HAS_CONTENT),
            anchor_is_valid: severity(config, ANCHOR_IS_VALID),
            control_has_label: severity(config, CONTROL_HAS_LABEL),
            heading_has_content: severity(config, HEADING_HAS_CONTENT),
            html_has_lang: severity(config, HTML_HAS_LANG),
            iframe_has_title: severity(config, IFRAME_HAS_TITLE),
            img_redundant_alt: severity(config, IMG_REDUNDANT_ALT),
            media_has_caption: severity(config, MEDIA_HAS_CAPTION),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.anchor_ambiguous_text,
            self.anchor_has_content,
            self.anchor_is_valid,
            self.control_has_label,
            self.heading_has_content,
            self.html_has_lang,
            self.iframe_has_title,
            self.img_redundant_alt,
            self.media_has_caption,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            ANCHOR_AMBIGUOUS_TEXT => self.anchor_ambiguous_text,
            ANCHOR_HAS_CONTENT => self.anchor_has_content,
            ANCHOR_IS_VALID => self.anchor_is_valid,
            CONTROL_HAS_LABEL => self.control_has_label,
            HEADING_HAS_CONTENT => self.heading_has_content,
            HTML_HAS_LANG => self.html_has_lang,
            IFRAME_HAS_TITLE => self.iframe_has_title,
            IMG_REDUNDANT_ALT => self.img_redundant_alt,
            MEDIA_HAS_CAPTION => self.media_has_caption,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, element: &jsx::Element<Loc, Loc>) {
    let levels = tree.content;
    let opening = &element.opening_element;
    if levels.control_has_label.is_some() {
        control_has_associated_label(tree, name, element);
    }
    match name {
        "a" => {
            if levels.anchor_is_valid.is_some() {
                anchor_is_valid(tree, opening);
            }
            if levels.anchor_has_content.is_some() {
                anchor_has_content(tree, element);
            }
            if levels.anchor_ambiguous_text.is_some() {
                anchor_ambiguous_text(tree, element);
            }
        }
        "html" if levels.html_has_lang.is_some() => html_has_lang(tree, opening),
        "iframe" if levels.iframe_has_title.is_some() => iframe_has_title(tree, opening),
        "img" if levels.img_redundant_alt.is_some() => img_redundant_alt(tree, opening),
        "audio" | "video" if levels.media_has_caption.is_some() => {
            media_has_caption(tree, name, element);
        }
        _ if levels.heading_has_content.is_some() && heading_level(name).is_some() => {
            heading_has_content(tree, name, element);
        }
        _ => {}
    }
}

// --- what an element gives a name -----------------------------------------

/// What a subtree contributes to the name of the element it is in.
///
/// Ordered so that the answer for several children is the greatest of theirs:
/// one child with something to say names the element whatever its siblings
/// are, and one child nobody can see into makes "nothing" impossible to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Content {
    /// Nothing a screen reader announces: whitespace, an empty string, a
    /// hidden element, an image with `alt=""`, an element with none of these
    /// inside it.
    Nothing,
    /// A component, an expression or a spread, whose output is not in this
    /// module.
    Unknown,
    /// Words.
    Something,
}

/// Whether `element` has nothing to be announced by — the question behind
/// both `a11y/anchor-has-content` and `a11y/heading-has-content`.
///
/// An element that is hidden, or that names itself with `aria-label`,
/// `aria-labelledby` or `title`, or that is filled through
/// `dangerouslySetInnerHTML` or a `children` prop, is not empty.
fn announces_nothing(scope: Scope, element: &jsx::Element<Loc, Loc>) -> bool {
    let opening = &element.opening_element;
    if has_spread(opening) || scope.aria_hidden(opening) != Some(false) {
        return false;
    }
    if own_name(scope, opening) != Content::Nothing
        || attribute(opening, "dangerouslySetInnerHTML").is_some()
        || attribute(opening, "children").is_some()
    {
        return false;
    }
    children_content(scope, &element.children.1) == Content::Nothing
}

/// The name an element gives itself through an attribute.
fn own_name(scope: Scope, opening: &jsx::Opening<Loc, Loc>) -> Content {
    ["aria-label", "aria-labelledby", "title"]
        .into_iter()
        .map(
            |name| match attribute(opening, name).map(|named| scope.value(named)) {
                Some(Value::Text(text)) => text_content(text),
                Some(Value::Number(_)) => Content::Something,
                Some(Value::Unknown) => Content::Unknown,
                Some(Value::Bool(_) | Value::Nullish) | None => Content::Nothing,
            },
        )
        .max()
        .unwrap_or(Content::Nothing)
}

fn children_content(scope: Scope, children: &[jsx::Child<Loc, Loc>]) -> Content {
    children
        .iter()
        .map(|child| child_content(scope, child))
        .max()
        .unwrap_or(Content::Nothing)
}

fn child_content(scope: Scope, child: &jsx::Child<Loc, Loc>) -> Content {
    match child {
        jsx::Child::Text { inner, .. } => text_content(&inner.value),
        jsx::Child::Element { inner, .. } => element_content(scope, inner),
        jsx::Child::Fragment { inner, .. } => children_content(scope, &inner.frag_children.1),
        jsx::Child::ExpressionContainer { inner, .. } => match &inner.expression {
            jsx::expression_container::Expression::Expression(expression) => {
                expression_content(scope, expression)
            }
            jsx::expression_container::Expression::EmptyExpression => Content::Nothing,
        },
        jsx::Child::SpreadChild { .. } => Content::Unknown,
    }
}

fn text_content(text: &str) -> Content {
    if text.chars().all(char::is_whitespace) {
        Content::Nothing
    } else {
        Content::Something
    }
}

/// What a `{ … }` child renders, when the source says.
///
/// `{null}`, `{undefined}` and a boolean render nothing, which is React's
/// rule; a number renders its digits.
fn expression_content(scope: Scope, expression: &ast::expression::Expression<Loc, Loc>) -> Content {
    match &**expression {
        ExpressionInner::JSXElement { inner, .. } => element_content(scope, inner),
        ExpressionInner::JSXFragment { inner, .. } => {
            children_content(scope, &inner.frag_children.1)
        }
        _ => match scope.expression_value(expression) {
            Value::Text(text) => text_content(text),
            Value::Number(_) => Content::Something,
            Value::Bool(_) | Value::Nullish => Content::Nothing,
            Value::Unknown => Content::Unknown,
        },
    }
}

/// What one child element contributes.
fn element_content(scope: Scope, element: &jsx::Element<Loc, Loc>) -> Content {
    let opening = &element.opening_element;
    match scope.aria_hidden(opening) {
        Some(true) => return Content::Nothing,
        None => return Content::Unknown,
        Some(false) => {}
    }
    let Some(name) = host_name(&opening.name) else {
        return Content::Unknown;
    };
    if has_spread(opening)
        || attribute(opening, "dangerouslySetInnerHTML").is_some()
        || attribute(opening, "children").is_some()
    {
        return Content::Unknown;
    }
    let named = own_name(scope, opening);
    if named != Content::Nothing {
        return named;
    }
    match name {
        // An image is named by its `alt`. A missing one is `a11y/alt-text`'s
        // finding, and what a browser falls back to without it varies, so it
        // is not claimed as nothing here.
        "img" | "area" => match attribute(opening, "alt").map(|alt| scope.value(alt)) {
            Some(Value::Text(alt)) => text_content(alt),
            Some(Value::Number(_)) => Content::Something,
            Some(Value::Bool(_) | Value::Nullish) => Content::Nothing,
            Some(Value::Unknown) | None => Content::Unknown,
        },
        "input" if string_attribute(opening, "type") == Some("hidden") => Content::Nothing,
        // A control inside a name contributes its value, which the source
        // does not hold.
        "input" | "select" | "textarea" => Content::Unknown,
        _ => children_content(scope, &element.children.1),
    }
}

/// Whether any child is an element, which decides how an empty finding is
/// worded: "empty" is wrong about `<a><svg /></a>`.
fn holds_elements(children: &[jsx::Child<Loc, Loc>]) -> bool {
    children.iter().any(|child| {
        matches!(
            child,
            jsx::Child::Element { .. } | jsx::Child::Fragment { .. }
        )
    })
}

// --- a11y/anchor-has-content ----------------------------------------------

/// An `<a>` with nothing to announce.
///
/// WAI-ARIA requires a link to have an accessible name, and one without is
/// read as "link" and nothing more, so a list of them cannot be told apart.
fn anchor_has_content(tree: &mut Tree<'_>, element: &jsx::Element<Loc, Loc>) {
    if !announces_nothing(tree.scope, element) {
        return;
    }
    let message = if holds_elements(&element.children.1) {
        "nothing inside this `<a>` can be announced — only hidden elements, icons with no \
         title or images with `alt=\"\"` — so a screen reader reads it as \"link\" and \
         nothing more, and WAI-ARIA requires a link to have a name; add the words, or name \
         the `<a>` with `aria-label`"
    } else {
        "this `<a>` is empty, so a screen reader reads it as \"link\" and nothing more, and \
         WAI-ARIA requires a link to have a name; put the words in it, or name it with \
         `aria-label` when it holds only an icon"
    };
    tree.report(
        &element.opening_element.loc,
        ANCHOR_HAS_CONTENT,
        message.to_owned(),
    );
}

// --- a11y/control-has-associated-label -------------------------------------

/// The controls this rule asks about.
///
/// `<a>` is `a11y/anchor-has-content`, and `<img>`, `<area>` and
/// `<input type="image">` are `a11y/alt-text`. Each of those already owns the
/// question "what is this announced as" for its own element, so leaving them
/// out is what keeps one piece of markup from being reported twice.
///
/// `<input type="hidden">` is not rendered and so is not a control to name.
fn is_named_control(name: &str, opening: &jsx::Opening<Loc, Loc>) -> bool {
    match name {
        "button" | "select" | "textarea" => true,
        "input" => !matches!(string_attribute(opening, "type"), Some("image" | "hidden")),
        _ => false,
    }
}

/// Whether this control is named by something its own kind supplies.
///
/// A submit or a reset button is announced by the user agent's own label —
/// "Submit", "Reset" — when no `value` overrides it, so there is no markup to
/// ask anybody for and the missing `value` is not a defect. `type="button"`
/// has no such default: its name is the `value` and nothing else, so an absent
/// or empty one leaves it announced as "button" and nothing more.
///
/// A `placeholder` is deliberately **not** counted. It is a hint, not a name:
/// screen readers do not announce it by default, it is rendered at reduced
/// contrast, and it disappears the moment anything is typed. A field whose
/// only text is a placeholder is exactly the field this rule is for, and
/// exactly what its advice — a `<label>`, an `aria-label`, or an `id` for one
/// to point at — asks somebody to add.
fn native_name(scope: Scope, name: &str, opening: &jsx::Opening<Loc, Loc>) -> bool {
    if name != "input" {
        return false;
    }
    match string_attribute(opening, "type") {
        Some("submit" | "reset") => true,
        Some("button") => match attribute(opening, "value").map(|value| scope.value(value)) {
            Some(Value::Text(text)) => !text.trim().is_empty(),
            Some(Value::Number(_)) => true,
            // A value this module does not hold may well be a name.
            Some(Value::Unknown) => true,
            Some(Value::Bool(_) | Value::Nullish) | None => false,
        },
        _ => false,
    }
}

/// A control with nothing to announce it by.
///
/// A `<button />` with no name is read as "button" and nothing else, and a
/// form of them cannot be told apart.
///
/// **Reported only for the shape that is certainly wrong.** The label that
/// names a control is usually a `<label htmlFor>` *beside* it, which this
/// module cannot see, so anything that leaves room for one is left alone: an
/// `id` for a label to point at, a `<label>` wrapped around it, a name of its
/// own, a name its own kind supplies ([`native_name`]), or — for the one
/// element whose content is its label — content. What is left is the control
/// that nothing anywhere could be naming.
fn control_has_associated_label(tree: &mut Tree<'_>, name: &str, element: &jsx::Element<Loc, Loc>) {
    let opening = &element.opening_element;
    if !is_named_control(name, opening)
        || has_spread(opening)
        || tree.scope.aria_hidden(opening) != Some(false)
    {
        return;
    }
    if attribute(opening, "id").is_some()
        || own_name(tree.scope, opening) != Content::Nothing
        || native_name(tree.scope, name, opening)
    {
        return;
    }
    // Content names the control only where the control is named by its
    // content. A `<button>`'s children are its label; a `<textarea>`'s are its
    // *value* — HTML gives a textarea no `value` attribute and takes the
    // initial one from between the tags — and a `<select>`'s are its options.
    // Counting those as a label is how an unnamed field goes unreported.
    if name == "button" && children_content(tree.scope, &element.children.1) != Content::Nothing {
        return;
    }
    if tree.nearest_host(|host| host == "label").is_some() {
        return;
    }

    tree.report(
        &opening.loc,
        CONTROL_HAS_LABEL,
        uf_infra::cstr!(
            "this `<{name}>` has no name and no `id` for a `<label>` to point at, so a screen \
             reader announces only what kind of control it is; give it an `aria-label`, put the \
             words inside it, or give it an `id` and point a `<label htmlFor>` at that"
        )
        .into_string(),
    );
}

// --- a11y/heading-has-content ---------------------------------------------

/// A heading with nothing to announce.
///
/// People using a screen reader move through a page by its headings, and an
/// empty one is a stop where nothing is said.
fn heading_has_content(tree: &mut Tree<'_>, name: &str, element: &jsx::Element<Loc, Loc>) {
    if !announces_nothing(tree.scope, element) {
        return;
    }
    tree.report(
        &element.opening_element.loc,
        HEADING_HAS_CONTENT,
        uf_infra::cstr!(
            "this `<{name}>` has nothing a screen reader can announce, so a reader moving \
             through the page by its headings lands on it and hears nothing; put the heading's \
             words in it, or remove it"
        )
        .into_string(),
    );
}

// --- a11y/anchor-ambiguous-text -------------------------------------------

/// Link text that means nothing out of context.
///
/// The phrases are eslint-plugin-jsx-a11y's — "click here", "here", "link",
/// "a link", "learn more" — plus "read more" and "more", which fail for the
/// same reason. The text compared is the link's accessible name as far as the
/// source spells it: `aria-label` when there is one, and otherwise its text,
/// through host elements, with hidden ones skipped and images read by `alt`.
/// Case, the punctuation `,.?¿!‽¡;:` and runs of whitespace do not count.
///
/// **Deliberately silent** when any part of that name is out of sight: a
/// component, an expression, a spread, or `aria-labelledby`, whose words are
/// somewhere else in the document.
fn anchor_ambiguous_text(tree: &mut Tree<'_>, element: &jsx::Element<Loc, Loc>) {
    let scope = tree.scope;
    let opening = &element.opening_element;
    if has_spread(opening)
        || scope.aria_hidden(opening) != Some(false)
        || attribute(opening, "aria-labelledby").is_some()
    {
        return;
    }
    let mut text = String::new();
    match attribute(opening, "aria-label").map(|label| scope.value(label)) {
        Some(Value::Text(label)) if !label.trim().is_empty() => text.push_str(label),
        Some(Value::Unknown | Value::Number(_)) => return,
        // An empty label is ignored by the name computation, which falls
        // through to the content.
        _ => {
            if !collect_text(scope, &element.children.1, &mut text) {
                return;
            }
        }
    }
    let spoken = normalize(&text);
    let Some(phrase) = AMBIGUOUS_TEXT.iter().find(|phrase| **phrase == spoken) else {
        return;
    };
    tree.report(
        &opening.loc,
        ANCHOR_AMBIGUOUS_TEXT,
        uf_infra::cstr!(
            "link text \"{phrase}\" does not say where the link goes, and a screen reader's list \
             of links reads it with nothing around it (WCAG 2.4.4); name the destination \
             instead, such as `<a>read the pricing guide</a>`"
        )
        .into_string(),
    );
}

/// Phrases that say nothing about where a link goes.
const AMBIGUOUS_TEXT: [&str; 7] = [
    "a link",
    "click here",
    "here",
    "learn more",
    "link",
    "more",
    "read more",
];

/// Append the text `children` render, or return `false` when some of it is
/// out of sight.
fn collect_text(scope: Scope, children: &[jsx::Child<Loc, Loc>], out: &mut String) -> bool {
    for child in children {
        let known = match child {
            jsx::Child::Text { inner, .. } => {
                out.push_str(&inner.value);
                true
            }
            jsx::Child::Element { inner, .. } => element_text(scope, inner, out),
            jsx::Child::Fragment { inner, .. } => collect_text(scope, &inner.frag_children.1, out),
            jsx::Child::ExpressionContainer { inner, .. } => match &inner.expression {
                jsx::expression_container::Expression::Expression(expression) => {
                    expression_text(scope, expression, out)
                }
                jsx::expression_container::Expression::EmptyExpression => true,
            },
            jsx::Child::SpreadChild { .. } => false,
        };
        if !known {
            return false;
        }
    }
    true
}

fn expression_text(
    scope: Scope,
    expression: &ast::expression::Expression<Loc, Loc>,
    out: &mut String,
) -> bool {
    match &**expression {
        ExpressionInner::JSXElement { inner, .. } => element_text(scope, inner, out),
        ExpressionInner::JSXFragment { inner, .. } => {
            collect_text(scope, &inner.frag_children.1, out)
        }
        _ => match scope.expression_value(expression) {
            Value::Text(text) => {
                out.push_str(text);
                true
            }
            Value::Number(number) => {
                out.push_str(&number.to_string());
                true
            }
            Value::Bool(_) | Value::Nullish => true,
            Value::Unknown => false,
        },
    }
}

fn element_text(scope: Scope, element: &jsx::Element<Loc, Loc>, out: &mut String) -> bool {
    let opening = &element.opening_element;
    match scope.aria_hidden(opening) {
        Some(true) => return true,
        None => return false,
        Some(false) => {}
    }
    let Some(name) = host_name(&opening.name) else {
        return false;
    };
    if has_spread(opening) {
        return false;
    }
    match attribute(opening, "aria-label").map(|label| scope.value(label)) {
        Some(Value::Text(label)) if !label.trim().is_empty() => {
            out.push_str(label);
            return true;
        }
        Some(Value::Unknown | Value::Number(_)) => return false,
        _ => {}
    }
    if name == "img" {
        return match attribute(opening, "alt").map(|alt| scope.value(alt)) {
            Some(Value::Text(alt)) => {
                out.push_str(alt);
                true
            }
            Some(Value::Unknown) => false,
            _ => true,
        };
    }
    collect_text(scope, &element.children.1, out)
}

/// Link text as it is compared: lowercase, without the punctuation a reader
/// does not hear, with whitespace collapsed.
///
/// The punctuation is eslint-plugin-jsx-a11y's set plus `…`, which is how
/// "Read more…" is usually typeset.
fn normalize(text: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | '.' | '?' | '¿' | '!' | '‽' | '¡' | ';' | ':' | '…'))
        .collect();
    kept.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

// --- a11y/anchor-is-valid -------------------------------------------------

/// An `<a>` that is not a link.
///
/// The three shapes eslint-plugin-jsx-a11y names: no `href`, an `href` that
/// goes nowhere (`#`, an empty string, a `javascript:` URL), and either of
/// those with an `onClick` — a button written as a link, which gets the
/// message that says so.
///
/// **Deliberately silent** on an element with `{...props}`, which may carry
/// the `href`; on one with an explicit `role`, whose author has made a claim
/// about what the element is that the role rules judge; and on an `href` that
/// is an expression, because a value the module does not hold is not a
/// missing one.
fn anchor_is_valid(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    if has_spread(opening) || attribute(opening, "role").is_some() {
        return;
    }
    let clicked = attribute(opening, "onClick").is_some();
    let Some(href) = attribute(opening, "href") else {
        tree.report(&opening.loc, ANCHOR_IS_VALID, no_href_message(clicked));
        return;
    };
    let dead = match tree.scope.value(href) {
        // React renders no attribute for `null`, `undefined` or a boolean.
        Value::Nullish | Value::Bool(_) => {
            tree.report(&opening.loc, ANCHOR_IS_VALID, no_href_message(clicked));
            return;
        }
        Value::Text(text) => match dead_href(text) {
            Some(dead) => dead,
            None => return,
        },
        Value::Number(_) | Value::Unknown => return,
    };
    tree.report(&href.loc, ANCHOR_IS_VALID, dead_href_message(dead, clicked));
}

/// An `href` that is present and goes nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeadHref {
    /// `href=""`, which reloads the page.
    Empty,
    /// `href="#"`, which scrolls to the top of it.
    Fragment,
    /// `href="javascript:…"`, which React 19 blocks.
    Script,
}

fn dead_href(href: &str) -> Option<DeadHref> {
    // Read the way the URL parser reads it; `super::cleaned_url` says how, and
    // `security/no-script-url` asks the scheme question through the same pair
    // so that the two rules cannot come to disagree about what runs.
    let cleaned = cleaned_url(href);
    if cleaned.is_empty() {
        Some(DeadHref::Empty)
    } else if cleaned == "#" {
        Some(DeadHref::Fragment)
    } else if has_javascript_scheme(&cleaned) {
        Some(DeadHref::Script)
    } else {
        None
    }
}

fn no_href_message(clicked: bool) -> String {
    if clicked {
        String::from(
            "this `<a>` has an `onClick` and no `href`, so it is a button only a mouse can \
             press: a keyboard cannot reach it and a screen reader does not announce it as \
             anything to activate; make it a `<button type=\"button\">`, or give it the `href` \
             it navigates to",
        )
    } else {
        String::from(
            "this `<a>` has no `href`, so it is not a link: a keyboard cannot reach it and a \
             screen reader does not announce it as one; give it the destination it stands for, \
             or use a `<button>` if it performs an action",
        )
    }
}

fn dead_href_message(dead: DeadHref, clicked: bool) -> String {
    let what = match dead {
        DeadHref::Empty => "`href=\"\"` reloads the page it is on",
        DeadHref::Fragment => "`href=\"#\"` jumps to the top of the page",
        DeadHref::Script => {
            "a `javascript:` URL goes nowhere, and React 19 blocks it before it can run"
        }
    };
    if clicked {
        uf_infra::cstr!(
            "{what}, so with an `onClick` this `<a>` is a button that a screen reader calls a \
             link, and Space, the key that presses a button, scrolls the page instead; make it a \
             `<button type=\"button\">`, or give it a real destination"
        )
        .into_string()
    } else {
        uf_infra::cstr!(
            "{what} rather than going anywhere; give the `<a>` the URL it stands for, or use a \
             `<button>` if it performs an action"
        )
        .into_string()
    }
}

// --- a11y/html-has-lang ---------------------------------------------------

/// A document that does not say what language it is in.
fn html_has_lang(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    if has_spread(opening) {
        return;
    }
    let loc = match attribute(opening, "lang") {
        None => &opening.loc,
        Some(lang) => match tree.scope.value(lang) {
            Value::Text(text) if !text.trim().is_empty() => return,
            Value::Unknown => return,
            _ => &lang.loc,
        },
    };
    tree.report(
        loc,
        HTML_HAS_LANG,
        String::from(
            "`<html>` names no language, so a screen reader reads the page with the voice and \
             pronunciation of its user's own (WCAG 3.1.1); set `lang` to the language the page \
             is written in, such as `lang=\"en\"`",
        ),
    );
}

// --- a11y/iframe-has-title ------------------------------------------------

/// A frame with no name.
///
/// **Deliberately silent** on `{...props}`, which eslint-plugin-jsx-a11y
/// reports: a wrapper that spreads its props onto an `<iframe>` is where a
/// `title` is passed through, and this rule cannot see one arrive. Also silent
/// on a frame hidden with `aria-hidden`, which nothing announces.
fn iframe_has_title(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    if has_spread(opening) || tree.scope.aria_hidden(opening) != Some(false) {
        return;
    }
    let Some(title) = attribute(opening, "title") else {
        tree.report(
            &opening.loc,
            IFRAME_HAS_TITLE,
            String::from(
                "this `<iframe>` has no `title`, so a screen reader announces a frame with no \
                 name and a reader has to go inside to learn what it holds (WCAG 4.1.2); give it \
                 a `title` that says, such as `title=\"Map of the venue\"`",
            ),
        );
        return;
    };
    match tree.scope.value(title) {
        Value::Text(text) if !text.trim().is_empty() => {}
        Value::Unknown => {}
        _ => tree.report(
            &title.loc,
            IFRAME_HAS_TITLE,
            String::from(
                "this `<iframe>`'s `title` has no words in it, so the frame is announced with no \
                 name (WCAG 4.1.2); make it say what the frame holds, such as \
                 `title=\"Map of the venue\"`",
            ),
        ),
    }
}

// --- a11y/img-redundant-alt -----------------------------------------------

/// `alt` text that calls the image an image.
///
/// A screen reader announces an `<img>` as an image before reading its `alt`,
/// so "Photo of Ada" is heard as "image, Photo of Ada".
///
/// Narrower than eslint-plugin-jsx-a11y, which reports the word anywhere:
/// here it is reported where it names the medium — as the first word, or
/// followed by "of" — so `alt="Photo of Ada"` is reported and
/// `alt="Ada taking a photo"`, where the photo is the subject, is not. Only
/// words the source spells out count: in ``alt={`Baz taking a ${photo}`}`` the
/// `photo` is a variable.
///
/// **Deliberately silent** on an image hidden with `aria-hidden`, which is not
/// announced, and when a `{...spread}` after `alt` may replace it.
fn img_redundant_alt(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    if tree.scope.aria_hidden(opening) != Some(false) {
        return;
    }
    let Some(alt) = attribute(opening, "alt") else {
        return;
    };
    if spread_may_set(opening, "alt") {
        return;
    }
    let word = match &alt.value {
        Some(jsx::attribute::Value::StringLiteral((_, literal))) => {
            redundant_word(&literal.value, true)
        }
        Some(jsx::attribute::Value::ExpressionContainer((_, container))) => {
            match &container.expression {
                jsx::expression_container::Expression::Expression(expression) => {
                    match &**expression {
                        ExpressionInner::StringLiteral { inner, .. } => {
                            redundant_word(&inner.value, true)
                        }
                        ExpressionInner::TemplateLiteral { inner, .. } => {
                            inner.quasis.iter().enumerate().find_map(|(index, quasi)| {
                                redundant_word(&quasi.value.cooked, index == 0)
                            })
                        }
                        _ => None,
                    }
                }
                jsx::expression_container::Expression::EmptyExpression => None,
            }
        }
        None => None,
    };
    let Some(word) = word else {
        return;
    };
    tree.report(
        &alt.loc,
        IMG_REDUNDANT_ALT,
        uf_infra::cstr!(
            "this `alt` says \"{word}\", but a screen reader already announces an `<img>` as an \
             image, so a reader hears it twice; describe what the image shows, such as \
             `alt=\"Ada at the summit\"` rather than `alt=\"Photo of Ada at the summit\"`"
        )
        .into_string(),
    );
}

/// Words that name the medium rather than what it shows.
const MEDIUM_WORDS: [&str; 8] = [
    "image",
    "images",
    "photo",
    "photograph",
    "photographs",
    "photos",
    "picture",
    "pictures",
];

/// The medium word `alt` uses to describe itself, if it does.
///
/// `at_start` is whether `alt` is the beginning of the text, which is where
/// "Photo: the team" names the medium; after an interpolation it is not.
fn redundant_word(alt: &str, at_start: bool) -> Option<&'static str> {
    let lowered = alt.to_lowercase();
    let words: Vec<&str> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let medium = |word: &str| MEDIUM_WORDS.iter().copied().find(|medium| *medium == word);
    if at_start
        && let Some(first) = words
            .iter()
            .copied()
            .find(|word| !matches!(*word, "a" | "an" | "the"))
        && let Some(word) = medium(first)
    {
        return Some(word);
    }
    words.windows(2).find_map(|pair| {
        if pair[1] == "of" {
            medium(pair[0])
        } else {
            None
        }
    })
}

// --- a11y/media-has-caption -----------------------------------------------

/// Audio or video with no captions track.
///
/// **Deliberately silent** on `{...props}`, which eslint-plugin-jsx-a11y
/// reports, because `muted` or the children may arrive in it; on `muted` media,
/// which has no sound to caption; and whenever a child is out of sight — a
/// component or an expression — because a `<track>` may be in it.
fn media_has_caption(tree: &mut Tree<'_>, name: &str, element: &jsx::Element<Loc, Loc>) {
    let scope = tree.scope;
    let opening = &element.opening_element;
    if has_spread(opening) {
        return;
    }
    // Only a literal `false`, or nothing, leaves the media audible.
    if let Some(muted) = attribute(opening, "muted")
        && !matches!(scope.value(muted), Value::Bool(false) | Value::Nullish)
    {
        return;
    }
    if captions(scope, &element.children.1) != Captions::Missing {
        return;
    }
    tree.report(
        &opening.loc,
        MEDIA_HAS_CAPTION,
        uf_infra::cstr!(
            "this `<{name}>` has no `<track kind=\"captions\">`, so whoever cannot hear it misses \
             what is said (WCAG 1.2.2); add a captions track, or `muted` if it has no sound to \
             caption"
        )
        .into_string(),
    );
}

/// Whether a captions track is among some children, ordered like [`Content`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Captions {
    Missing,
    Unknown,
    Found,
}

fn captions(scope: Scope, children: &[jsx::Child<Loc, Loc>]) -> Captions {
    children
        .iter()
        .map(|child| match child {
            jsx::Child::Text { .. } => Captions::Missing,
            jsx::Child::Element { inner, .. } => track_captions(scope, inner),
            jsx::Child::Fragment { inner, .. } => captions(scope, &inner.frag_children.1),
            jsx::Child::ExpressionContainer { inner, .. } => match &inner.expression {
                jsx::expression_container::Expression::EmptyExpression => Captions::Missing,
                jsx::expression_container::Expression::Expression(expression) => {
                    match &**expression {
                        ExpressionInner::JSXElement { inner, .. } => track_captions(scope, inner),
                        ExpressionInner::JSXFragment { inner, .. } => {
                            captions(scope, &inner.frag_children.1)
                        }
                        _ => match scope.expression_value(expression) {
                            Value::Unknown => Captions::Unknown,
                            _ => Captions::Missing,
                        },
                    }
                }
            },
            jsx::Child::SpreadChild { .. } => Captions::Unknown,
        })
        .max()
        .unwrap_or(Captions::Missing)
}

/// Whether one child is a captions track.
///
/// A spread that may decide `kind` — one written after it, or any at all when
/// `kind` is not written — makes the answer unknown, whatever `kind` says.
fn track_captions(scope: Scope, element: &jsx::Element<Loc, Loc>) -> Captions {
    let opening = &element.opening_element;
    match host_name(&opening.name) {
        None => Captions::Unknown,
        Some("track") if spread_may_set(opening, "kind") => Captions::Unknown,
        Some("track") => match attribute(opening, "kind").map(|kind| scope.value(kind)) {
            Some(Value::Text(kind)) if kind.trim().eq_ignore_ascii_case("captions") => {
                Captions::Found
            }
            Some(Value::Unknown) => Captions::Unknown,
            _ => Captions::Missing,
        },
        Some(_) => Captions::Missing,
    }
}
