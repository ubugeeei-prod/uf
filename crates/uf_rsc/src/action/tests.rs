use super::*;
use crate::directive::FunctionOwner;
use crate::graph::{EntryKind, RscGraphBuilder, RscModuleInput};

fn build_id() -> BuildId {
    BuildId::new("build-id-for-tests").expect("valid build id")
}

fn graph_with_reachable_action() -> RscGraph {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/page.js",
        "import Counter from \"./Counter.js\";\nimport { refresh } from \"../server/actions.js\";\n",
    );
    builder.add_source("app/Counter.js", "\"use client\";\n");
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function refresh() {}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    builder.build()
}

#[test]
fn hmac_matches_rfc_4231_test_case_1() {
    let mac = hmac_sha256(&[0x0b; 20], b"Hi There");
    assert_eq!(
        hex(&mac),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn hmac_matches_rfc_4231_test_case_2() {
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex(&mac),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn hmac_matches_rfc_4231_test_case_6_with_an_oversized_key() {
    let mac = hmac_sha256(
        &[0xaa; 131],
        b"Test Using Larger Than Block-Size Key - Hash Key First",
    );
    assert_eq!(
        hex(&mac),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn a_short_build_id_is_rejected() {
    assert_eq!(
        BuildId::new("short"),
        Err(BuildIdError::TooShort { len: 5 })
    );
}

#[test]
fn an_empty_build_id_is_rejected() {
    assert_eq!(BuildId::new(""), Err(BuildIdError::TooShort { len: 0 }));
}

#[test]
fn an_oversized_build_id_is_rejected() {
    let value = "x".repeat(MAX_BUILD_ID_BYTES + 1);
    assert_eq!(
        BuildId::new(value),
        Err(BuildIdError::TooLong {
            len: MAX_BUILD_ID_BYTES + 1
        })
    );
}

#[test]
fn a_generated_build_id_is_long_and_unique() {
    let first = BuildId::generate();
    let second = BuildId::generate();
    assert!(first.value.len() >= MIN_BUILD_ID_BYTES);
    assert_ne!(first, second);
}

#[test]
fn a_build_id_never_prints_itself() {
    let formatted = format!("{:?}", build_id());
    assert_eq!(formatted, "BuildId(<redacted>)");
    assert!(!formatted.contains("build-id-for-tests"));
}

#[test]
fn action_ids_are_stable_across_runs() {
    let first = ActionId::derive(
        &build_id(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let second = ActionId::derive(
        &build_id(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    assert_eq!(first, second);
}

#[test]
fn action_ids_change_with_the_build_id() {
    let first = ActionId::derive(
        &BuildId::new("build-id-one").unwrap(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let second = ActionId::derive(
        &BuildId::new("build-id-two").unwrap(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    assert_ne!(first, second);
}

#[test]
fn action_ids_change_with_the_module_path() {
    let first = ActionId::derive(
        &build_id(),
        "server/a.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let second = ActionId::derive(
        &build_id(),
        "server/b.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    assert_ne!(first, second);
}

#[test]
fn action_ids_change_with_the_export_name() {
    let first = ActionId::derive(
        &build_id(),
        "server/a.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let second = ActionId::derive(
        &build_id(),
        "server/a.js",
        "reload",
        ServerActionKind::ModuleExport,
    );
    assert_ne!(first, second);
}

#[test]
fn action_ids_change_with_the_declaration_kind() {
    let first = ActionId::derive(
        &build_id(),
        "server/a.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let second = ActionId::derive(
        &build_id(),
        "server/a.js",
        "refresh",
        ServerActionKind::InlineClosure,
    );
    assert_ne!(first, second);
}

#[test]
fn length_prefixing_stops_field_boundaries_from_sliding() {
    let first = ActionId::derive(&build_id(), "ab", "cd", ServerActionKind::ModuleExport);
    let second = ActionId::derive(&build_id(), "a", "bcd", ServerActionKind::ModuleExport);
    assert_ne!(first, second);
}

#[test]
fn an_action_id_never_contains_the_module_path() {
    let id = ActionId::derive(
        &build_id(),
        "server/secret-actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    assert!(!id.to_hex().contains("secret"));
    assert_eq!(id.to_hex().len(), ACTION_ID_HEX_LEN);
}

#[test]
fn action_ids_round_trip_through_hex() {
    let id = ActionId::derive(
        &build_id(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    assert_eq!(ActionId::parse(&id.to_hex()).unwrap(), id);
}

#[test]
fn a_truncated_action_id_is_rejected() {
    let id = ActionId::derive(
        &build_id(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let hex = id.to_hex();
    assert_eq!(
        ActionId::parse(&hex[..hex.len() - 1]),
        Err(ActionIdError::InvalidLength { len: 63 })
    );
}

#[test]
fn an_oversized_action_id_is_rejected_before_any_allocation() {
    let oversized = "a".repeat(1_000_000);
    assert_eq!(
        ActionId::parse(&oversized),
        Err(ActionIdError::InvalidLength { len: 1_000_000 })
    );
}

#[test]
fn an_empty_action_id_is_rejected() {
    assert_eq!(
        ActionId::parse(""),
        Err(ActionIdError::InvalidLength { len: 0 })
    );
}

#[test]
fn a_non_hex_action_id_is_rejected() {
    let text = "z".repeat(ACTION_ID_HEX_LEN);
    assert_eq!(ActionId::parse(&text), Err(ActionIdError::InvalidCharacter));
}

#[test]
fn an_uppercase_action_id_is_rejected() {
    let id = ActionId::derive(
        &build_id(),
        "server/actions.js",
        "refresh",
        ServerActionKind::ModuleExport,
    );
    let upper = id.to_hex().to_uppercase();
    assert!(ActionId::parse(&upper).is_err());
}

#[test]
fn constant_time_comparison_agrees_with_equality() {
    let left = [7u8; 32];
    let mut right = [7u8; 32];
    assert_eq!(constant_time_eq(&left, &right), 1);
    right[31] = 8;
    assert_eq!(constant_time_eq(&left, &right), 0);
    right[31] = 7;
    right[0] = 8;
    assert_eq!(constant_time_eq(&left, &right), 0);
}

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
