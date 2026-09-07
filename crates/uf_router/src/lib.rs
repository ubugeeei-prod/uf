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

pub const RESERVED_LAYOUT: &str = "_uf.layout.js";
pub const RESERVED_PAGE: &str = "_uf.page.js";
pub const RESERVED_MIDDLEWARE: &str = "_uf.middleware.js";
pub const RESERVED_ROUTE: &str = "_uf.route.js";

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
    /// Every `_uf.middleware.js` that runs before this route resolves,
    /// outermost first.
    ///
    /// Inherited down the tree, the way layouts are, because that is what
    /// `packages/vite/internal/routes.js` puts on the route table the build
    /// actually runs (`ownMiddleware`, accumulated root-first). This field was
    /// a `has_middleware: bool` read off `directory`, and the two answer
    /// different questions: `app/dashboard/_uf.middleware.js` guards
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
        /// The `_uf.page.js` that cannot be reached.
        page: Utf8PathBuf,
        /// The catch-all directory, as it is written on disk.
        catch_all: String,
        /// The first routing directory below it.
        following: String,
        /// The catch-all's parameter name, for the suggested spelling.
        parameter: String,
    },
    /// A directory spelled the way a parallel route or an intercepting route
    /// is: `@team`, `(.)photo`. uf has neither.
    ///
    /// Refused rather than served, and the refusal is the whole of what
    /// ubugeeei-prod/uf#267 asks for first. Neither spelling was one this
    /// grammar had an opinion about, so both fell through to a literal
    /// segment: `@team` became the URL `/@team`, and `(.)photo` became
    /// `/(.)photo`, because the test for a `(group)` is that the segment
    /// *ends* in `)`. Both then appeared in the generated `RoutePath` union
    /// and in `uf inspect`, so a project migrating from Next.js got output
    /// that looked like it worked.
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
}

pub fn discover_routes(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<Route>, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

    // Before any route is built, because the point is that none is: a slot or
    // an interception used to become a literal URL segment and a live route.
    refuse_unsupported_directories(&app_root)?;

    let mut routes = Vec::new();
    for entry in WalkDir::new(&app_root) {
        let entry = entry.map_err(|source| RouterError::Walk {
            path: app_root.clone(),
            source,
        })?;
        if !entry.file_type().is_file() || entry.file_name() != RESERVED_PAGE {
            continue;
        }

        let page = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
        let directory = page.parent().unwrap_or(&app_root).to_path_buf();
        let relative = directory.strip_prefix(&app_root).unwrap_or(&directory);
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
            has_layout: directory.join(RESERVED_LAYOUT).exists(),
            middleware: middleware_chain(&app_root, &directory),
            directory,
            page,
            params,
        });
    }

    routes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(routes)
}

/// Refuse the directory spellings uf reserves without serving.
///
/// A pass of its own rather than a check inside the page walk above, and the
/// difference is what gets caught: `app/@team/` may hold a layout, a loading
/// file and no page at all, and it is still a directory the author wrote
/// expecting a parallel route. The walk above only ever sees `_uf.page.js`.
///
/// Private directories are pruned, because the build's router prunes them: a
/// leading `.` or `_` means the directory is a place to put things rather than
/// a route, so `app/_drafts/@team/` is not a route uf would have served and is
/// not one it should refuse.
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

/// Every `_uf.middleware.js` from `app_root` down to `directory`, outermost
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
        let file = dir.join(RESERVED_MIDDLEWARE);
        if file.is_file() {
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

pub fn generate_router_flow(routes: &[Route]) -> String {
    let mut output = String::from("// @flow\n\n");
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
    }
    // Written the way `uf fmt` writes it, down to the trailing comma: uf
    // scaffolds a project and then checks it with its own formatter, so a
    // generated file the formatter disagrees with fails `uf fmt --check` on
    // code nobody wrote. `the_generated_router_is_already_formatted` is what
    // keeps the two in step.
    output.push_str(
        "declare export function route<Path extends RoutePath>(\n  path: Path,\n  params: RouteParams[Path],\n): string;\n",
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
    fs::write(&manifest, generate_router_flow(&routes)).map_err(|source| RouterError::Write {
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
/// is fine.
fn non_terminal_catch_all(relative: &Utf8Path) -> Option<(String, String)> {
    let mut catch_all: Option<&str> = None;
    for segment in relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        if classify_route_segment(segment) == RouteSegment::Group {
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
            // A slot or an interception cannot reach here: `discover_routes`
            // refuses the directory before it builds a path. Spelled out
            // rather than folded into the literal arm so that a third
            // unsupported spelling has to be decided about here too.
            RouteSegment::Literal(name) => segments.push(name.to_string()),
            RouteSegment::Slot(_) | RouteSegment::Interception { .. } => {
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

fn route_params_type(params: &[RouteParam]) -> String {
    if params.is_empty() {
        return "empty".to_string();
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
