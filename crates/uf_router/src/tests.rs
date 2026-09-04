//! Route discovery, the reserved-file grammar, and the generated types.

use super::*;

#[test]
fn discovers_root_and_dynamic_routes() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/users/[id]")).unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/_uf.layout.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/users/[id]/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/users/[id]/_uf.middleware.js"), "// @flow\n").unwrap();

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
        page: "app/users/[id]/_uf.page.js".into(),
        params: vec![RouteParam {
            name: "id".into(),
            kind: RouteParamKind::Single,
        }],
        has_layout: false,
        has_middleware: false,
    };

    let source = generate_router_flow(&[route]);

    assert!(source.contains("export type RoutePath = \"/users/:id\";"));
    assert!(source.contains("\"/users/:id\": { id: string }"));
}

/// The generated route table is the whole set of routes, so its types are
/// exact: an inexact one would let a caller pass a path this router does not
/// serve, which is the mistake the generated types exist to prevent.
///
/// Exactness is now spelled with plain braces. Flow has been exact by default
/// since 2023 and rejects `exact_by_default=false` as deprecated, so `{ … }`
/// *is* the exact type and `{| … |}` is the legacy spelling of the same thing.
/// What actually has to be absent is the `...` that makes a type inexact.
#[test]
fn generated_router_types_are_exact() {
    let source = generate_router_flow(&[Route {
        path: "/users/:id".into(),
        directory: Utf8PathBuf::from("app/users/[id]"),
        page: Utf8PathBuf::from("app/users/[id]/_uf.page.js"),
        params: vec![RouteParam {
            name: "id".into(),
            kind: RouteParamKind::Single,
        }],
        has_layout: false,
        has_middleware: false,
    }]);

    assert!(
        !source.contains("..."),
        "an inexact route table would accept a path this router does not \
         serve:\n{source}"
    );
    assert!(
        !source.contains("{|"),
        "the legacy exact spelling is not what uf tells anyone to write:\n{source}"
    );
    assert!(source.contains("export type RouteParams = {\n"));
    assert!(source.contains(r#"  "/users/:id": { id: string },"#));
    assert!(source.ends_with("string;\n"));
}

#[test]
fn an_empty_project_still_generates_exact_types() {
    let source = generate_router_flow(&[]);

    assert!(source.contains("export type RoutePath = empty;"));
    assert!(
        source.contains("export type RouteParams = {};"),
        "an empty table is an empty exact object:\n{source}"
    );
    assert!(!source.contains("..."));
}

#[test]
fn a_route_handler_is_a_reserved_file_rather_than_a_violation() {
    // `_uf.route.js` answers a request instead of rendering a page. It was
    // an unknown name until route handlers existed, and the two tests that
    // used it as their example of an invalid one now use `_uf.handler.js`.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/api")).unwrap();
    fs::write(root.join("app/api/_uf.route.js"), "// @flow\n").unwrap();

    let violations = find_reserved_file_violations(&root, &UniflowedConfig::default()).unwrap();
    assert!(violations.is_empty(), "{violations:?}");

    let classified = classify_reserved_file(RESERVED_ROUTE);
    assert!(!classified.is_unknown());
}

#[test]
fn finds_invalid_reserved_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(root.join("app/_uf.handler.js"), "// @flow\n").unwrap();

    let violations = find_reserved_file_violations(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].path.file_name(), Some("_uf.handler.js"));
}

#[test]
fn writes_router_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();

    let manifest = write_router_manifest(&root, &UniflowedConfig::default())
        .unwrap()
        .unwrap();

    assert_eq!(manifest.file_name(), Some("router.js"));
    assert!(fs::read_to_string(manifest).unwrap().contains("RoutePath"));
}

/// The router types uf generates must be what `uf fmt` would write.
///
/// uf scaffolds a project and then checks it with its own formatter, so a
/// generated file the formatter disagrees with makes `uf fmt --check` fail on
/// code nobody wrote. It did: the declaration was emitted as
/// `route < Path extends RoutePath > (`, with spaces the printer does not put
/// there, and `uf fmt --check` failed on `docs/router.js` in uf's own
/// repository.
#[test]
fn the_generated_router_is_already_formatted() {
    let routes = vec![
        Route {
            path: "/".into(),
            directory: Utf8PathBuf::from("app"),
            page: Utf8PathBuf::from("app/_uf.page.js"),
            params: Vec::new(),
            has_layout: true,
            has_middleware: false,
        },
        Route {
            path: "/posts/:id".into(),
            directory: Utf8PathBuf::from("app/posts/[id]"),
            page: Utf8PathBuf::from("app/posts/[id]/_uf.page.js"),
            params: vec![RouteParam {
                name: "id".into(),
                kind: RouteParamKind::Single,
            }],
            has_layout: false,
            has_middleware: false,
        },
    ];

    let generated = generate_router_flow(&routes);
    let formatted = uf_fmt::format_source(&generated, &uf_config::FmtConfig::default())
        .expect("the generated router parses");

    similar_asserts::assert_eq!(generated, formatted.output);
    assert!(
        !formatted.changed,
        "uf generated a router the formatter it ships would rewrite"
    );
}

/// An empty route table is generated too — a project with no pages yet — and
/// is held to the same bar.
#[test]
fn an_empty_generated_router_is_already_formatted() {
    let generated = generate_router_flow(&[]);
    let formatted = uf_fmt::format_source(&generated, &uf_config::FmtConfig::default())
        .expect("the generated router parses");

    similar_asserts::assert_eq!(generated, formatted.output);
}
