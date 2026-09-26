//! The rules that ask whether a name or a value is a thing at all.
//!
//! [`super::trust`] asks what markup lets somebody else *do*. These four ask a
//! flatter question: is this name one React binds, and is this value one the
//! attribute takes. Two are name checks — `react/no-unknown-property` on an
//! attribute, `react/no-namespace` on the element — and two are value checks —
//! `react/style-prop-object` on a type, `markup/no-invalid-rel` on a keyword.
//! In every case the answer is decidable from the one name or the one value in
//! front of the rule, which is the shape they share rather than any machinery.
//!
//! # `react/no-unknown-property` reports a rename, never an unknown
//!
//! The plugin's rule owns a table of every DOM property it knows and reports
//! anything absent from it. uf does not keep that table, and the reason is not
//! effort: it is thousands of names that grow with the platform, so the copy
//! would go out of date and start reporting attributes that had become real —
//! a linter wrong about correct markup, which is the expensive direction.
//!
//! What uf reports instead is the case it can name the fix for: an attribute
//! written with its **HTML spelling** when React's prop is spelled differently.
//! `class` for `className`, `for` for `htmlFor`, a lowercase `onclick` for
//! `onClick`. Every one of those is certainly wrong, the correct spelling is
//! known, and the set is small and stable because HTML attribute names do not
//! churn. An attribute uf has never heard of is left alone. The guide row says
//! so, because it is a place uf deliberately answers less than the plugin.
//!
//! # What this module does not own
//!
//! * **`aria-*`** belongs to `a11y/aria-props`, which asks the same kind of
//!   question — is this attribute real — of the generated ARIA table. No
//!   `aria-` name appears in [`RENAMES`], so the two cannot both answer.
//! * **`autocomplete`** belongs to `a11y/autocomplete-valid`, which reads that
//!   attribute under both React's `autoComplete` and the lowercase HTML
//!   spelling. The lowercase one is deliberately absent from
//!   [`RENAMES`]: adding it would put a second rule on an attribute a shipped
//!   rule already answers. Pinned by a test.
//! * **A namespaced attribute** (`xlink:href`) is not an [`jsx::attribute::Name::Identifier`],
//!   so the rename check never sees one, and a namespaced *element* never
//!   reaches [`check`] at all because [`super::host_name`] refuses it. That
//!   makes `react/no-namespace` the only rule that answers `<svg:rect class="x" />`,
//!   by construction rather than by agreement — and there is a test for it.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::Value;
use super::{Tree, attribute};
use crate::{Severity, severity};

/// `markup/no-invalid-rel`.
const NO_INVALID_REL: &str = "markup/no-invalid-rel";

/// `react/no-namespace`.
const NO_NAMESPACE: &str = "react/no-namespace";

/// `react/no-unknown-property`.
const NO_UNKNOWN_PROPERTY: &str = "react/no-unknown-property";

/// `react/style-prop-object`.
const STYLE_PROP_OBJECT: &str = "react/style-prop-object";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    no_invalid_rel: Option<Severity>,
    no_namespace: Option<Severity>,
    no_unknown_property: Option<Severity>,
    style_prop_object: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            no_invalid_rel: severity(config, NO_INVALID_REL),
            no_namespace: severity(config, NO_NAMESPACE),
            no_unknown_property: severity(config, NO_UNKNOWN_PROPERTY),
            style_prop_object: severity(config, STYLE_PROP_OBJECT),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.no_invalid_rel,
            self.no_namespace,
            self.no_unknown_property,
            self.style_prop_object,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            NO_INVALID_REL => self.no_invalid_rel,
            NO_NAMESPACE => self.no_namespace,
            NO_UNKNOWN_PROPERTY => self.no_unknown_property,
            STYLE_PROP_OBJECT => self.style_prop_object,
            _ => None,
        }
    }
}

