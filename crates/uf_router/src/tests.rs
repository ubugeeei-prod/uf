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
    assert!(routes[1].has_own_middleware());
    assert_eq!(
        routes[1].middleware,
        vec![root.join("app/users/[id]/_uf.middleware.js")]
    );
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
        middleware: Vec::new(),
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
        middleware: Vec::new(),
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
            middleware: Vec::new(),
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
            middleware: Vec::new(),
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

/// A middleware guards everything below the directory that declares it.
///
/// The composition rule layouts already have, and the one
/// `packages/vite/internal/routes.js` implements for the table the build runs.
/// Route discovery answered the narrower question — "does *this* directory
/// declare one" — so `/dashboard/settings` looked unguarded to every caller in
/// Rust while the build's own router had `app/dashboard/_uf.middleware.js` on
/// it. `uf build` asks this to decide which prerendered files a guard never
/// sees, and the narrow answer would have told it none of them.
#[test]
fn a_middleware_guards_every_route_beneath_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/dashboard/settings")).unwrap();
    fs::create_dir_all(root.join("app/about")).unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/about/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/dashboard/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/dashboard/_uf.middleware.js"), "// @flow\n").unwrap();
    fs::write(
        root.join("app/dashboard/settings/_uf.page.js"),
        "// @flow\n",
    )
    .unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();
    let guarded = |path: &str| {
        routes
            .iter()
            .find(|route| route.path == path)
            .unwrap_or_else(|| panic!("no route {path} in {routes:#?}"))
    };

    assert!(!guarded("/").is_guarded());
    assert!(!guarded("/about").is_guarded());
    assert!(guarded("/dashboard").has_own_middleware());
    assert_eq!(
        guarded("/dashboard/settings").middleware,
        vec![root.join("app/dashboard/_uf.middleware.js")],
        "the guard is inherited, and the route below it is the one nobody \
         would notice was unguarded"
    );
    assert!(
        !guarded("/dashboard/settings").has_own_middleware(),
        "its own directory declares nothing, which is a different fact"
    );
}

/// A guard at the router root guards everything, and a nested one composes
/// with it outermost first.
#[test]
fn guards_compose_outermost_first() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/admin/users")).unwrap();
    fs::write(root.join("app/_uf.middleware.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/admin/_uf.middleware.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/admin/users/_uf.page.js"), "// @flow\n").unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(routes[0].path, "/");
    assert_eq!(
        routes[0].middleware,
        vec![root.join("app/_uf.middleware.js")]
    );
    assert_eq!(routes[1].path, "/admin/users");
    assert_eq!(
        routes[1].middleware,
        vec![
            root.join("app/_uf.middleware.js"),
            root.join("app/admin/_uf.middleware.js"),
        ]
    );
}

/// A prerendered file is named by URL, and the guard that covers it is known
/// by route, so one has to be matched against the other.
#[test]
fn a_route_recognises_the_urls_it_serves() {
    let route = |path: &str| Route {
        path: path.into(),
        directory: Utf8PathBuf::from("app"),
        page: Utf8PathBuf::from("app/_uf.page.js"),
        params: Vec::new(),
        has_layout: false,
        middleware: Vec::new(),
    };

    assert!(route("/").matches_url("/"));
    assert!(!route("/").matches_url("/about"));
    assert!(route("/about").matches_url("/about"));
    assert!(route("/about").matches_url("/about/"));
    assert!(!route("/about").matches_url("/about/us"));
    assert!(!route("/about").matches_url("/"));

    assert!(route("/posts/:slug").matches_url("/posts/hello-world"));
    assert!(!route("/posts/:slug").matches_url("/posts"));
    assert!(!route("/posts/:slug").matches_url("/posts/a/b"));
    assert!(!route("/posts/:slug").matches_url("/pages/hello-world"));

    // A catch-all takes the rest of the path and needs something to take:
    // `[...slug]` is not the directory above it.
    assert!(route("/docs/:slug*").matches_url("/docs/guide/routing"));
    assert!(route("/docs/:slug*").matches_url("/docs/guide"));
    assert!(!route("/docs/:slug*").matches_url("/docs"));
}

/// The ranking that decides which of two matching routes answers, checked
/// against the numbers in `packages/router/internal/runtime.js`'s
/// `specificity` — three for a static segment, two for a parameter, one for a
/// catch-all. That function is the source of truth; this is the copy, and a
/// copy that has drifted is worse than no copy, because `uf build` reports
/// guards in terms of it.
#[test]
fn route_specificity_scores_the_way_the_runtime_does() {
    let route = |path: &str| Route {
        path: path.into(),
        directory: Utf8PathBuf::from("app"),
        page: Utf8PathBuf::from("app/_uf.page.js"),
        params: Vec::new(),
        has_layout: false,
        middleware: Vec::new(),
    };

    assert_eq!(route("/").specificity(), 0);
    assert_eq!(route("/about").specificity(), 3);
    assert_eq!(route("/posts/:slug").specificity(), 5);
    assert_eq!(route("/docs/:slug*").specificity(), 4);

    // The pair literal-counting could not tell apart: two literals each, and
    // the parameter route is the more specific one.
    assert!(route("/posts/:a/:b/edit").specificity() > route("/posts/archive/:z*").specificity());

    // A longer path outranks a shorter one that also matches.
    assert!(route("/docs/:a/:b").specificity() > route("/docs/:slug*").specificity());
}

