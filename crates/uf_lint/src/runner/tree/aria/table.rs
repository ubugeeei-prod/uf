//! The ARIA table: roles, the attributes each one takes, and the roles
//! HTML elements carry on their own.
//!
//! **Generated — do not edit by hand.** `tools/aria/gen-table.cjs`
//! emits this file from `aria-query` 5.3.2, which is the encoding of
//! [WAI-ARIA 1.2][aria] that `eslint-plugin-jsx-a11y` itself reads, plus the
//! two ARIA 1.3 index-text attributes `a11y/aria-props` already accepts.
//!
//! [aria]: https://www.w3.org/TR/wai-aria-1.2/

use super::{Attr, Flags, Implicit, Kind, Required, Role, Spec, Tag};

/// Every ARIA attribute, sorted by name. The index of a name in this table
/// is its bit in a role's attribute masks.
pub(super) static ATTRIBUTES: &[Spec] = &[
    Spec {
        name: "aria-activedescendant",
        kind: Kind::Id,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-atomic",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-autocomplete",
        kind: Kind::Token(&["inline", "list", "both", "none"]),
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-braillelabel",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-brailleroledescription",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-busy",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-checked",
        kind: Kind::Tristate,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-colcount",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-colindex",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-colindextext",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-colspan",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-controls",
        kind: Kind::IdList,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-current",
        kind: Kind::Token(&["page", "step", "location", "date", "time"]),
        boolean_spelling: true,
        undefined_token: false,
    },
    Spec {
        name: "aria-describedby",
        kind: Kind::IdList,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-description",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-details",
        kind: Kind::Id,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-disabled",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-dropeffect",
        kind: Kind::TokenList(&["copy", "execute", "link", "move", "none", "popup"]),
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-errormessage",
        kind: Kind::Id,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-expanded",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-flowto",
        kind: Kind::IdList,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-grabbed",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-haspopup",
        kind: Kind::Token(&["menu", "listbox", "tree", "grid", "dialog"]),
        boolean_spelling: true,
        undefined_token: false,
    },
    Spec {
        name: "aria-hidden",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-invalid",
        kind: Kind::Token(&["grammar", "spelling"]),
        boolean_spelling: true,
        undefined_token: false,
    },
    Spec {
        name: "aria-keyshortcuts",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-label",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-labelledby",
        kind: Kind::IdList,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-level",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-live",
        kind: Kind::Token(&["assertive", "off", "polite"]),
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-modal",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-multiline",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-multiselectable",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-orientation",
        kind: Kind::Token(&["vertical", "horizontal"]),
        boolean_spelling: false,
        undefined_token: true,
    },
    Spec {
        name: "aria-owns",
        kind: Kind::IdList,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-placeholder",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-posinset",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-pressed",
        kind: Kind::Tristate,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-readonly",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-relevant",
        kind: Kind::TokenList(&["additions", "all", "removals", "text"]),
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-required",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-roledescription",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-rowcount",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-rowindex",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-rowindextext",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-rowspan",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-selected",
        kind: Kind::Boolean,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-setsize",
        kind: Kind::Integer,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-sort",
        kind: Kind::Token(&["ascending", "descending", "none", "other"]),
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-valuemax",
        kind: Kind::Number,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-valuemin",
        kind: Kind::Number,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-valuenow",
        kind: Kind::Number,
        boolean_spelling: false,
        undefined_token: false,
    },
    Spec {
        name: "aria-valuetext",
        kind: Kind::Text,
        boolean_spelling: false,
        undefined_token: false,
    },
];

