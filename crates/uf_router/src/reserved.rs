//! The names `uf` reserves inside the router root: the `$*` files, and the
//! directory spellings that are not ordinary URL segments.
//!
//! `uf` reserves the `$` prefix inside the router root so a project cannot
//! accidentally shadow a framework file. A reserved name is
//! `$<role>[.<variant>].js`, where the role is what the file does and the
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
//! allowed to disagree" — while `$not-found` was in that table and not in
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
//! So [`RouteSegment::Slot`] and [`RouteSegment::Interception`] became names in
//! this grammar, and both routers refused them by name rather than serving
//! them.
//!
//! A slot is a route now. [`RouteSegment::Slot`] contributes no URL segment —
//! it is a `(group)` in that one respect — and everything under it is a route
//! *into a named place* rather than into the one page a URL has: the layout of
//! the segment that declares the slot receives it as a prop beside `children`.
//! [`ReservedRole::Default`] is the other half, and it is the reason a slot
//! can be a route at all: a URL that says nothing about a slot still has to
//! leave something in it.
//!
//! [`RouteSegment::Interception`] is a route now too, and only inside a slot.
//! An interception renders a route *somewhere other than where its path says*,
//! and "somewhere" is a named place — which is what a slot is. So
//! `app/feed/@modal/(.)photo/$page.js` is what a client navigation from a page
//! the slot is on puts in the `modal` slot when it reaches `/feed/photo`, and
//! every other way of arriving at that URL — the address bar, a reload, a
//! shared link, a prerender — renders `app/feed/photo/$page.js` instead.
//! Outside a slot the spelling stays refused, because there would be no second
//! place to render into and the only thing left to do with it would be to
//! serve it as a URL, which is what it used to do by accident.
//!
//! The marker says how far to climb, in URL segments, from the directory the
//! interception sits in: [`InterceptionClimb`]. See ubugeeei-prod/uf#267.

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
    /// Renders in a slot the URL says nothing about.
    ///
    /// A slot — `app/dashboard/@team/` — is matched against the same URL its
    /// segment is, and a URL is free to address one slot and not another:
    /// `/dashboard/members` has a page for `@team` and nothing at all for
    /// `@analytics`. Without this file the second slot would be empty, so a
    /// link into one half of a two-slot layout would produce half a page, and
    /// arriving at that URL directly would produce it too.
    ///
    /// It is a page in every way that matters — it is a component and it may
    /// be Markdown — except that no path leads to it, which is why it is not
    /// one of [`route_parts`](ReservedRole::route_parts) for the same reason
    /// [`NotFound`](ReservedRole::NotFound) is not.
    ///
    /// Only inside a slot. Next.js also reads a `default.js` beside an
    /// ordinary page, for the `children` slot on a hard navigation; uf's
    /// `children` is the page the URL matched and there is no case where it is
    /// missing, so a `$default.js` outside a slot would be a file the
    /// router never reaches. `discover_routes` refuses it rather than leaving
    /// it there to be wondered about.
    Default,
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
    /// A segment's `$loading.js` is the fallback of a `<Suspense>` around
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
    /// Startup and request instrumentation at the application root.
    Instrumentation,
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
            Self::Default => "default",
            Self::Middleware => "middleware",
            Self::NotFound => "not-found",
            Self::Error => "error",
            Self::Loading => "loading",
            Self::Route => "route",
            Self::Story => "story",
            Self::Instrumentation => "instrumentation",
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
    pub const fn all() -> [Self; 11] {
        [
            Self::Layout,
            Self::Template,
            Self::Page,
            Self::Default,
            Self::Middleware,
            Self::NotFound,
            Self::Error,
            Self::Loading,
            Self::Route,
            Self::Story,
            Self::Instrumentation,
        ]
    }

    /// The roles a rendered route is built from.
    ///
    /// A `route` answers a request rather than rendering, a `story` names a
    /// state of a component, a `not-found` is reached by no path, a `default`
    /// stands in for a slot the URL did not address, an `error` renders
    /// instead of the route rather than as part of it, and a `loading` renders
    /// while it is not there yet — so none of the six composes a route, though
    /// all six are reserved names.
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
            "default" => Ok(Self::Default),
            "middleware" => Ok(Self::Middleware),
            "not-found" => Ok(Self::NotFound),
            "error" => Ok(Self::Error),
            "loading" => Ok(Self::Loading),
            "route" => Ok(Self::Route),
            "story" => Ok(Self::Story),
            "instrumentation" => Ok(Self::Instrumentation),
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
    /// Browser instrumentation; never a route entry.
    Client,
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
            Self::Client => Some("client"),
        }
    }

    /// Whether this variant can be the file the router resolves for a target.
    ///
    /// A platform variant is a route for the target that selects it; a
    /// colocated test is a companion to a route and never is.
    #[must_use]
    pub const fn is_route_entry(self) -> bool {
        !matches!(self, Self::Test | Self::Client)
    }

    /// Every variant, in declaration order.
    #[must_use]
    pub const fn all() -> [Self; 7] {
        [
            Self::Default,
            Self::Native,
            Self::Ios,
            Self::Android,
            Self::Web,
            Self::Test,
            Self::Client,
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
            "client" => Ok(Self::Client),
            _ => Err(()),
        }
    }
}

