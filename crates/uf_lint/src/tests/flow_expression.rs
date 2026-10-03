//! Flow lints over expressions: property accessors, `Object.assign`, and
//! optional chaining on a base that cannot be null.

use super::*;

#[test]
fn unsafe_getters_setters_rejects_accessors() {
    let diagnostics = lint_js(
        "flow/unsafe-getters-setters",
        "// @flow\nclass Box {\n  get value(): number { return 1; }\n  set value(next: number) {}\n}\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 3));
}

#[test]
fn unsafe_getters_setters_ignores_an_accessor_inside_a_string() {
    let diagnostics = lint_js(
        "flow/unsafe-getters-setters",
        "// @flow\nconst source = `\nget value() { return 1; }\n`;\nclass Box {\n  get value(): number { return 1; }\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 6);
    assert_eq!(diagnostics[0].column, 3);
}

#[test]
fn unsafe_getters_setters_accepts_plain_methods() {
    let diagnostics = lint_js(
        "flow/unsafe-getters-setters",
        "// @flow\nclass Box {\n  getValue(): number { return 1; }\n}\nconst v = map.get(key);\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unsafe_object_assign_rejects_object_assign() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst merged = Object.assign({}, base, patch);\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 16));
}

#[test]
fn unsafe_object_assign_ignores_a_longer_name() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst x = Object.assignment;\nconst y = Object.assigned;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unsafe_object_assign_accepts_object_spread() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst merged = { ...base, ...patch };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unnecessary_optional_chain_rejects_optional_this() {
    let diagnostics = lint_js(
        "flow/unnecessary-optional-chain",
        "// @flow\nclass A { run() { return this?.value; } }\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn unnecessary_optional_chain_accepts_chains_on_nullable_bases() {
    let diagnostics = lint_js(
        "flow/unnecessary-optional-chain",
        "// @flow\nconst v = props?.meta?.title;\nconst w = this.value;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unsafe_object_assign_ignores_jsx_text() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst view = <p>Object.assign(a, b)</p>;\nconst continued = <p>\nObject.assign(a, b)\n</p>;\nconst real = Object.assign({}, a);\nconst expr = <p>{Object.assign({}, a)}</p>;\n",
    );

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 6);
    assert_eq!(diagnostics[1].line, 7);
}

#[test]
fn unsafe_object_assign_ignores_a_string_that_contains_one() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst s = \"Object.assign(a, b)\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unnecessary_optional_chain_ignores_a_string_that_contains_one() {
    // `"this?.foo"` is text. A quick fix that rewrote it would change what the
    // program prints, which is why the fix was not offered until this was.
    let diagnostics = lint_js(
        "flow/unnecessary-optional-chain",
        "// @flow\nconst s = \"this?.foo\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
