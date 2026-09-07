//! The names `uf` reserves inside the router root: the `_uf.*` files, and the
//! directory spellings that are not ordinary URL segments.
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
//! [`ReservedRole::Error`] was the first role added since, and it is what those
//! tests are for: a role here that the build router does not scan for, or a
//! name there this enum does not know, now fails by name rather than becoming
//! the next thing nobody noticed. [`ReservedRole::Loading`] is the second, and
//! the tests did their job — adding it here alone left
//! `every_role_the_router_resolves_is_in_the_build_router` failing until
//! `RESERVED` in the build router named it too.
//!
//! # Directory names
//!
//! [`classify_route_segment`] is the other half, and it is here for the reason
//! the file names are: it is one grammar with three readers. A file name says
//! what a file *does*; a directory name says what a segment *is* — `(group)`
//! is not a URL segment, `[name]` captures one, `[...name]` captures the rest,
//! and everything else is a literal.
//!
//! That list had a hole in it, in the direction of a feature uf does not have.
//! Next.js spells a parallel route `@team` and an intercepting route
//! `(.)photo`, and both fell through to "literal" here and in the build
//! router: `@team` became the URL `/@team`, `(.)photo` became `/(.)photo` —
//! `(` and `)` are not a group unless the segment *ends* in `)` — and the
//! generated `RoutePath` union contained both, so `route("/@team", …)` type
//! checked. A convention silently served as a URL is worse than one that is
//! not supported: the project looks like it works.
//!
//! So [`RouteSegment::Slot`] and [`RouteSegment::Interception`] are names in
//! this grammar without being routes. Both routers refuse them, `uf lint`
//! reports them, and the refusal says which feature the spelling belongs to.
//! See ubugeeei-prod/uf#267.

use std::str::FromStr;

/// What a reserved file does for the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReservedRole {
    /// Wraps a route subtree.
    Layout,
    /// Wraps a route subtree, and remounts on every navigation.
    ///
    /// A layout persists — that is the whole point of one, and it is why a
    /// sidebar keeps its scroll position when the page under it changes. A
    /// template is the same wrapper with the opposite answer to the same
    /// question, for the cases where persistence is the wrong default: an
    /// enter animation that should play again, a `useEffect` that should run
    /// again, state that should start over.
    ///
    /// It is one of [`route_parts`](ReservedRole::route_parts), unlike the
    /// boundaries: a template is part of the rendered route rather than
    /// something that renders instead of it. It sits inside its own segment's
    /// layout and outside everything below, which is where its `key` has to be
    /// for a remount to mean "this segment and what is under it".
    Template,
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
    /// Renders while the subtree under it has not resolved.
    ///
    /// A segment's `_uf.loading.js` is the fallback of a `<Suspense>` around
    /// that segment's page and everything below it, which is what lets the
    /// renderer send the layouts around it before the page's data is in hand.
    ///
    /// It is not one of [`route_parts`](ReservedRole::route_parts) for the
    /// same reason [`Error`](ReservedRole::Error) is not: it renders while the
    /// route is *not* there, and the finished route contains none of it. The
    /// boundary it declares outlives the fallback, but the boundary is the
    /// router's and the file is only what fills it.
    Loading,
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
            Self::Template => "template",
            Self::Page => "page",
            Self::Middleware => "middleware",
            Self::NotFound => "not-found",
            Self::Error => "error",
            Self::Loading => "loading",
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
    pub const fn all() -> [Self; 9] {
        [
            Self::Layout,
            Self::Template,
            Self::Page,
            Self::Middleware,
            Self::NotFound,
            Self::Error,
            Self::Loading,
            Self::Route,
            Self::Story,
        ]
    }

    /// The roles a rendered route is built from.
    ///
    /// A `route` answers a request rather than rendering, a `story` names a
    /// state of a component, a `not-found` is reached by no path, an `error`
    /// renders instead of the route rather than as part of it, and a `loading`
    /// renders while it is not there yet — so none of the five composes a
    /// route, though all five are reserved names.
    ///
    /// A `template` does compose one. It is a layout that remounts, and the
    /// rendered route contains it exactly the way it contains a layout.
    #[must_use]
    pub const fn route_parts() -> [Self; 4] {
        [Self::Layout, Self::Template, Self::Page, Self::Middleware]
    }
}

