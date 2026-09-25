mod http_client;
pub mod native;
pub mod reserved;
pub mod scaffold;

use std::fs;
use std::str::FromStr;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

pub use crate::reserved::{
    InterceptionClimb, ReservedFile, ReservedName, ReservedRole, ReservedVariant, RouteSegment,
    classify_reserved_file, classify_route_segment, interception_climb,
};

pub const RESERVED_LAYOUT: &str = "$layout.js";
pub const RESERVED_PAGE: &str = "$page.js";
pub const RESERVED_MIDDLEWARE: &str = "$middleware.js";
pub const RESERVED_ROUTE: &str = "$route.js";

/// The stem every reserved page is spelled with, without an extension.
pub const RESERVED_PAGE_STEM: &str = "$page";
/// The stem every reserved layout is spelled with.
pub const RESERVED_LAYOUT_STEM: &str = "$layout";
/// The stem every reserved middleware is spelled with.
pub const RESERVED_MIDDLEWARE_STEM: &str = "$middleware";
/// The stem every reserved route handler is spelled with.
pub const RESERVED_ROUTE_STEM: &str = "$route";
/// The stem a slot's stand-in page is spelled with.
pub const RESERVED_DEFAULT_STEM: &str = "$default";

const UNSUPPORTED_TEMPLATE_FILES: [&str; 2] = ["template.js", "_uf.template.js"];
const UNSUPPORTED_SLOT_BOUNDARY_FILES: [(&str, &str); 6] = [
    ("error.js", "error"),
    ("loading.js", "loading"),
    ("not-found.js", "not-found"),
    ("_uf.error.js", "error"),
    ("_uf.loading.js", "loading"),
    ("_uf.not-found.js", "not-found"),
];

/// What a page may be written in, in the order a directory holding two is
/// resolved.
///
/// # Why this is a list and why it is here twice
///
/// `packages/vite/internal/routes.js` is the router the build actually runs,
/// and it has accepted `.jsx` and `.mdx` since it was written. This crate
/// accepted `.js` and nothing else, and it is the one `uf build` asks for the
/// summary — so uf's own documentation site reported `routes 1` beside
/// `prerendered pages 30`, with every `.mdx` page in the guide invisible
/// (ubugeeei-prod/uf#437, and #386 for the `.jsx` half).
///
/// The lists are the same list written twice because one side is Rust and the
/// other is JavaScript, which is exactly how they drifted. They are held
/// together by a test that reads the JavaScript and compares — see
/// `tests::the_two_routers_accept_the_same_extensions`. A shared spelling that
/// nothing checks is not shared.
pub const PAGE_EXTENSIONS: [&str; 3] = [".js", ".jsx", ".mdx"];

/// What a layout, middleware or route handler may be written in.
///
/// No `.mdx`: a layout is a component and a route handler exports functions,
/// and neither is something Markdown can be.
pub const MODULE_EXTENSIONS: [&str; 2] = [".js", ".jsx"];

/// Which application target the file-system router is resolving.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RouteTarget {
    /// A React DOM application.
    #[default]
    Web,
    /// A React Native application, before it has been narrowed to a platform.
    Native,
    /// A React Native application on iOS.
    Ios,
    /// A React Native application on Android.
    Android,
}

impl RouteTarget {
    /// The spelling `uf build --target` accepts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Native => "native",
            Self::Ios => "ios",
            Self::Android => "android",
        }
    }

    /// The reserved-file variants this target checks, most specific first.
    #[must_use]
    pub const fn variants(self) -> &'static [ReservedVariant] {
        match self {
            Self::Web => &[ReservedVariant::Web, ReservedVariant::Default],
            Self::Native => &[ReservedVariant::Native, ReservedVariant::Default],
            Self::Ios => &[
                ReservedVariant::Ios,
                ReservedVariant::Native,
                ReservedVariant::Default,
            ],
            Self::Android => &[
                ReservedVariant::Android,
                ReservedVariant::Native,
                ReservedVariant::Default,
            ],
        }
    }
}

impl FromStr for RouteTarget {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "web" => Ok(Self::Web),
            "native" | "react-native" => Ok(Self::Native),
            "ios" => Ok(Self::Ios),
            "android" => Ok(Self::Android),
            _ => Err(()),
        }
    }
}