/// Run the host-element rules in this module that are on.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let levels = tree.shape;
    if levels.no_unknown_property.is_some() {
        no_unknown_property(tree, name, opening);
    }
    if levels.style_prop_object.is_some() {
        style_prop_object(tree, name, opening);
    }
    if levels.no_invalid_rel.is_some() {
        no_invalid_rel(tree, name, opening);
    }
}

/// `react/no-namespace`, which runs for **every** element rather than only the
/// host ones.
///
/// A namespaced name is precisely what [`super::host_name`] returns [`None`]
/// for, so a rule about one cannot be reached from the host-element path.
pub(super) fn check_name(tree: &mut Tree<'_>, name: &jsx::Name<Loc, Loc>) {
    if tree.shape.no_namespace.is_some() {
        no_namespace(tree, name);
    }
}

// --- react/no-namespace -----------------------------------------------------

/// `<svg:rect />`, and any other namespaced element name.
///
/// JSX takes a lowercase name as a host element and a capitalised one as a
/// value in scope, and a namespaced name is neither: React has nowhere to look
/// `svg` up and no way to hand the namespace to the DOM. SVG in JSX does not
/// need one — `<svg>` and its children are already in the SVG namespace because
/// of where they sit — so the prefix is at best noise and at worst markup that
/// never renders.
fn no_namespace(tree: &mut Tree<'_>, name: &jsx::Name<Loc, Loc>) {
    let jsx::Name::NamespacedName(namespaced) = name else {
        return;
    };
    let namespace = &*namespaced.namespace.name;
    let local = &*namespaced.name.name;
    tree.report(
        &namespaced.loc,
        NO_NAMESPACE,
        uf_infra::into_string(uf_infra::cstr!(
            "`<{namespace}:{local}>` is a namespaced element name, which React does not support: \
             JSX reads a lowercase name as an HTML tag and a capitalised one as a value in scope, \
             and `{namespace}:{local}` is neither, so there is nothing for React to render. Inside \
             an `<svg>` the children are already in the SVG namespace — write `<{local}>`"
        )),
    );
}

// --- react/no-unknown-property ----------------------------------------------

/// An attribute written with its HTML spelling instead of React's.
///
/// Reported only for a name in [`RENAMES`], where the correct spelling is known
/// and can be named in the message. See the module documentation for why uf
/// does not keep a table of every DOM property and report the rest.
fn no_unknown_property(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    for written in &*opening.attributes {
        let jsx::OpeningAttribute::Attribute(written) = written else {
            continue;
        };
        // A namespaced attribute is `react/no-namespace`'s subject, not this
        // rule's, and it cannot be a rename of anything.
        let jsx::attribute::Name::Identifier(spelled) = &written.name else {
            continue;
        };
        let spelled = &*spelled.name;
        let Some(react) = RENAMES.get(spelled) else {
            continue;
        };
        tree.report(
            &written.loc,
            NO_UNKNOWN_PROPERTY,
            uf_infra::into_string(uf_infra::cstr!(
                "`{spelled}` is the HTML spelling of this attribute and React spells the prop \
                 `{react}`, so the name written on this `<{name}>` is not the one React binds and \
                 whatever it was meant to do does not happen — React reports it as an unknown DOM \
                 property in development. Write `{react}`"
            )),
        );
    }
}