/// A catch-all takes every remaining segment of the URL, so a directory below
/// one is a page no request can reach: `/docs/:slug*/edit` needs a segment
/// after the catch-all has consumed them all. Both routers agree, and neither
/// used to say so — `matchSegments` compares `parts[parts.length]`, which is
/// `undefined`, against `edit` and gives up, and `matches_url` returns `false`
/// for every URL. Discovery accepted the directory and `uf build` then omitted
/// its prerendered documents from the guard report without a word.
#[test]
fn a_catch_all_with_a_directory_below_it_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/docs/[...slug]/edit")).unwrap();
    fs::write(
        root.join("app/docs/[...slug]/edit/_uf.page.js"),
        "// @flow\n",
    )
    .unwrap();

    let error = discover_routes(&root, &UniflowedConfig::default()).unwrap_err();

    let message = error.to_string();
    // The file to open, the segment that is wrong, and the segment that
    // proves it: a refusal that only says "invalid route" is a refusal the
    // reader has to reproduce before they can act on it.
    assert!(
        message.contains("app/docs/[...slug]/edit/_uf.page.js"),
        "{message}"
    );
    assert!(message.contains("[...slug]"), "{message}");
    assert!(message.contains("edit"), "{message}");
}

/// A catch-all that is last is what `[...slug]` is for, and a `(group)` below
/// it contributes no segment, so it is still last.
#[test]
fn a_terminal_catch_all_is_discovered() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/docs/[...slug]")).unwrap();
    fs::create_dir_all(root.join("app/files/[...path]/(internal)")).unwrap();
    fs::write(root.join("app/docs/[...slug]/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(
        root.join("app/files/[...path]/(internal)/_uf.page.js"),
        "// @flow\n",
    )
    .unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(
        routes
            .iter()
            .map(|route| route.path.as_str())
            .collect::<Vec<_>>(),
        ["/docs/:slug*", "/files/:path*"]
    );
}

/// The two routers have to accept the same files, and this is the only thing
/// that says so.
///
/// One of them decides what runs and the other decides what `uf build` says
/// about it. They drifted, and the result was uf's own documentation site
/// reporting `routes 1` beside `prerendered pages 30` — every `.mdx` page in
/// the guide invisible to the count (ubugeeei-prod/uf#437, #386).
///
/// Read out of the JavaScript rather than restated, because a second copy of a
/// list is how the first one went wrong. If `routes.js` moves, this fails and
/// names which list.
#[test]
fn the_two_routers_accept_the_same_extensions() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/vite/internal/routes.js"),
    )
    .expect("the router the build runs");

    for (name, ours) in [
        ("PAGE_EXTENSIONS", &PAGE_EXTENSIONS[..]),
        ("MODULE_EXTENSIONS", &MODULE_EXTENSIONS[..]),
    ] {
        let theirs = extensions_named(&source, name);
        assert_eq!(
            theirs, ours,
            "`{name}` in packages/vite/internal/routes.js and in uf_router disagree"
        );
    }
}

/// The `[".js", ".jsx"]` on the right of `const NAME =`, as a list.
///
/// `export` is optional in front of it: the two lists are module-private in
/// `routes.js` today, and exporting one is not a change that should turn this
/// check off by making the list look renamed.
fn extensions_named(source: &str, name: &str) -> Vec<&'static str> {
    let line = source
        .lines()
        .find(|line| {
            line.trim_start()
                .trim_start_matches("export ")
                .starts_with(&format!("const {name} ="))
        })
        .unwrap_or_else(|| panic!("routes.js no longer declares {name}"));
    let list = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inside, _)| inside)
        .unwrap_or_else(|| panic!("{name} is no longer an array literal: {line}"));
    list.split(',')
        .map(|entry| entry.trim().trim_matches('"'))
        .filter(|entry| !entry.is_empty())
        // Leaked so the comparison is against `&'static str` on both sides,
        // in a test that runs once.
        .map(|entry| &*Box::leak(entry.to_owned().into_boxed_str()))
        .collect()
}