impl FromStr for ReservedRole {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "layout" => Ok(Self::Layout),
            "template" => Ok(Self::Template),
            "page" => Ok(Self::Page),
            "middleware" => Ok(Self::Middleware),
            "not-found" => Ok(Self::NotFound),
            "error" => Ok(Self::Error),
            "loading" => Ok(Self::Loading),
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

// ---------------------------------------------------------------------------
// Directory names
// ---------------------------------------------------------------------------

/// What a directory name inside the router root means to the route path.
///
/// The borrow is the part of the name that carries information: a parameter's
/// name, an interception's target. `Literal` borrows the whole segment, so a
/// caller can `match` once rather than matching and then re-reading the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSegment<'a> {
    /// `(marketing)` — organises files without appearing in the URL.
    Group,
    /// `[slug]` — captures one URL segment, under the borrowed name.
    Param(&'a str),
    /// `[...path]` — captures the rest of the URL, under the borrowed name.
    CatchAll(&'a str),
    /// An ordinary URL segment, spelled exactly as the directory is.
    Literal(&'a str),
    /// `@team` — a parallel-route slot. Not a route uf can serve.
    ///
    /// A slot is a route that matches into a *named place* rather than into
    /// the one page a URL has, and a layout renders several of them at once,
    /// each with its own loading and error state. uf's `RouteRecord` is one
    /// page and a list of layouts and `RouteView` composes exactly that, so
    /// there is no second child to give a layout and no place to put one.
    Slot(&'a str),
    /// `(.)photo`, `(..)photo`, `(...)photo`, `(..)(..)photo` — an intercepting
    /// route. Not a route uf can serve.
    ///
    /// Interception matches a path *from within a segment* and renders it
    /// there, leaving the URL alone. That needs the router to know where a
    /// navigation came from, which is a change to what a navigation is: uf's
    /// `navigate` has a destination and nothing else.
    Interception {
        /// The `(.)`-style prefix, as written.
        marker: &'a str,
        /// What follows it — the route being intercepted.
        route: &'a str,
    },
}

impl RouteSegment<'_> {
    /// One directory name per spelling uf refuses.
    ///
    /// Here so that `tests/reserved_names.rs` can hold the build router to the
    /// same list, the way it already holds it to [`ReservedRole`]. A spelling
    /// one router refuses and the other serves is the disagreement this module
    /// exists to prevent, and the two are separate implementations.
    pub const UNSUPPORTED_EXAMPLES: &'static [&'static str] = &[
        "@team",
        "(.)photo",
        "(..)photo",
        "(...)photo",
        "(..)(..)photo",
    ];

    /// Whether uf serves a segment spelled this way.
    ///
    /// False for the two Next.js conventions uf has reserved without
    /// implementing. Refusing is the point: they used to be literals.
    #[must_use]
    pub const fn is_supported(&self) -> bool {
        !matches!(self, Self::Slot(_) | Self::Interception { .. })
    }

    /// Why uf refuses a directory spelled this way, or [`None`] when it
    /// serves it.
    ///
    /// One sentence for the reader and one for the author: which feature the
    /// spelling belongs to, and that the directory is refused rather than
    /// served as a URL — because being served as a URL is what used to happen,
    /// and a person who reads only "unsupported" would reasonably assume it
    /// was ignored.
    ///
    /// Here rather than on `RouterError` so `uf lint` says the same thing the
    /// build does. Two messages for one refusal is how a linter ends up
    /// disagreeing with the compiler about what is wrong.
    #[must_use]
    pub fn unsupported_reason(&self, segment: &str) -> Option<String> {
        match self {
            Self::Slot(_) => Some(format!(
                "`{segment}` is a parallel-route slot, and uf does not have parallel routes — a \
                 route here renders in one place, so there is nothing for a slot to render into. \
                 It is refused rather than served as the URL segment `/{segment}`, which is what \
                 it used to become. Rename the directory; a URL segment that really starts with \
                 `@` has no spelling in this grammar, so capture it with a `[param]`. \
                 https://github.com/ubugeeei-prod/uf/issues/267"
            )),
            Self::Interception { route, .. } => Some(format!(
                "`{segment}` is an intercepting route, and uf does not have interception — a \
                 navigation carries where it is going and not where it came from, so nothing here \
                 could match `{route}`. It is refused rather than served as the URL segment \
                 `/{segment}`, which is what it used to become. Move the route to the path it \
                 belongs at, or rename the directory. \
                 https://github.com/ubugeeei-prod/uf/issues/267"
            )),
            Self::Group | Self::Param(_) | Self::CatchAll(_) | Self::Literal(_) => None,
        }
    }
}

/// Classify one directory name from the router root.
///
/// Takes a single segment, not a path, for the reason
/// [`classify_reserved_file`] takes a file name: a caller that passed a path
/// would get an answer about a string no router ever classifies.
#[must_use]
pub fn classify_route_segment(segment: &str) -> RouteSegment<'_> {
    if let Some(name) = segment.strip_prefix('@') {
        return RouteSegment::Slot(name);
    }
    if let Some((marker, route)) = interception_marker(segment) {
        return RouteSegment::Interception { marker, route };
    }
    // After the interception test, and that order is the whole difference
    // between the two: a group *ends* in `)` and an interception marker is a
    // prefix with a route after it. `(.)` on its own is neither a marker nor a
    // convention anybody writes, and stays the group it has always been.
    if segment.starts_with('(') && segment.ends_with(')') {
        return RouteSegment::Group;
    }
    if let Some(name) = segment
        .strip_prefix("[...")
        .and_then(|name| name.strip_suffix(']'))
    {
        return RouteSegment::CatchAll(name);
    }
    if let Some(name) = segment
        .strip_prefix('[')
        .and_then(|name| name.strip_suffix(']'))
    {
        return RouteSegment::Param(name);
    }
    RouteSegment::Literal(segment)
}

/// The `(.)`-style prefix of `segment` and the route after it, if it has one.
///
/// One or more of `(.)`, `(..)` and `(...)`, which is every marker Next.js
/// defines — `(..)(..)` is two of them and not a fourth spelling — followed by
/// something for them to intercept. A marker with nothing after it is not an
/// interception, because there is no route named.
fn interception_marker(segment: &str) -> Option<(&str, &str)> {
    let mut consumed = 0;
    while let Some(open) = segment[consumed..].strip_prefix('(') {
        let Some(close) = open.find(')') else { break };
        let inner = &open[..close];
        if inner.is_empty() || inner.len() > 3 || !inner.bytes().all(|byte| byte == b'.') {
            break;
        }
        consumed += close + 2;
    }
    if consumed == 0 || consumed == segment.len() {
        return None;
    }
    Some((&segment[..consumed], &segment[consumed..]))
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
    fn a_loading_file_is_reserved_and_is_not_part_of_a_route() {
        // The fallback of the `<Suspense>` the router puts around a segment.
        // `_uf.loader.js` is not this file and never was — a loader is an
        // export of a page module — so the near-miss stays unknown.
        assert_eq!(recognized("_uf.loading.js").role, ReservedRole::Loading);
        assert_eq!(
            recognized("_uf.loading.web.js").variant,
            ReservedVariant::Web
        );
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Loading),
            "a fallback renders while the route is not there, so no route is built from one"
        );
        assert!(classify_reserved_file("_uf.loader.js").is_unknown());
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
    fn a_template_is_a_layout_that_remounts_and_is_part_of_a_route() {
        // The third of the three features ubugeeei-prod/uf#267 asked for, and
        // the cheapest: a reserved role, a table that nests the way layouts
        // already do, and a `key` on the element in `RouteView`.
        assert_eq!(recognized("_uf.template.js").role, ReservedRole::Template);
        assert_eq!(
            recognized("_uf.template.native.js").variant,
            ReservedVariant::Native
        );
        assert!(
            ReservedRole::route_parts().contains(&ReservedRole::Template),
            "a template composes a route the way a layout does"
        );
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
    fn the_three_segment_kinds_a_route_path_is_built_from() {
        assert_eq!(classify_route_segment("(marketing)"), RouteSegment::Group);
        assert_eq!(
            classify_route_segment("[slug]"),
            RouteSegment::Param("slug")
        );
        assert_eq!(
            classify_route_segment("[...path]"),
            RouteSegment::CatchAll("path")
        );
        assert_eq!(
            classify_route_segment("posts"),
            RouteSegment::Literal("posts")
        );
        for segment in ["(marketing)", "[slug]", "[...path]", "posts"] {
            assert!(classify_route_segment(segment).is_supported(), "{segment}");
        }
    }

    #[test]
    fn a_slot_is_named_rather_than_served_as_a_url() {
        // `@team` used to be the literal URL segment `/@team`, in both routers
        // and in the generated `RoutePath`. See ubugeeei-prod/uf#267.
        assert_eq!(classify_route_segment("@team"), RouteSegment::Slot("team"));
        assert!(!classify_route_segment("@team").is_supported());
        // Not a slot: the `@` has to start the segment, so a scoped-looking
        // name in the middle is an ordinary literal.
        assert_eq!(
            classify_route_segment("mail@example"),
            RouteSegment::Literal("mail@example")
        );
    }

    #[test]
    fn every_interception_marker_next_defines_is_recognized() {
        for (segment, marker) in [
            ("(.)photo", "(.)"),
            ("(..)photo", "(..)"),
            ("(...)photo", "(...)"),
            ("(..)(..)photo", "(..)(..)"),
        ] {
            assert_eq!(
                classify_route_segment(segment),
                RouteSegment::Interception {
                    marker,
                    route: "photo"
                },
                "{segment}"
            );
            assert!(!classify_route_segment(segment).is_supported(), "{segment}");
        }
    }

    #[test]
    fn a_group_is_still_a_group() {
        // The near-miss that made `(.)photo` a literal in the first place: the
        // test for a group is that the segment *ends* in `)`, which `(.)photo`
        // does not. Reading the parenthesis alone would have turned every
        // route group into an interception.
        for name in ["(marketing)", "(.)", "(..)", "(shop)", "(a.b)"] {
            assert_eq!(classify_route_segment(name), RouteSegment::Group, "{name}");
            assert!(classify_route_segment(name).is_supported(), "{name}");
        }
    }

    #[test]
    fn every_unsupported_example_is_one_of_the_two_kinds() {
        for segment in RouteSegment::UNSUPPORTED_EXAMPLES {
            let classified = classify_route_segment(segment);
            assert!(
                !classified.is_supported(),
                "{segment} is listed as unsupported and classifies as a route"
            );
            let reason = classified
                .unsupported_reason(segment)
                .unwrap_or_else(|| panic!("{segment} is refused without a reason"));
            // The refusal has to say it is a refusal. "Not supported" reads as
            // "ignored", and being quietly ignored is what this replaced.
            assert!(reason.contains("refused"), "{segment}: {reason}");
            assert!(reason.contains(segment), "{segment}: {reason}");
        }
    }

    #[test]
    fn a_segment_uf_serves_has_no_reason_to_refuse_it() {
        for segment in ["(marketing)", "[slug]", "[...path]", "posts"] {
            assert_eq!(
                classify_route_segment(segment).unsupported_reason(segment),
                None,
                "{segment}"
            );
        }
    }

    #[test]
    fn segment_classification_does_not_panic_on_odd_input() {
        for name in [
            "",
            "(",
            ")",
            "@",
            "(.",
            "(....)x",
            "()x",
            "[",
            "[...]",
            "(.)(",
            "\u{1f600}",
        ] {
            let _ = classify_route_segment(name);
        }
    }

    #[test]
    fn classification_does_not_panic_on_odd_input() {
        for name in ["", "_uf.", "_uf..js", "_uf....js", "_uf.\u{1f600}.js"] {
            let _ = classify_reserved_file(name);
        }
    }
}
