//! The `_uf.*` reserved file-name grammar.
//!
//! `uf` reserves the `_uf.` prefix inside the router root so a project cannot
//! accidentally shadow a framework file. A reserved name is
//! `_uf.<role>[.<variant>].js`, where the role is what the file does and the
//! variant narrows which build it applies to.
//!
//! This is the single source of truth for that grammar. `uf create` generates
//! these names, `discover_routes` looks for them, and `uf lint`'s
//! `router/reserved-files` rule rejects the ones that do not fit — so all three
//! read it from here rather than each spelling out the same `matches!`.
//!
//! Not every role is the router's. `route` answers a request and `story` names
//! a rendered state of a component; neither is one of
//! [`ReservedRole::route_parts`], which is the roles a rendered route is built
//! from. They are here because the *grammar* is the toolchain's rather than the
//! router's: a second spelling for the same idea is how the scaffold, the
//! router and the linter drifted apart the first time.
//!
//! It drifted again anyway, in the direction this module could not see.
//! `packages/vite/internal/routes.js` is the router the build actually runs,
//! it keeps its own `RESERVED` table, and its comment says the two "cannot be
//! allowed to disagree" — while `_uf.not-found` was in that table and not in
//! this enum, so `uf lint` rejected the file name uf's own documentation site
//! uses for its 404 page. `every_name_the_build_router_reserves_is_a_role`
//! reads that table now, which is the part that was missing: a rule that both
//! files must agree, enforced by neither, is a comment.
//!
//! [`ReservedRole::Error`] is the first role added since, and it is what those
//! tests are for: a role here that the build router does not scan for, or a
//! name there this enum does not know, now fails by name rather than becoming
//! the next thing nobody noticed.

use std::str::FromStr;

/// What a reserved file does for the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReservedRole {
    /// Wraps a route subtree.
    Layout,
    /// Renders a route.
    Page,
    /// Runs before a route resolves.
    Middleware,
    /// Renders a path no route matched.
    ///
    /// A page in every way that matters — it is wrapped in the layouts above
    /// it and rendered like one — except that no path leads to it, which is
    /// why it is not one of [`route_parts`](ReservedRole::route_parts).
    NotFound,
    /// Renders in place of a subtree that threw.
    ///
    /// One role rather than three. `forbidden` and `unauthorized` were the
    /// obvious alternative — Next.js has a file for each, and uf already has
    /// `not-found` as a role of its own, which argues the same way — and it is
    /// the wrong shape here. A `not-found` is a *page*: a 404 is an ordinary
    /// answer a site gives, reached by a URL that matched nothing. An `error`
    /// is the unhappy path, and 401, 403 and 500 differ only in why the
    /// subtree stopped. Three files per segment to say three sentences about
    /// one thing is what the union in `@uniflowed/router`'s `RouteError`
    /// replaces, and `match` over it is checked where three files are not.
    ///
    /// It is not one of [`route_parts`](ReservedRole::route_parts): it renders
    /// *instead* of a route, never as part of one.
    Error,
    /// Answers a request instead of rendering a page.
    Route,
    /// Names a rendered state of a component, for `@uniflowed/story`.
    Story,
}

impl ReservedRole {
    /// The role segment as it appears in a file name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Layout => "layout",
            Self::Page => "page",
            Self::Middleware => "middleware",
            Self::NotFound => "not-found",
            Self::Error => "error",
            Self::Route => "route",
            Self::Story => "story",
        }
    }

    /// Every role, in declaration order.
    ///
    /// Named to mean the same thing [`ReservedVariant::all`] does, which it did
    /// not: this returned three of the five roles, because it was written when
    /// "every role" and "every role a route is built from" were the same set.
    /// Two `all` in one module meaning two different things is the drift this
    /// module exists to prevent, one level up.
    #[must_use]
    pub const fn all() -> [Self; 7] {
        [
            Self::Layout,
            Self::Page,
            Self::Middleware,
            Self::NotFound,
            Self::Error,
            Self::Route,
            Self::Story,
        ]
    }

    /// The roles a rendered route is built from.
    ///
    /// A `route` answers a request rather than rendering, a `story` names a
    /// state of a component, a `not-found` is reached by no path, and an
    /// `error` renders instead of the route rather than as part of it — so
    /// none of the four composes a route, though all four are reserved names.
    #[must_use]
    pub const fn route_parts() -> [Self; 3] {
        [Self::Layout, Self::Page, Self::Middleware]
    }
}

impl FromStr for ReservedRole {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "layout" => Ok(Self::Layout),
            "page" => Ok(Self::Page),
            "middleware" => Ok(Self::Middleware),
            "not-found" => Ok(Self::NotFound),
            "error" => Ok(Self::Error),
            "route" => Ok(Self::Route),
            "story" => Ok(Self::Story),
            _ => Err(()),
        }
    }
}

