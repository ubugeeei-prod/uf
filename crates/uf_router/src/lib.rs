pub mod reserved;

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

pub use crate::reserved::{
    ReservedFile, ReservedName, ReservedRole, ReservedVariant, classify_reserved_file,
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
                // The last thing in the path by construction, so whatever is
                // left of the URL is the catch-all's, and there must be some.
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
}

pub fn discover_routes(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<Route>, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    if !app_root.exists() {
        return Ok(Vec::new());
    }

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

fn route_path_and_params(relative: &Utf8Path) -> (String, Vec<RouteParam>) {
    let mut params = Vec::new();
    let mut segments = Vec::new();

    for segment in relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        if segment.starts_with('(') && segment.ends_with(')') {
            continue;
        }

        if let Some(name) = segment
            .strip_prefix("[...")
            .and_then(|name| name.strip_suffix(']'))
        {
            params.push(RouteParam {
                name: name.to_compact_string(),
                kind: RouteParamKind::CatchAll,
            });
            segments.push(format!(":{name}*"));
            continue;
        }

        if let Some(name) = segment
            .strip_prefix('[')
            .and_then(|name| name.strip_suffix(']'))
        {
            params.push(RouteParam {
                name: name.to_compact_string(),
                kind: RouteParamKind::Single,
            });
            segments.push(format!(":{name}"));
            continue;
        }

        segments.push(segment.to_string());
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