/// Every ARIA role, sorted by name.
pub(super) static ROLES: &[Role] = &[
    Role {
        name: "alert",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "alertdialog",
        flags: Flags::NONE,
        supported: 0x2846eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "application",
        flags: Flags::NONE,
        supported: 0x2842fffb823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "article",
        flags: Flags::NONE,
        supported: 0x82942eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "banner",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "blockquote",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "button",
        flags: Flags::WIDGET,
        supported: 0x2a42efbb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "caption",
        flags: Flags::CONTEXTUAL,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "cell",
        flags: Flags::CONTEXTUAL,
        supported: 0x2a842eb2bd22,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "checkbox",
        flags: Flags::WIDGET,
        supported: 0x3c42fbfb862,
        required: 0x40,
        prohibited: 0x0,
    },
    Role {
        name: "code",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "columnheader",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x16bc42fffbd22,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "combobox",
        flags: Flags::WIDGET,
        supported: 0x3c42fffb827,
        required: 0x80800,
        prohibited: 0x0,
    },
    Role {
        name: "command",
        flags: Flags::ABSTRACT.union(Flags::WIDGET),
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "complementary",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "composite",
        flags: Flags::ABSTRACT.union(Flags::WIDGET),
        supported: 0x2842eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "contentinfo",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "definition",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "deletion",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "dialog",
        flags: Flags::NONE,
        supported: 0x2846eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "directory",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-abstract",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-acknowledgments",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-afterword",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-appendix",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-backlink",
        flags: Flags::WIDGET,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-biblioentry",
        flags: Flags::CONTEXTUAL,
        supported: 0x82943fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-bibliography",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-biblioref",
        flags: Flags::WIDGET,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-chapter",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-colophon",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-conclusion",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-cover",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-credit",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-credits",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-dedication",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-endnote",
        flags: Flags::CONTEXTUAL,
        supported: 0x82943fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-endnotes",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-epigraph",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-epilogue",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-errata",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-example",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-footnote",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-foreword",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-glossary",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-glossref",
        flags: Flags::WIDGET,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-index",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-introduction",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-noteref",
        flags: Flags::WIDGET,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-notice",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-pagebreak",
        flags: Flags::NONE,
        supported: 0x1e02862fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-pagefooter",
        flags: Flags::NONE,
        supported: 0x2842ff7f83a,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-pageheader",
        flags: Flags::NONE,
        supported: 0x2842ff7f83a,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-pagelist",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-part",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-preface",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-prologue",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-pullquote",
        flags: Flags::NONE,
        supported: 0x0,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-qna",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-subtitle",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-tip",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "doc-toc",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "document",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "emphasis",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "feed",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "figure",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "form",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "generic",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "graphics-document",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "graphics-object",
        flags: Flags::NONE,
        supported: 0x2842fffb823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "graphics-symbol",
        flags: Flags::NONE,
        supported: 0x2842fffb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "grid",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x6c52eb3b8a3,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "gridcell",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x6bc42fffbd22,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "group",
        flags: Flags::NONE,
        supported: 0x2842eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "heading",
        flags: Flags::NONE,
        supported: 0x2843eb2b822,
        required: 0x10000000,
        prohibited: 0x0,
    },
    Role {
        name: "img",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "input",
        flags: Flags::ABSTRACT.union(Flags::WIDGET),
        supported: 0x2842eb3b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "insertion",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "landmark",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "link",
        flags: Flags::WIDGET,
        supported: 0x2842efbb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "list",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "listbox",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x3c72fbfb823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "listitem",
        flags: Flags::CONTEXTUAL,
        supported: 0x82943eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "log",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "main",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "mark",
        flags: Flags::NONE,
        supported: 0x2842eb2f83a,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "marquee",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "math",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "menu",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x2862eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "menubar",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x2862eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "menuitem",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x82942efbb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "menuitemcheckbox",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x83d42fffb862,
        required: 0x40,
        prohibited: 0x0,
    },
    Role {
        name: "menuitemradio",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x83d42fffb862,
        required: 0x40,
        prohibited: 0x0,
    },
    Role {
        name: "meter",
        flags: Flags::NONE,
        supported: 0x1e02842eb2b822,
        required: 0x8000000000000,
        prohibited: 0x0,
    },
    Role {
        name: "navigation",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "none",
        flags: Flags::NONE,
        supported: 0x0,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "note",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "option",
        flags: Flags::WIDGET,
        supported: 0xc2942eb3b862,
        required: 0x400000000000,
        prohibited: 0x0,
    },
    Role {
        name: "paragraph",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "presentation",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "progressbar",
        flags: Flags::WIDGET,
        supported: 0x1e02842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "radio",
        flags: Flags::WIDGET,
        supported: 0x82942eb3b862,
        required: 0x40,
        prohibited: 0x0,
    },
    Role {
        name: "radiogroup",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x3c62fb7b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "range",
        flags: Flags::ABSTRACT,
        supported: 0xe02842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "region",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "roletype",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "row",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0xca943ebbb923,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "rowgroup",
        flags: Flags::CONTEXTUAL,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "rowheader",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0x16bc42fffbd22,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "scrollbar",
        flags: Flags::WIDGET,
        supported: 0x1e02862eb3b822,
        required: 0x8000000000800,
        prohibited: 0x0,
    },
    Role {
        name: "search",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "searchbox",
        flags: Flags::WIDGET,
        supported: 0x3ccaff7b827,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "section",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "sectionhead",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "select",
        flags: Flags::ABSTRACT.union(Flags::COMPOSITE.union(Flags::WIDGET)),
        supported: 0x2862eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "separator",
        flags: Flags::NONE,
        supported: 0x1e02862eb3b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "slider",
        flags: Flags::WIDGET,
        supported: 0x1e02c62ff7b822,
        required: 0x8000000000000,
        prohibited: 0x0,
    },
    Role {
        name: "spinbutton",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x1e03c42fb7b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "status",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "strong",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "structure",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "subscript",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "superscript",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0xc000000,
    },
    Role {
        name: "switch",
        flags: Flags::WIDGET,
        supported: 0x3c42fbfb862,
        required: 0x40,
        prohibited: 0x0,
    },
    Role {
        name: "tab",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0xc2942efbb822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "table",
        flags: Flags::NONE,
        supported: 0x6842eb2b8a2,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "tablist",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x2873eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "tabpanel",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "term",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "textbox",
        flags: Flags::WIDGET,
        supported: 0x3ccaff7b827,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "time",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "timer",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "toolbar",
        flags: Flags::NONE,
        supported: 0x2862eb3b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "tooltip",
        flags: Flags::NONE,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "tree",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x3872fb7b823,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "treegrid",
        flags: Flags::COMPOSITE.union(Flags::WIDGET),
        supported: 0x7c72fb7b8a3,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "treeitem",
        flags: Flags::WIDGET.union(Flags::CONTEXTUAL),
        supported: 0xc2943efbb862,
        required: 0x400000000000,
        prohibited: 0x0,
    },
    Role {
        name: "widget",
        flags: Flags::ABSTRACT,
        supported: 0x2842eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
    Role {
        name: "window",
        flags: Flags::ABSTRACT,
        supported: 0x2846eb2b822,
        required: 0x0,
        prohibited: 0x0,
    },
];

/// What role an HTML element carries without being told, most specific first.
pub(super) static IMPLICIT_ROLES: &[Implicit] = &[
    Implicit {
        element: "a",
        attributes: &[Required {
            name: "href",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "link",
    },
    Implicit {
        element: "a",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "address",
        attributes: &[],
        placed: false,
        role: "group",
    },
    Implicit {
        element: "area",
        attributes: &[Required {
            name: "href",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "link",
    },
    Implicit {
        element: "area",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "article",
        attributes: &[],
        placed: false,
        role: "article",
    },
    Implicit {
        element: "aside",
        attributes: &[Required {
            name: "aria-label",
            value: None,
            set: true,
            unset: false,
        }],
        placed: true,
        role: "complementary",
    },
    Implicit {
        element: "aside",
        attributes: &[Required {
            name: "aria-labelledby",
            value: None,
            set: true,
            unset: false,
        }],
        placed: true,
        role: "complementary",
    },
    Implicit {
        element: "aside",
        attributes: &[],
        placed: true,
        role: "complementary",
    },
    Implicit {
        element: "aside",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "b",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "bdo",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "blockquote",
        attributes: &[],
        placed: false,
        role: "blockquote",
    },
    Implicit {
        element: "body",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "button",
        attributes: &[],
        placed: false,
        role: "button",
    },
    Implicit {
        element: "caption",
        attributes: &[],
        placed: false,
        role: "caption",
    },
    Implicit {
        element: "code",
        attributes: &[],
        placed: false,
        role: "code",
    },
    Implicit {
        element: "data",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "datalist",
        attributes: &[],
        placed: false,
        role: "listbox",
    },
    Implicit {
        element: "dd",
        attributes: &[],
        placed: false,
        role: "definition",
    },
    Implicit {
        element: "del",
        attributes: &[],
        placed: false,
        role: "deletion",
    },
    Implicit {
        element: "details",
        attributes: &[],
        placed: false,
        role: "group",
    },
    Implicit {
        element: "dfn",
        attributes: &[],
        placed: false,
        role: "term",
    },
    Implicit {
        element: "dialog",
        attributes: &[],
        placed: false,
        role: "dialog",
    },
    Implicit {
        element: "div",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "dt",
        attributes: &[],
        placed: false,
        role: "term",
    },
    Implicit {
        element: "em",
        attributes: &[],
        placed: false,
        role: "emphasis",
    },
    Implicit {
        element: "fieldset",
        attributes: &[],
        placed: false,
        role: "group",
    },
    Implicit {
        element: "figure",
        attributes: &[],
        placed: false,
        role: "figure",
    },
    Implicit {
        element: "footer",
        attributes: &[],
        placed: true,
        role: "contentinfo",
    },
    Implicit {
        element: "footer",
        attributes: &[],
        placed: true,
        role: "generic",
    },
    Implicit {
        element: "form",
        attributes: &[Required {
            name: "aria-label",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "form",
    },
    Implicit {
        element: "form",
        attributes: &[Required {
            name: "aria-labelledby",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "form",
    },
    Implicit {
        element: "form",
        attributes: &[Required {
            name: "name",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "form",
    },
    Implicit {
        element: "h1",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "h2",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "h3",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "h4",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "h5",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "h6",
        attributes: &[],
        placed: false,
        role: "heading",
    },
    Implicit {
        element: "header",
        attributes: &[],
        placed: true,
        role: "banner",
    },
    Implicit {
        element: "header",
        attributes: &[],
        placed: true,
        role: "generic",
    },
    Implicit {
        element: "hgroup",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "hr",
        attributes: &[],
        placed: false,
        role: "separator",
    },
    Implicit {
        element: "html",
        attributes: &[],
        placed: false,
        role: "document",
    },
    Implicit {
        element: "i",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "img",
        attributes: &[Required {
            name: "alt",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "img",
    },
    Implicit {
        element: "img",
        attributes: &[Required {
            name: "alt",
            value: None,
            set: false,
            unset: true,
        }],
        placed: false,
        role: "img",
    },
    Implicit {
        element: "img",
        attributes: &[Required {
            name: "alt",
            value: Some(""),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "presentation",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: true,
                unset: false,
            },
            Required {
                name: "type",
                value: Some("email"),
                set: false,
                unset: false,
            },
        ],
        placed: false,
        role: "combobox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: true,
                unset: false,
            },
            Required {
                name: "type",
                value: Some("search"),
                set: false,
                unset: false,
            },
        ],
        placed: false,
        role: "combobox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: true,
                unset: false,
            },
            Required {
                name: "type",
                value: Some("tel"),
                set: false,
                unset: false,
            },
        ],
        placed: false,
        role: "combobox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: true,
                unset: false,
            },
            Required {
                name: "type",
                value: Some("text"),
                set: false,
                unset: false,
            },
        ],
        placed: false,
        role: "combobox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: true,
                unset: false,
            },
            Required {
                name: "type",
                value: Some("url"),
                set: false,
                unset: false,
            },
        ],
        placed: false,
        role: "combobox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "type",
                value: Some("search"),
                set: false,
                unset: false,
            },
        ],
        placed: true,
        role: "searchbox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "type",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
        ],
        placed: true,
        role: "textbox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "type",
                value: Some("email"),
                set: false,
                unset: false,
            },
        ],
        placed: true,
        role: "textbox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "type",
                value: Some("tel"),
                set: false,
                unset: false,
            },
        ],
        placed: true,
        role: "textbox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "type",
                value: Some("text"),
                set: false,
                unset: false,
            },
        ],
        placed: true,
        role: "textbox",
    },
    Implicit {
        element: "input",
        attributes: &[
            Required {
                name: "list",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "type",
                value: Some("url"),
                set: false,
                unset: false,
            },
        ],
        placed: true,
        role: "textbox",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("button"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "button",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("image"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "button",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("reset"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "button",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("submit"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "button",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("checkbox"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "checkbox",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("radio"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "radio",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("range"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "slider",
    },
    Implicit {
        element: "input",
        attributes: &[Required {
            name: "type",
            value: Some("number"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "spinbutton",
    },
    Implicit {
        element: "ins",
        attributes: &[],
        placed: false,
        role: "insertion",
    },
    Implicit {
        element: "li",
        attributes: &[],
        placed: true,
        role: "listitem",
    },
    Implicit {
        element: "main",
        attributes: &[],
        placed: false,
        role: "main",
    },
    Implicit {
        element: "mark",
        attributes: &[],
        placed: false,
        role: "mark",
    },
    Implicit {
        element: "math",
        attributes: &[],
        placed: false,
        role: "math",
    },
    Implicit {
        element: "menu",
        attributes: &[],
        placed: false,
        role: "list",
    },
    Implicit {
        element: "meter",
        attributes: &[],
        placed: false,
        role: "meter",
    },
    Implicit {
        element: "nav",
        attributes: &[],
        placed: false,
        role: "navigation",
    },
    Implicit {
        element: "ol",
        attributes: &[],
        placed: false,
        role: "list",
    },
    Implicit {
        element: "optgroup",
        attributes: &[],
        placed: false,
        role: "group",
    },
    Implicit {
        element: "option",
        attributes: &[],
        placed: false,
        role: "option",
    },
    Implicit {
        element: "output",
        attributes: &[],
        placed: false,
        role: "status",
    },
    Implicit {
        element: "p",
        attributes: &[],
        placed: false,
        role: "paragraph",
    },
    Implicit {
        element: "pre",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "progress",
        attributes: &[],
        placed: false,
        role: "progressbar",
    },
    Implicit {
        element: "q",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "samp",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "section",
        attributes: &[Required {
            name: "aria-label",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "region",
    },
    Implicit {
        element: "section",
        attributes: &[Required {
            name: "aria-labelledby",
            value: None,
            set: true,
            unset: false,
        }],
        placed: false,
        role: "region",
    },
    Implicit {
        element: "section",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "select",
        attributes: &[
            Required {
                name: "multiple",
                value: None,
                set: false,
                unset: true,
            },
            Required {
                name: "size",
                value: None,
                set: false,
                unset: true,
            },
        ],
        placed: true,
        role: "combobox",
    },
    Implicit {
        element: "select",
        attributes: &[Required {
            name: "size",
            value: None,
            set: false,
            unset: false,
        }],
        placed: true,
        role: "listbox",
    },
    Implicit {
        element: "select",
        attributes: &[Required {
            name: "multiple",
            value: None,
            set: false,
            unset: false,
        }],
        placed: false,
        role: "listbox",
    },
    Implicit {
        element: "small",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "span",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "strong",
        attributes: &[],
        placed: false,
        role: "strong",
    },
    Implicit {
        element: "sub",
        attributes: &[],
        placed: false,
        role: "subscript",
    },
    Implicit {
        element: "sup",
        attributes: &[],
        placed: false,
        role: "superscript",
    },
    Implicit {
        element: "table",
        attributes: &[],
        placed: false,
        role: "table",
    },
    Implicit {
        element: "tbody",
        attributes: &[],
        placed: false,
        role: "rowgroup",
    },
    Implicit {
        element: "td",
        attributes: &[],
        placed: true,
        role: "cell",
    },
    Implicit {
        element: "td",
        attributes: &[],
        placed: true,
        role: "gridcell",
    },
    Implicit {
        element: "textarea",
        attributes: &[],
        placed: false,
        role: "textbox",
    },
    Implicit {
        element: "tfoot",
        attributes: &[],
        placed: false,
        role: "rowgroup",
    },
    Implicit {
        element: "th",
        attributes: &[Required {
            name: "scope",
            value: Some("col"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "columnheader",
    },
    Implicit {
        element: "th",
        attributes: &[Required {
            name: "scope",
            value: Some("colgroup"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "columnheader",
    },
    Implicit {
        element: "th",
        attributes: &[Required {
            name: "scope",
            value: Some("row"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "rowheader",
    },
    Implicit {
        element: "th",
        attributes: &[Required {
            name: "scope",
            value: Some("rowgroup"),
            set: false,
            unset: false,
        }],
        placed: false,
        role: "rowheader",
    },
    Implicit {
        element: "th",
        attributes: &[],
        placed: false,
        role: "columnheader",
    },
    Implicit {
        element: "thead",
        attributes: &[],
        placed: false,
        role: "rowgroup",
    },
    Implicit {
        element: "time",
        attributes: &[],
        placed: false,
        role: "time",
    },
    Implicit {
        element: "tr",
        attributes: &[],
        placed: false,
        role: "row",
    },
    Implicit {
        element: "u",
        attributes: &[],
        placed: false,
        role: "generic",
    },
    Implicit {
        element: "ul",
        attributes: &[],
        placed: false,
        role: "list",
    },
];

/// The HTML elements that already are each role.
pub(super) static ROLE_ELEMENTS: &[(&str, &[Tag])] = &[
    (
        "article",
        &[Tag {
            name: "article",
            attributes: &[],
        }],
    ),
    (
        "banner",
        &[Tag {
            name: "header",
            attributes: &[],
        }],
    ),
    (
        "blockquote",
        &[Tag {
            name: "blockquote",
            attributes: &[],
        }],
    ),
    (
        "button",
        &[
            Tag {
                name: "input",
                attributes: &[Attr {
                    name: "type",
                    value: Some("button"),
                }],
            },
            Tag {
                name: "input",
                attributes: &[Attr {
                    name: "type",
                    value: Some("image"),
                }],
            },
            Tag {
                name: "input",
                attributes: &[Attr {
                    name: "type",
                    value: Some("reset"),
                }],
            },
            Tag {
                name: "input",
                attributes: &[Attr {
                    name: "type",
                    value: Some("submit"),
                }],
            },
            Tag {
                name: "button",
                attributes: &[],
            },
        ],
    ),
    (
        "caption",
        &[Tag {
            name: "caption",
            attributes: &[],
        }],
    ),
    (
        "cell",
        &[Tag {
            name: "td",
            attributes: &[],
        }],
    ),
    (
        "checkbox",
        &[Tag {
            name: "input",
            attributes: &[Attr {
                name: "type",
                value: Some("checkbox"),
            }],
        }],
    ),
    (
        "code",
        &[Tag {
            name: "code",
            attributes: &[],
        }],
    ),
    (
        "columnheader",
        &[
            Tag {
                name: "th",
                attributes: &[],
            },
            Tag {
                name: "th",
                attributes: &[Attr {
                    name: "scope",
                    value: Some("col"),
                }],
            },
            Tag {
                name: "th",
                attributes: &[Attr {
                    name: "scope",
                    value: Some("colgroup"),
                }],
            },
        ],
    ),
    (
        "combobox",
        &[
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("email"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("search"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("tel"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("text"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("url"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("url"),
                    },
                ],
            },
            Tag {
                name: "select",
                attributes: &[
                    Attr {
                        name: "multiple",
                        value: None,
                    },
                    Attr {
                        name: "size",
                        value: None,
                    },
                ],
            },
        ],
    ),
    (
        "complementary",
        &[
            Tag {
                name: "aside",
                attributes: &[],
            },
            Tag {
                name: "aside",
                attributes: &[Attr {
                    name: "aria-label",
                    value: None,
                }],
            },
            Tag {
                name: "aside",
                attributes: &[Attr {
                    name: "aria-labelledby",
                    value: None,
                }],
            },
        ],
    ),
    (
        "contentinfo",
        &[Tag {
            name: "footer",
            attributes: &[],
        }],
    ),
    (
        "definition",
        &[Tag {
            name: "dd",
            attributes: &[],
        }],
    ),
    (
        "deletion",
        &[Tag {
            name: "del",
            attributes: &[],
        }],
    ),
    (
        "dialog",
        &[Tag {
            name: "dialog",
            attributes: &[],
        }],
    ),
    (
        "document",
        &[Tag {
            name: "html",
            attributes: &[],
        }],
    ),
    (
        "emphasis",
        &[Tag {
            name: "em",
            attributes: &[],
        }],
    ),
    (
        "figure",
        &[Tag {
            name: "figure",
            attributes: &[],
        }],
    ),
    (
        "form",
        &[
            Tag {
                name: "form",
                attributes: &[Attr {
                    name: "aria-label",
                    value: None,
                }],
            },
            Tag {
                name: "form",
                attributes: &[Attr {
                    name: "aria-labelledby",
                    value: None,
                }],
            },
            Tag {
                name: "form",
                attributes: &[Attr {
                    name: "name",
                    value: None,
                }],
            },
        ],
    ),
    (
        "generic",
        &[
            Tag {
                name: "a",
                attributes: &[],
            },
            Tag {
                name: "area",
                attributes: &[],
            },
            Tag {
                name: "aside",
                attributes: &[],
            },
            Tag {
                name: "b",
                attributes: &[],
            },
            Tag {
                name: "bdo",
                attributes: &[],
            },
            Tag {
                name: "body",
                attributes: &[],
            },
            Tag {
                name: "data",
                attributes: &[],
            },
            Tag {
                name: "div",
                attributes: &[],
            },
            Tag {
                name: "footer",
                attributes: &[],
            },
            Tag {
                name: "header",
                attributes: &[],
            },
            Tag {
                name: "hgroup",
                attributes: &[],
            },
            Tag {
                name: "i",
                attributes: &[],
            },
            Tag {
                name: "pre",
                attributes: &[],
            },
            Tag {
                name: "q",
                attributes: &[],
            },
            Tag {
                name: "samp",
                attributes: &[],
            },
            Tag {
                name: "section",
                attributes: &[],
            },
            Tag {
                name: "small",
                attributes: &[],
            },
            Tag {
                name: "span",
                attributes: &[],
            },
            Tag {
                name: "u",
                attributes: &[],
            },
        ],
    ),
    (
        "gridcell",
        &[Tag {
            name: "td",
            attributes: &[],
        }],
    ),
    (
        "group",
        &[
            Tag {
                name: "details",
                attributes: &[],
            },
            Tag {
                name: "fieldset",
                attributes: &[],
            },
            Tag {
                name: "optgroup",
                attributes: &[],
            },
            Tag {
                name: "address",
                attributes: &[],
            },
        ],
    ),
    (
        "heading",
        &[
            Tag {
                name: "h1",
                attributes: &[],
            },
            Tag {
                name: "h2",
                attributes: &[],
            },
            Tag {
                name: "h3",
                attributes: &[],
            },
            Tag {
                name: "h4",
                attributes: &[],
            },
            Tag {
                name: "h5",
                attributes: &[],
            },
            Tag {
                name: "h6",
                attributes: &[],
            },
        ],
    ),
    (
        "img",
        &[
            Tag {
                name: "img",
                attributes: &[Attr {
                    name: "alt",
                    value: None,
                }],
            },
            Tag {
                name: "img",
                attributes: &[Attr {
                    name: "alt",
                    value: None,
                }],
            },
        ],
    ),
    (
        "insertion",
        &[Tag {
            name: "ins",
            attributes: &[],
        }],
    ),
    (
        "link",
        &[
            Tag {
                name: "a",
                attributes: &[Attr {
                    name: "href",
                    value: None,
                }],
            },
            Tag {
                name: "area",
                attributes: &[Attr {
                    name: "href",
                    value: None,
                }],
            },
        ],
    ),
    (
        "list",
        &[
            Tag {
                name: "menu",
                attributes: &[],
            },
            Tag {
                name: "ol",
                attributes: &[],
            },
            Tag {
                name: "ul",
                attributes: &[],
            },
        ],
    ),
    (
        "listbox",
        &[
            Tag {
                name: "select",
                attributes: &[Attr {
                    name: "size",
                    value: None,
                }],
            },
            Tag {
                name: "select",
                attributes: &[Attr {
                    name: "multiple",
                    value: None,
                }],
            },
            Tag {
                name: "datalist",
                attributes: &[],
            },
        ],
    ),
    (
        "listitem",
        &[Tag {
            name: "li",
            attributes: &[],
        }],
    ),
    (
        "main",
        &[Tag {
            name: "main",
            attributes: &[],
        }],
    ),
    (
        "mark",
        &[Tag {
            name: "mark",
            attributes: &[],
        }],
    ),
    (
        "math",
        &[Tag {
            name: "math",
            attributes: &[],
        }],
    ),
    (
        "meter",
        &[Tag {
            name: "meter",
            attributes: &[],
        }],
    ),
    (
        "navigation",
        &[Tag {
            name: "nav",
            attributes: &[],
        }],
    ),
    (
        "option",
        &[Tag {
            name: "option",
            attributes: &[],
        }],
    ),
    (
        "paragraph",
        &[Tag {
            name: "p",
            attributes: &[],
        }],
    ),
    (
        "presentation",
        &[Tag {
            name: "img",
            attributes: &[Attr {
                name: "alt",
                value: Some(""),
            }],
        }],
    ),
    (
        "progressbar",
        &[Tag {
            name: "progress",
            attributes: &[],
        }],
    ),
    (
        "radio",
        &[Tag {
            name: "input",
            attributes: &[Attr {
                name: "type",
                value: Some("radio"),
            }],
        }],
    ),
    (
        "region",
        &[
            Tag {
                name: "section",
                attributes: &[Attr {
                    name: "aria-label",
                    value: None,
                }],
            },
            Tag {
                name: "section",
                attributes: &[Attr {
                    name: "aria-labelledby",
                    value: None,
                }],
            },
        ],
    ),
    (
        "row",
        &[Tag {
            name: "tr",
            attributes: &[],
        }],
    ),
    (
        "rowgroup",
        &[
            Tag {
                name: "tbody",
                attributes: &[],
            },
            Tag {
                name: "tfoot",
                attributes: &[],
            },
            Tag {
                name: "thead",
                attributes: &[],
            },
        ],
    ),
    (
        "rowheader",
        &[
            Tag {
                name: "th",
                attributes: &[Attr {
                    name: "scope",
                    value: Some("row"),
                }],
            },
            Tag {
                name: "th",
                attributes: &[Attr {
                    name: "scope",
                    value: Some("rowgroup"),
                }],
            },
        ],
    ),
    (
        "searchbox",
        &[Tag {
            name: "input",
            attributes: &[
                Attr {
                    name: "list",
                    value: None,
                },
                Attr {
                    name: "type",
                    value: Some("search"),
                },
            ],
        }],
    ),
    (
        "separator",
        &[Tag {
            name: "hr",
            attributes: &[],
        }],
    ),
    (
        "slider",
        &[Tag {
            name: "input",
            attributes: &[Attr {
                name: "type",
                value: Some("range"),
            }],
        }],
    ),
    (
        "spinbutton",
        &[Tag {
            name: "input",
            attributes: &[Attr {
                name: "type",
                value: Some("number"),
            }],
        }],
    ),
    (
        "status",
        &[Tag {
            name: "output",
            attributes: &[],
        }],
    ),
    (
        "strong",
        &[Tag {
            name: "strong",
            attributes: &[],
        }],
    ),
    (
        "subscript",
        &[Tag {
            name: "sub",
            attributes: &[],
        }],
    ),
    (
        "superscript",
        &[Tag {
            name: "sup",
            attributes: &[],
        }],
    ),
    (
        "table",
        &[Tag {
            name: "table",
            attributes: &[],
        }],
    ),
    (
        "term",
        &[
            Tag {
                name: "dfn",
                attributes: &[],
            },
            Tag {
                name: "dt",
                attributes: &[],
            },
        ],
    ),
    (
        "textbox",
        &[
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "type",
                        value: None,
                    },
                    Attr {
                        name: "list",
                        value: None,
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("email"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("tel"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("text"),
                    },
                ],
            },
            Tag {
                name: "input",
                attributes: &[
                    Attr {
                        name: "list",
                        value: None,
                    },
                    Attr {
                        name: "type",
                        value: Some("url"),
                    },
                ],
            },
            Tag {
                name: "textarea",
                attributes: &[],
            },
        ],
    ),
    (
        "time",
        &[Tag {
            name: "time",
            attributes: &[],
        }],
    ),
];

/// The HTML elements ARIA reserves.
///
/// None of them is rendered, so none of them is in the accessibility tree
/// for a role or an `aria-*` to say anything about.
pub(super) static RESERVED_ELEMENTS: &[&str] = &[
    "base", "col", "colgroup", "head", "html", "link", "meta", "noembed", "noscript", "param",
    "picture", "script", "source", "style", "title", "track",
];