/// A recognized `$*` file.
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
            Some(variant) => format!("${}.{variant}.js", self.role.as_str()),
            None => format!("${}.js", self.role.as_str()),
        }
    }
}

/// How a file name relates to the reserved grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedName {
    /// The name does not use the `$` prefix, so the project owns it.
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

impl ReservedRole {
    /// What this role may be written in.
    ///
    /// A page may be Markdown; nothing else may. A layout, a middleware and a
    /// route handler are code — a layout wraps a subtree, a route handler
    /// exports functions — and Markdown is not something either can be. The
    /// two lists are `crate::PAGE_EXTENSIONS` and `crate::MODULE_EXTENSIONS`,
    /// which the build's own router uses under the same names.
    #[must_use]
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            // A `not-found` is a page in every way that matters, so it is one
            // here too, and so is a `default`: both render in place of a page
            // and neither is handed anything Markdown cannot receive.
            Self::Page | Self::NotFound | Self::Default => &crate::PAGE_EXTENSIONS,
            // A story names a rendered state of a component, so it is code
            // for the same reason a layout is.
            // A template is a layout that remounts, and a story names a
            // rendered state of a component: both are code for the same reason
            // a layout is.
            Self::Layout
            | Self::Middleware
            | Self::Error
            | Self::Loading
            | Self::Route
            | Self::Story
            | Self::Instrumentation
            | Self::Template => &crate::MODULE_EXTENSIONS,
        }
    }
}