/// The first spelling of `stem` this target would load from `directory`.
///
/// Platform before extension: `$page.native.mdx` is more specific to the
/// target than `$page.js`, and a directory that wrote both has made the
/// native one the route for a native build.
fn find_module_for_target(
    directory: &Utf8Path,
    stem: &str,
    extensions: &[&str],
    target: RouteTarget,
) -> Option<Utf8PathBuf> {
    for variant in target.variants() {
        for extension in extensions {
            let file = match variant.as_str() {
                Some(variant) => format!("{stem}.{variant}{extension}"),
                None => format!("{stem}{extension}"),
            };
            let candidate = directory.join(file);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Whether `name` is the page this target might resolve.
fn is_reserved_page_for_target(name: &str, target: RouteTarget) -> bool {
    target.variants().iter().any(|variant| {
        PAGE_EXTENSIONS
            .iter()
            .any(|extension| match variant.as_str() {
                Some(variant) => name == format!("{RESERVED_PAGE_STEM}.{variant}{extension}"),
                None => name == format!("{RESERVED_PAGE_STEM}{extension}"),
            })
    })
}

fn reserved_file_applies_to_target(file: ReservedFile, target: RouteTarget) -> bool {
    file.variant.is_route_entry() && target.variants().contains(&file.variant)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteParamKind {
    /// `[slug]`, spelled `:slug`: exactly one segment.
    Single,
    /// `[...slug]`, spelled `:slug*`: the rest of the path, at least one
    /// segment of it.
    CatchAll,
    /// `[[...slug]]`, spelled `:slug*?`: the rest of the path, which may be
    /// none of it — so the directory above it is one of the URLs it serves.
    OptionalCatchAll,
}

impl RouteParamKind {
    /// Whether the parameter takes the rest of the path, as a list.
    #[must_use]
    pub const fn is_catch_all(self) -> bool {
        matches!(self, Self::CatchAll | Self::OptionalCatchAll)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteParam {
    pub name: CompactString,
    pub kind: RouteParamKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub path: CompactString,
    pub directory: Utf8PathBuf,
    pub page: Utf8PathBuf,
    pub params: Vec<RouteParam>,
    pub has_layout: bool,
    /// Every `$middleware.js` that runs before this route resolves,
    /// outermost first.
    ///
    /// Inherited down the tree, the way layouts are, because that is what
    /// `packages/vite/internal/routes.js` puts on the route table the build
    /// actually runs (`ownMiddleware`, accumulated root-first). This field was
    /// a `has_middleware: bool` read off `directory`, and the two answer
    /// different questions: `app/dashboard/$middleware.js` guards
    /// `/dashboard/settings`, whose own directory declares nothing. Anything
    /// asking "is this route guarded" — and `uf build` now asks, so it can say
    /// which prerendered files a guard never sees — got `false` from the
    /// per-directory form for every route below the one that declared it.
    ///
    /// Resolved through the same extension and target ordering as the build
    /// router, because `uf build` reads this before it asks Vite to bundle.
    pub middleware: Vec<Utf8PathBuf>,
}

impl Route {
    /// Whether this route's own directory declares a middleware.
    ///
    /// The question `uf inspect --json`'s `hasMiddleware` has always answered.
    #[must_use]
    pub fn has_own_middleware(&self) -> bool {
        self.middleware
            .last()
            .is_some_and(|file| file.parent() == Some(self.directory.as_path()))
    }

    /// Whether any middleware runs before this route resolves.
    #[must_use]
    pub fn is_guarded(&self) -> bool {
        !self.middleware.is_empty()
    }

    /// Whether `url` — a concrete path, with every parameter already filled
    /// in — is served by this route.
    ///
    /// The prerender names the files it wrote by URL, not by route, so this is
    /// how a caller gets from `/posts/hello-world` back to `/posts/:slug` and
    /// the guards above it. A catch-all consumes the rest of the path and
    /// requires at least one segment to consume, which is what
    /// `[...slug]` means: `/docs` is not `/docs/[...slug]`. An optional
    /// catch-all, `[[...slug]]`, consumes the rest whatever there is of it,
    /// none included — `/docs` *is* `/docs/[[...slug]]`.
    #[must_use]
    pub fn matches_url(&self, url: &str) -> bool {
        let mut actual = url
            .split('?')
            .next()
            .unwrap_or(url)
            .split('/')
            .filter(|segment| !segment.is_empty());
        let mut expected = self.path.split('/').filter(|segment| !segment.is_empty());

        while let Some(segment) = expected.next() {
            if segment.starts_with(':') && segment.ends_with("*?") {
                // The same, except that nothing left is a match too.
                return expected.next().is_none();
            }
            if segment.starts_with(':') && segment.ends_with('*') {
                // The last thing in the path, so whatever is left of the URL
                // is the catch-all's, and there must be some. That it is last
                // is [`discover_routes`]'s doing: it refuses a `[...param]`
                // with a routing directory below it, because a catch-all takes
                // every remaining segment and leaves nothing for what follows.
                // `expected.next().is_none()` is what says so here rather than
                // assuming it, since `Route` is a public struct anyone can
                // fill in by hand.
                return actual.next().is_some() && expected.next().is_none();
            }
            let Some(given) = actual.next() else {
                return false;
            };
            if !segment.starts_with(':') && segment != given {
                return false;
            }
        }
        actual.next().is_none()
    }

    /// How specific this route is, for ranking two that both serve a URL.
    ///
    /// A static segment outranks a parameter, which outranks a catch-all,
    /// which outranks an optional one, and a longer path outranks a shorter
    /// one — three, two, one and nothing per segment.
    ///
    /// `packages/router/internal/runtime.js`'s `specificity` is the source of
    /// truth for these numbers, and this is a copy of it. Only one of the two
    /// decides which route answers a request, and it is that one: it runs in
    /// the server and in the browser, and this crate runs in neither. What
    /// this copy is for is `uf build` saying *which* route a prerendered
    /// document belongs to, which is a claim about what the runtime will do
    /// and is worth nothing if the two disagree.
    ///
    /// Counting literal segments was the earlier approximation and is not the
    /// same function: `/posts/:a/:b/edit` and `/posts/archive/:z*` both have
    /// two literals and both serve `/posts/archive/foo/edit`, and the runtime
    /// answers with the first. Naming the second in a warning describes a
    /// request that never happens.
    ///
    /// Two routes can still tie — `app/(marketing)/posts/new` and
    /// `app/posts/new` are one path twice — and the runtime breaks that tie by
    /// route-table order, which is not this crate's ordering. A tie is a
    /// genuinely ambiguous project rather than a disagreement between the two
    /// rankings, and it is left alone here.
    #[must_use]
    pub fn specificity(&self) -> usize {
        self.path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| match segment.as_bytes() {
                [b':', .., b'*', b'?'] => 0,
                [b':', .., b'*'] => 1,
                [b':', ..] => 2,
                _ => 3,
            })
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedFileViolation {
    pub path: Utf8PathBuf,
}

#[derive(Debug, Error)]
pub enum RouterError {
    /// Instrumentation is application-wide, never local to a route or slot.
    #[error("{file}: instrumentation modules must be directly inside the router root")]
    InstrumentationOutsideRoot { file: Utf8PathBuf },
    #[error("failed to walk {path}: {source}")]
    Walk {
        path: Utf8PathBuf,
        #[source]
        source: walkdir::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("path is not UTF-8: {0}")]
    NonUtf8(String),
    /// A `[...param]` directory with a routing directory below it.
    ///
    /// Refused rather than served, because there is nothing to serve. A
    /// catch-all takes every segment of the URL that is left, so a segment
    /// after it has nothing to match against: `packages/router/internal/
    /// runtime.js`'s `matchSegments` compares `parts[parts.length]` — which is
    /// `undefined` — against the following segment and gives up, and
    /// [`Route::matches_url`] returns `false` for every URL. Both routers
    /// already agreed the page was unreachable; discovery was the only place
    /// that did not say so, and it produced a route that looked live in
    /// `uf inspect`, in the generated `RoutePath`, and in the guard report
    /// `uf build` writes — where a prerendered document under it was simply
    /// left out.
    #[error(
        "{page}: `{catch_all}` is a catch-all and `{following}` is below it, so no URL can reach \
         this page — a catch-all takes every segment of the path that is left, and there is \
         nothing after it to match `{following}` with. Move the page so `{catch_all}` is the last \
         routing directory in it, or make `{catch_all}` a `[{parameter}]`."
    )]
    NonTerminalCatchAll {
        /// The `$page.js` that cannot be reached.
        page: Utf8PathBuf,
        /// The catch-all directory, as it is written on disk.
        catch_all: String,
        /// The first routing directory below it.
        following: String,
        /// The catch-all's parameter name, for the suggested spelling.
        parameter: String,
    },
    /// A `[[...param]]` page beside a page at the path above it.
    ///
    /// An optional catch-all serves the directory it sits in as well as every
    /// path below it: `app/docs/[[...slug]]/$page.js` answers `/docs` with an
    /// empty `slug`. A second page at `/docs` would be a second answer to one
    /// URL, and nothing in the URL says which was meant — the runtime would
    /// pick one by ranking and the other would be a page nobody reaches.
    /// Next.js refuses the same pair, and for the same reason.
    #[error(
        "{page}: `{catch_all}` is an optional catch-all, so it serves `{path}` itself as well as \
         every path below it, and `{other}` serves `{path}` too — one URL with two pages, and \
         nothing in the URL says which. Remove `{other}` and render `{path}` here, where \
         `{parameter}` is an empty list, or make it `[...{parameter}]`, which leaves `{path}` \
         to `{other}`."
    )]
    OptionalCatchAllBesidePage {
        /// The optional catch-all's `$page.js`.
        page: Utf8PathBuf,
        /// The optional catch-all directory, as it is written on disk.
        catch_all: String,
        /// The route path both serve: the catch-all's own path without it.
        path: String,
        /// The page that already serves that path.
        other: Utf8PathBuf,
        /// The catch-all's parameter name, for the suggested spelling.
        parameter: String,
    },
    /// A directory spelled like an intercepting route that uf refuses: a
    /// marker it does not read, an interception outside a `@slot`, or one that
    /// climbs past the router root.
    ///
    /// Refused rather than served, and the refusal was the whole of what
    /// ubugeeei-prod/uf#267 asked for first. Neither `(.)photo` nor `@team`
    /// was a spelling this grammar had an opinion about, so both fell through
    /// to a literal segment: `@team` became the URL `/@team`, and `(.)photo`
    /// became `/(.)photo`, because the test for a `(group)` is that the segment
    /// *ends* in `)`. Both then appeared in the generated `RoutePath` union
    /// and in `uf inspect`, so a project migrating from Next.js got output
    /// that looked like it worked.
    ///
    /// `@team` is a slot uf serves now, and `(.)photo` is an interception it
    /// serves inside one. What is left of the refusal is the ways an
    /// interception can be written without being one.
    ///
    /// `reason` comes from [`RouteSegment::unsupported_reason`],
    /// [`RouteSegment::outside_slot_reason`] or [`RouteSegment::climb_reason`],
    /// so this and `uf lint`'s `router/unsupported-segment` say the same
    /// sentence about the same directory.
    #[error("{directory}: {reason}")]
    UnsupportedRouteDirectory {
        /// The directory, as it is written on disk.
        directory: Utf8PathBuf,
        /// What is wrong with it and what to do instead.
        reason: String,
    },
    /// An intercepting route whose URL no page serves.
    ///
    /// An interception renders in its slot only for a client navigation that
    /// starts on a page the slot is on. Everybody else who arrives at the URL —
    /// a reload, a shared link, a crawler, the prerender — is given the page
    /// the URL names, and here there is none: the photo a reader opened in a
    /// modal would be a 404 the moment they reloaded it or sent it to somebody,
    /// and nothing would say so until then. So it is refused where the file is.
    ///
    /// "Serves" is every URL the interception matches, not a path spelled the
    /// same way: `app/docs/[...path]/$page.js` serves what
    /// `app/docs/@panel/(.)[slug]/$page.js` intercepts, and
    /// `app/photo/[id]/$page.js` does not serve `(.)[...rest]`.
    #[error(
        "{page}: this intercepting route stands in for `{intercepts}` when a client navigation \
         reaches it, and no page serves `{intercepts}`, so a reload of that URL, a link to it and \
         the prerender would all be a 404. Add `{ordinary}`, the page everybody who does not \
         arrive by that navigation gets, or remove the interception."
    )]
    InterceptionWithoutPage {
        /// The intercepting `$page.js`, as it is written on disk.
        page: Utf8PathBuf,
        /// The URL it intercepts, as a route path: `/feed/photo/:id`.
        intercepts: String,
        /// Where the page that serves that URL would go.
        ordinary: Utf8PathBuf,
    },
    /// A file name that looks like a route template but is not one uf opens.
    ///
    /// `$template.js` is uf's template convention. Next.js's `template.js`, and
    /// the old-looking `_uf.template.js`, used to sit in a public route
    /// directory as ordinary project files. That is worse than unsupported:
    /// the author asked for remount behaviour, got no error, and the route
    /// looked like it worked.
    #[error("{file}: {reason}")]
    UnsupportedTemplateFile {
        /// The file, as it is written on disk.
        file: Utf8PathBuf,
        /// What is wrong with it and what to do instead.
        reason: String,
    },
    /// A `@slot` whose segment declares no layout of its own.
    ///
    /// A slot renders *into* a layout — that is the whole of what a parallel
    /// route is — so the segment that declares one has to have a layout to
    /// render it into. Inheriting the layout above would put the slot in a
    /// frame that knows nothing about it, and every route under that frame
    /// would be asked to position a prop it never declared.
    ///
    /// Refused rather than dropped, for the reason the spelling itself is:
    /// a slot that renders nowhere is a directory of pages nothing reaches,
    /// and the project looks like it works.
    #[error(
        "{directory}: `{slot}` is a parallel-route slot and `{segment}` declares no layout of its \
         own, so there is nothing to render the slot into — a slot is a prop the segment's own \
         layout receives beside `children`. Add `{layout}`, or move the slot to a segment that \
         has one."
    )]
    SlotWithoutLayout {
        /// The slot directory, as it is written on disk.
        directory: Utf8PathBuf,
        /// The slot directory's name, as it is written.
        slot: String,
        /// The route path of the segment that declares it.
        segment: String,
        /// The layout file that would receive it.
        layout: Utf8PathBuf,
    },
    /// A `$default.js` that is not directly inside a `@slot` directory.
    ///
    /// A default answers one question — what a slot renders when the URL
    /// addresses the other one — so it is asked once, at the slot, and is
    /// meaningful in exactly one place. uf has no `children` default the way
    /// Next.js does: `children` is the page the URL matched, and a URL that
    /// matches no page is a 404 rather than a segment with a hole in it.
    ///
    /// Refused rather than ignored, because a file the router never opens is
    /// the failure this whole issue is about.
    #[error(
        "{file}: `$default.js` is what a `@slot` renders when the URL says nothing about it, \
         and it belongs directly inside the slot directory — one per slot, beside that slot's own \
         pages. Nothing would ever render this one. uf has no `default` for `children`: a URL \
         that matches no page is a 404."
    )]
    DefaultOutsideSlot {
        /// The file, as it is written on disk.
        file: Utf8PathBuf,
    },
    /// A `$route.js` or `$middleware.js` inside a `@slot`.
    ///
    /// A slot is matched and rendered as part of the page at a URL; it never
    /// answers a request of its own and never guards one. A handler or a guard
    /// under a slot is a file with no request to see — and worse than useless,
    /// because the directory contributes no URL segment, so the path it would
    /// appear to claim is the *declaring* segment's, which is somebody else's.
    #[error(
        "{file}: a `@slot` renders inside the page at a URL and answers no request of its own, so \
         `${role}.js` here would never run. A slot directory contributes no URL segment, so \
         this would claim `{path}` — which belongs to the segment that declares the slot. Move it \
         out of `{slot}`."
    )]
    ServerModuleInsideSlot {
        /// The file, as it is written on disk.
        file: Utf8PathBuf,
        /// Which of the two roles it is, as it is written in the name.
        role: &'static str,
        /// The route path the directory would otherwise contribute to.
        path: String,
        /// The slot directory it is under, as it is written.
        slot: String,
    },
    /// A not-found boundary inside a `@slot` that still has no slot-local
    /// answer.
    ///
    /// A slot renders a page and the layouts under the slot. It may carry a
    /// `$loading.js` and `$error.js`, but it still has no 404 of its own.
    /// Next.js gives a slot all three, and this is the part of parallel routes
    /// uf has not built.
    ///
    /// Refused rather than ignored, because the whole of ubugeeei-prod/uf#267
    /// is that a file the router never opens must not look like one it does.
    /// This is also the list of what is left to implement, stated where
    /// somebody writing the file will read it.
    #[error(
        "{file}: a `@slot` renders a page and the layouts inside the slot, and has no `{role}` of \
         its own — uf's parallel routes do not carry per-slot not-found boundaries yet, so this \
         file would never be opened. Put it outside `{slot}`, where it covers the whole segment. \
         https://github.com/ubugeeei-prod/uf/issues/267"
    )]
    BoundaryInsideSlot {
        /// The file, as it is written on disk.
        file: Utf8PathBuf,
        /// The role, as it is written in the name.
        role: &'static str,
        /// The slot directory it is under, as it is written.
        slot: String,
    },
    /// A boundary-like spelling inside a `@slot` that is not a uf route file.
    #[error(
        "{file}: `{file_name}` looks like a `{role}` boundary for a `@slot`, but it is not a uf \
         route file there. Use `$loading.js` for slot loading and `$error.js` for slot errors; \
         per-slot not-found boundaries are still not implemented. \
         https://github.com/ubugeeei-prod/uf/issues/267"
    )]
    UnsupportedSlotBoundaryFile {
        /// The file, as it is written on disk.
        file: Utf8PathBuf,
        /// The file name, as it is written.
        file_name: String,
        /// The role the unsupported spelling resembles.
        role: &'static str,
        /// The slot directory it is under, as it is written.
        slot: String,
    },
    /// A slot named after a prop every layout already receives.
    ///
    /// A slot arrives as a prop named after the directory, so `@children` and
    /// `@params` are the two names that would land on top of something the
    /// layout already has. `@children` is the likelier of the two by a long
    /// way: `children` is what Next.js calls its *implicit* slot, so it is the
    /// first thing somebody migrating writes down.
    ///
    /// Refused rather than resolved by precedence. Whichever way the tie were
    /// broken, one of the two would be silently missing — the page, or the
    /// slot — and a page that quietly does not render is the failure this whole
    /// issue is about.
    #[error(
        "{directory}: a slot arrives as a prop named after its directory, and `{name}` is a prop \
         every layout already receives, so one of the two would silently go missing. Rename the \
         slot. The page a URL matches is always `children` — uf has no `@children` slot, which is \
         the name Next.js gives that page."
    )]
    SlotNameIsALayoutProp {
        /// The slot directory, as it is written on disk.
        directory: Utf8PathBuf,
        /// The slot's name, without the `@`.
        name: String,
    },
}

