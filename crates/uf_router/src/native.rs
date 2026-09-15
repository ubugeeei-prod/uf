//! The native route table: what Metro bundles where Vite serves
//! `virtual:uf/routes`.
//!
//! The web router's table is a virtual module `@uniflowed/vite` generates
//! inside Vite's plugin pipeline, and Metro has no virtual modules. So for a
//! React Native app the table is a real file, written beside the web router's
//! typed `router.js` once per platform — `router.ios.js`, `router.android.js`
//! and `router.native.js` — and Metro's own platform resolution does the
//! choosing: a module that imports `./router` gets `router.ios.js` in an iOS
//! bundle and `router.android.js` in an Android one. See ubugeeei-prod/uf#981.
//!
//! # Why one module per platform
//!
//! The other design was one module over Metro's resolution: import
//! `./app/users/[id]/$page` and let Metro pick `$page.ios.js`. That would make
//! Metro the second place the precedence is decided, and it is not the same
//! rule — Metro knows nothing of `$page.web.js`, and a web-only directory
//! would become a native route that fails to resolve. Each module here names
//! the exact file [`crate::discover_routes_for_target`] chose, so the
//! precedence is decided once, by the scanner `uf build` and `uf routes list`
//! already use, and `tests/scanners_agree.rs` holds that scanner to the one the
//! web build runs.
//!
//! All three are written whichever native target was named, because one Metro
//! server — `expo start`, `react-native start` — serves iOS and Android at
//! once, and `router.native.js` is what an out-of-tree platform gets.
//!
//! # What the table holds
//!
//! The `RouteTable` shape `@uniflowed/router/native` reads: every route with a
//! page on the platform, its parameters and its layouts, root first, and the
//! not-found and error boundaries the project declared. Layouts are one lazy
//! module each, shared by every route under them, so a navigator can group
//! routes by the layout they share; the separate `layouts` export says which
//! directory each one belongs to, which is what nesting navigators needs.
//!
//! Not in it: loading boundaries, templates and parallel-route slots, which are
//! the web renderer's, and the boundaries the web table synthesises at `/` when
//! a project declares none — a native navigator has no document to put them in.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

use super::{
    MODULE_EXTENSIONS, PAGE_EXTENSIONS, RESERVED_LAYOUT_STEM, Route, RouteParamKind, RouteTarget,
    RouterError, discover_routes_for_target, find_module_for_target, generate_router_flow,
    route_path_and_params, slot_in,
};
use crate::reserved::ReservedRole;

/// The platforms a native route table is written for, in the order they are
/// written.
pub const NATIVE_PLATFORMS: [RouteTarget; 3] =
    [RouteTarget::Ios, RouteTarget::Android, RouteTarget::Native];

/// One route in a table, with the layouts that wrap it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRoute {
    /// The route, as [`discover_routes_for_target`] found it.
    pub route: Route,
    /// Every layout from the router root down to the route's directory, root
    /// first — the chain the web router's `scanRoutes` builds on its descent.
    pub layouts: Vec<Utf8PathBuf>,
}

/// A not-found or error boundary a project declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBoundary {
    /// The path of the segment that declares it.
    pub path: CompactString,
    /// The boundary module.
    pub file: Utf8PathBuf,
    /// The layouts around it, root first.
    pub layouts: Vec<Utf8PathBuf>,
}

/// The route table for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTable {
    /// The target whose files were chosen.
    pub target: RouteTarget,
    /// Every route with a page on the target, sorted by path.
    pub routes: Vec<TableRoute>,
    /// The `$not-found` boundaries, sorted by path.
    pub not_found: Vec<TableBoundary>,
    /// The `$error` boundaries, sorted by path.
    pub errors: Vec<TableBoundary>,
}

/// What [`write_native_route_tables`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeRouterModules {
    /// Every module that now holds a table, written or already current.
    pub files: Vec<Utf8PathBuf>,
    /// The modules whose contents changed.
    pub changed: Vec<Utf8PathBuf>,
    /// How many distinct route paths the tables hold between them.
    pub routes: usize,
}