/// HTML attribute spellings, mapped to the prop React spells instead.
///
/// Deliberately **not** a table of every DOM property: see the module
/// documentation. Two names that belong here by shape are left out on purpose,
/// because a shipped rule already answers them — `aria-*` is
/// `a11y/aria-props`', and `autocomplete` is `a11y/autocomplete-valid`', which
/// reads that attribute under exactly this lowercase spelling.
static RENAMES: phf::Map<&'static str, &'static str> = phf::phf_map! {
    // The two everybody writes by accident, coming from HTML.
    "class" => "className",
    "for" => "htmlFor",
    // The rest of the camelCased attribute names.
    "accesskey" => "accessKey",
    "allowfullscreen" => "allowFullScreen",
    "autocapitalize" => "autoCapitalize",
    "autofocus" => "autoFocus",
    "autoplay" => "autoPlay",
    "cellpadding" => "cellPadding",
    "cellspacing" => "cellSpacing",
    "charset" => "charSet",
    "colspan" => "colSpan",
    "contenteditable" => "contentEditable",
    "crossorigin" => "crossOrigin",
    "datetime" => "dateTime",
    "enctype" => "encType",
    "formaction" => "formAction",
    "formmethod" => "formMethod",
    "formnovalidate" => "formNoValidate",
    "formtarget" => "formTarget",
    "frameborder" => "frameBorder",
    "hreflang" => "hrefLang",
    "inputmode" => "inputMode",
    "itemprop" => "itemProp",
    "itemscope" => "itemScope",
    "itemtype" => "itemType",
    "marginheight" => "marginHeight",
    "marginwidth" => "marginWidth",
    "maxlength" => "maxLength",
    "minlength" => "minLength",
    "novalidate" => "noValidate",
    "playsinline" => "playsInline",
    "readonly" => "readOnly",
    "rowspan" => "rowSpan",
    "spellcheck" => "spellCheck",
    "srcdoc" => "srcDoc",
    "srclang" => "srcLang",
    "srcset" => "srcSet",
    "tabindex" => "tabIndex",
    "usemap" => "useMap",
    // `http-equiv` is the one with a hyphen rather than a case change.
    "http-equiv" => "httpEquiv",
    // Event handlers, which HTML spells in one lowercase run.
    "onblur" => "onBlur",
    "onchange" => "onChange",
    "onclick" => "onClick",
    "ondblclick" => "onDoubleClick",
    "onerror" => "onError",
    "onfocus" => "onFocus",
    "oninput" => "onInput",
    "onkeydown" => "onKeyDown",
    "onkeypress" => "onKeyPress",
    "onkeyup" => "onKeyUp",
    "onload" => "onLoad",
    "onmousedown" => "onMouseDown",
    "onmouseenter" => "onMouseEnter",
    "onmouseleave" => "onMouseLeave",
    "onmouseout" => "onMouseOut",
    "onmouseover" => "onMouseOver",
    "onmouseup" => "onMouseUp",
    "onscroll" => "onScroll",
    "onsubmit" => "onSubmit",
};

// --- react/style-prop-object ------------------------------------------------

/// A `style` that is not an object.
///
/// `style="color: red"` is the HTML habit, and React does not parse it: the
/// prop takes a mapping from property names to values, and React throws on a
/// string rather than rendering it. So this is a crash the module can see, not
/// a matter of taste.
///
/// A value the source does not settle is left alone, and so is `style={null}`,
/// which React accepts as no style at all.
fn style_prop_object(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "style") else {
        return;
    };
    let wrote = match tree.scope.value(written) {
        Value::Text(_) => "a string",
        Value::Number(_) => "a number",
        Value::Bool(_) => "a boolean",
        // `style={null}` is no style at all, and a value this module does not
        // hold is not a wrong one.
        Value::Nullish | Value::Unknown => return,
    };
    tree.report(
        &written.loc,
        STYLE_PROP_OBJECT,
        uf_infra::into_string(uf_infra::cstr!(
            "`style` on this `<{name}>` is {wrote}, and React's `style` prop takes an object \
             mapping property names to values — it throws on anything else rather than rendering \
             it, so this is a crash rather than a matter of taste. Write \
             `style={{{{ color: \"red\" }}}}`, with the property names camelCased"
        )),
    );
}

// --- markup/no-invalid-rel --------------------------------------------------