/// A page written in any of the three is a route, which is the whole of #437.
#[test]
fn a_page_is_a_route_in_every_spelling_the_build_accepts() {
    for extension in PAGE_EXTENSIONS {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = camino::Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
        let app = root.join("app/guide");
        std::fs::create_dir_all(&app).expect("a directory");
        std::fs::write(app.join(format!("_uf.page{extension}")), "// @flow\n").expect("a page");

        let routes =
            discover_routes(root, &uf_config::UniflowedConfig::default()).expect("discovery");

        assert_eq!(
            routes
                .iter()
                .map(|route| route.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/guide"],
            "a `_uf.page{extension}` was not a route"
        );
    }
}

/// A directory holding two spellings of the page is one route, and it is the
/// one the build renders.
///
/// The walk that finds pages is over *files*, so `app/guide/` with both
/// `_uf.page.js` and `_uf.page.mdx` in it reached the route builder twice and
/// produced two `/guide` routes — while `packages/vite/internal/routes.js`
/// asks `findModule` once and renders exactly one of them. The generated
/// `RoutePath` carried the same string twice, `uf build`'s summary counted the
/// page twice, and `uf inspect` listed a route nothing serves.
///
/// Both halves are asserted, because "one route" alone would pass if the wrong
/// spelling won: the surviving page has to be the file the build loads, which
/// is the first entry of `PAGE_EXTENSIONS` that is present.
#[test]
fn a_directory_with_two_page_spellings_is_one_route() {
    // Every non-empty subset of the spellings, so the assertion is about the
    // order rather than about the one pair that motivated it.
    for present in 1u32..(1 << PAGE_EXTENSIONS.len()) {
        let spellings: Vec<&str> = PAGE_EXTENSIONS
            .iter()
            .enumerate()
            .filter(|(index, _)| present & (1 << index) != 0)
            .map(|(_, extension)| *extension)
            .collect();

        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = camino::Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
        let app = root.join("app/guide");
        std::fs::create_dir_all(&app).expect("a directory");
        for extension in &spellings {
            std::fs::write(app.join(format!("_uf.page{extension}")), "// @flow\n").expect("a page");
        }

        let routes =
            discover_routes(root, &uf_config::UniflowedConfig::default()).expect("discovery");

        assert_eq!(
            routes
                .iter()
                .map(|route| route.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/guide"],
            "{spellings:?} in one directory is one route"
        );
        assert_eq!(
            routes[0].page.file_name(),
            Some(format!("_uf.page{}", spellings[0]).as_str()),
            "{spellings:?} resolved to a page the build router would not have loaded"
        );
    }
}

/// And a layout or a middleware beside it is found in either of its two.
#[test]
fn a_layout_and_a_middleware_are_found_in_either_spelling() {
    for extension in MODULE_EXTENSIONS {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = camino::Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
        let app = root.join("app/guide");
        std::fs::create_dir_all(&app).expect("a directory");
        std::fs::write(app.join("_uf.page.js"), "// @flow\n").expect("a page");
        std::fs::write(app.join(format!("_uf.layout{extension}")), "// @flow\n").expect("a layout");
        std::fs::write(
            root.join(format!("app/_uf.middleware{extension}")),
            "// @flow\n",
        )
        .expect("a middleware");

        let routes =
            discover_routes(root, &uf_config::UniflowedConfig::default()).expect("discovery");

        assert!(routes[0].has_layout, "a `_uf.layout{extension}` was missed");
        assert_eq!(
            routes[0].middleware.len(),
            1,
            "a `_uf.middleware{extension}` was missed"
        );
    }
}

/// A slot directory is refused rather than served as the URL segment `/@team`.
///
/// The whole of ubugeeei-prod/uf#267's first piece. `@team` was not a spelling
/// this grammar had an opinion about, so it fell through to a literal: the
/// route was discovered, `/@team` went into the generated `RoutePath`, and
/// `route("/@team", …)` type checked. A project migrating from Next.js got
/// output that looked like it worked.
#[test]
fn a_parallel_route_slot_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/dashboard/@team")).unwrap();
    fs::write(root.join("app/dashboard/@team/_uf.page.js"), "// @flow\n").unwrap();

    let error = discover_routes(&root, &UniflowedConfig::default()).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("app/dashboard/@team"), "{message}");
    assert!(message.contains("parallel route"), "{message}");
    // It has to say it is refused. "Not supported" reads as "ignored", and
    // being quietly ignored is what this replaced.
    assert!(message.contains("refused"), "{message}");
}