/// The prop names a layout already receives, which a slot may therefore not
/// take.
///
/// Two, and both are `RouteView`'s: `children` is what the layout wraps and
/// `params` is the route's. Kept here rather than at the check so that
/// `packages/vite/internal/routes.js` has one list to mirror.
pub const LAYOUT_PROP_NAMES: [&str; 2] = ["children", "params"];

pub fn discover_routes(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<Route>, RouterError> {
    discover_routes_for_target(root, config, RouteTarget::Web)
}

pub fn discover_routes_for_target(
    root: &Utf8Path,
    config: &UniflowedConfig,
    target: RouteTarget,
) -> Result<Vec<Route>, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

    preflight_router_root(&app_root, target)?;

    let mut routes = Vec::new();
    for entry in WalkDir::new(&app_root) {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.clone(),
            source,
        })?;
        if !entry.file_type().is_file()
            || !entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_reserved_page_for_target(name, target))
        {
            continue;
        }

        let page = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let directory = page.parent().unwrap_or(&app_root).to_path_buf();
        // One page per directory, and the walk is over files: a directory
        // holding both `$page.js` and `$page.mdx` reached here twice and
        // pushed two `/guide` routes, while the router the build runs called
        // `findModule` once and rendered one of them. Two `Route`s with the
        // same path put the same string in the generated `RoutePath` twice,
        // counted the page twice in `uf build`'s summary, and gave
        // `uf inspect` a route that is not served.
        //
        // Which spelling wins is [`find_module`]'s answer and nothing else's,
        // so the precedence is written once, in `PAGE_EXTENSIONS`, and both
        // routers read the same order. File names rather than whole paths,
        // because the two are built from different halves of the walk.
        if find_module_for_target(&directory, RESERVED_PAGE_STEM, &PAGE_EXTENSIONS, target)
            .is_none_or(|resolved| resolved.file_name() != page.file_name())
        {
            continue;
        }
        let relative = directory.strip_prefix(&app_root).unwrap_or(&directory);
        // A page inside a `@slot` is not a URL, and this is the line that says
        // so. A slot never *adds* a path: it renders into the layout of the
        // segment that declares it, matched against the URL that segment's own
        // routes are matched against. So `app/dashboard/@team/members/
        // $page.js` is what `/dashboard/members` puts in the `team` slot,
        // and it is not a second page at that path — which is what it would be
        // if it fell through to here, since the slot segment contributes
        // nothing to `route_path_and_params`.
        //
        // The consequence is worth stating rather than discovering: a slot
        // decorates URLs that already exist. `/dashboard/members` with no
        // `app/dashboard/members/$page.js` is a 404 whatever the slots hold,
        // and that is uf's answer to the question Next.js answers with a
        // `default.js` for `children`.
        if slot_in(relative).is_some() {
            continue;
        }
        // Before the path is built, because the path is where the evidence
        // goes missing: `/docs/:slug*/edit` reads like a route, and only the
        // directory names say which `[...param]` the author wrote.
        if let Some((catch_all, following)) = non_terminal_catch_all(relative) {
            return Err(RouterError::NonTerminalCatchAll {
                parameter: catch_all_parameter(&catch_all),
                page,
                catch_all,
                following,
            });
        }
        let (path, params) = route_path_and_params(relative);

        routes.push(Route {
            path: path.to_compact_string(),
            has_layout: find_module_for_target(
                &directory,
                RESERVED_LAYOUT_STEM,
                &MODULE_EXTENSIONS,
                target,
            )
            .is_some(),
            middleware: middleware_chain(&app_root, &directory, target),
            directory,
            page,
            params,
        });
    }

    routes.sort_by(|a, b| a.path.cmp(&b.path));
    refuse_optional_catch_all_collisions(&routes)?;
    Ok(routes)
}