/// Which build a reserved file applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReservedVariant {
    /// Applies to every target. This is the file the router resolves.
    Default,
    /// Applies to React Native on every platform.
    Native,
    /// Applies to iOS only.
    Ios,
    /// Applies to Android only.
    Android,
    /// Applies to the web target only.
    Web,
    /// A test colocated with the route it covers.
    Test,
}

impl ReservedVariant {
    /// The variant segment as it appears in a file name, or [`None`] for the
    /// default variant, which has no segment.
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Native => Some("native"),
            Self::Ios => Some("ios"),
            Self::Android => Some("android"),
            Self::Web => Some("web"),
            Self::Test => Some("test"),
        }
    }

    /// Whether this variant is the one the router resolves as the route itself.
    ///
    /// Platform variants and colocated tests are companions to a route, never
    /// routes of their own, which is why `discover_routes` only matches the
    /// default variant.
    #[must_use]
    pub const fn is_route_entry(self) -> bool {
        matches!(self, Self::Default)
    }

    /// Every variant, in declaration order.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Default,
            Self::Native,
            Self::Ios,
            Self::Android,
            Self::Web,
            Self::Test,
        ]
    }
}

impl FromStr for ReservedVariant {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native" => Ok(Self::Native),
            "ios" => Ok(Self::Ios),
            "android" => Ok(Self::Android),
            "web" => Ok(Self::Web),
            "test" => Ok(Self::Test),
            _ => Err(()),
        }
    }
}

/// A recognized `_uf.*` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReservedFile {
    /// What the file does.
    pub role: ReservedRole,
    /// Which build it applies to.
    pub variant: ReservedVariant,
}

impl ReservedFile {
    /// Render the file name this describes.
    #[must_use]
    pub fn file_name(self) -> String {
        match self.variant.as_str() {
            Some(variant) => format!("_uf.{}.{variant}.js", self.role.as_str()),
            None => format!("_uf.{}.js", self.role.as_str()),
        }
    }
}

/// How a file name relates to the reserved grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedName {
    /// The name does not use the `_uf.` prefix, so the project owns it.
    NotReserved,
    /// A name `uf` defines.
    Recognized(ReservedFile),
    /// Uses the reserved prefix but is not a name `uf` defines.
    Unknown,
}

impl ReservedName {
    /// The recognized file, if the name is one.
    #[must_use]
    pub const fn recognized(self) -> Option<ReservedFile> {
        match self {
            Self::Recognized(file) => Some(file),
            _ => None,
        }
    }