/// The route table `target` resolves: routes, layouts and boundaries.
///
/// # Errors
///
/// Whatever [`discover_routes_for_target`] refuses, and a directory the walk
/// cannot read.
pub fn discover_route_table(
    root: &Utf8Path,
    config: &UniflowedConfig,
    target: RouteTarget,
) -> Result<RouteTable, RouterError> {
    let app_root = root.join(config.app.router.root.as_str());
    let routes = discover_routes_for_target(root, config, target)?
        .into_iter()
        .map(|route| TableRoute {
            layouts: layout_chain(&app_root, &route.directory, target),
            route,
        })
        .collect();

    let mut not_found = Vec::new();
    let mut errors = Vec::new();
    if app_root.is_dir() {
        let not_found_stem = format!("${}", ReservedRole::NotFound.as_str());
        let error_stem = format!("${}", ReservedRole::Error.as_str());
        for entry in WalkDir::new(&app_root).sort_by_file_name() {
            let entry = entry.map_err(|source| RouterError::Walk {
                path: app_root.clone(),
                source,
            })?;
            if !entry.file_type().is_dir() {
                continue;
            }
            let directory = Utf8PathBuf::from_path_buf(entry.into_path())
                .map_err(|path| RouterError::NonUtf8(path.display().to_string()))?;
            let relative = directory
                .strip_prefix(&app_root)
                .unwrap_or(&directory)
                .to_path_buf();
            // A slot is a place a layout renders into rather than a segment a
            // navigator has, so nothing under one is in a native table.
            if slot_in(&relative).is_some() {
                continue;
            }
            let path = CompactString::from(route_path_and_params(&relative).0);
            if let Some(file) =
                find_module_for_target(&directory, &not_found_stem, &PAGE_EXTENSIONS, target)
            {
                not_found.push(TableBoundary {
                    path: path.clone(),
                    file,
                    layouts: layout_chain(&app_root, &directory, target),
                });
            }
            if let Some(file) =
                find_module_for_target(&directory, &error_stem, &MODULE_EXTENSIONS, target)
            {
                errors.push(TableBoundary {
                    path,
                    file,
                    layouts: layout_chain(&app_root, &directory, target),
                });
            }
        }
    }
    let by_path = |a: &TableBoundary, b: &TableBoundary| {
        a.path.cmp(&b.path).then_with(|| a.file.cmp(&b.file))
    };
    not_found.sort_by(by_path);
    errors.sort_by(by_path);

    Ok(RouteTable {
        target,
        routes,
        not_found,
        errors,
    })
}

