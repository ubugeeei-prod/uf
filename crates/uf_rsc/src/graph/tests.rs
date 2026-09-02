use super::*;

fn client(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Client)
}

fn server(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Server)
}

fn actions(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::ServerActions)
}

#[test]
fn an_empty_graph_has_no_modules_or_diagnostics() {
    let graph = RscGraphBuilder::new().build();
    assert!(graph.modules().is_empty());
    assert!(graph.diagnostics().is_empty());
    assert!(!graph.has_errors());
}

#[test]
fn modules_are_ordered_by_path() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("z.js"));
    builder.add_module(server("a.js"));
    builder.add_module(server("m.js"));
    let graph = builder.build();
    let paths: Vec<_> = graph
        .modules()
        .iter()
        .map(|module| module.path.as_str())
        .collect();
    assert_eq!(paths, vec!["a.js", "m.js", "z.js"]);
}

#[test]
fn duplicate_module_paths_are_collapsed() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js"));
    builder.add_module(server("a.js"));
    assert_eq!(builder.build().modules().len(), 1);
}

#[test]
fn relative_imports_resolve_to_modules() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget.js"));
    builder.add_module(server("app/widget.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    let page = graph.module("app/page.js").unwrap();
    assert_eq!(page.imports.len(), 1);
    assert_eq!(
        graph.module_by_id(page.imports[0]).unwrap().path,
        "app/widget.js"
    );
}

#[test]
fn extensionless_imports_resolve_to_the_js_file() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget"));
    builder.add_module(server("app/widget.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn directory_imports_resolve_to_index_js() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget"));
    builder.add_module(server("app/widget/index.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn parent_relative_imports_resolve() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("../server/actions.js"));
    builder.add_module(actions("server/actions.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn bare_specifiers_are_external() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("@uniflowed/react"));
    let graph = builder.build();
    let page = graph.module("app/page.js").unwrap();
    assert!(page.imports.is_empty());
    assert_eq!(page.external_imports.as_slice(), &["@uniflowed/react"]);
}

#[test]
fn an_import_climbing_out_of_the_project_is_rejected() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("../../../../etc/passwd"));
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/import-escapes-project-root"
    );
}

#[test]
fn a_module_path_outside_the_project_is_dropped() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("../outside.js"));
    let graph = builder.build();
    assert!(graph.modules().is_empty());
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/module-outside-project-root"
    );
}

#[test]
fn an_absolute_module_path_is_dropped() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("/etc/passwd"));
    assert!(builder.build().modules().is_empty());
}

#[test]
fn without_entries_every_module_is_unreachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js"));
    let graph = builder.build();
    assert_eq!(
        graph.module("app/page.js").unwrap().reachability,
        ModuleReachability::Unreachable
    );
}

#[test]
fn a_server_entry_makes_its_imports_server_reachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./data.js"));
    builder.add_module(server("app/data.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/data.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_client_module_imported_by_a_server_module_is_a_boundary() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(graph.client_boundaries().len(), 1);
    let boundary = graph.client_boundaries()[0];
    assert_eq!(
        graph.module_by_id(boundary.importer).unwrap().path,
        "app/page.js"
    );
    assert_eq!(
        graph.module_by_id(boundary.client_module).unwrap().path,
        "app/Counter.js"
    );
    assert_eq!(graph.client_bundle_roots().len(), 1);
    assert_eq!(
        graph.module("app/Counter.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn code_below_a_client_boundary_is_client_reachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./format.js"));
    builder.add_module(server("app/format.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/format.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn shared_code_is_reachable_from_both_halves() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_import("./Counter.js")
            .with_import("./format.js"),
    );
    builder.add_module(client("app/Counter.js").with_import("./format.js"));
    builder.add_module(server("app/format.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/format.js").unwrap().reachability,
        ModuleReachability::ServerAndClient
    );
}

#[test]
fn a_client_entry_marks_its_module_a_bundle_root() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/entry.js"));
    builder.add_entry("app/entry.js", EntryKind::Client);
    let graph = builder.build();
    assert_eq!(graph.client_bundle_roots().len(), 1);
}

#[test]
fn a_client_module_named_as_a_server_entry_is_still_a_client_module() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/entry.js"));
    builder.add_entry("app/entry.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/entry.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn a_two_module_cycle_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./b.js"));
    builder.add_module(server("b.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("b.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_self_import_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    assert_eq!(
        builder.build().module("a.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_cycle_crossing_a_client_boundary_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./b.js"));
    builder.add_module(client("b.js").with_import("./c.js"));
    builder.add_module(server("c.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("a.js").unwrap().reachability,
        ModuleReachability::ServerAndClient
    );
    assert_eq!(graph.client_boundaries().len(), 1);
}

#[test]
fn a_long_cycle_terminates() {
    let mut builder = RscGraphBuilder::new();
    let size = 5_000usize;
    for position in 0..size {
        let next = (position + 1) % size;
        builder.add_module(server(format!("m{position}.js")).with_import(format!("./m{next}.js")));
    }
    builder.add_entry("m0.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(graph.modules().len(), size);
    assert!(
        graph
            .modules()
            .iter()
            .all(|module| module.reachability == ModuleReachability::ServerOnly)
    );
}

#[test]
fn a_ten_thousand_module_graph_builds() {
    let mut builder = RscGraphBuilder::new();
    let size = 10_000usize;
    for position in 0..size {
        let mut module = server(format!("m{position}.js"));
        if position + 1 < size {
            module = module.with_import(format!("./m{}.js", position + 1));
        }
        if position + 7 < size {
            module = module.with_import(format!("./m{}.js", position + 7));
        }
        builder.add_module(module);
    }
    builder.add_entry("m0.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(graph.modules().len(), size);
    assert!(
        graph
            .modules()
            .iter()
            .all(|module| module.reachability.is_server_reachable())
    );
}

#[test]
fn a_client_module_importing_a_server_only_package_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("@uniflowed/server"));
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/server-only-import-in-client"
    );
    assert!(graph.has_errors());
}

#[test]
fn a_client_module_importing_a_server_only_subpath_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("@uniflowed/server/db"));
    assert_eq!(builder.build().diagnostics().len(), 1);
}

