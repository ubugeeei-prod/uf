use super::*;

#[test]
fn the_crate_reexports_the_public_surface() {
    assert_eq!(module_environment(""), ModuleEnvironment::Server);
    assert_eq!(RSC_MANIFEST_FILE_NAME, "uf-rsc-manifest.json");
    assert_eq!(RSC_MANIFEST_VERSION, 3);
    assert_eq!(ACTION_ID_HEX_LEN, 64);
}

#[test]
fn errors_render_with_their_path() {
    let error = RscError::NonUtf8Source {
        path: Utf8PathBuf::from("app/page.js"),
    };
    assert_eq!(error.to_string(), "module app/page.js is not valid UTF-8");
}

#[test]
fn a_full_analysis_flows_from_sources_to_a_manifest() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/$page.js",
        "import Counter from \"./Counter.js\";\nimport { save } from \"../server/actions.js\";\n",
    );
    builder.add_source("app/Counter.js", "\"use client\";\n");
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function save() {}\n",
    );
    builder.add_entry("app/$page.js", EntryKind::Server);

    let graph = builder.build();
    let build_id = BuildId::new("lib-test-build-id").unwrap();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id);
    let manifest = RscManifest::new(&graph, &registry);

    assert_eq!(manifest.modules.len(), 3);
    assert_eq!(manifest.client_boundaries.len(), 1);
    assert_eq!(manifest.server_actions.len(), 1);
    assert!(manifest.diagnostics.is_empty());
}

/// The guard on `uf_lib::CLIENT_MODULE_SUBPATHS`, and the reason writing the
/// names down is defensible at all.
///
/// The list exists because the scan skips `node_modules` (#718), so in an
/// installed tree uf cannot see its own components' directives. A list that
/// nothing checks is a list that is wrong by the second component added, so
/// this holds it to the files — the modules `npm/ui/index.js` imports and
/// re-exports from, since the barrel is the package's only entry point and a
/// module it does not reach is not one a project can use.
///
/// Deliberately in `uf_rsc` and deliberately through [`module_environment`]:
/// the question "is this a client module" already has an answer in this
/// repository, and a test that re-answered it with a different rule would
/// check that two rules agree rather than that the list is right. `uf_lib`
/// holds the data; the crate that owns the rule holds the guard.
#[test]
fn the_client_module_list_names_exactly_the_ui_modules_that_are_client_modules() {
    let package = repository_root().join("npm").join("ui");
    let barrel =
        std::fs::read_to_string(package.join("index.js")).expect("npm/ui/index.js cannot be read");

    let mut client: Vec<String> = Vec::new();
    for import in crate::scan::scan_imports(&barrel).iter() {
        // `./switch.js` is the module uf names `@uniflowed/ui/switch`.
        let Some(module) = import
            .specifier
            .strip_prefix("./")
            .and_then(|file| file.strip_suffix(".js"))
        else {
            continue;
        };
        let file = package.join(format!("{module}.js"));
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", file.display()));
        if module_environment(&source).runs_on_client() {
            client.push(module.to_owned());
        }
    }
    // The barrel names `interactions.js` twice, for its hooks and its types.
    client.sort();
    client.dedup();

    let listed: Vec<String> = uf_lib::CLIENT_MODULE_SUBPATHS
        .iter()
        .map(|subpath| (*subpath).to_string())
        .collect();
    let missing: Vec<&String> = client.iter().filter(|s| !listed.contains(s)).collect();
    let absent: Vec<&String> = listed.iter().filter(|s| !client.contains(s)).collect();
    assert!(
        missing.is_empty() && absent.is_empty(),
        "CLIENT_MODULE_SUBPATHS and @uniflowed/ui disagree\n  \
         declares `use client` and the list omits: {missing:?}\n  \
         the list names and the module does not declare: {absent:?}"
    );
    // A floor as well as an equality: an exports map that stopped parsing into
    // anything would otherwise make two empty lists agree.
    assert!(
        client.len() > 20,
        "almost nothing came back as a client module, so this is checking nothing: {}",
        client.len()
    );
    // The list is what `is_client_module` answers from, so check the specifier
    // form once here rather than trusting that a caller assembles it the same
    // way. The barrel re-exports client modules without being one.
    assert!(uf_lib::is_client_module(&format!(
        "{}/{}",
        uf_lib::CLIENT_MODULE_PACKAGE,
        client.first().expect("a client module")
    )));
    assert!(!uf_lib::is_client_module(uf_lib::CLIENT_MODULE_PACKAGE));
    assert!(!uf_lib::is_client_module("@uniflowed/ui/does-not-exist"));
}

