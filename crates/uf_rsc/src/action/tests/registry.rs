//! Registration, exposure and the non-enumerable lookup.

use super::*;

#[test]
fn a_reachable_module_export_is_registered_as_an_endpoint() {
    let graph = graph_with_reachable_action();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id());
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.actions()[0].export, "refresh");
    assert_eq!(
        registry.actions()[0].exposure,
        ActionExposure::CallableEndpoint
    );
    assert_eq!(registry.callable_actions().count(), 1);
}

#[test]
fn a_registered_endpoint_resolves_by_its_id() {
    let graph = graph_with_reachable_action();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id());
    let id = registry.actions()[0].id.to_hex();
    assert_eq!(registry.resolve(&id).unwrap().export, "refresh");
}

#[test]
fn an_unreachable_action_is_never_resolvable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/orphan.js",
        "\"use server\";\nexport async function drop() {}\n",
    );
    let graph = builder.build();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id());

    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry.actions()[0].exposure,
        ActionExposure::UnreachableFromClient
    );
    assert_eq!(registry.callable_actions().count(), 0);

    let id = registry.actions()[0].id.to_hex();
    assert_eq!(registry.resolve(&id), Err(UnknownAction));
}

#[test]
fn an_action_reached_only_from_a_server_module_without_a_boundary_is_not_callable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "import \"../server/actions.js\";\n");
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function refresh() {}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert_eq!(
        registry.actions()[0].exposure,
        ActionExposure::UnreachableFromClient
    );
}

#[test]
fn an_action_imported_by_a_client_module_is_callable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "import Form from \"./Form.js\";\n");
    builder.add_source(
        "app/Form.js",
        "\"use client\";\nimport { save } from \"../server/actions.js\";\n",
    );
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function save() {}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert_eq!(
        registry.actions()[0].exposure,
        ActionExposure::CallableEndpoint
    );
}

#[test]
fn an_inline_closure_action_is_registered_separately() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/page.js",
        "import Form from \"./Form.js\";\nexport default function Page() {\n const save = async () => {\n  \"use server\";\n };\n return save;\n}\n",
    );
    builder.add_source("app/Form.js", "\"use client\";\n");
    builder.add_entry("app/page.js", EntryKind::Server);
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.actions()[0].kind, ServerActionKind::InlineClosure);
    assert_eq!(registry.actions()[0].export, "save");
    assert_eq!(
        registry.actions()[0].exposure,
        ActionExposure::CallableEndpoint
    );
}

#[test]
fn a_sync_export_of_a_server_actions_module_is_not_registered() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport function refresh() {}\n",
    );
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert!(registry.is_empty());
}

#[test]
fn a_non_function_export_of_a_server_actions_module_is_not_registered() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport const n = 1;\n",
    );
    assert!(ServerActionRegistry::from_graph(&builder.build(), &build_id()).is_empty());
}

#[test]
fn a_forged_id_is_rejected_with_the_same_error_as_an_unknown_one() {
    let graph = graph_with_reachable_action();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id());

    let forged = "0".repeat(ACTION_ID_HEX_LEN);
    let malformed = "not-an-id";
    assert_eq!(registry.resolve(&forged), Err(UnknownAction));
    assert_eq!(registry.resolve(malformed), Err(UnknownAction));
    assert_eq!(
        registry.resolve(&forged).unwrap_err().to_string(),
        registry.resolve(malformed).unwrap_err().to_string()
    );
}

#[test]
fn an_id_from_another_build_does_not_resolve() {
    let graph = graph_with_reachable_action();
    let current = ServerActionRegistry::from_graph(&graph, &build_id());
    let previous =
        ServerActionRegistry::from_graph(&graph, &BuildId::new("previous-build-id").unwrap());
    let stale = previous.actions()[0].id.to_hex();
    assert_eq!(current.resolve(&stale), Err(UnknownAction));
}

#[test]
fn the_unknown_action_error_says_nothing_about_the_registry() {
    let message = UnknownAction.to_string();
    assert_eq!(message, "unknown server action");
}

#[test]
fn ids_are_unique_across_a_large_registry() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "import \"./Counter.js\";\n");
    builder.add_source("app/Counter.js", "\"use client\";\n");
    for index in 0..2_000 {
        builder.add_module(
            RscModuleInput::new(
                format!("server/actions{index}.js"),
                ModuleEnvironment::ServerActions,
            )
            .with_export("refresh", ExportKind::AsyncFunction)
            .with_export("reload", ExportKind::AsyncFunction),
        );
    }
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert_eq!(registry.len(), 4_000);

    let mut ids: Vec<_> = registry
        .actions()
        .iter()
        .map(|action| action.id.to_hex())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4_000);
}

#[test]
fn duplicate_declarations_collapse_to_one_action() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        RscModuleInput::new("server/actions.js", ModuleEnvironment::ServerActions)
            .with_export("refresh", ExportKind::AsyncFunction)
            .with_export("refresh", ExportKind::AsyncFunction),
    );
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert_eq!(registry.len(), 1);
}

#[test]
fn the_build_fingerprint_is_derived_and_not_the_build_id() {
    let registry = ServerActionRegistry::from_graph(&graph_with_reachable_action(), &build_id());
    assert_eq!(registry.build_fingerprint().len(), ACTION_ID_HEX_LEN);
    assert!(!registry.build_fingerprint().contains("build-id-for-tests"));
    assert_eq!(registry.build_fingerprint(), build_id().fingerprint());
}

#[test]
fn registries_built_twice_from_the_same_input_are_identical() {
    let graph = graph_with_reachable_action();
    let first = ServerActionRegistry::from_graph(&graph, &build_id());
    let second = ServerActionRegistry::from_graph(&graph, &build_id());
    assert_eq!(first.actions(), second.actions());
}

#[test]
fn inline_closures_in_the_same_module_get_distinct_ids() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        RscModuleInput::new("app/page.js", ModuleEnvironment::Server)
            .with_function_action(FunctionOwner::Anonymous { ordinal: 0 })
            .with_function_action(FunctionOwner::Anonymous { ordinal: 1 }),
    );
    let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
    assert_eq!(registry.len(), 2);
    assert_ne!(registry.actions()[0].id, registry.actions()[1].id);
}
