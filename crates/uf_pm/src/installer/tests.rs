use super::*;

#[test]
fn frozen_local_install_is_offline_and_rejects_changed_local_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"app","dependencies":{"lib":"file:lib"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/package.json"),
        r#"{"name":"lib","version":"1.0.0"}"#,
    )
    .unwrap();
    let store = || store::Store {
        root: root.join("store"),
    };
    let config = UniflowedConfig::default();
    let first = install_with_store(root, &config, Options::default(), store()).unwrap();
    assert_eq!(first.downloaded, 0);
    assert!(root.join("node_modules/lib/package.json").is_file());
    let bytes = fs::read(root.join("uf.lock")).unwrap();
    let next = install_with_store(
        root,
        &config,
        Options {
            frozen: true,
            ..Options::default()
        },
        store(),
    )
    .unwrap();
    assert_eq!(next.downloaded, 0);
    assert_eq!(next.hardlinked, 0);
    assert_eq!(fs::read(root.join("uf.lock")).unwrap(), bytes);
    fs::write(
        root.join("lib/package.json"),
        r#"{"name":"lib","version":"2.0.0"}"#,
    )
    .unwrap();
    assert!(
        install_with_store(
            root,
            &config,
            Options {
                frozen: true,
                ..Options::default()
            },
            store()
        )
        .is_err()
    );
    assert_eq!(fs::read(root.join("uf.lock")).unwrap(), bytes);
}

#[test]
fn npm_ranges_tags_and_aliases_resolve_without_guessing() {
    let document = serde_json::json!({"dist-tags":{"latest":"2.0.0"},"versions":{
        "1.2.0":{"name":"lib","version":"1.2.0","dist":{"tarball":"https://example.org/lib.tgz","integrity":"sha512-x"}},
        "1.5.0":{"name":"lib","version":"1.5.0","dist":{"tarball":"https://example.org/lib.tgz","integrity":"sha512-x"}},
        "2.0.0":{"name":"lib","version":"2.0.0","dist":{"tarball":"https://example.org/lib.tgz","integrity":"sha512-x"}}}});
    assert_eq!(
        registry_node(
            &document,
            "lib",
            "^1.2 || ~1.5",
            "https://registry.npmjs.org"
        )
        .unwrap()
        .version,
        "1.5.0"
    );
    assert_eq!(
        registry_node(&document, "lib", "latest", "https://registry.npmjs.org")
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        alias("renamed", "npm:@scope/lib@^1"),
        ("@scope/lib".into(), "^1".into())
    );
    assert!(!satisfies("^1.0.0", "1.3.0-rc.1"));
}

#[test]
fn production_selection_keeps_transitive_packages_and_skips_optional_platforms() {
    let mut importer = Importer {
        path: ".".into(),
        name: "app".into(),
        version: "1.0.0".into(),
        dependencies: Edges::new(),
        dev_dependencies: Edges::new(),
        optional_dependencies: Edges::new(),
        peer_dependencies: Edges::new(),
        scripts: BTreeMap::new(),
    };
    importer.dependencies.insert("a".into(), "a".into());
    importer.dev_dependencies.insert("dev".into(), "dev".into());
    importer
        .optional_dependencies
        .insert("alien".into(), "alien".into());
    let graph = Graph {
        fingerprint: "x".into(),
        importers: vec![importer],
        nodes: BTreeMap::from([
            (
                "a".into(),
                Node {
                    dependencies: BTreeMap::from([("b".into(), "b".into())]),
                    ..Node::default()
                },
            ),
            ("b".into(), Node::default()),
            ("dev".into(), Node::default()),
            (
                "alien".into(),
                Node {
                    os: vec!["nonexistent-platform".into()],
                    ..Node::default()
                },
            ),
        ]),
    };
    assert_eq!(
        selected(&graph, true).unwrap(),
        BTreeSet::from(["a".into(), "b".into()])
    );
    assert_eq!(selected(&graph, false).unwrap().len(), 3);
}

#[test]
fn locked_scopes_and_importer_paths_cannot_be_retargeted() {
    let root = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(root.path()).unwrap();
    let graph = Graph {
        fingerprint: "x".into(),
        importers: vec![],
        nodes: BTreeMap::from([(
            "id".into(),
            Node {
                name: "@private/lib".into(),
                version: "1.0.0".into(),
                registry: "https://registry.npmjs.org".into(),
                tarball: "https://registry.npmjs.org/x.tgz".into(),
                integrity: "sha512-x".into(),
                ..Node::default()
            },
        )]),
    };
    let mut config = UniflowedConfig::default();
    config
        .pm
        .scopes
        .insert("@private".into(), "https://private.example.org".into());
    assert!(validate_graph(&graph, root, &config).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn optional_packages_select_the_hosts_c_library() {
    let current = if cfg!(target_env = "musl") {
        "musl"
    } else {
        "glibc"
    };
    let other = if current == "musl" { "glibc" } else { "musl" };
    assert!(compatible(&Node {
        libc: vec![current.into()],
        ..Node::default()
    }));
    assert!(!compatible(&Node {
        libc: vec![other.into()],
        ..Node::default()
    }));
}