#[test]
fn a_client_module_importing_a_dot_server_file_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("./secrets.server.js"));
    builder.add_module(server("app/secrets.server.js"));
    builder.add_entry("app/Counter.js", EntryKind::Client);
    let graph = builder.build();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.rule() == "rsc/server-only-import-in-client")
    );
}

#[test]
fn a_module_pulled_into_the_client_graph_may_not_import_server_only_code() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./shared.js"));
    builder.add_module(server("app/shared.js").with_import("@uniflowed/server"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.rule() == "rsc/server-only-import-in-client")
    );
}

#[test]
fn a_server_module_may_import_server_only_code() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("@uniflowed/server"));
    builder.add_entry("app/page.js", EntryKind::Server);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_server_module_calling_a_client_only_api_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/page.js",
        "export default function Page() {\n const [a] = useState(1);\n return a;\n}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/client-only-api-in-server"
    );
}

#[test]
fn a_client_module_calling_a_client_only_api_is_fine() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/Counter.js",
        "\"use client\";\nexport default function Counter() {\n const [a] = useState(1);\n return a;\n}\n",
    );
    builder.add_entry("app/Counter.js", EntryKind::Client);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_shared_module_reached_only_from_the_client_may_use_hooks() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "import Counter from \"./Counter.js\";\n");
    builder.add_source(
        "app/Counter.js",
        "\"use client\";\nimport { useCounter } from \"./useCounter.js\";\n",
    );
    builder.add_source(
        "app/useCounter.js",
        "export function useCounter() {\n return useState(0);\n}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn an_unreachable_server_module_using_hooks_is_not_reported() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/orphan.js", "const a = useState(1);\n");
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_sync_server_action_export_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport function refresh() {}\n",
    );
    let graph = builder.build();
    assert_eq!(graph.diagnostics()[0].rule(), "rsc/server-action-not-async");
}

#[test]
fn a_non_function_server_action_export_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport const limit = 5;\n",
    );
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/server-action-not-a-function"
    );
}

#[test]
fn an_exported_class_in_a_server_actions_module_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("server/actions.js", "\"use server\";\nexport class Db {}\n");
    assert_eq!(
        builder.build().diagnostics()[0].rule(),
        "rsc/server-action-not-a-function"
    );
}

#[test]
fn an_async_server_action_export_is_accepted() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function refresh() {}\n",
    );
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_server_action_factory_export_is_accepted() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport const refresh = serverAction(async () => {});\n",
    );
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn directive_issues_are_lifted_into_graph_diagnostics() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "const a = 1;\n\"use client\";\n");
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/directive-not-in-prologue"
    );
}

#[test]
fn proximity_marks_modules_that_can_hand_a_closure_to_the_client() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./section.js"));
    builder.add_module(server("app/section.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_module(server("app/lonely.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        graph.module("app/page.js").unwrap().proximity,
        ClientBoundaryProximity::ReachesBoundary
    );
    assert_eq!(
        graph.module("app/section.js").unwrap().proximity,
        ClientBoundaryProximity::ReachesBoundary
    );
    assert_eq!(
        graph.module("app/lonely.js").unwrap().proximity,
        ClientBoundaryProximity::Isolated
    );
}

#[test]
fn diagnostics_are_ordered_deterministically() {
    let build = || {
        let mut builder = RscGraphBuilder::new();
        builder.add_source("b.js", "\"use client\";\nimport \"@uniflowed/server\";\n");
        builder.add_source("a.js", "\"use client\";\nimport \"server-only\";\n");
        builder.build()
    };
    let first: Vec<_> = build()
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect();
    let second: Vec<_> = build()
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect();
    assert_eq!(first, second);
    assert!(first[0].contains("a.js"));
}

#[test]
fn function_level_actions_survive_into_the_graph() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_function_action(FunctionOwner::Named(CompactString::const_new("save"))),
    );
    let graph = builder.build();
    assert_eq!(
        graph.module("app/page.js").unwrap().function_actions.len(),
        1
    );
}

#[test]
fn server_only_specifier_detection_covers_packages_and_files() {
    assert!(is_server_only_specifier("@uniflowed/server"));
    assert!(is_server_only_specifier("@uniflowed/server/db"));
    assert!(is_server_only_specifier("server-only"));
    assert!(is_server_only_specifier("./secrets.server.js"));
    assert!(!is_server_only_specifier("@uniflowed/react"));
    assert!(!is_server_only_specifier("./server.js"));
    assert!(!is_server_only_specifier("@uniflowed/serverless"));
}

#[test]
fn windows_style_paths_are_normalized() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app\\page.js").with_import(".\\widget.js"));
    builder.add_module(server("app/widget.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn building_the_same_input_twice_gives_the_same_graph() {
    let build = || {
        let mut builder = RscGraphBuilder::new();
        builder.add_module(server("app/page.js").with_import("./Counter.js"));
        builder.add_module(client("app/Counter.js"));
        builder.add_entry("app/page.js", EntryKind::Server);
        builder.build()
    };
    let first = build();
    let second = build();
    assert_eq!(first.modules(), second.modules());
    assert_eq!(first.client_boundaries(), second.client_boundaries());
}