/// Refuse a `[[...param]]` page that shares its parent path with a page.
///
/// Asked of the table rather than of the directories, because the two pages
/// need not be neighbours on disk: `app/(site)/docs/$page.js` serves `/docs`
/// exactly as `app/docs/$page.js` does, and it is the path they share that
/// makes them collide. So may their parameter names differ:
/// `app/users/[id]/$page.js` and `app/users/[uid]/[[...tab]]/$page.js` both
/// answer `/users/1`, which is why the paths are compared by shape. The
/// catch-all is last in its path — a page where it is not was refused by name
/// before the table was built — so its parent path is its own path without
/// the last segment.
fn refuse_optional_catch_all_collisions(routes: &[Route]) -> Result<(), RouterError> {
    for route in routes {
        let Some(param) = route
            .params
            .last()
            .filter(|param| param.kind == RouteParamKind::OptionalCatchAll)
        else {
            continue;
        };
        let parent = match route.path.rsplit_once('/') {
            Some(("", _)) | None => "/",
            Some((parent, _)) => parent,
        };
        if let Some(other) = routes
            .iter()
            .find(|other| path_shape(&other.path) == path_shape(parent))
        {
            return Err(RouterError::OptionalCatchAllBesidePage {
                page: route.page.clone(),
                catch_all: format!("[[...{}]]", param.name),
                path: parent.to_string(),
                other: other.page.clone(),
                parameter: param.name.to_string(),
            });
        }
    }
    Ok(())
}

/// A route path with its parameter names left out: `/users/:` for
/// `/users/:id`. Two paths of one shape serve the same URLs.
fn path_shape(path: &str) -> String {
    path.split('/')
        .map(|segment| match segment.strip_prefix(':') {
            Some(name) => format!(
                ":{}",
                name.trim_start_matches(|c: char| c != '*' && c != '?')
            ),
            None => segment.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The parameter name a catch-all directory captures under: `slug` for
/// `[...slug]` and for `[[...slug]]`.
fn catch_all_parameter(directory: &str) -> String {
    match classify_route_segment(directory) {
        RouteSegment::CatchAll(name) | RouteSegment::OptionalCatchAll(name) => name.to_string(),
        _ => directory.to_string(),
    }
}

/// What a module in the router root needs a server for.
///
/// Two roles rather than every reserved name, because these are the two that
/// are *only* ever a server: a `$route.js` answers a request instead of
/// rendering, and a `$middleware.js` runs before a route resolves. Every
/// other reserved file — a layout, a template, a loading boundary — is part of
/// a document a prerender can write to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerModuleKind {
    /// `$route.js`: answers a request instead of rendering a page.
    RouteHandler,
    /// `$middleware.js`: runs before a route resolves.
    Middleware,
}

impl ServerModuleKind {
    /// The reserved role, as it is written in the file name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RouteHandler => "route",
            Self::Middleware => "middleware",
        }
    }
}

/// One module the router runs on a server, and the URL it sits at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerModule {
    /// The route path of the directory holding it, in the same `/posts/:slug`
    /// spelling [`Route::path`] uses.
    ///
    /// A middleware's path is the subtree it guards rather than a URL that is
    /// necessarily served: `app/dashboard/$middleware.js` is reported at
    /// `/dashboard` whether or not that directory has a page of its own.
    pub path: CompactString,
    /// The file, as it is written on disk.
    pub file: Utf8PathBuf,
    /// Which of the two it is.
    pub kind: ServerModuleKind,
}

