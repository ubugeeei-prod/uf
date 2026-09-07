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
//!
//! The directory names are the second half. `@team` and `(.)photo` are Next.js
//! conventions uf does not implement, and until ubugeeei-prod/uf#267 *neither*
//! router had an opinion about them: both fell through to a literal URL
//! segment, so a project that wrote one got `/@team` and no error. They agree
//! about that now, and the tests at the bottom are what keeps them agreeing —
//! a spelling one refuses and the other serves is worse than the hole was.

use std::collections::BTreeSet;
use std::path::Path;

use uf_router::{ReservedRole, RouteSegment, classify_route_segment};

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
fn build_router_source() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/vite/internal/routes.js"),
    )
    .expect("the build router is in this repository")
}

fn build_router_names() -> BTreeSet<String> {
    let source = build_router_source();

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

/// The directory names the build router refuses, read out of its own source.
///
/// The same technique as `build_router_names`, for the same reason: a copy of
/// the list here would be a third place the grammar is written down, and this
/// file exists to stop the second one drifting.
fn build_router_unsupported_segments() -> BTreeSet<String> {
    let source = build_router_source();

    let table = source
        .split_once("export const UNSUPPORTED_SEGMENTS = Object.freeze([")
        .expect("`UNSUPPORTED_SEGMENTS` is a frozen array literal; if it is not, this test is out of date")
        .1
        .split_once("]);")
        .expect("the literal is closed")
        .0;

    let names: BTreeSet<String> = table
        .split('"')
        .filter(|value| !value.trim().is_empty() && !value.contains(','))
        .map(str::to_owned)
        .collect();

    assert!(
        names.len() > 1,
        "read {} name(s) out of the build router's list, which is not a list:\n{table}",
        names.len()
    );
    names
}

#[test]
fn both_routers_refuse_the_same_directory_spellings() {
    let build_router = build_router_unsupported_segments();
    let grammar: BTreeSet<String> = RouteSegment::UNSUPPORTED_EXAMPLES
        .iter()
        .map(|segment| (*segment).to_owned())
        .collect();

    assert_eq!(
        grammar, build_router,
        "`RouteSegment::UNSUPPORTED_EXAMPLES` and `UNSUPPORTED_SEGMENTS` in \
         `packages/vite/internal/routes.js` name different directory spellings, so one router \
         refuses a directory the other serves as a URL"
    );
}

#[test]
fn every_spelling_the_build_router_refuses_is_unsupported_here() {
    for segment in build_router_unsupported_segments() {
        assert!(
            !classify_route_segment(&segment).is_supported(),
            "`packages/vite/internal/routes.js` refuses `{segment}` and \
             `uf_router::classify_route_segment` calls it a route, so `uf build` would generate a \
             `RoutePath` for a directory the build router will not serve"
        );
    }
}

/// The two routers have to accept the same *extensions* too, not only the same
/// roles.
///
/// The role check above is what caught `_uf.not-found`. This is the same
/// disagreement in the other direction: the grammar accepted `.js` and nothing
/// else while the build router had accepted `.jsx` and `.mdx` since it was
/// written. So uf's own documentation site reported `routes 1` beside
/// `prerendered pages 30` — every `.mdx` page in the guide invisible to the
/// count — and `router/reserved-files` told the author to rename files the
/// build resolves. ubugeeei-prod/uf#291, #386, #437.
///
/// An equality rather than one direction plus exceptions, because unlike the
/// roles these two sets *are* meant to be the same: an extension one router
/// runs and the other does not know about is a file that either works and is
/// reported, or is reported and does not work.
#[test]
fn both_routers_accept_the_same_extensions() {
    let source = build_router_source();

    for (name, ours) in [
        ("PAGE_EXTENSIONS", &uf_router::PAGE_EXTENSIONS[..]),
        ("MODULE_EXTENSIONS", &uf_router::MODULE_EXTENSIONS[..]),
    ] {
        assert_eq!(
            extensions_named(&source, name),
            ours,
            "`{name}` in packages/vite/internal/routes.js and in uf_router disagree"
        );
    }
}

/// And every role says which of the two lists is its own, so a role that may
/// not be Markdown cannot quietly become one.
#[test]
fn only_a_page_may_be_written_in_markdown() {
    for role in [ReservedRole::Page, ReservedRole::NotFound] {
        assert!(
            role.extensions().contains(&".mdx"),
            "{role:?} is a page and may be Markdown"
        );
    }
    for role in [
        ReservedRole::Layout,
        ReservedRole::Middleware,
        ReservedRole::Route,
        ReservedRole::Error,
        ReservedRole::Loading,
        ReservedRole::Story,
        ReservedRole::Template,
    ] {
        assert!(
            !role.extensions().contains(&".mdx"),
            "{role:?} is code, and Markdown is not something it can be"
        );
    }
}

/// The `[".js", ".jsx"]` on the right of `const NAME =`, as a list.
fn extensions_named(source: &str, name: &str) -> Vec<String> {
    let line = source
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("const {name} =")))
        .unwrap_or_else(|| panic!("routes.js no longer declares {name}"));
    let list = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inside, _)| inside)
        .unwrap_or_else(|| panic!("{name} is no longer an array literal: {line}"));
    list.split(',')
        .map(|entry| entry.trim().trim_matches('"').to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}
