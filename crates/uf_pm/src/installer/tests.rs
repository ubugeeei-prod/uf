use super::*;

#[cfg(unix)]
#[test]
fn scoped_dependency_hooks_can_run_their_transitive_executables() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    let store = store::Store {
        root: root.join("store"),
    };
    let graph = Graph {
        fingerprint: "hook-fixture".into(),
        importers: vec![],
        nodes: [
            (
                "plugin".into(),
                Node {
                    name: "@scope/plugin".into(),
                    version: "1.0.0".into(),
                    integrity: "sha512-plugin".into(),
                    dependencies: [("helper".into(), "helper".into())].into(),
                    scripts: [("postinstall".into(), "native-helper > installed.txt".into())]
                        .into(),
                    ..Default::default()
                },
            ),
            (
                "helper".into(),
                Node {
                    name: "helper".into(),
                    version: "1.0.0".into(),
                    integrity: "sha512-helper".into(),
                    bin: [("native-helper".into(), "cli.sh".into())].into(),
                    ..Default::default()
                },
            ),
        ]
        .into(),
    };
    for node in graph.nodes.values() {
        let package = store.package(node).join("package");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("package.json"),
            serde_json::to_vec(&serde_json::json!({"name":node.name,"version":node.version}))
                .unwrap(),
        )
        .unwrap();
    }
    let helper = store.package(&graph.nodes["helper"]).join("package/cli.sh");
    fs::write(&helper, "#!/bin/sh\nprintf 'ready\\n'\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let selected = graph.nodes.keys().cloned().collect();
    store::materialize(root, &store, &graph, &selected, false, true).unwrap();
    assert!(!root.join("node_modules/.bin/native-helper").exists());
    lifecycle(root, &graph, &selected, &[], true).unwrap();
    let plugin = store::installed_package(root, "plugin", &graph.nodes["plugin"]);
    assert_eq!(fs::read(plugin.join("installed.txt")).unwrap(), b"ready\n");
    assert!(
        !store
            .package(&graph.nodes["plugin"])
            .join("package/installed.txt")
            .exists()
    );
}

#[test]
fn row_updates_pin_the_same_package_in_unselected_dependency_fields() {
    let mut importer = Importer {
        path: ".".into(),
        name: "app".into(),
        version: "1.0.0".into(),
        dependencies: [("lib".into(), "^1".into())].into(),
        dev_dependencies: [("lib".into(), "^1".into())].into(),
        optional_dependencies: Edges::new(),
        peer_dependencies: Edges::new(),
        scripts: BTreeMap::new(),
    };
    let mut previous = importer.clone();
    previous.dependencies.insert("lib".into(), "lib-id".into());
    previous
        .dev_dependencies
        .insert("lib".into(), "lib-id".into());
    let graph = Graph {
        fingerprint: "old".into(),
        importers: vec![previous],
        nodes: [(
            "lib-id".into(),
            Node {
                name: "lib".into(),
                version: "1.0.0".into(),
                ..Default::default()
            },
        )]
        .into(),
    };
    pin_unselected(
        std::slice::from_mut(&mut importer),
        &graph,
        &[],
        &[UpdateTarget {
            importer: ".".into(),
            field: "dependencies".into(),
            name: "lib".into(),
        }],
    );
    assert_eq!(importer.dependencies["lib"], "^1");
    assert_eq!(importer.dev_dependencies["lib"], "1.0.0");
}

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
