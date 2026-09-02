//! Flow built-ins about what a module declares and imports: mixing `require`
//! into an ES module, exporting a binding that can be reassigned, renaming a
//! binding to `default` on the way out, and top-level names that collide with a
//! JSX intrinsic element.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, ends_word, find_all, find_words, identifier_len, next_non_space, prev_non_space,
    starts_word,
};
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_flow_mixed_import_and_require(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::MixedImportAndRequire.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };
    if !scan.facts.has_esm_import {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "require") {
            if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
                continue;
            }
            if !next_non_space(code, at + "require".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "this module already uses `import`; do not mix in `require`",
            );
        }
    }
}

pub(crate) fn run_flow_non_const_var_export(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::NonConstVarExport.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if identifier_len(code, at) != "export".len() || &code[at..at + 6] != "export" {
            continue;
        }
        let Some((keyword_at, _)) = next_non_space(code, at + 6) else {
            continue;
        };
        let len = identifier_len(code, keyword_at);
        if len == 0 || !matches!(&code[keyword_at..keyword_at + len], "var" | "let") {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            keyword_at,
            "exported bindings must be `const`; a mutable export is a live binding",
        );
    }
}

pub(crate) fn run_flow_export_renamed_default(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::ExportRenamedDefault.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "as default")
            .filter(|&at| starts_word(code, at) && ends_word(code, at + "as default".len()))
        {
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "renaming an export to `default` hides the real name; export it directly",
            );
        }
    }
}

/// Lowercase JSX intrinsic element names, which local bindings must not shadow.
static JSX_INTRINSICS: phf::Set<&'static str> = phf::phf_set! {
    "a", "abbr", "address", "area", "article", "aside", "audio",
    "b", "base", "bdi", "bdo", "big", "blockquote", "body", "br", "button",
    "canvas", "caption", "circle", "cite", "clipPath", "code", "col", "colgroup",
    "data", "datalist", "dd", "defs", "del", "details", "dfn", "dialog", "div",
    "dl", "dt", "ellipse", "em", "embed", "fieldset", "figcaption", "figure",
    "footer", "foreignObject", "form", "g", "h1", "h2", "h3", "h4", "h5", "h6",
    "head", "header", "hgroup", "hr", "html", "i", "iframe", "image", "img",
    "input", "ins", "kbd", "label", "legend", "li", "line", "linearGradient",
    "link", "main", "map", "mark", "marker", "mask", "menu", "meta", "meter",
    "nav", "noscript", "object", "ol", "optgroup", "option", "output", "p",
    "param", "path", "pattern", "picture", "polygon", "polyline", "pre",
    "progress", "q", "radialGradient", "rect", "rp", "rt", "ruby", "s", "samp",
    "script", "search", "section", "select", "slot", "small", "source", "span",
    "stop", "strong", "style", "sub", "summary", "sup", "svg", "table", "tbody",
    "td", "template", "text", "textarea", "tfoot", "th", "thead", "time",
    "title", "tr", "track", "tspan", "u", "ul", "use", "var", "video", "wbr",
};

/// Keywords that introduce a binding whose name follows.
const BINDING_KEYWORDS: [&str; 7] = [
    "const",
    "let",
    "var",
    "function",
    "component",
    "hook",
    "class",
];

pub(crate) fn run_flow_react_intrinsic_overlap(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::ReactIntrinsicOverlap.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((mut at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if code[at..].starts_with("export ") {
            let Some((next, _)) = next_non_space(code, at + "export ".len()) else {
                continue;
            };
            at = next;
        }
        let len = identifier_len(code, at);
        if len == 0 || !BINDING_KEYWORDS.contains(&&code[at..at + len]) {
            continue;
        }
        let Some((name_at, _)) = next_non_space(code, at + len) else {
            continue;
        };
        let name_len = identifier_len(code, name_at);
        if name_len == 0 || !JSX_INTRINSICS.contains(&code[name_at..name_at + name_len]) {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            name_at,
            "this name shadows a JSX intrinsic element and silently changes what JSX means",
        );
    }
}