/// The guard on `uf_lib::client_modules_exporting`: every name the barrel binds
/// reaches exactly the client modules its value comes from.
///
/// Read from `npm/ui/index.js` itself, with this crate's scanner for its
/// imports and re-exports, so the rule is held to what the barrel does rather
/// than to a second description of it. A namespace such as `Dialog` is a
/// module re-exported whole (`export * as Dialog from "./dialog.js"`,
/// ubugeeei-prod/uf#1453), and comes from that module and from every module it
/// re-exports parts of: `ContextMenu` is `context-menu.js` and the `Menu`
/// parts it hands on from `menu.js`.
#[test]
fn the_barrel_names_the_client_modules_each_export_comes_from() {
    use std::collections::{BTreeMap, BTreeSet};

    let package = repository_root().join("npm/ui");
    let source =
        std::fs::read_to_string(package.join("index.js")).expect("npm/ui/index.js cannot be read");
    let module_of = |specifier: &str| -> Option<String> {
        Some(
            specifier
                .strip_prefix("./")?
                .strip_suffix(".js")?
                .to_owned(),
        )
    };
    let client_module = |specifier: &str| -> Option<String> {
        module_of(specifier)
            .filter(|module| uf_lib::CLIENT_MODULE_SUBPATHS.contains(&module.as_str()))
    };

    // Every name the barrel imports or re-exports on its own, and the client
    // module it is from, if it is from one; and every namespace, with the
    // client modules its parts come from.
    let mut origin: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut namespaces: Vec<(String, BTreeSet<String>)> = Vec::new();
    for import in crate::scan::scan_imports(&source).iter() {
        for binding in &import.bindings {
            match binding.imported {
                crate::scan::ImportedName::Named(_) => {
                    origin.insert(binding.local.to_string(), client_module(&import.specifier));
                }
                crate::scan::ImportedName::Namespace => {
                    let module = module_of(&import.specifier).unwrap_or_else(|| {
                        panic!("{} is not a module of the package", import.specifier)
                    });
                    let file = package.join(format!("{module}.js"));
                    let own = std::fs::read_to_string(&file).unwrap_or_else(|error| {
                        panic!("{} cannot be read: {error}", file.display())
                    });
                    let mut modules: BTreeSet<String> =
                        client_module(&import.specifier).into_iter().collect();
                    for handed_on in crate::scan::scan_imports(&own).iter() {
                        if handed_on.kind == crate::scan::ImportKind::ReExport
                            && let Some(module) = client_module(&handed_on.specifier)
                        {
                            modules.insert(module);
                        }
                    }
                    namespaces.push((binding.local.to_string(), modules));
                }
                crate::scan::ImportedName::Default => {}
            }
        }
    }

    // A floor on both, so a barrel that stopped parsing cannot pass by being
    // empty.
    assert!(
        origin.len() > 20 && namespaces.len() > 30,
        "almost nothing came out of npm/ui/index.js: {} names, {} namespaces",
        origin.len(),
        namespaces.len()
    );

    let mut wrong: Vec<String> = Vec::new();
    let expected = origin
        .iter()
        .map(|(name, module)| {
            (
                name.clone(),
                module.iter().cloned().collect::<BTreeSet<_>>(),
            )
        })
        .chain(namespaces);
    for (name, modules) in expected {
        let rule: BTreeSet<String> = uf_lib::client_modules_exporting(&name)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if rule != modules {
            wrong.push(format!(
                "{name}: the rule says {rule:?}, the barrel says {modules:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "client_modules_exporting and npm/ui/index.js disagree:\n  {}",
        wrong.join("\n  ")
    );
}

/// The repository, from this crate's manifest directory.
fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/uf_rsc has a grandparent")
        .to_path_buf()
}