/// Every route handler and middleware under the router root.
///
/// [`discover_routes`] answers "what can be rendered"; this answers "what has
/// to run", and they are different questions asked of the same tree. A route
/// handler has no page and so appears in no `Route`, and a middleware appears
/// in [`Route::middleware`] only for routes that have a page beneath it — so a
/// caller deciding whether a deployment target can serve this project at all
/// cannot get either from the route table.
///
/// The directory walk resolves each role through [`find_module`], so a
/// directory holding both `$route.js` and `$route.jsx` reports the one
/// the build's router would run rather than both. Sorted by path so a message
/// built from this does not depend on the order the filesystem hands entries
/// back.
pub fn discover_server_modules(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<ServerModule>, RouterError> {
    discover_server_modules_for_target(root, config, RouteTarget::Web)
}

pub fn discover_server_modules_for_target(
    root: &Utf8Path,
    config: &UniflowedConfig,
    target: RouteTarget,
) -> Result<Vec<ServerModule>, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

    preflight_router_root(&app_root, target)?;

    let mut found = Vec::new();
    for entry in WalkDir::new(&app_root).sort_by_file_name() {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.clone(),
            source,
        })?;
        if !entry.file_type().is_dir() {
            continue;
        }
        let directory = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let relative = directory.strip_prefix(&app_root).unwrap_or(&directory);
        let (path, _) = route_path_and_params(relative);
        for (stem, kind) in [
            (RESERVED_ROUTE_STEM, ServerModuleKind::RouteHandler),
            (RESERVED_MIDDLEWARE_STEM, ServerModuleKind::Middleware),
        ] {
            if let Some(file) = find_module_for_target(&directory, stem, &MODULE_EXTENSIONS, target)
            {
                found.push(ServerModule {
                    path: path.to_compact_string(),
                    file,
                    kind,
                });
            }
        }
    }

    found.sort_by(|a, b| a.path.cmp(&b.path).then(a.file.cmp(&b.file)));
    Ok(found)
}

/// Validate the router grammar before a caller reads any table from it.
///
/// Routes, route handlers and middleware are three views over one filesystem
/// router. The unsupported spellings have to fail before any of them builds a
/// table, or one API can reject a directory while another reports the same
/// directory as a live path.
fn preflight_router_root(app_root: &Utf8Path, target: RouteTarget) -> Result<(), RouterError> {
    // Before any route is built, because the point is that none is: an
    // interception used to become a literal URL segment and a live route, and
    // so did a slot before slots were served.
    refuse_unsupported_directories(app_root)?;
    // A wrong template spelling is the file version of the same failure: the
    // project asked for remount behaviour and got a file the router never
    // opens.
    refuse_unsupported_template_files(app_root)?;
    // And before any route is built for the opposite reason: a slot's pages are
    // routes uf renders, so what is wrong with a slot has to be said here
    // rather than discovered as a missing prop at render time.
    check_slots(app_root, target)?;
    // Last, because it is about the tree rather than a directory: an
    // intercepting page is only as good as the ordinary page that serves its
    // URL to everybody the interception does not.
    check_interceptions(app_root, target)?;
    Ok(())
}

/// Every page under an interception has an ordinary page that serves its URL,
/// and its URL is one a request can reach.
///
/// A walk of its own rather than a question for [`discover_routes`], because
/// the preflight is shared with [`discover_server_modules`] and the two views
/// have to refuse the same trees. It collects the route path of every ordinary
/// page for `target` and of every page under an interception, and asks of each
/// of the second whether some page in the first serves every URL it matches.
///
/// A catch-all that is not last in an intercepted URL is refused the way it is
/// for an ordinary page, as [`RouterError::NonTerminalCatchAll`]: nothing could
/// ever match it. A climb is what can put one there — `(.)edit` in a slot under
/// `docs/[...path]/` stands in for `/docs/:path*/edit` — which is why this is
/// asked of the path rather than left to the directory names.
///
/// Private directories are pruned for the reason every other walk here prunes
/// them.
fn check_interceptions(app_root: &Utf8Path, target: RouteTarget) -> Result<(), RouterError> {
    let walk = WalkDir::new(app_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !entry.file_name().to_string_lossy().starts_with(['.', '_'])
        });

    let mut served: Vec<Vec<PathSegment>> = Vec::new();
    let mut intercepting: Vec<(Utf8PathBuf, Vec<PathSegment>)> = Vec::new();
    for entry in walk {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.to_path_buf(),
            source,
        })?;
        if !entry.file_type().is_file()
            || !entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_reserved_page_for_target(name, target))
        {
            continue;
        }
        let page = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let directory = page.parent().unwrap_or(app_root);
        let relative = directory.strip_prefix(app_root).unwrap_or(directory);
        // A climb past the root has been refused already, by
        // `refuse_unsupported_directories`, which runs first.
        let Some(segments) = path_segments(relative) else {
            continue;
        };
        let under_interception = relative.as_str().split('/').any(|segment| {
            matches!(
                classify_route_segment(segment),
                RouteSegment::Interception { .. }
            )
        });
        if under_interception {
            intercepting.push((page, segments));
        } else if slot_in(relative).is_none() {
            served.push(segments);
        }
    }

    for (page, segments) in intercepting {
        let catch_all = segments.iter().position(|segment| {
            segment
                .param
                .as_ref()
                .is_some_and(|param| param.kind.is_catch_all())
        });
        if let Some(position) = catch_all
            && let Some(following) = segments.get(position + 1)
        {
            let parameter = segments[position]
                .param
                .as_ref()
                .map_or_else(String::new, |param| param.name.to_string());
            return Err(RouterError::NonTerminalCatchAll {
                catch_all: directory_spelling(&segments[position]),
                following: directory_spelling(following),
                parameter,
                page,
            });
        }
        if served
            .iter()
            .any(|ordinary| serves_every_url_of(ordinary, &segments))
        {
            continue;
        }
        let ordinary = segments
            .iter()
            .fold(app_root.to_path_buf(), |directory, segment| {
                directory.join(directory_spelling(segment))
            })
            .join(RESERVED_PAGE);
        return Err(RouterError::InterceptionWithoutPage {
            intercepts: path_from(&segments),
            ordinary,
            page,
        });
    }
    Ok(())
}

/// Whether every URL `intercepted` matches is one `ordinary` serves.
///
/// Segment by segment, the way the runtime's matcher reads both: a static
/// segment serves only itself, a parameter serves any one segment but not a
/// catch-all's many, a catch-all serves whatever is left as long as
/// something is, and an optional catch-all whatever is left. Not whether the
/// two are spelled alike — `/docs/:path*` serves `/docs/:slug`, `/photo/:id`
/// does not serve `/photo/:rest*`, and `/docs/:path*` does not serve
/// `/docs/:path*?`, whose `/docs` it has no answer for.
fn serves_every_url_of(ordinary: &[PathSegment], intercepted: &[PathSegment]) -> bool {
    let kind_at = |segments: &[PathSegment], position: usize| {
        segments
            .get(position)
            .and_then(|segment| segment.param.as_ref())
            .map(|param| param.kind)
    };
    for (position, segment) in ordinary.iter().enumerate() {
        let kind = segment.param.as_ref().map(|param| param.kind);
        if kind == Some(RouteParamKind::OptionalCatchAll) {
            return true;
        }
        if kind == Some(RouteParamKind::CatchAll) {
            return intercepted.len() > position
                && kind_at(intercepted, position) != Some(RouteParamKind::OptionalCatchAll);
        }
        let Some(other) = intercepted.get(position) else {
            return false;
        };
        let other_kind = other.param.as_ref().map(|param| param.kind);
        let serves = if kind == Some(RouteParamKind::Single) {
            !other_kind.is_some_and(RouteParamKind::is_catch_all)
        } else {
            other_kind.is_none() && other.spelling == segment.spelling
        };
        if !serves {
            return false;
        }
    }
    intercepted.len() == ordinary.len()
}