/// A `rel` keyword that is not one this element takes.
///
/// `rel` is an unordered set of space-separated keywords, matched without
/// regard to case, and which keywords are allowed depends on the element: a
/// `<link>` takes `stylesheet`, an `<a>` does not, and a `<form>` takes a
/// smaller set than either.
///
/// **A keyword uf does not know is never reported.** The registries `rel` draws
/// on are open — IANA's link relations, the HTML standard, and the microformats
/// wiki the standard points at — so `rel="apple-touch-icon"` is a real value
/// that no closed list contains. Reporting an unknown token would be reporting
/// correct markup. What is reported is the certainly-wrong pair: a keyword that
/// **is** a link type but belongs to another element, and the three spellings
/// the HTML standard names as non-conforming.
fn no_invalid_rel(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some(allowed) = rel_keywords(name) else {
        return;
    };
    let Some(written) = attribute(opening, "rel") else {
        return;
    };
    let Value::Text(text) = tree.scope.value(written) else {
        return;
    };
    for token in text.split_ascii_whitespace() {
        let lowered = token.to_ascii_lowercase();
        let lowered = lowered.as_str();
        if let Some(correct) = NONCONFORMING.get(lowered) {
            tree.report(
                &written.loc,
                NO_INVALID_REL,
                uf_infra::into_string(uf_infra::cstr!(
                    "`rel=\"{token}\"` is not a link type the HTML standard defines — it names \
                     `{correct}` as the conforming spelling — so a browser reading this finds no \
                     relationship at all and the link is left with none. Write `{correct}`"
                )),
            );
            return;
        }
        if allowed.contains(lowered) {
            continue;
        }
        // A keyword uf does not know is not a wrong one: the registries are
        // open. Only one that is a link type *somewhere else* is certainly out
        // of place here.
        if !is_link_type(lowered) {
            continue;
        }
        tree.report(
            &written.loc,
            NO_INVALID_REL,
            uf_infra::into_string(uf_infra::cstr!(
                "`{token}` is a link type, but not one a `<{name}>` takes, so the browser ignores \
                 it and this element carries the relationship it was meant to declare nowhere. \
                 Drop it, or put it on the element the keyword belongs to"
            )),
        );
        return;
    }
}

/// The keywords `name` takes, or [`None`] when the element has no `rel`.
fn rel_keywords(name: &str) -> Option<&'static phf::Set<&'static str>> {
    match name {
        "a" | "area" => Some(&HYPERLINK_RELS),
        "form" => Some(&FORM_RELS),
        "link" => Some(&LINK_RELS),
        _ => None,
    }
}

/// Whether `token` is a link type on any element at all.
fn is_link_type(token: &str) -> bool {
    HYPERLINK_RELS.contains(token) || FORM_RELS.contains(token) || LINK_RELS.contains(token)
}

/// The keywords `<a>` and `<area>` take.
static HYPERLINK_RELS: phf::Set<&'static str> = phf::phf_set! {
    "alternate", "author", "bookmark", "external", "help", "license", "me", "next", "nofollow",
    "noopener", "noreferrer", "opener", "prev", "privacy-policy", "search", "tag",
    "terms-of-service",
};

/// The keywords `<form>` takes.
static FORM_RELS: phf::Set<&'static str> = phf::phf_set! {
    "external", "help", "license", "next", "nofollow", "noopener", "noreferrer", "opener", "prev",
    "search",
};

/// The keywords `<link>` takes.
static LINK_RELS: phf::Set<&'static str> = phf::phf_set! {
    "alternate", "author", "canonical", "compression-dictionary", "dns-prefetch", "expect", "help",
    "icon", "license", "manifest", "me", "modulepreload", "next", "pingback", "preconnect",
    "prefetch", "preload", "prerender", "prev", "privacy-policy", "search", "stylesheet",
    "terms-of-service",
};

/// Spellings the HTML standard names as non-conforming, and what to write.
///
/// `shortcut` is the one people still write in front of `icon`; the standard
/// says authors must not use it any more, and `icon` alone is the whole value.
static NONCONFORMING: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "copyright" => "license",
    "previous" => "prev",
    "shortcut" => "icon",
};
