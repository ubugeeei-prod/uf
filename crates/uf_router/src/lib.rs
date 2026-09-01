use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

pub const RESERVED_LAYOUT: &str = "_uf.layout.flow";
pub const RESERVED_PAGE: &str = "_uf.page.flow";
pub const RESERVED_MIDDLEWARE: &str = "_uf.middleware.flow";

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
    pub has_middleware: bool,
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
            has_middleware: directory.join(RESERVED_MIDDLEWARE).exists(),
            directory,
            page,
            params,
        });
    }

    routes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(routes)
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
        if file_name.starts_with("_uf.")
            && file_name != RESERVED_LAYOUT
            && file_name != RESERVED_PAGE
            && file_name != RESERVED_MIDDLEWARE
        {
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
                .map(|route| format!("'{}'", route.path))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        output.push_str(";\n\n");
    }

    output.push_str("export type RouteParams = {\n");
    for route in routes {
        output.push_str(&format!(
            "  +'{}': {},\n",
            route.path,
            route_params_type(&route.params)
        ));
    }
    output.push_str("};\n\n");
    output.push_str(
        "declare export function route<Path: RoutePath>(path: Path, params: $ElementType<RouteParams, Path>): string;\n",
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
            format!("+{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {fields} }}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_root_and_dynamic_routes() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::create_dir_all(root.join("app/users/[id]")).unwrap();
        fs::write(root.join("app/_uf.page.flow"), "// @flow\n").unwrap();
        fs::write(root.join("app/_uf.layout.flow"), "// @flow\n").unwrap();
        fs::write(root.join("app/users/[id]/_uf.page.flow"), "// @flow\n").unwrap();
        fs::write(
            root.join("app/users/[id]/_uf.middleware.flow"),
            "// @flow\n",
        )
        .unwrap();

        let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].path, "/");
        assert!(routes[0].has_layout);
        assert_eq!(routes[1].path, "/users/:id");
        assert_eq!(routes[1].params[0].name, "id");
        assert!(routes[1].has_middleware);
    }

    #[test]
    fn generates_router_flow_with_params() {
        let route = Route {
            path: "/users/:id".into(),
            directory: "app/users/[id]".into(),
            page: "app/users/[id]/_uf.page.flow".into(),
            params: vec![RouteParam {
                name: "id".into(),
                kind: RouteParamKind::Single,
            }],
            has_layout: false,
            has_middleware: false,
        };

        let source = generate_router_flow(&[route]);

        assert!(source.contains("export type RoutePath = '/users/:id';"));
        assert!(source.contains("+'/users/:id': { +id: string }"));
    }

    #[test]
    fn finds_invalid_reserved_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("app/_uf.route.flow"), "// @flow\n").unwrap();

        let violations = find_reserved_file_violations(&root, &UniflowedConfig::default()).unwrap();

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.file_name(), Some("_uf.route.flow"));
    }

    #[test]
    fn writes_router_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("app/_uf.page.flow"), "// @flow\n").unwrap();

        let manifest = write_router_manifest(&root, &UniflowedConfig::default())
            .unwrap()
            .unwrap();

        assert_eq!(manifest.file_name(), Some("router.flow"));
        assert!(fs::read_to_string(manifest).unwrap().contains("RoutePath"));
    }
}