/// The directory name that would produce `segment`: `photo`, `[id]`,
/// `[...rest]`, `[[...rest]]`.
fn directory_spelling(segment: &PathSegment) -> String {
    match &segment.param {
        Some(RouteParam {
            name,
            kind: RouteParamKind::OptionalCatchAll,
        }) => format!("[[...{name}]]"),
        Some(RouteParam {
            name,
            kind: RouteParamKind::CatchAll,
        }) => format!("[...{name}]"),
        Some(RouteParam {
            name,
            kind: RouteParamKind::Single,
        }) => format!("[{name}]"),
        None => segment.spelling.clone(),
    }
}

/// The route path `segments` spell: `/` for none.
fn path_from(segments: &[PathSegment]) -> String {
    if segments.is_empty() {
        return "/".to_string();
    }
    format!(
        "/{}",
        segments
            .iter()
            .map(|segment| segment.spelling.as_str())
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// Refuse the directory spellings uf reserves without serving.
///
/// A pass of its own rather than a check inside the page walk above, and the
/// difference is what gets caught: `app/feed/(.)photo/` may hold a layout, a
/// loading file and no page at all, and it is still a directory the author
/// wrote expecting an intercepting route. The walk above only ever sees
/// `$page.js`.
///
/// Private directories are pruned, because the build's router prunes them: a
/// leading `.` or `_` means the directory is a place to put things rather than
/// a route, so `app/_drafts/(.)photo/` is not a route uf would have served and
/// is not one it should refuse.
///
/// The first offender wins, sorted by name so the message does not depend on
/// the order the filesystem hands entries back.
fn refuse_unsupported_directories(app_root: &Utf8Path) -> Result<(), RouterError> {
    let walk = WalkDir::new(app_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !entry.file_name().to_string_lossy().starts_with(['.', '_'])
        });

    for entry in walk {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.to_path_buf(),
            source,
        })?;
        if entry.depth() == 0 || !entry.file_type().is_dir() {
            continue;
        }
        let segment = entry.file_name().to_string_lossy().into_owned();
        let classified = classify_route_segment(&segment);
        // Three refusals, and which one a reader gets is the difference between
        // "you spelled the marker wrong", "you put it in the wrong place" and
        // "it climbs to somewhere that is not there". An interception inside a
        // slot is a route; the same directory beside an ordinary page is not,
        // because there is no named place for it to render into; and inside a
        // slot it still may not climb past the router root. The path is what
        // says which, so the decision is here rather than in the grammar.
        let parents = Utf8Path::from_path(entry.path())
            .and_then(|path| path.strip_prefix(app_root).ok())
            .and_then(Utf8Path::parent)
            .unwrap_or(Utf8Path::new(""));
        let reason = classified.unsupported_reason(&segment).or_else(|| {
            if slot_in(parents).is_none() {
                return classified.outside_slot_reason(&segment);
            }
            // The walk is top-down and stops at the first refusal, so every
            // directory above this one was judged first and none of them
            // climbs past the root: this counts what they really contribute.
            let depth = path_segments(parents).map_or(0, |segments| segments.len());
            classified.climb_reason(&segment, depth)
        });
        let Some(reason) = reason else {
            continue;
        };
        let directory = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        return Err(RouterError::UnsupportedRouteDirectory { directory, reason });
    }
    Ok(())
}

fn refuse_unsupported_template_files(app_root: &Utf8Path) -> Result<(), RouterError> {
    let walk = WalkDir::new(app_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !entry.file_name().to_string_lossy().starts_with(['.', '_'])
        });

    for entry in walk {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.to_path_buf(),
            source,
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if !UNSUPPORTED_TEMPLATE_FILES.contains(&file_name.as_ref()) {
            continue;
        }
        let file = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let reason = unsupported_template_file_reason(&file_name);
        return Err(RouterError::UnsupportedTemplateFile { file, reason });
    }
    Ok(())
}

fn unsupported_template_file_reason(file_name: &str) -> String {
    format!(
        "`{file_name}` looks like a route template, but uf's route template file is \
         `$template.js`. This file would be ignored rather than remounting the route, so it is \
         refused; rename it to `$template.js`. https://github.com/ubugeeei-prod/uf/issues/267"
    )
}

fn unsupported_slot_boundary_role(file_name: &str) -> Option<&'static str> {
    UNSUPPORTED_SLOT_BOUNDARY_FILES
        .iter()
        .find_map(|(candidate, role)| (*candidate == file_name).then_some(*role))
}

/// The first `@slot` segment of a path relative to the router root, if it has
/// one.
///
/// One question asked in three places — discovery skips a page under a slot,
/// [`check_slots`] decides which files are inside one, and the path builder
/// drops the segment — so it is one function. A slot inside a slot answers with
/// the outermost, which is the one that decides whether a file is "in a slot"
/// at all.
fn slot_in(relative: &Utf8Path) -> Option<&str> {
    relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .find_map(|segment| classify_route_segment(segment).slot().map(|_| segment))
}

/// What a `@slot` needs to be a slot, checked before any route is built.
///
/// Each rule names a file that would otherwise be opened by nobody. A slot
/// needs a layout at the segment that declares it, because a slot *is* a prop
/// that layout receives, and it needs a name that is not one of the props the
/// layout already has. A `$default.js` needs to be directly inside a slot,
/// because that is the only question it answers. A `$route.js` or
/// `$middleware.js` inside a slot has no request to see, and `$not-found.js`
/// still has no slot-local runtime to render into. `$loading.js` and
/// `$error.js` compose like layouts, so they are allowed.
///
/// Private directories are pruned for the reason
/// [`refuse_unsupported_directories`] prunes them: a leading `.` or `_` is a
/// subtree neither router walks into, so nothing in it is a route uf would
/// have served.
///
/// Sorted by name so the first offender does not depend on the order the
/// filesystem hands entries back.
fn check_slots(app_root: &Utf8Path, target: RouteTarget) -> Result<(), RouterError> {
    let walk = WalkDir::new(app_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !entry.file_name().to_string_lossy().starts_with(['.', '_'])
        });

    for entry in walk {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.to_path_buf(),
            source,
        })?;
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let relative = path.strip_prefix(app_root).unwrap_or(&path);

        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(slot_name) = classify_route_segment(&name).slot().map(str::to_owned) else {
                continue;
            };
            if LAYOUT_PROP_NAMES.contains(&slot_name.as_str()) {
                return Err(RouterError::SlotNameIsALayoutProp {
                    directory: path,
                    name: slot_name,
                });
            }
            // The *own* layout of the segment above, not the layouts in scope
            // there. A slot rendered into an inherited layout would be a prop
            // that layout never declared, on every route below it.
            let declaring = path.parent().unwrap_or(app_root);
            if find_module_for_target(declaring, RESERVED_LAYOUT_STEM, &MODULE_EXTENSIONS, target)
                .is_none()
            {
                let (segment_path, _) =
                    route_path_and_params(declaring.strip_prefix(app_root).unwrap_or(declaring));
                return Err(RouterError::SlotWithoutLayout {
                    layout: declaring.join(RESERVED_LAYOUT),
                    directory: path,
                    slot: name,
                    segment: segment_path,
                });
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().into_owned();
        if let Some(slot) = slot_in(relative)
            && let Some(role) = unsupported_slot_boundary_role(&file_name)
        {
            return Err(RouterError::UnsupportedSlotBoundaryFile {
                role,
                slot: slot.to_owned(),
                file_name,
                file: path,
            });
        }
        let reserved = classify_reserved_file(&file_name).recognized();
        if reserved.is_some_and(|file| {
            file.role == ReservedRole::Instrumentation && file.variant != ReservedVariant::Test
        }) && path.parent() != Some(app_root)
        {
            return Err(RouterError::InstrumentationOutsideRoot { file: path });
        }
        let Some(role) = reserved
            .filter(|file| reserved_file_applies_to_target(*file, target))
            .map(|file| file.role)
        else {
            continue;
        };
        // The slot's *own* directory, not any slot above: one default per slot,
        // beside that slot's own pages. A deeper one would be a second answer
        // to a question that is asked once, at the slot.
        let inside_a_slot_root = path
            .parent()
            .and_then(Utf8Path::file_name)
            .is_some_and(|name| classify_route_segment(name).slot().is_some());

        match role {
            ReservedRole::Default if !inside_a_slot_root => {
                return Err(RouterError::DefaultOutsideSlot { file: path });
            }
            ReservedRole::Route | ReservedRole::Middleware => {
                if let Some(slot) = slot_in(relative) {
                    let directory = path.parent().unwrap_or(app_root);
                    let (claimed, _) = route_path_and_params(
                        directory.strip_prefix(app_root).unwrap_or(directory),
                    );
                    return Err(RouterError::ServerModuleInsideSlot {
                        role: role.as_str(),
                        slot: slot.to_owned(),
                        path: claimed,
                        file: path,
                    });
                }
            }
            // What a slot does not have yet. `layout`, `template`, `loading`,
            // `error`, `page` and `default` are what it does have, and a
            // `story` is not the router's at all.
            ReservedRole::NotFound => {
                if let Some(slot) = slot_in(relative) {
                    return Err(RouterError::BoundaryInsideSlot {
                        role: role.as_str(),
                        slot: slot.to_owned(),
                        file: path,
                    });
                }
            }
            ReservedRole::Layout
            | ReservedRole::Template
            | ReservedRole::Loading
            | ReservedRole::Error
            | ReservedRole::Page
            | ReservedRole::Default
            | ReservedRole::Story
            | ReservedRole::Instrumentation => {}
        }
    }
    Ok(())
}