    /// Whether the name claims the reserved prefix without being a name `uf`
    /// defines. This is what `router/reserved-files` reports.
    #[must_use]
    pub const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Classify a bare file name against the reserved grammar.
///
/// Takes a file name, not a path: callers already have one, and accepting a
/// path here would invite a caller to pass an unnormalized one and get a
/// different answer than the router did.
#[must_use]
pub fn classify_reserved_file(file_name: &str) -> ReservedName {
    let Some(rest) = file_name.strip_prefix("_uf.") else {
        return ReservedName::NotReserved;
    };
    let Some(rest) = rest.strip_suffix(".js") else {
        return ReservedName::Unknown;
    };

    let mut segments = rest.split('.');
    let Some(Ok(role)) = segments.next().map(ReservedRole::from_str) else {
        return ReservedName::Unknown;
    };
    let variant = match segments.next() {
        None => ReservedVariant::Default,
        Some(segment) => match ReservedVariant::from_str(segment) {
            Ok(variant) => variant,
            Err(()) => return ReservedName::Unknown,
        },
    };
    if segments.next().is_some() {
        // `_uf.page.native.test.js` and friends: one variant, not a stack of them.
        return ReservedName::Unknown;
    }

    ReservedName::Recognized(ReservedFile { role, variant })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recognized(file_name: &str) -> ReservedFile {
        classify_reserved_file(file_name)
            .recognized()
            .unwrap_or_else(|| panic!("{file_name} should be recognized"))
    }

    #[test]
    fn a_story_file_is_reserved_without_being_a_route() {
        // The grammar is the toolchain's, not the router's: `@uniflowed/story`
        // names its files by it, and `uf lint` has to recognise them or a
        // package cannot use the convention the repository asked it to use.
        assert_eq!(recognized("_uf.story.js").role, ReservedRole::Story);
        assert_eq!(
            recognized("_uf.story.native.js").variant,
            ReservedVariant::Native
        );
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Story),
            "a story is not something a route is built from"
        );
    }

    #[test]
    fn a_not_found_file_is_reserved_without_a_route_leading_to_it() {
        // The file uf's own documentation site uses for its 404 page.
        // `packages/vite/internal/routes.js` has reserved this name since the
        // router was written; this enum did not, so `uf lint` told the site to
        // rename a file the router resolves. See `tests/reserved_names.rs`.
        assert_eq!(recognized("_uf.not-found.js").role, ReservedRole::NotFound);
        assert_eq!(
            recognized("_uf.not-found.web.js").variant,
            ReservedVariant::Web
        );
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::NotFound),
            "no path leads to a not-found page, so no route is built from one"
        );
    }

    #[test]
    fn an_error_file_is_reserved_and_is_not_part_of_a_route() {
        // One role for 401, 403 and 500, not three files per segment; see
        // `ReservedRole::Error`. `_uf.forbidden.js` and `_uf.unauthorized.js`
        // are therefore names uf does not define, and the linter says so.
        assert_eq!(recognized("_uf.error.js").role, ReservedRole::Error);
        assert_eq!(
            recognized("_uf.error.native.js").variant,
            ReservedVariant::Native
        );
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Error),
            "an error boundary renders instead of a route, not as part of one"
        );
        for name in [
            "_uf.forbidden.js",
            "_uf.unauthorized.js",
            "_uf.global-error.js",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn plain_project_files_are_not_reserved() {
        for name in [
            "page.js",
            "Counter.js",
            "index.js",
            "_private.js",
            "uf.config.js",
            "_ufo.page.js",
        ] {
            assert_eq!(
                classify_reserved_file(name),
                ReservedName::NotReserved,
                "{name}"
            );
        }
    }

    #[test]
    fn the_three_router_roles_are_recognized() {
        assert_eq!(
            recognized("_uf.layout.js"),
            ReservedFile {
                role: ReservedRole::Layout,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("_uf.page.js"),
            ReservedFile {
                role: ReservedRole::Page,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("_uf.middleware.js"),
            ReservedFile {
                role: ReservedRole::Middleware,
                variant: ReservedVariant::Default
            }
        );
    }

    #[test]
    fn platform_variants_are_recognized() {
        for (name, variant) in [
            ("_uf.page.native.js", ReservedVariant::Native),
            ("_uf.page.ios.js", ReservedVariant::Ios),
            ("_uf.page.android.js", ReservedVariant::Android),
            ("_uf.page.web.js", ReservedVariant::Web),
        ] {
            assert_eq!(recognized(name).variant, variant, "{name}");
            assert_eq!(recognized(name).role, ReservedRole::Page, "{name}");
        }
    }

    #[test]
    fn colocated_tests_are_recognized() {
        assert_eq!(
            recognized("_uf.page.test.js").variant,
            ReservedVariant::Test
        );
        assert_eq!(
            recognized("_uf.layout.test.js").variant,
            ReservedVariant::Test
        );
    }

    #[test]
    fn every_role_and_variant_pair_round_trips_through_its_file_name() {
        for role in ReservedRole::all() {
            for variant in ReservedVariant::all() {
                let file = ReservedFile { role, variant };
                assert_eq!(recognized(&file.file_name()), file);
            }
        }
    }

    #[test]
    fn only_the_default_variant_is_a_route_entry() {
        assert!(ReservedVariant::Default.is_route_entry());
        for variant in ReservedVariant::all()
            .into_iter()
            .filter(|variant| *variant != ReservedVariant::Default)
        {
            assert!(!variant.is_route_entry(), "{variant:?}");
        }
    }

    #[test]
    fn an_unknown_role_is_rejected() {
        for name in [
            "_uf.handler.js",
            "_uf.loader.js",
            "_uf.js",
            "_uf.page",
            "_uf.PAGE.js",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn an_unknown_variant_is_rejected() {
        for name in [
            "_uf.page.server.js",
            "_uf.page.windows.js",
            "_uf.page.NATIVE.js",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn variants_do_not_stack() {
        assert!(classify_reserved_file("_uf.page.native.test.js").is_unknown());
        assert!(classify_reserved_file("_uf.page.ios.android.js").is_unknown());
    }

    #[test]
    fn a_reserved_prefix_without_a_js_extension_is_rejected() {
        for name in [
            "_uf.page.ts",
            "_uf.page.jsx",
            "_uf.page.js.flow",
            "_uf.page",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn classification_does_not_panic_on_odd_input() {
        for name in ["", "_uf.", "_uf..js", "_uf....js", "_uf.\u{1f600}.js"] {
            let _ = classify_reserved_file(name);
        }
    }
}