/// A slot with no page at all is still refused.
///
/// The reason the check is a directory pass rather than a line in the page
/// walk: `app/@team/` may hold a layout, a loading file and a `default.js` and
/// no page, and it is still a directory somebody wrote expecting a parallel
/// route. The page walk only ever sees `_uf.page.js`.
#[test]
fn a_slot_holding_no_page_is_refused_too() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/@team")).unwrap();
    fs::write(root.join("app/@team/_uf.layout.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();

    let error = discover_routes(&root, &UniflowedConfig::default()).unwrap_err();

    assert!(error.to_string().contains("@team"), "{error}");
}

/// Every interception spelling Next.js defines, refused by the same rule.
#[test]
fn an_intercepting_route_is_refused() {
    for segment in ["(.)photo", "(..)photo", "(...)photo", "(..)(..)photo"] {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::create_dir_all(root.join("app/feed").join(segment)).unwrap();
        fs::write(
            root.join("app/feed").join(segment).join("_uf.page.js"),
            "// @flow\n",
        )
        .unwrap();

        let error = discover_routes(&root, &UniflowedConfig::default()).unwrap_err();

        let message = error.to_string();
        assert!(message.contains(segment), "{segment}: {message}");
        assert!(
            message.contains("intercepting route"),
            "{segment}: {message}"
        );
        assert!(message.contains("refused"), "{segment}: {message}");
    }
}

/// A route group is not an interception, which is the near-miss that made
/// `(.)photo` a literal in the first place: the test for a group is that the
/// segment *ends* in `)`.
#[test]
fn a_route_group_is_still_discovered() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/(marketing)/about")).unwrap();
    fs::write(root.join("app/(marketing)/about/_uf.page.js"), "// @flow\n").unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/about");
}

/// A private directory is not a route, so a slot inside one is not refused.
///
/// Both routers skip a directory whose name starts with `.` or `_`, so
/// `app/_drafts/@team/` was never going to be served — refusing it would be
/// the linter inventing a rule about a place the router does not look.
#[test]
fn a_slot_inside_a_private_directory_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/_drafts/@team")).unwrap();
    fs::write(root.join("app/_drafts/@team/notes.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/");
}

/// The generated types are the reason the refusal matters at all.
///
/// `RoutePath` is a closed union built from what discovery found, so a slot
/// that discovery accepted became a path a caller could pass to `route()` and
/// Flow would agree with them. Refusing the directory is what keeps the union
/// honest — there is no route, so there is no type for one.
#[test]
fn a_refused_directory_never_reaches_the_generated_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/@team")).unwrap();
    fs::write(root.join("app/@team/_uf.page.js"), "// @flow\n").unwrap();

    let error = write_router_manifest(&root, &UniflowedConfig::default()).unwrap_err();

    assert!(error.to_string().contains("@team"), "{error}");
    assert!(
        !root.join("router.js").exists(),
        "a manifest was written for a project the router refuses"
    );
}

/// The two roles a static host has no answer for, found where the route table
/// cannot report them.
///
/// Both halves of that are the assertion. `app/api/health` has a handler and
/// no page, so it is in no `Route` at all; `app/dashboard` declares a
/// middleware and has no page either, so it is in no `Route::middleware`
/// list — the only route below it that would have carried the chain is
/// `/dashboard/settings`, and a project may guard a subtree before it has one.
/// A caller asking "can this project be served by a static host" got `false`
/// from neither.
#[test]
fn discovers_the_route_handlers_and_middleware_the_route_table_does_not_carry() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/api/health")).unwrap();
    fs::create_dir_all(root.join("app/dashboard")).unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/api/health/_uf.route.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/dashboard/_uf.middleware.js"), "// @flow\n").unwrap();

    let routes = discover_routes(&root, &UniflowedConfig::default()).unwrap();
    assert_eq!(routes.len(), 1, "only `/` has a page");
    assert!(routes[0].middleware.is_empty());

    let modules = discover_server_modules(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0].path, "/api/health");
    assert_eq!(modules[0].kind, ServerModuleKind::RouteHandler);
    assert_eq!(modules[0].file, root.join("app/api/health/_uf.route.js"));
    assert_eq!(modules[1].path, "/dashboard");
    assert_eq!(modules[1].kind, ServerModuleKind::Middleware);
}

/// One report per directory, whichever spelling the build's router would run.
///
/// `find_module` decides that for pages, and it has to decide it here too: a
/// directory holding both spellings is one handler, and reporting two would
/// make a refusal that lists them read as if the project had a problem twice.
#[test]
fn a_directory_with_two_spellings_of_a_handler_is_one_handler() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/api")).unwrap();
    fs::write(root.join("app/api/_uf.route.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/api/_uf.route.jsx"), "// @flow\n").unwrap();

    let modules = discover_server_modules(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].file, root.join("app/api/_uf.route.js"));
}

/// A project with neither is a project a static host can serve, and says so
/// with an empty list rather than with an absent one.
#[test]
fn a_project_with_no_server_modules_reports_none() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/guide")).unwrap();
    fs::write(root.join("app/_uf.page.js"), "// @flow\n").unwrap();
    fs::write(root.join("app/guide/_uf.page.js"), "// @flow\n").unwrap();

    assert!(
        discover_server_modules(&root, &UniflowedConfig::default())
            .unwrap()
            .is_empty()
    );
}