/// Classify a bare file name against the reserved grammar.
///
/// Takes a file name, not a path: callers already have one, and accepting a
/// path here would invite a caller to pass an unnormalized one and get a
/// different answer than the router did.
///
/// Every extension the build's own router accepts is a spelling of the same
/// name — `.js`, `.jsx` and `.mdx` — so `$page.mdx` is a page and not a
/// misspelling of one. Reading only `.js` here made `router/reserved-files`
/// report every `.mdx` page in a documentation site as a name uf does not
/// define, which is the same drift as ubugeeei-prod/uf#437 in the rule that
/// exists to catch drift.
#[must_use]
pub fn classify_reserved_file(file_name: &str) -> ReservedName {
    let Some(rest) = file_name.strip_prefix("$") else {
        return ReservedName::NotReserved;
    };
    let Some((rest, extension)) = crate::PAGE_EXTENSIONS.iter().find_map(|extension| {
        rest.strip_suffix(extension)
            .map(|stripped| (stripped, *extension))
    }) else {
        return ReservedName::Unknown;
    };

    let mut segments = rest.split('.');
    let Some(Ok(role)) = segments.next().map(ReservedRole::from_str) else {
        return ReservedName::Unknown;
    };
    // Which extensions a role may be written in is the role's own fact.
    // `$layout.mdx` reads as a layout and is not one — a layout is a
    // component and Markdown cannot be one — so the build's router would never
    // find it and `router/reserved-files` would never say why. Naming it
    // unknown is what makes that file's silence audible.
    if !role.extensions().contains(&extension) {
        return ReservedName::Unknown;
    }
    let variant = match segments.next() {
        None => ReservedVariant::Default,
        Some(segment) => match ReservedVariant::from_str(segment) {
            Ok(variant) => variant,
            Err(()) => return ReservedName::Unknown,
        },
    };
    if segments.next().is_some() {
        // `$page.native.test.js` and friends: one variant, not a stack of them.
        return ReservedName::Unknown;
    }

    if variant == ReservedVariant::Client && role != ReservedRole::Instrumentation {
        return ReservedName::Unknown;
    }
    if role == ReservedRole::Instrumentation
        && !matches!(
            variant,
            ReservedVariant::Default | ReservedVariant::Client | ReservedVariant::Test
        )
    {
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
    /// There has to be some: `app/docs/[...path]/` does not serve `/docs`.
    CatchAll(&'a str),
    /// `[[...path]]` — captures the rest of the URL, which may be nothing,
    /// under the borrowed name: `app/docs/[[...path]]/` serves `/docs` with
    /// an empty list as well as `/docs/a/b`.
    OptionalCatchAll(&'a str),
    /// An ordinary URL segment, spelled exactly as the directory is.
    Literal(&'a str),
    /// `@team` — a parallel-route slot, under the borrowed name.
    ///
    /// A slot is a route that matches into a *named place* rather than into
    /// the one page a URL has: the segment holding `@team` gives its layout a
    /// `team` prop beside `children`, and the two are matched against the same
    /// URL independently. So the directory contributes no URL segment, exactly
    /// as a [`Group`](RouteSegment::Group) does — `app/dashboard/@team/
    /// members/$page.js` is what `/dashboard/members` puts in the `team`
    /// slot, and it is not a second page at that path.
    ///
    /// A URL is free to address one slot and not another, which is what
    /// [`ReservedRole::Default`] answers.
    Slot(&'a str),
    /// `(.)photo`, `(..)photo`, `(...)photo`, `(..)(..)photo` — an intercepting
    /// route, which uf serves inside a `@slot` and nowhere else.
    ///
    /// Interception renders a route *from within a segment* and leaves the URL
    /// alone: a client navigation from a page the slot is on renders what the
    /// directory holds in that slot, and every other way of reaching the URL —
    /// a reload, a shared link, a prerender — renders the ordinary page. The URL
    /// it intercepts is [`InterceptionClimb`] applied to the route path of the
    /// directory it sits in, with `route` appended.
    ///
    /// Refused everywhere, slot or not, when the marker climbs nowhere —
    /// `(.)(.)photo`, `(....)photo` — or when what follows it is not a URL
    /// segment: `(.)(gallery)` and `(.)@photo` name no path to intercept.
    /// [`interception_climb`](RouteSegment::interception_climb) is [`None`] for
    /// all of those.
    Interception {
        /// The `(.)`-style prefix, as written.
        marker: &'a str,
        /// What follows it — the segment being intercepted.
        route: &'a str,
    },
}

impl<'a> RouteSegment<'a> {
    /// One directory name per spelling uf refuses everywhere.
    ///
    /// Here so that `tests/reserved_names.rs` can hold the build router to the
    /// same list, the way it already holds it to [`ReservedRole`]. A spelling
    /// one router refuses and the other serves is the disagreement this module
    /// exists to prevent, and the two are separate implementations.
    ///
    /// Every entry is spelled like an interception and cannot be one: a marker
    /// that climbs nowhere, or a marker followed by something that is not a URL
    /// segment. The spellings that *are* interceptions moved to
    /// [`SLOT_ONLY_EXAMPLES`](RouteSegment::SLOT_ONLY_EXAMPLES) when
    /// interception became a route.
    pub const UNSUPPORTED_EXAMPLES: &'static [&'static str] = &[
        "(.)(.)photo",
        "(.)(..)photo",
        "(...)(..)photo",
        "(....)photo",
        "(.)(gallery)",
        "(.)@photo",
    ];

    /// One directory name per spelling uf serves inside a `@slot` and refuses
    /// outside one.
    ///
    /// A separate list from [`UNSUPPORTED_EXAMPLES`](RouteSegment::UNSUPPORTED_EXAMPLES)
    /// because the two refusals are different sentences, and a reader who gets
    /// the wrong one is told to rename a directory that was spelled correctly.
    pub const SLOT_ONLY_EXAMPLES: &'static [&'static str] = &[
        "(.)photo",
        "(..)photo",
        "(...)photo",
        "(..)(..)photo",
        "(..)(..)(..)photo",
        "(.)[id]",
    ];

    /// Whether uf serves a segment spelled this way, given where it sits.
    ///
    /// `inside_slot` is the whole of the difference for an interception: it is
    /// a route in a slot and a refusal anywhere else. Every other spelling
    /// answers the same either way, and passing the flag rather than asking two
    /// questions is what keeps one caller from checking only the first.
    ///
    /// A well-spelled interception inside a slot can still be in the wrong
    /// place — climbing past the router root — and that is a question about
    /// the path rather than the name, answered by
    /// [`climb_reason`](RouteSegment::climb_reason) for the callers that have
    /// the path.
    #[must_use]
    pub fn is_supported(&self, inside_slot: bool) -> bool {
        match self {
            Self::Interception { .. } => inside_slot && self.interception_climb().is_some(),
            Self::Group
            | Self::Param(_)
            | Self::CatchAll(_)
            | Self::OptionalCatchAll(_)
            | Self::Literal(_)
            | Self::Slot(_) => true,
        }
    }

    /// How far this segment climbs before it matches, when it is an
    /// interception uf reads: a marker that climbs, with a URL segment after it.
    #[must_use]
    pub fn interception_climb(&self) -> Option<InterceptionClimb> {
        match self {
            Self::Interception { marker, route } if names_a_url_segment(route) => {
                interception_climb(marker)
            }
            _ => None,
        }
    }

    /// Why an interception `depth` URL segments below the router root climbs
    /// past it, or [`None`] when it does not.
    ///
    /// `depth` is how many URL segments the directory holding the interception
    /// contributes, after any interception above it has been applied — so a
    /// caller counts what [`InterceptionClimb::remaining`] counts, and this
    /// only supplies the sentence. Here rather than on `RouterError` for the
    /// reason [`unsupported_reason`](RouteSegment::unsupported_reason) is: the
    /// build and `uf lint` refuse the same directory with the same words.
    #[must_use]
    pub fn climb_reason(&self, segment: &str, depth: usize) -> Option<String> {
        let climb = self.interception_climb()?;
        if climb.remaining(depth).is_some() {
            return None;
        }
        let InterceptionClimb::Up(levels) = climb else {
            return None;
        };
        let climbs = if levels == 1 {
            "one level".to_owned()
        } else {
            format!("{levels} levels")
        };
        let sits = match depth {
            0 => "at the router root".to_owned(),
            1 => "one level below it".to_owned(),
            _ => format!("{depth} levels below it"),
        };
        Some(format!(
            "`{segment}` climbs {climbs} from the directory it is in, which is {sits}, so the URL \
             it intercepts would be above the router root, and there is no such URL. It is \
             refused rather than read as a climb to the root. Remove a `(..)`, or write `(...)` \
             to intercept from the router root. https://github.com/ubugeeei-prod/uf/issues/267"
        ))
    }

    /// The slot this segment names, if it names one.
    ///
    /// Here rather than at each `matches!` site because three callers ask the
    /// same question — discovery skips the segment, the path builder drops it,
    /// and the refusal below needs to know a slot from a literal — and a slot
    /// that one of them read differently from the others is a page rendered in
    /// a place no other part of uf agrees about.
    #[must_use]
    pub const fn slot(&self) -> Option<&'a str> {
        match self {
            Self::Slot(name) => Some(name),
            _ => None,
        }
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
    ///
    /// For an interception this is the refusal that holds everywhere, and it is
    /// about the spelling. Where a correctly spelled one is placed is a separate
    /// question with separate sentences: [`outside_slot_reason`] and
    /// [`climb_reason`].
    ///
    /// [`outside_slot_reason`]: RouteSegment::outside_slot_reason
    /// [`climb_reason`]: RouteSegment::climb_reason
    #[must_use]
    pub fn unsupported_reason(&self, segment: &str) -> Option<String> {
        let Self::Interception { marker, route } = self else {
            return None;
        };
        if interception_climb(marker).is_none() {
            return Some(format!(
                "`{segment}` is spelled like an intercepting route and `{marker}` is not a marker \
                 uf reads. The markers are `(.)` for the level the directory is at, `(..)` for one \
                 above it — repeated for each further level — and `(...)` for the router root. \
                 It is refused rather than served as the URL segment `/{segment}`, which is what \
                 it used to become. Spell the marker as one of those and put the directory inside \
                 a `@slot`, or rename it to the literal segment `{route}`. \
                 https://github.com/ubugeeei-prod/uf/issues/267"
            ));
        }
        if !names_a_url_segment(route) {
            return Some(format!(
                "`{segment}` is spelled like an intercepting route, and `{route}` after the marker \
                 is not a URL segment, so there is no path for it to intercept: an interception \
                 names the segment it stands in for, the way `{marker}photo` and `{marker}[id]` \
                 do. It is refused rather than served as the URL segment `/{segment}`, which is \
                 what it used to become. Put a segment name after the marker, or rename the \
                 directory. https://github.com/ubugeeei-prod/uf/issues/267"
            ));
        }
        None
    }

    /// Why uf refuses this directory *here*, when it would serve the same
    /// spelling inside a `@slot`.
    ///
    /// Separate from [`unsupported_reason`](RouteSegment::unsupported_reason)
    /// because the two are different answers: that one says the spelling is
    /// wrong, and this one says the place is. Telling an author to rename a
    /// directory they spelled correctly is the worse of the two mistakes, so
    /// the caller has to have decided which it is.
    #[must_use]
    pub fn outside_slot_reason(&self, segment: &str) -> Option<String> {
        let Self::Interception { route, .. } = self else {
            return None;
        };
        self.interception_climb()?;
        Some(format!(
            "`{segment}` is an intercepting route, and an intercepting route renders into a \
             `@slot`: it is what a client navigation shows in a named place instead of the page \
             its URL names, and outside a slot there is no named place for it to show in. It is \
             refused rather than served as the URL segment `/{segment}`, which is what it used to \
             become. Move it inside a slot directory beside the layout that renders the slot, or \
             rename the directory to the literal segment `{route}`. \
             https://github.com/ubugeeei-prod/uf/issues/267"
        ))
    }
}

/// Whether what follows an interception marker is a segment a URL has — a
/// literal, a `[param]`, a `[...rest]` or a `[[...rest]]`.
///
/// A `(group)` or a `@slot` after the marker contributes no URL segment of its
/// own, so there is nothing for the interception to stand in for, and reading
/// `(.)(gallery)` as "intercept the directory I am in" would be a meaning
/// nobody wrote down.
fn names_a_url_segment(route: &str) -> bool {
    matches!(
        classify_route_segment(route),
        RouteSegment::Literal(_)
            | RouteSegment::Param(_)
            | RouteSegment::CatchAll(_)
            | RouteSegment::OptionalCatchAll(_)
    )
}

/// How far an interception climbs before it matches.
///
/// Counted in *URL* segments from the directory the interception sits in —
/// which, for the usual shape of an interception directly inside its slot, is
/// the segment that declares the slot. Directories are not the unit: a
/// `(group)` and a `@slot` contribute no URL segment, so they are not levels to
/// climb past, and `app/feed/@modal/(..)photo` intercepts `/photo` rather than
/// `/feed/photo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterceptionClimb {
    /// `(.)` is zero, `(..)` is one, `(..)(..)` is two, and so on.
    Up(usize),
    /// `(...)`: the router root, however deep the directory is.
    ///
    /// Its own variant rather than a large [`Up`](InterceptionClimb::Up),
    /// because "from the root" is what the author wrote and clamping a number
    /// at the root would silently accept `(..)` repeated past it.
    Root,
}

impl InterceptionClimb {
    /// How many of the `depth` URL segments above an interception are left
    /// once it has climbed, or [`None`] when it climbs past the router root.
    ///
    /// One function for a question three places ask — the URL an interception
    /// stands in for, the router refusing a climb it cannot make, and
    /// `uf lint` saying the same from a file path — because the answer is
    /// arithmetic, and arithmetic written three times is what comes out
    /// different the third time.
    #[must_use]
    pub const fn remaining(self, depth: usize) -> Option<usize> {
        match self {
            Self::Up(levels) => depth.checked_sub(levels),
            Self::Root => Some(0),
        }
    }
}

/// Read a marker as a climb, or [`None`] when it is not one uf reads.
///
/// `(.)`, `(..)` repeated, and `(...)` alone. The combinations that are not
/// on that list — `(.)(.)`, `(...)(..)`, `(....)` — parse as a marker and mean
/// nothing, which is why this is separate from
/// [`interception_marker`]: one says "this is the shape of a marker", the
/// other says "and this is what it does".
#[must_use]
pub fn interception_climb(marker: &str) -> Option<InterceptionClimb> {
    let mut rest = marker;
    let mut up = 0usize;
    while let Some(after) = rest.strip_prefix('(') {
        let close = after.find(')')?;
        let inner = &after[..close];
        rest = &after[close + 1..];
        match inner {
            // Only as the whole marker: `(.)` says "the level this directory
            // is at", and there is nothing for a second marker to say after it.
            "." => return (up == 0 && rest.is_empty()).then_some(InterceptionClimb::Up(0)),
            ".." => up += 1,
            // Same: the root is where the climb ends, so nothing may follow.
            "..." => return (up == 0 && rest.is_empty()).then_some(InterceptionClimb::Root),
            _ => return None,
        }
    }
    if !rest.is_empty() || up == 0 {
        return None;
    }
    Some(InterceptionClimb::Up(up))
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
    // Before the other two bracket spellings, both of which it also matches:
    // read as `[...name]` it would be a catch-all named `[...name]`'s inside.
    if let Some(name) = segment
        .strip_prefix("[[...")
        .and_then(|name| name.strip_suffix("]]"))
    {
        return RouteSegment::OptionalCatchAll(name);
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
/// One or more parenthesised runs of dots, followed by something for them to
/// intercept. Which runs *mean* anything is [`interception_climb`]'s question
/// and deliberately not this one: `(....)photo` is the shape of an
/// interception written by somebody who miscounted, and reading it as a
/// literal URL segment is how that miscount becomes a page at `/(....)photo`.
/// This says "an interception was intended"; the climb says whether it can be
/// honoured; and the two together are what produces a sentence about the
/// marker rather than a route nobody meant.
///
/// A marker with nothing after it is not an interception, because there is no
/// route named — `(.)` alone stays the `(group)` it has always been.
fn interception_marker(segment: &str) -> Option<(&str, &str)> {
    let mut consumed = 0;
    while let Some(open) = segment[consumed..].strip_prefix('(') {
        let Some(close) = open.find(')') else { break };
        let inner = &open[..close];
        if inner.is_empty() || !inner.bytes().all(|byte| byte == b'.') {
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
        assert_eq!(recognized("$story.js").role, ReservedRole::Story);
        assert_eq!(
            recognized("$story.native.js").variant,
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
        assert_eq!(recognized("$not-found.js").role, ReservedRole::NotFound);
        assert_eq!(
            recognized("$not-found.web.js").variant,
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
        // `ReservedRole::Error`. `$forbidden.js` and `$unauthorized.js`
        // are therefore names uf does not define, and the linter says so.
        assert_eq!(recognized("$error.js").role, ReservedRole::Error);
        assert_eq!(
            recognized("$error.native.js").variant,
            ReservedVariant::Native
        );
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Error),
            "an error boundary renders instead of a route, not as part of one"
        );
        for name in ["$forbidden.js", "$unauthorized.js", "$global-error.js"] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn a_loading_file_is_reserved_and_is_not_part_of_a_route() {
        // The fallback of the `<Suspense>` the router puts around a segment.
        // `$loader.js` is not this file and never was — a loader is an
        // export of a page module — so the near-miss stays unknown.
        assert_eq!(recognized("$loading.js").role, ReservedRole::Loading);
        assert_eq!(recognized("$loading.web.js").variant, ReservedVariant::Web);
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Loading),
            "a fallback renders while the route is not there, so no route is built from one"
        );
        assert!(classify_reserved_file("$loader.js").is_unknown());
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
        assert_eq!(recognized("$template.js").role, ReservedRole::Template);
        assert_eq!(
            recognized("$template.native.js").variant,
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
            recognized("$layout.js"),
            ReservedFile {
                role: ReservedRole::Layout,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("$page.js"),
            ReservedFile {
                role: ReservedRole::Page,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("$middleware.js"),
            ReservedFile {
                role: ReservedRole::Middleware,
                variant: ReservedVariant::Default
            }
        );
    }

    #[test]
    fn platform_variants_are_recognized() {
        for (name, variant) in [
            ("$page.native.js", ReservedVariant::Native),
            ("$page.ios.js", ReservedVariant::Ios),
            ("$page.android.js", ReservedVariant::Android),
            ("$page.web.js", ReservedVariant::Web),
        ] {
            assert_eq!(recognized(name).variant, variant, "{name}");
            assert_eq!(recognized(name).role, ReservedRole::Page, "{name}");
        }
    }

    #[test]
    fn colocated_tests_are_recognized() {
        assert_eq!(recognized("$page.test.js").variant, ReservedVariant::Test);
        assert_eq!(recognized("$layout.test.js").variant, ReservedVariant::Test);
    }

    #[test]
    fn every_role_and_variant_pair_round_trips_through_its_file_name() {
        for role in ReservedRole::all() {
            for variant in ReservedVariant::all() {
                let file = ReservedFile { role, variant };
                let allowed = if role == ReservedRole::Instrumentation {
                    matches!(
                        variant,
                        ReservedVariant::Default | ReservedVariant::Client | ReservedVariant::Test
                    )
                } else {
                    variant != ReservedVariant::Client
                };
                if allowed {
                    assert_eq!(recognized(&file.file_name()), file);
                } else {
                    assert!(classify_reserved_file(&file.file_name()).is_unknown());
                }
            }
        }
    }

    #[test]
    fn every_variant_but_a_colocated_test_can_be_a_route_entry() {
        for variant in ReservedVariant::all() {
            assert_eq!(
                variant.is_route_entry(),
                !matches!(variant, ReservedVariant::Test | ReservedVariant::Client),
                "{variant:?}"
            );
        }
    }

    #[test]
    fn an_unknown_role_is_rejected() {
        for name in ["$handler.js", "$loader.js", "$js", "$page", "$PAGE.js"] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn an_unknown_variant_is_rejected() {
        for name in ["$page.server.js", "$page.windows.js", "$page.NATIVE.js"] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn variants_do_not_stack() {
        assert!(classify_reserved_file("$page.native.test.js").is_unknown());
        assert!(classify_reserved_file("$page.ios.android.js").is_unknown());
    }

    #[test]
    fn a_reserved_prefix_in_an_extension_nothing_runs_is_rejected() {
        for name in ["$page.ts", "$page.js.flow", "$page", "$pagejs"] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    /// ubugeeei-prod/uf#437, #386: the build's router has accepted `.jsx` and
    /// `.mdx` since it was written, and this said they were names uf does not
    /// define.
    #[test]
    fn a_page_is_reserved_in_every_extension_the_build_runs() {
        for name in ["$page.js", "$page.jsx", "$page.mdx"] {
            assert!(
                !classify_reserved_file(name).is_unknown(),
                "{name} should be a page"
            );
        }
    }

    /// And a role that cannot be Markdown is not, because a `$layout.mdx`
    /// the build would never load should be reported rather than accepted into
    /// a silence.
    #[test]
    fn only_a_page_may_be_markdown() {
        for name in ["$layout.mdx", "$middleware.mdx", "$route.mdx"] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} is not something the build can load"
            );
        }
        for name in ["$layout.jsx", "$middleware.jsx", "$route.jsx"] {
            assert!(
                !classify_reserved_file(name).is_unknown(),
                "{name} is a module the build loads"
            );
        }
        // A `not-found` is a page, so it may be.
        assert!(!classify_reserved_file("$not-found.mdx").is_unknown());
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
        // Not a `[param]` named `[...path]`, which is what it was read as
        // before it meant anything.
        assert_eq!(
            classify_route_segment("[[...path]]"),
            RouteSegment::OptionalCatchAll("path")
        );
        assert_eq!(
            classify_route_segment("posts"),
            RouteSegment::Literal("posts")
        );
        for segment in ["(marketing)", "[slug]", "[...path]", "[[...path]]", "posts"] {
            // Everywhere: only an interception's answer depends on where it is.
            assert!(
                classify_route_segment(segment).is_supported(false),
                "{segment}"
            );
            assert!(
                classify_route_segment(segment).is_supported(true),
                "{segment}"
            );
        }
    }

    #[test]
    fn a_slot_is_a_route_that_contributes_no_url_segment() {
        // `@team` used to be the literal URL segment `/@team`, in both routers
        // and in the generated `RoutePath`; then it was refused by name; now it
        // is a slot uf serves. See ubugeeei-prod/uf#267.
        assert_eq!(classify_route_segment("@team"), RouteSegment::Slot("team"));
        assert!(classify_route_segment("@team").is_supported(false));
        assert_eq!(classify_route_segment("@team").slot(), Some("team"));
        // Not a slot: the `@` has to start the segment, so a scoped-looking
        // name in the middle is an ordinary literal.
        assert_eq!(
            classify_route_segment("mail@example"),
            RouteSegment::Literal("mail@example")
        );
        assert_eq!(classify_route_segment("mail@example").slot(), None);
    }

    #[test]
    fn a_default_file_is_reserved_and_is_not_part_of_a_route() {
        // What a slot renders when the URL addresses the other one. A page in
        // every way but the path that leads to it, which is why it takes the
        // page extensions and is not one of `route_parts`.
        assert_eq!(recognized("$default.js").role, ReservedRole::Default);
        assert_eq!(
            recognized("$default.native.js").variant,
            ReservedVariant::Native
        );
        assert!(!classify_reserved_file("$default.mdx").is_unknown());
        assert!(
            !ReservedRole::route_parts().contains(&ReservedRole::Default),
            "a default stands in for a slot's page rather than composing a route"
        );
    }

    #[test]
    fn every_interception_marker_next_defines_is_recognized() {
        for (segment, marker, climb) in [
            ("(.)photo", "(.)", InterceptionClimb::Up(0)),
            ("(..)photo", "(..)", InterceptionClimb::Up(1)),
            ("(...)photo", "(...)", InterceptionClimb::Root),
            ("(..)(..)photo", "(..)(..)", InterceptionClimb::Up(2)),
            (
                "(..)(..)(..)photo",
                "(..)(..)(..)",
                InterceptionClimb::Up(3),
            ),
        ] {
            let classified = classify_route_segment(segment);
            assert_eq!(
                classified,
                RouteSegment::Interception {
                    marker,
                    route: "photo"
                },
                "{segment}"
            );
            assert_eq!(classified.interception_climb(), Some(climb), "{segment}");
            // A route in a slot, and a refusal anywhere else. Both directions,
            // because the whole of the feature is the difference between them.
            assert!(classified.is_supported(true), "{segment}");
            assert!(!classified.is_supported(false), "{segment}");
            assert_eq!(classified.unsupported_reason(segment), None, "{segment}");
            let outside = classified
                .outside_slot_reason(segment)
                .unwrap_or_else(|| panic!("{segment} is refused outside a slot without a reason"));
            assert!(outside.contains("`@slot`"), "{segment}: {outside}");
        }
    }

    #[test]
    fn a_marker_that_parses_and_climbs_nowhere_is_refused_everywhere() {
        // The shape of an interception written by somebody who miscounted.
        // Reading these as literals is how `/(....)photo` becomes a page, so
        // they classify as interceptions with no climb and are refused in a
        // slot as well as outside one.
        for segment in [
            "(.)(.)photo",
            "(.)(..)photo",
            "(...)(..)photo",
            "(....)photo",
        ] {
            let classified = classify_route_segment(segment);
            assert!(
                matches!(classified, RouteSegment::Interception { .. }),
                "{segment} classified as {classified:?}"
            );
            assert_eq!(classified.interception_climb(), None, "{segment}");
            assert!(!classified.is_supported(true), "{segment}");
            assert!(!classified.is_supported(false), "{segment}");
            // The refusal is about the marker, so `outside_slot_reason` — which
            // is about the *place* — has nothing to say about it.
            assert_eq!(classified.outside_slot_reason(segment), None, "{segment}");
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
            assert!(classify_route_segment(name).is_supported(false), "{name}");
        }
    }

    #[test]
    fn every_unsupported_example_is_an_interception() {
        for segment in RouteSegment::UNSUPPORTED_EXAMPLES {
            let classified = classify_route_segment(segment);
            assert!(
                !classified.is_supported(true),
                "{segment} is listed as unsupported and classifies as a route even in a slot"
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
    fn every_slot_only_example_is_a_route_in_a_slot_and_a_refusal_outside_one() {
        for segment in RouteSegment::SLOT_ONLY_EXAMPLES {
            let classified = classify_route_segment(segment);
            assert!(classified.is_supported(true), "{segment}");
            assert!(!classified.is_supported(false), "{segment}");
            // And it is not on the other list, because a reader who is told to
            // rename a correctly spelled directory has been told the wrong
            // thing.
            assert!(
                !RouteSegment::UNSUPPORTED_EXAMPLES.contains(segment),
                "{segment} is on both lists"
            );
            let reason = classified
                .outside_slot_reason(segment)
                .unwrap_or_else(|| panic!("{segment} is refused outside a slot without a reason"));
            assert!(reason.contains(segment), "{segment}: {reason}");
        }
    }

    #[test]
    fn a_marker_with_no_url_segment_after_it_is_refused_everywhere() {
        // `(.)(gallery)` and `(.)@photo` are spelled like interceptions and name
        // no segment to stand in for. Reading them as "intercept the directory
        // I am in" would be a meaning nobody wrote, so they are refused in a
        // slot as well as outside one, with the sentence about the spelling.
        for (segment, route) in [("(.)(gallery)", "(gallery)"), ("(.)@photo", "@photo")] {
            let classified = classify_route_segment(segment);
            assert_eq!(
                classified,
                RouteSegment::Interception {
                    marker: "(.)",
                    route
                },
                "{segment}"
            );
            assert_eq!(classified.interception_climb(), None, "{segment}");
            assert!(!classified.is_supported(true), "{segment}");
            let reason = classified
                .unsupported_reason(segment)
                .unwrap_or_else(|| panic!("{segment} is refused without a reason"));
            assert!(reason.contains("not a URL segment"), "{segment}: {reason}");
            assert_eq!(classified.outside_slot_reason(segment), None, "{segment}");
        }
    }

    #[test]
    fn an_interception_may_stand_in_for_a_parameter() {
        assert_eq!(
            classify_route_segment("(.)[id]"),
            RouteSegment::Interception {
                marker: "(.)",
                route: "[id]"
            }
        );
        assert_eq!(
            classify_route_segment("(..)[...rest]").interception_climb(),
            Some(InterceptionClimb::Up(1))
        );
    }

    #[test]
    fn a_climb_counts_url_segments_and_stops_at_the_router_root() {
        assert_eq!(InterceptionClimb::Up(0).remaining(2), Some(2));
        assert_eq!(InterceptionClimb::Up(1).remaining(2), Some(1));
        assert_eq!(InterceptionClimb::Up(2).remaining(2), Some(0));
        assert_eq!(InterceptionClimb::Up(2).remaining(1), None);
        // `(...)` is the root from anywhere, the root included.
        assert_eq!(InterceptionClimb::Root.remaining(0), Some(0));
        assert_eq!(InterceptionClimb::Root.remaining(5), Some(0));
    }

    #[test]
    fn a_climb_past_the_router_root_is_refused_with_a_sentence_of_its_own() {
        let one = classify_route_segment("(..)photo");
        assert_eq!(one.climb_reason("(..)photo", 1), None);
        let at_root = one
            .climb_reason("(..)photo", 0)
            .expect("one level up from the router root is nowhere");
        assert!(at_root.contains("router root"), "{at_root}");
        assert!(at_root.contains("refused"), "{at_root}");
        let two = classify_route_segment("(..)(..)photo")
            .climb_reason("(..)(..)photo", 1)
            .expect("two levels up from one is nowhere");
        assert!(two.contains("2 levels"), "{two}");
        // `(...)` never climbs past the root, and a segment that is not an
        // interception has no climb to refuse.
        assert_eq!(
            classify_route_segment("(...)photo").climb_reason("(...)photo", 0),
            None
        );
        assert_eq!(
            classify_route_segment("photo").climb_reason("photo", 0),
            None
        );
    }

    #[test]
    fn a_segment_uf_serves_has_no_reason_to_refuse_it() {
        for segment in [
            "(marketing)",
            "[slug]",
            "[...path]",
            "[[...path]]",
            "posts",
            "@team",
        ] {
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
            "[[...]]",
            "[[...",
            "[[...x]",
            "(.)(",
            "\u{1f600}",
        ] {
            let _ = classify_route_segment(name);
        }
    }

    #[test]
    fn classification_does_not_panic_on_odd_input() {
        for name in ["", "$", "$.js", "$...js", "$\u{1f600}.js"] {
            let _ = classify_reserved_file(name);
        }
    }
}
