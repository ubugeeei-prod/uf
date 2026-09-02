//! Action-id derivation, unguessability and hexadecimal parsing.

use super::*;

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
