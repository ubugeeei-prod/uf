//! The reserved-name grammar, against the router that actually runs.
//!
//! `crates/uf_router/src/reserved.rs` calls itself the single source of truth
//! for `_uf.<role>[.<variant>]`, and `packages/vite/internal/routes.js` is the
//! file-system router the build runs, with a `RESERVED` table of its own and a
//! comment saying the two "cannot be allowed to disagree". Nothing compared
//! them, and they disagreed: `_uf.not-found` was in the JavaScript table and
//! not in the Rust enum, so `uf lint`'s `router/reserved-files` reported the
//! file name uf's own documentation site uses for its 404 page as one the
//! router would not recognize — and told the reader to rename it.
//!
//! A rule that two files must agree, enforced by neither, is a comment. This
//! is the enforcement.
//!
//! It is one direction plus a named exception rather than an equality, because
//! the two sets are not meant to be equal: the grammar is the toolchain's and
//! the table is the router's, so a role no router resolves belongs in the first
//! and not the second. `story` is that role today, and naming it here is what
//! makes adding a second one a decision somebody has to write down.

use std::collections::BTreeSet;
use std::path::Path;

use uf_router::ReservedRole;

/// Roles that are reserved names without being anything the router resolves.
///
/// `story` names a rendered state of a component and is found by `uf story`,
/// which walks the project itself. Adding to this list means saying that a
/// reserved name is deliberately invisible to the build router; that is a real
/// decision and the point of writing it here is that it cannot be made by
/// forgetting.
const NOT_THE_ROUTERS: &[ReservedRole] = &[ReservedRole::Story];

/// The `_uf.*` names `packages/vite/internal/routes.js` reserves.
///
/// Read out of the source rather than duplicated, because a copy here would be
/// a fourth spelling of the grammar and this file exists to stop the third.
/// The table is a frozen object literal of `key: "_uf.name"` pairs, so the
/// names are every `"_uf.…"` string inside it — no JavaScript parser needed,
/// and a table that stops having that shape fails loudly below rather than
/// quietly matching nothing.
fn build_router_names() -> BTreeSet<String> {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/vite/internal/routes.js"),
    )
    .expect("the build router is in this repository");

    let table = source
        .split_once("export const RESERVED = Object.freeze({")
        .expect("`RESERVED` is a frozen object literal; if it is not, this test is out of date")
        .1
        .split_once("});")
        .expect("the literal is closed")
        .0;

    let names: BTreeSet<String> = table
        .split('"')
        .filter(|value| value.starts_with("_uf."))
        .map(|value| value["_uf.".len()..].to_owned())
        .collect();

    assert!(
        names.len() > 1,
        "read {} name(s) out of the build router's table, which is not a table:\n{table}",
        names.len()
    );
    names
}

#[test]
fn every_name_the_build_router_reserves_is_a_role() {
    let roles: BTreeSet<&str> = ReservedRole::all().map(ReservedRole::as_str).into();

    for name in build_router_names() {
        assert!(
            roles.contains(name.as_str()),
            "`packages/vite/internal/routes.js` reserves `_uf.{name}` and `ReservedRole` has no \
             such role, so `uf lint` rejects a file the router resolves"
        );
    }
}

#[test]
fn every_role_the_router_resolves_is_in_the_build_router() {
    let names = build_router_names();

    for role in ReservedRole::all() {
        if NOT_THE_ROUTERS.contains(&role) {
            assert!(
                !names.contains(role.as_str()),
                "`{}` is listed as not the router's and the build router reserves it; one of the \
                 two is wrong",
                role.as_str()
            );
            continue;
        }
        assert!(
            names.contains(role.as_str()),
            "`ReservedRole::{role:?}` is a reserved name the build router does not scan for, so \
             `uf lint` accepts a file name nothing resolves. Add it to `RESERVED` in \
             `packages/vite/internal/routes.js`, or to `NOT_THE_ROUTERS` here with a reason"
        );
    }
}
