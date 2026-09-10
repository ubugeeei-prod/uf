pub mod reserved;
pub mod scaffold;

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

pub use crate::reserved::{
    ReservedFile, ReservedName, ReservedRole, ReservedVariant, RouteSegment,
    classify_reserved_file, classify_route_segment,
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

/// The first spelling of `stem` that exists in `directory`.
///
/// First rather than "the only one", mirroring `findModule` in
/// `packages/vite/internal/routes.js`: a directory holding both `$page.js`
/// and `$page.mdx` resolves to the same one on both sides, which matters
/// more than which one it is.
fn find_module(directory: &Utf8Path, stem: &str, extensions: &[&str]) -> Option<Utf8PathBuf> {
    extensions.iter().find_map(|extension| {
        let candidate = directory.join(format!("{stem}{extension}"));
        candidate.is_file().then_some(candidate)
    })
}

/// Whether `name` is a reserved page in any of its spellings.
fn is_reserved_page(name: &str) -> bool {
    PAGE_EXTENSIONS
        .iter()
        .any(|extension| name == format!("{RESERVED_PAGE_STEM}{extension}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteParamKind {
    Single,
    CatchAll,
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
    /// Kept in the `.js` grammar `reserved` documents. The build's router also
    /// accepts `.jsx`, which this discovery does not, for pages as much as for
    /// middleware; see ubugeeei-prod/uf#386.
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
    /// `[...slug]` means: `/docs` is not `/docs/[...slug]`.
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
    /// A static segment outranks a parameter, which outranks a catch-all, and
    /// a longer path outranks a shorter one — three, two and one per segment.
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
    /// A directory spelled the way an intercepting route is: `(.)photo`.
    ///
    /// Refused rather than served, and the refusal is the whole of what
    /// ubugeeei-prod/uf#267 asks for first. Neither this spelling nor `@team`
    /// was one this grammar had an opinion about, so both fell through to a
    /// literal segment: `@team` became the URL `/@team`, and `(.)photo` became
    /// `/(.)photo`, because the test for a `(group)` is that the segment
    /// *ends* in `)`. Both then appeared in the generated `RoutePath` union
    /// and in `uf inspect`, so a project migrating from Next.js got output
    /// that looked like it worked.
    ///
    /// `@team` is a slot uf serves now. Interception is not, and this is what
    /// is left of the refusal.
    ///
    /// `reason` comes from [`RouteSegment::unsupported_reason`], so this and
    /// `uf lint`'s `router/unsupported-segment` say the same sentence about
    /// the same directory.
    #[error("{directory}: {reason}")]
    UnsupportedRouteDirectory {
        /// The directory, as it is written on disk.
        directory: Utf8PathBuf,
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
    /// A boundary or a template inside a `@slot`.
    ///
    /// A slot renders a page and the layouts under the slot, and nothing else
    /// yet: it has no `<Suspense>` of its own, no error boundary of its own and
    /// no 404 of its own. Next.js gives a slot all three, and that is the part
    /// of parallel routes uf has not built.
    ///
    /// Refused rather than ignored, because the whole of ubugeeei-prod/uf#267
    /// is that a file the router never opens must not look like one it does.
    /// This is also the list of what is left to implement, stated where
    /// somebody writing the file will read it.
    #[error(
        "{file}: a `@slot` renders a page and the layouts inside the slot, and has no `{role}` of \
         its own — uf's parallel routes do not carry per-slot boundaries yet, so this file would \
         never be opened. Put it outside `{slot}`, where it covers the whole segment. \
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
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

    // Before any route is built, because the point is that none is: an
    // interception used to become a literal URL segment and a live route, and
    // so did a slot before slots were served.
    refuse_unsupported_directories(&app_root)?;
    // And before any route is built for the opposite reason: a slot's pages are
    // routes uf renders, so what is wrong with a slot has to be said here
    // rather than discovered as a missing prop at render time.
    check_slots(&app_root)?;

    let mut routes = Vec::new();
    for entry in WalkDir::new(&app_root) {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.clone(),
            source,
        })?;
        if !entry.file_type().is_file() || !entry.file_name().to_str().is_some_and(is_reserved_page)
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
        if find_module(&directory, RESERVED_PAGE_STEM, &PAGE_EXTENSIONS)
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
                parameter: catch_all
                    .trim_start_matches("[...")
                    .trim_end_matches(']')
                    .to_string(),
                page,
                catch_all,
                following,
            });
        }
        let (path, params) = route_path_and_params(relative);

        routes.push(Route {
            path: path.to_compact_string(),
            has_layout: find_module(&directory, RESERVED_LAYOUT_STEM, &MODULE_EXTENSIONS).is_some(),
            middleware: middleware_chain(&app_root, &directory),
            directory,
            page,
            params,
        });
    }

    routes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(routes)
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
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

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
            if let Some(file) = find_module(&directory, stem, &MODULE_EXTENSIONS) {
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
        let Some(reason) = classify_route_segment(&segment).unsupported_reason(&segment) else {
            continue;
        };
        let directory = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        return Err(RouterError::UnsupportedRouteDirectory { directory, reason });
    }
    Ok(())
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
/// Four rules, and each is a file that would otherwise be opened by nobody. A
/// slot needs a layout at the segment that declares it, because a slot *is* a
/// prop that layout receives, and it needs a name that is not one of the props
/// the layout already has. A `$default.js` needs to be directly inside a
/// slot, because that is the only question it answers. A `$route.js` or
/// `$middleware.js` inside a slot has no request to see, and the path it
/// would appear to claim belongs to the segment above.
///
/// Private directories are pruned for the reason
/// [`refuse_unsupported_directories`] prunes them: a leading `.` or `_` is a
/// subtree neither router walks into, so nothing in it is a route uf would
/// have served.
///
/// Sorted by name so the first offender does not depend on the order the
/// filesystem hands entries back.
fn check_slots(app_root: &Utf8Path) -> Result<(), RouterError> {
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
            if find_module(declaring, RESERVED_LAYOUT_STEM, &MODULE_EXTENSIONS).is_none() {
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
        let Some(role) = classify_reserved_file(&file_name)
            .recognized()
            .filter(|file| file.variant.is_route_entry())
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
            // What a slot does not have yet. `layout`, `page` and `default` are
            // what it does have, and a `story` is not the router's at all.
            ReservedRole::NotFound
            | ReservedRole::Error
            | ReservedRole::Loading
            | ReservedRole::Template => {
                if let Some(slot) = slot_in(relative) {
                    return Err(RouterError::BoundaryInsideSlot {
                        role: role.as_str(),
                        slot: slot.to_owned(),
                        file: path,
                    });
                }
            }
            ReservedRole::Layout
            | ReservedRole::Page
            | ReservedRole::Default
            | ReservedRole::Story => {}
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
fn middleware_chain(app_root: &Utf8Path, directory: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut chain = Vec::new();
    let mut current = Some(directory);
    while let Some(dir) = current {
        if let Some(file) = find_module(dir, RESERVED_MIDDLEWARE_STEM, &MODULE_EXTENSIONS) {
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
/// The body is one call into `@uniflowed/router`, where `buildRoute` is built
/// out of the same `compile` the matcher uses. A builder generated here would
/// be a second implementation of the pattern grammar, and the two would drift
/// — as a link that 404s, which is the failure typed routes exist to remove.
pub fn generate_router_flow(routes: &[Route]) -> String {
    let mut output = String::from("// @flow\n\n");
    output.push_str("import { buildRoute } from \"@uniflowed/router\";\n\n");
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
    if !config.app.router.enabled {
        return Ok(None);
    }
    let routes = discover_routes(root, config)?;
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
    let generated = generate_router_flow(&routes);
    let source = uf_fmt::format_source(&generated, &config.fmt)
        .map_or(generated, |formatted| formatted.output);
    fs::write(&manifest, source).map_err(|source| RouterError::Write {
        path: manifest.clone(),
        source,
    })?;
    Ok(Some(manifest))
}

/// The first `[...param]` in `relative` that has a routing directory below it,
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
        if matches!(classify_route_segment(segment), RouteSegment::CatchAll(_)) {
            catch_all = Some(segment);
        }
    }
    None
}

fn route_path_and_params(relative: &Utf8Path) -> (String, Vec<RouteParam>) {
    let mut params = Vec::new();
    let mut segments = Vec::new();

    for segment in relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        match classify_route_segment(segment) {
            RouteSegment::Group => {}
            RouteSegment::CatchAll(name) => {
                params.push(RouteParam {
                    name: name.to_compact_string(),
                    kind: RouteParamKind::CatchAll,
                });
                segments.push(format!(":{name}*"));
            }
            RouteSegment::Param(name) => {
                params.push(RouteParam {
                    name: name.to_compact_string(),
                    kind: RouteParamKind::Single,
                });
                segments.push(format!(":{name}"));
            }
            // A slot contributes no URL segment, exactly as a group does: it
            // is a named place a route renders *into*, and the URL it is
            // matched against is the declaring segment's. The two arms are
            // separate because they are separate decisions — a group organises
            // files and a slot organises rendering — and folding them together
            // is how the next spelling gets one of the two answers by accident.
            RouteSegment::Slot(_) => {}
            RouteSegment::Literal(name) => segments.push(name.to_string()),
            // An interception cannot reach here: `discover_routes` refuses the
            // directory before it builds a path. Spelled out rather than
            // folded into the literal arm so that a second unsupported
            // spelling has to be decided about here too.
            RouteSegment::Interception { .. } => {
                segments.push(segment.to_string());
            }
        }
    }

    let path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
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
            let ty = match param.kind {
                RouteParamKind::Single => "string",
                RouteParamKind::CatchAll => "$ReadOnlyArray<string>",
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