/// Every `$middleware.js` from `app_root` down to `directory`, outermost
/// first.
///
/// Walked upwards and reversed rather than accumulated on the way down,
/// because `discover_routes` finds pages with `WalkDir` and never sees a
/// directory as a directory. The result is the same list
/// `packages/vite/internal/routes.js` builds on its descent, and it has to be:
/// one of the two decides what runs, and the other decides what `uf build`
/// says about it.
fn middleware_chain(
    app_root: &Utf8Path,
    directory: &Utf8Path,
    target: RouteTarget,
) -> Vec<Utf8PathBuf> {
    let mut chain = Vec::new();
    let mut current = Some(directory);
    while let Some(dir) = current {
        if let Some(file) =
            find_module_for_target(dir, RESERVED_MIDDLEWARE_STEM, &MODULE_EXTENSIONS, target)
        {
            chain.push(file);
        }
        if dir == app_root {
            break;
        }
        current = dir.parent();
    }
    chain.reverse();
    chain
}

/// Every `$layout.js` that renders around a route in `directory`, outermost
/// first.
///
/// The walk [`middleware_chain`] makes, for the other reserved file a route
/// inherits down the tree. Public because `uf build` asks which modules render
/// a route before Vite runs: a layout that reads the request makes every route
/// under it a render about one request.
pub fn layout_chain(
    app_root: &Utf8Path,
    directory: &Utf8Path,
    target: RouteTarget,
) -> Vec<Utf8PathBuf> {
    let mut chain = Vec::new();
    let mut current = Some(directory);
    while let Some(dir) = current {
        if let Some(file) =
            find_module_for_target(dir, RESERVED_LAYOUT_STEM, &MODULE_EXTENSIONS, target)
        {
            chain.push(file);
        }
        if dir == app_root {
            break;
        }
        current = dir.parent();
    }
    chain.reverse();
    chain
}

pub fn find_reserved_file_violations(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<ReservedFileViolation>, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

    let mut violations = Vec::new();
    for entry in WalkDir::new(&app_root) {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.clone(),
            source,
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if classify_reserved_file(&file_name).is_unknown() {
            let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
                .unwrap_or_else(|path| Utf8PathBuf::from(path.display().to_string()));
            violations.push(ReservedFileViolation { path });
        }
    }

    violations.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(violations)
}

