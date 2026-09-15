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
/// this holds it to the files — and to the exports map, since the list claims
/// to say what a project can *import*, not what is on disk.
///
/// Deliberately in `uf_rsc` and deliberately through [`module_environment`]:
/// the question "is this a client module" already has an answer in this
/// repository, and a test that re-answered it with a different rule would
/// check that two rules agree rather than that the list is right. `uf_lib`
/// holds the data; the crate that owns the rule holds the guard.
#[test]
fn the_client_module_list_names_exactly_the_ui_subpaths_that_are_client_modules() {
    let package = repository_root().join("packages").join("ui");
    let manifest = std::fs::read_to_string(package.join("package.json"))
        .expect("packages/ui/package.json cannot be read");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest).expect("packages/ui/package.json does not parse");
    let exports = manifest["exports"]
        .as_object()
        .expect("packages/ui/package.json has no exports map");

    let mut client: Vec<String> = Vec::new();
    for (specifier, target) in exports {
        // The barrel is `.`; every other key is `./name`, and the subpath is
        // what a project writes after the package name.
        let Some(subpath) = specifier.strip_prefix("./") else {
            continue;
        };
        let target = target
            .as_str()
            .unwrap_or_else(|| panic!("the export {specifier} is a conditions object, not a file"));
        let file = package.join(target.trim_start_matches("./"));
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", file.display()));
        if module_environment(&source).runs_on_client() {
            client.push(subpath.to_string());
        }
    }
    client.sort();

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
/// Read from `packages/ui/index.js` itself, with this crate's scanner for its
/// imports and re-exports, so the rule is held to what the barrel does rather
/// than to a second description of it. A namespace such as `Dialog` is an object
/// of parts, and comes from every module one of its parts does.
#[test]
fn the_barrel_names_the_client_modules_each_export_comes_from() {
    use std::collections::{BTreeMap, BTreeSet};

    let source = std::fs::read_to_string(repository_root().join("packages/ui/index.js"))
        .expect("packages/ui/index.js cannot be read");
    let client_module = |specifier: &str| -> Option<String> {
        let module = specifier.strip_prefix("./")?.strip_suffix(".js")?;
        uf_lib::CLIENT_MODULE_SUBPATHS
            .contains(&module)
            .then(|| module.to_owned())
    };

    // Every name the barrel imports or re-exports, and the client module it is
    // from, if it is from one.
    let mut origin: BTreeMap<String, Option<String>> = BTreeMap::new();
    for import in crate::scan::scan_imports(&source).iter() {
        for binding in &import.bindings {
            if matches!(binding.imported, crate::scan::ImportedName::Named(_)) {
                origin.insert(binding.local.to_string(), client_module(&import.specifier));
            }
        }
    }

    // Every `export const Name = { Part: Value, … };`, and the modules its
    // values are from.
    let mut namespaces: Vec<(String, BTreeSet<String>)> = Vec::new();
    let mut open: Option<(String, BTreeSet<String>)> = None;
    for line in source.lines() {
        if let Some(name) = line
            .strip_prefix("export const ")
            .and_then(|rest| rest.strip_suffix(" = {"))
        {
            open = Some((name.to_owned(), BTreeSet::new()));
            continue;
        }
        if open.is_some() && line.starts_with("};") {
            namespaces.extend(open.take());
            continue;
        }
        if let Some((_, modules)) = open.as_mut()
            && let Some((_, value)) = line.split_once(':')
            && let Some(Some(module)) = origin.get(value.trim().trim_end_matches(','))
        {
            modules.insert(module.clone());
        }
    }

    // A floor on both, so a barrel that stopped parsing cannot pass by being
    // empty.
    assert!(
        origin.len() > 100 && namespaces.len() > 20,
        "almost nothing came out of packages/ui/index.js: {} names, {} namespaces",
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
        "client_modules_exporting and packages/ui/index.js disagree:\n  {}",
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