/// The table for every native platform, or none when the project has no router
/// root to read one from.
///
/// None rather than three empty tables: a React Native app is as likely to be
/// one `App.js` registered with `AppRegistry`, and three generated files
/// appearing in it would be files nobody asked for.
///
/// # Errors
///
/// Whatever [`discover_route_table`] refuses.
pub fn discover_native_route_tables(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<RouteTable>, RouterError> {
    if !config.app.router.enabled || !root.join(config.app.router.root.as_str()).is_dir() {
        return Ok(Vec::new());
    }
    NATIVE_PLATFORMS
        .iter()
        .map(|platform| discover_route_table(root, config, *platform))
        .collect()
}

/// Write each table beside the web router's manifest, touching only the files
/// whose contents change.
///
/// A file rewritten with the same bytes is still a change to Metro's watcher,
/// which answers it with a rebuild and a hot update, so an unchanged table
/// leaves its file alone.
///
/// # Errors
///
/// A module that cannot be written.
pub fn write_native_route_tables(
    root: &Utf8Path,
    config: &UniflowedConfig,
    tables: &[RouteTable],
) -> Result<NativeRouterModules, RouterError> {
    let mut written = NativeRouterModules::default();
    let mut paths = BTreeSet::new();
    for table in tables {
        paths.extend(table.routes.iter().map(|route| route.route.path.clone()));
        let generated = generate_native_router_flow(table, root, config);
        // Through the formatter, for `write_router_manifest_for_target`'s
        // reason: a generated file `uf fmt --check` rejects fails a project on
        // code nobody wrote.
        let contents = uf_fmt::format_source(&generated, &config.fmt)
            .map_or(generated, |formatted| formatted.output);
        let module = native_router_module(root, config, table.target);
        if fs::read_to_string(&module).ok().as_deref() != Some(contents.as_str()) {
            fs::write(&module, &contents).map_err(|source| RouterError::Write {
                path: module.clone(),
                source,
            })?;
            written.changed.push(module.clone());
        }
        written.files.push(module);
    }
    written.routes = paths.len();
    Ok(written)
}

/// [`discover_native_route_tables`] and [`write_native_route_tables`] in one.
///
/// # Errors
///
/// Either's.
pub fn write_native_router_modules(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<NativeRouterModules, RouterError> {
    let tables = discover_native_route_tables(root, config)?;
    write_native_route_tables(root, config, &tables)
}

/// Where `platform`'s table is written: the web manifest's name with the
/// platform before the extension, so `router.js` becomes `router.ios.js`.
#[must_use]
pub fn native_router_module(
    root: &Utf8Path,
    config: &UniflowedConfig,
    platform: RouteTarget,
) -> Utf8PathBuf {
    let manifest = root.join(config.app.router.manifest.as_str());
    let stem = manifest.file_stem().unwrap_or("router");
    let extension = manifest.extension().unwrap_or("js");
    manifest.with_file_name(format!("{stem}.{}.{extension}", platform.as_str()))
}

/// The Flow module for one table: the typed routes `router.js` has, and the
/// table itself.
#[must_use]
pub fn generate_native_router_flow(
    table: &RouteTable,
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> String {
    let module = native_router_module(root, config, table.target);
    let directory = module.parent().unwrap_or(root);
    let manifest = Utf8Path::new(config.app.router.manifest.as_str())
        .file_name()
        .unwrap_or("router.js");
    let platform = match table.target {
        RouteTarget::Ios => "an iOS",
        RouteTarget::Android => "an Android",
        RouteTarget::Native => "a React Native",
        RouteTarget::Web => "a web",
    };

    let mut layouts: Vec<&Utf8Path> = Vec::new();
    let chains = table
        .routes
        .iter()
        .map(|route| &route.layouts)
        .chain(table.not_found.iter().map(|boundary| &boundary.layouts))
        .chain(table.errors.iter().map(|boundary| &boundary.layouts));
    for chain in chains {
        for layout in chain {
            if !layouts.contains(&layout.as_path()) {
                layouts.push(layout);
            }
        }
    }
    let layout_ids = |chain: &[Utf8PathBuf]| {
        chain
            .iter()
            .filter_map(|layout| layouts.iter().position(|known| *known == layout.as_path()))
            .map(|index| format!("layout{index}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let lazy = |file: &Utf8Path| {
        format!(
            "() => import({})",
            js_string(&import_specifier(directory, file))
        )
    };
    let display = |file: &Utf8Path| js_string(&display_path(root, file));

    let mut output = format!(
        "// @flow\n//\n// The route table for {platform} bundle, generated by `uf` from the router root. Metro\n// resolves `./router` to this file for that platform, beside the web router's `{manifest}`.\n// Do not edit it: it is written again whenever the routes change.\n\nimport type {{ RouteTable }} from \"@uniflowed/router/routing\";\n"
    );
    let routes = table
        .routes
        .iter()
        .map(|route| route.route.clone())
        .collect::<Vec<_>>();
    let typed = generate_router_flow(&routes);
    output.push_str(typed.strip_prefix("// @flow\n\n").unwrap_or(&typed));
    output.push('\n');

    for (index, layout) in layouts.iter().enumerate() {
        let _ = writeln!(output, "const layout{index} = {};", lazy(layout));
    }
    if !layouts.is_empty() {
        output.push('\n');
    }

    output.push_str(
        "export const routeTable: RouteTable<mixed, mixed, mixed, mixed, mixed> = {\n  routes: [\n",
    );
    for route in &table.routes {
        let params = route
            .route
            .params
            .iter()
            .map(|param| {
                format!(
                    "{{ name: {}, catchAll: {} }}",
                    js_string(&param.name),
                    matches!(param.kind, RouteParamKind::CatchAll)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            output,
            "    {{\n      path: {},\n      params: [{params}],\n      mdx: {},\n      file: {},\n      page: {},\n      layouts: [{}],\n    }},\n",
            js_string(&route.route.path),
            route.route.page.extension() == Some("mdx"),
            display(&route.route.page),
            lazy(&route.route.page),
            layout_ids(&route.layouts),
        );
    }
    output.push_str("  ],\n  notFound: [\n");
    for boundary in &table.not_found {
        let _ = write!(
            output,
            "    {{\n      path: {},\n      mdx: {},\n      file: {},\n      page: {},\n      layouts: [{}],\n    }},\n",
            js_string(&boundary.path),
            boundary.file.extension() == Some("mdx"),
            display(&boundary.file),
            lazy(&boundary.file),
            layout_ids(&boundary.layouts),
        );
    }
    output.push_str("  ],\n  errors: [\n");
    for boundary in &table.errors {
        let _ = write!(
            output,
            "    {{\n      path: {},\n      file: {},\n      module: {},\n      layouts: [{}],\n    }},\n",
            js_string(&boundary.path),
            display(&boundary.file),
            lazy(&boundary.file),
            layout_ids(&boundary.layouts),
        );
    }
    output.push_str("  ],\n};\n\n");

    output.push_str(
        "export const layouts: $ReadOnlyArray<{\n  readonly segment: string,\n  readonly file: string,\n  readonly module: () => Promise<mixed>,\n}> = [\n",
    );
    let app_root = root.join(config.app.router.root.as_str());
    for (index, layout) in layouts.iter().enumerate() {
        let segment = layout
            .parent()
            .and_then(|parent| parent.strip_prefix(&app_root).ok())
            .map_or_else(
                || "/".to_string(),
                |relative| format!("/{}", slash_joined(relative)),
            );
        let segment = if segment == "/" {
            segment
        } else {
            segment.trim_end_matches('/').to_string()
        };
        let _ = writeln!(
            output,
            "  {{ segment: {}, file: {}, module: layout{index} }},",
            js_string(&segment),
            display(layout),
        );
    }
    output.push_str("];\n");
    output
}

/// Every layout from `app_root` down to `directory`, root first.
fn layout_chain(
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

/// `file` as a specifier imported from `directory`: `./app/$page.ios.js`, or
/// `../app/$page.ios.js` from a manifest one level down.
fn import_specifier(directory: &Utf8Path, file: &Utf8Path) -> String {
    let from = directory.components().collect::<Vec<_>>();
    let to = file.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len() - common];
    parts.extend(to[common..].iter().map(|component| component.as_str()));
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

/// `file` relative to the project root, with `/` separators, as the web
/// table's `file` fields are written: a path a person can act on that does not
/// describe the machine that generated it.
fn display_path(root: &Utf8Path, file: &Utf8Path) -> String {
    file.strip_prefix(root)
        .map_or_else(|_| file.to_string(), slash_joined)
}

fn slash_joined(path: &Utf8Path) -> String {
    path.components()
        .map(|component| component.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// A JavaScript string literal for `value`.
fn js_string(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\u{2028}' => literal.push_str("\\u2028"),
            '\u{2029}' => literal.push_str("\\u2029"),
            character if character.is_control() => {
                let _ = write!(literal, "\\u{:04x}", u32::from(character));
            }
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project whose router root exercises every precedence rule once.
    fn project() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        for file in [
            "app/$layout.native.js",
            "app/$page.js",
            "app/$not-found.native.js",
            "app/(tabs)/$layout.js",
            "app/(tabs)/feed/$page.js",
            "app/(tabs)/feed/$page.ios.js",
            "app/users/$error.js",
            "app/users/[id]/$page.native.js",
            "app/users/[id]/$page.android.js",
            "app/docs/[...slug]/$page.js",
            "app/web-only/$page.web.js",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "// @flow\n").unwrap();
        }
        (dir, root)
    }

    fn relative(root: &Utf8Path, file: &Utf8Path) -> String {
        display_path(root, file)
    }

    #[test]
    fn each_platform_gets_its_own_files_and_the_layouts_above_them() {
        let (_dir, root) = project();
        let config = UniflowedConfig::default();

        let ios = discover_route_table(&root, &config, RouteTarget::Ios).unwrap();
        let android = discover_route_table(&root, &config, RouteTarget::Android).unwrap();

        let pages = |table: &RouteTable| {
            table
                .routes
                .iter()
                .map(|route| {
                    format!(
                        "{} {}",
                        route.route.path,
                        relative(&root, &route.route.page)
                    )
                })
                .collect::<Vec<_>>()
        };
        similar_asserts::assert_eq!(
            pages(&ios),
            [
                "/ app/$page.js",
                "/docs/:slug* app/docs/[...slug]/$page.js",
                "/feed app/(tabs)/feed/$page.ios.js",
                "/users/:id app/users/[id]/$page.native.js",
            ]
        );
        similar_asserts::assert_eq!(
            pages(&android),
            [
                "/ app/$page.js",
                "/docs/:slug* app/docs/[...slug]/$page.js",
                "/feed app/(tabs)/feed/$page.js",
                "/users/:id app/users/[id]/$page.android.js",
            ]
        );
        let feed = &ios.routes[2];
        assert_eq!(
            feed.layouts
                .iter()
                .map(|layout| relative(&root, layout))
                .collect::<Vec<_>>(),
            ["app/$layout.native.js", "app/(tabs)/$layout.js"]
        );
        assert_eq!(ios.not_found.len(), 1);
        assert_eq!(ios.not_found[0].path, "/");
        assert_eq!(ios.errors.len(), 1);
        assert_eq!(ios.errors[0].path, "/users");
        assert_eq!(relative(&root, &ios.errors[0].file), "app/users/$error.js");
    }

    #[test]
    fn the_module_imports_each_file_once_relative_to_where_it_is_written() {
        let (_dir, root) = project();
        let mut config = UniflowedConfig::default();
        config.app.router.manifest = CompactString::const_new("src/router.js");
        let table = discover_route_table(&root, &config, RouteTarget::Ios).unwrap();

        let module = generate_native_router_flow(&table, &root, &config);

        assert_eq!(
            native_router_module(&root, &config, RouteTarget::Ios),
            root.join("src/router.ios.js")
        );
        assert!(
            module.contains("const layout0 = () => import(\"../app/$layout.native.js\");"),
            "{module}"
        );
        assert_eq!(
            module
                .matches("import(\"../app/$layout.native.js\")")
                .count(),
            1,
            "{module}"
        );
        assert!(
            module.contains("page: () => import(\"../app/(tabs)/feed/$page.ios.js\")"),
            "{module}"
        );
        assert!(
            module.contains("file: \"app/users/[id]/$page.native.js\""),
            "{module}"
        );
        assert!(
            module.contains("params: [{ name: \"slug\", catchAll: true }]"),
            "{module}"
        );
        assert!(
            module.contains(
                "export type RoutePath = \"/\" | \"/docs/:slug*\" | \"/feed\" | \"/users/:id\";"
            ),
            "{module}"
        );
        assert!(
            module.contains(
                "{ segment: \"/(tabs)\", file: \"app/(tabs)/$layout.js\", module: layout1 },"
            ),
            "{module}"
        );
        assert!(!module.contains("web-only"), "{module}");
    }

    #[test]
    fn modules_are_written_beside_the_manifest_and_only_when_they_change() {
        let (_dir, root) = project();
        let config = UniflowedConfig::default();

        let first = write_native_router_modules(&root, &config).unwrap();
        assert_eq!(
            first
                .files
                .iter()
                .map(|file| relative(&root, file))
                .collect::<Vec<_>>(),
            ["router.ios.js", "router.android.js", "router.native.js"]
        );
        assert_eq!(first.changed, first.files);
        assert_eq!(first.routes, 4);
        assert!(
            !root.join("router.js").exists(),
            "the web router is not touched"
        );

        let second = write_native_router_modules(&root, &config).unwrap();
        assert!(second.changed.is_empty(), "{second:?}");

        fs::create_dir_all(root.join("app/about")).unwrap();
        fs::write(root.join("app/about/$page.native.js"), "// @flow\n").unwrap();
        let third = write_native_router_modules(&root, &config).unwrap();
        assert_eq!(third.changed.len(), 3);
        assert!(
            fs::read_to_string(root.join("router.android.js"))
                .unwrap()
                .contains("\"/about\"")
        );
    }

    #[test]
    fn a_project_without_a_router_root_gets_no_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

        let written = write_native_router_modules(&root, &UniflowedConfig::default()).unwrap();

        assert_eq!(written, NativeRouterModules::default());
        assert!(!root.join("router.ios.js").exists());
    }

    #[test]
    fn specifiers_are_relative_to_the_module_and_strings_are_escaped() {
        assert_eq!(
            import_specifier(Utf8Path::new("/p"), Utf8Path::new("/p/app/$page.js")),
            "./app/$page.js"
        );
        assert_eq!(
            import_specifier(Utf8Path::new("/p/src"), Utf8Path::new("/p/app/$page.js")),
            "../app/$page.js"
        );
        assert_eq!(js_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