/// The `router.js` a project imports: the paths that exist, the parameters
/// each takes, and a link builder typed by both.
///
/// # Why the parameters are a tuple as well as an object
///
/// `RouteParams` is the useful shape — `RouteParams["/posts/:slug"]` is
/// `{ slug: string }`, and a page annotates its props with it. But a function
/// whose second argument is `RouteParams[Path]` cannot be called for a route
/// that has no parameters: the object is `{}`, and writing `route("/", {})` at
/// every static link is a tax on the common case. Worse, it used to be
/// `empty`, which *nothing* inhabits, so `route("/")` could not be written at
/// all — a static route was untypable rather than merely awkward. See #653.
///
/// `RouteArgs` is the same information as the argument list it describes: `[]`
/// for a route with no parameters and `[{ … }]` for one with them. Spread into
/// the signature it makes `route("/")` and `route("/posts/:slug", { slug })`
/// both exact, and a missing or extra argument an arity error.
///
/// # Why it delegates rather than substitutes
///
/// The body is one call into `@uniflowed/router/routing`, where `buildRoute` is built
/// out of the same `compile` the matcher uses. A builder generated here would
/// be a second implementation of the pattern grammar, and the two would drift
/// — as a link that 404s, which is the failure typed routes exist to remove.
pub fn generate_router_flow(routes: &[Route]) -> String {
    let mut output = String::from("// @flow\n\n");
    output.push_str("import { buildRoute } from \"@uniflowed/router/routing\";\n\n");
    output.push_str("export type RoutePath = ");
    if routes.is_empty() {
        output.push_str("empty;\n\n");
    } else {
        output.push_str(
            &routes
                .iter()
                .map(|route| format!("\"{}\"", route.path))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        output.push_str(";\n\n");
    }

    // Exact, because the generated table *is* the whole set of routes: an
    // inexact type would let a caller pass a path this router does not serve,
    // which is the mistake the generated types exist to prevent. Plain braces
    // say exactly that in modern Flow, which has been exact by default since
    // 2023 — `{| |}` is the legacy spelling of the same thing.
    if routes.is_empty() {
        output.push_str("export type RouteParams = {};\n\n");
        output.push_str("export type RouteArgs = {};\n\n");
    } else {
        output.push_str("export type RouteParams = {\n");
        for route in routes {
            output.push_str(&format!(
                "  \"{}\": {},\n",
                route.path,
                route_params_type(&route.params)
            ));
        }
        output.push_str("};\n\n");

        output.push_str("export type RouteArgs = {\n");
        for route in routes {
            output.push_str(&format!(
                "  \"{}\": {},\n",
                route.path,
                route_args_type(&route.params)
            ));
        }
        output.push_str("};\n\n");
    }
    // Written the way `uf fmt` writes it, down to the trailing comma: uf
    // scaffolds a project and then checks it with its own formatter, so a
    // generated file the formatter disagrees with fails `uf fmt --check` on
    // code nobody wrote. `the_generated_router_is_already_formatted` is what
    // keeps the two in step.
    output.push_str(
        "export function route<Path extends RoutePath>(path: Path, ...params: RouteArgs[Path]): string {\n  return buildRoute(path, ...params);\n}\n",
    );
    output
}

pub fn write_router_manifest(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Option<Utf8PathBuf>, RouterError> {
    write_router_manifest_for_target(root, config, RouteTarget::Web)
}

pub fn write_router_manifest_for_target(
    root: &Utf8Path,
    config: &UniflowedConfig,
    target: RouteTarget,
) -> Result<Option<Utf8PathBuf>, RouterError> {
    if !config.app.router.enabled {
        return Ok(None);
    }
    let routes = discover_routes_for_target(root, config, target)?;
    let manifest = root.join(config.app.router.manifest.as_str());
    // Through the formatter uf ships, with this project's own `fmt` settings.
    //
    // `generate_router_flow` writes what the printer would write, and for a
    // handful of routes the two agree — but a union of thirty-seven paths is
    // one line the printer would break across thirty-seven, and a project's
    // `fmt.lineWidth` can move where that happens. `uf prepare` then fails at
    // `run-format-check` on a file nobody wrote, which is what it did on this
    // repository's own docs site. Formatting the output rather than predicting
    // it makes the two agree by construction instead of by care.
    //
    // A generator that produced something unparseable would be a bug in this
    // file rather than in the project, so the unformatted source is written
    // instead of failing the build: an unformatted `router.js` still type
    // checks and still runs.
    let mut generated = generate_router_flow(&routes);
    // Native clients call the web handler table; no server module is imported.
    let handlers = discover_server_modules(root, config)?;
    generated.push_str(&http_client::generate_http_client(&handlers));
    let source = uf_fmt::format_source(&generated, &config.fmt)
        .map_or(generated, |formatted| formatted.output);
    fs::write(&manifest, source).map_err(|source| RouterError::Write {
        path: manifest.clone(),
        source,
    })?;
    Ok(Some(manifest))
}

/// The first `[...param]` or `[[...param]]` in `relative` that has a routing
/// directory below it,
/// with that directory, if there is one.
///
/// A `(group)` is not a routing directory — it contributes no segment to the
/// path — so `app/files/[...path]/(internal)/` leaves the catch-all last and
/// is fine. A `@slot` contributes none either, for a different reason and with
/// the same consequence here.
fn non_terminal_catch_all(relative: &Utf8Path) -> Option<(String, String)> {
    let mut catch_all: Option<&str> = None;
    for segment in relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        let classified = classify_route_segment(segment);
        if classified == RouteSegment::Group || classified.slot().is_some() {
            continue;
        }
        if let Some(found) = catch_all {
            return Some((found.to_string(), segment.to_string()));
        }
        if matches!(
            classified,
            RouteSegment::CatchAll(_) | RouteSegment::OptionalCatchAll(_)
        ) {
            catch_all = Some(segment);
        }
    }
    None
}

/// One segment of a route path, with the parameter it captures, if any.
struct PathSegment {
    /// As a route path spells it: `posts`, `:slug`, `:rest*`, `:rest*?`.
    spelling: String,
    param: Option<RouteParam>,
}

/// The route path segments `relative` contributes, with every interception in
/// it applied, or [`None`] when an interception in it climbs past the router
/// root.
///
/// The one place a directory path becomes a route path, so that an ordinary
/// page's path, the URL an intercepting route stands in for, and the path a
/// refusal quotes are one computation rather than three. An interception is
/// where "one directory, one segment" stops holding: `(..)photo` takes a
/// segment *away* before it adds its own, which is what
/// [`InterceptionClimb::remaining`] counts.
///
/// A marker uf does not read is spelled back as a literal. The preflight
/// refuses such a directory before any caller builds a path from it, so no
/// project is given that path; it keeps this total for a path handed over
/// unchecked.
fn path_segments(relative: &Utf8Path) -> Option<Vec<PathSegment>> {
    let mut segments: Vec<PathSegment> = Vec::new();
    for segment in relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        let classified = classify_route_segment(segment);
        let named = match classified {
            RouteSegment::Group => continue,
            // A slot contributes no URL segment, exactly as a group does: it
            // is a named place a route renders *into*, and the URL it is
            // matched against is the declaring segment's. The two arms are
            // separate because they are separate decisions — a group organises
            // files and a slot organises rendering — and folding them together
            // is how the next spelling gets one of the two answers by accident.
            RouteSegment::Slot(_) => continue,
            RouteSegment::Interception { route, .. } => {
                let Some(climb) = classified.interception_climb() else {
                    segments.push(PathSegment {
                        spelling: segment.to_owned(),
                        param: None,
                    });
                    continue;
                };
                let kept = climb.remaining(segments.len())?;
                segments.truncate(kept);
                // And then the segment it names, read the way any directory's
                // name is.
                classify_route_segment(route)
            }
            RouteSegment::Param(_)
            | RouteSegment::CatchAll(_)
            | RouteSegment::OptionalCatchAll(_)
            | RouteSegment::Literal(_) => classified,
        };
        match named {
            RouteSegment::OptionalCatchAll(name) => segments.push(PathSegment {
                spelling: format!(":{name}*?"),
                param: Some(RouteParam {
                    name: name.to_compact_string(),
                    kind: RouteParamKind::OptionalCatchAll,
                }),
            }),
            RouteSegment::CatchAll(name) => segments.push(PathSegment {
                spelling: format!(":{name}*"),
                param: Some(RouteParam {
                    name: name.to_compact_string(),
                    kind: RouteParamKind::CatchAll,
                }),
            }),
            RouteSegment::Param(name) => segments.push(PathSegment {
                spelling: format!(":{name}"),
                param: Some(RouteParam {
                    name: name.to_compact_string(),
                    kind: RouteParamKind::Single,
                }),
            }),
            RouteSegment::Literal(name) => segments.push(PathSegment {
                spelling: name.to_owned(),
                param: None,
            }),
            // Only what follows a marker reaches here as anything else, and a
            // group or a slot there names no segment: `unsupported_reason`
            // refuses that directory by name.
            RouteSegment::Group | RouteSegment::Slot(_) | RouteSegment::Interception { .. } => {}
        }
    }
    Some(segments)
}

fn route_path_and_params(relative: &Utf8Path) -> (String, Vec<RouteParam>) {
    // A climb past the root is refused before any table is built — see
    // `refuse_unsupported_directories` — so the root path here keeps this total
    // rather than being an answer any project is given.
    let segments = path_segments(relative).unwrap_or_default();
    let params = segments
        .iter()
        .filter_map(|segment| segment.param.clone())
        .collect();
    let path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!(
            "/{}",
            segments
                .iter()
                .map(|segment| segment.spelling.as_str())
                .collect::<Vec<_>>()
                .join("/")
        )
    };
    (path, params)
}

/// The argument list after the path, as a tuple type.
///
/// `[]` for a route that takes no parameters, so `route("/")` is the whole
/// call; `[{ … }]` for one that does. Spread into `route`'s signature this is
/// what makes the second argument required exactly when the route has
/// parameters, rather than always or never.
fn route_args_type(params: &[RouteParam]) -> String {
    if params.is_empty() {
        return "[]".to_string();
    }
    format!("[{}]", route_params_type(params))
}

fn route_params_type(params: &[RouteParam]) -> String {
    // `{}` and not `empty`. A route with no parameters takes an empty object,
    // and `empty` is Flow's bottom type: no value has it, so `RouteParams`
    // named a type for every static route that no caller could ever produce.
    // See #653.
    if params.is_empty() {
        return "{}".to_string();
    }

    let fields = params
        .iter()
        .map(|param| {
            // One list type for both catch-alls. The optional one is what
            // takes an empty list — `route("/docs/:slug*?", { slug: [] })` is
            // `/docs` — and the builder refuses an empty one for `:slug*`,
            // whose route would not match the URL it built.
            let ty = match param.kind {
                RouteParamKind::Single => "string",
                RouteParamKind::CatchAll | RouteParamKind::OptionalCatchAll => {
                    "$ReadOnlyArray<string>"
                }
            };
            format!("{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    // Exact for the same reason: a route's parameters are exactly the segments
    // in its path.
    format!("{{ {fields} }}")
}

#[cfg(test)]
mod tests;
