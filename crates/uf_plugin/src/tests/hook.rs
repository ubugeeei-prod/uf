//! The hook vocabulary and the bitset built on it.

use crate::hook::{HookDispatch, HookSet, PluginHook};

#[test]
fn all_holds_every_hook_exactly_once() {
    let mut seen = HookSet::EMPTY;
    for hook in PluginHook::ALL {
        assert!(!seen.contains(hook), "{hook} appears twice in ALL");
        seen = seen.with(hook);
    }
    assert_eq!(seen, HookSet::ALL);
    assert_eq!(PluginHook::ALL.len(), PluginHook::COUNT);
}

#[test]
fn every_hook_has_a_unique_index() {
    for (position, hook) in PluginHook::ALL.into_iter().enumerate() {
        assert_eq!(hook.index(), position, "{hook} is out of position");
    }
}

#[test]
fn every_hook_has_a_unique_id() {
    let mut ids = PluginHook::ALL.map(PluginHook::as_str).to_vec();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "two hooks share an id");
}

#[test]
fn hook_ids_are_kebab_case() {
    for hook in PluginHook::ALL {
        let id = hook.as_str();
        assert!(!id.is_empty());
        assert!(
            id.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "{id} is not kebab-case"
        );
        assert!(!id.starts_with('-') && !id.ends_with('-'), "{id}");
    }
}

#[test]
fn every_hook_bit_fits_a_u16_and_is_unique() {
    let mut mask = 0u16;
    for hook in PluginHook::ALL {
        assert_eq!(hook.bit().count_ones(), 1, "{hook} is not a single bit");
        assert_eq!(mask & hook.bit(), 0, "{hook} reuses a bit");
        mask |= hook.bit();
    }
    assert_eq!(mask, HookSet::ALL.bits());
}

#[test]
fn resolve_id_and_load_are_first_wins() {
    assert_eq!(PluginHook::ResolveId.dispatch(), HookDispatch::FirstWins);
    assert_eq!(PluginHook::Load.dispatch(), HookDispatch::FirstWins);
}

#[test]
fn transform_and_the_other_rewriting_hooks_chain() {
    for hook in [
        PluginHook::Transform,
        PluginHook::RenderChunk,
        PluginHook::TransformIndexHtml,
        PluginHook::HandleHotUpdate,
    ] {
        assert_eq!(hook.dispatch(), HookDispatch::Chained, "{hook}");
    }
}

#[test]
fn lifecycle_hooks_broadcast() {
    for hook in [
        PluginHook::Config,
        PluginHook::ConfigResolved,
        PluginHook::BuildStart,
        PluginHook::ModuleParsed,
        PluginHook::BuildEnd,
        PluginHook::GenerateBundle,
        PluginHook::WriteBundle,
        PluginHook::ConfigureServer,
    ] {
        assert_eq!(hook.dispatch(), HookDispatch::Broadcast, "{hook}");
    }
}

#[test]
fn dispatch_ids_are_stable() {
    assert_eq!(HookDispatch::Broadcast.as_str(), "broadcast");
    assert_eq!(HookDispatch::FirstWins.as_str(), "first-wins");
    assert_eq!(HookDispatch::Chained.as_str(), "chained");
}

#[test]
fn hook_display_matches_its_id() {
    for hook in PluginHook::ALL {
        assert_eq!(hook.to_string(), hook.as_str());
    }
}

#[test]
fn an_empty_set_contains_nothing() {
    let set = HookSet::EMPTY;
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert_eq!(set.iter().count(), 0);
    for hook in PluginHook::ALL {
        assert!(!set.contains(hook), "{hook}");
    }
}

#[test]
fn a_full_set_contains_every_hook() {
    let set = HookSet::ALL;
    assert!(!set.is_empty());
    assert_eq!(set.len() as usize, PluginHook::COUNT);
    for hook in PluginHook::ALL {
        assert!(set.contains(hook), "{hook}");
    }
}

#[test]
fn adding_a_hook_twice_changes_nothing() {
    let once = HookSet::of(PluginHook::Transform);
    let twice = once.with(PluginHook::Transform);

    assert_eq!(once, twice);
    assert_eq!(twice.len(), 1);
}

#[test]
fn removing_a_hook_leaves_the_rest() {
    let set = HookSet::ALL.without(PluginHook::Transform);

    assert!(!set.contains(PluginHook::Transform));
    assert_eq!(set.len() as usize, PluginHook::COUNT - 1);
    assert_eq!(set.with(PluginHook::Transform), HookSet::ALL);
}

#[test]
fn removing_an_absent_hook_changes_nothing() {
    let set = HookSet::of(PluginHook::Load);

    assert_eq!(set.without(PluginHook::Transform), set);
}

#[test]
fn union_and_intersection_agree_with_membership() {
    let left = HookSet::of(PluginHook::Load).with(PluginHook::Transform);
    let right = HookSet::of(PluginHook::Transform).with(PluginHook::BuildEnd);

    assert_eq!(left.union(right).len(), 3);
    assert_eq!(left.intersection(right), HookSet::of(PluginHook::Transform));
    assert_eq!(left.intersection(HookSet::EMPTY), HookSet::EMPTY);
    assert_eq!(left.union(HookSet::ALL), HookSet::ALL);
}

#[test]
fn a_set_iterates_in_pipeline_order() {
    let set = HookSet::of(PluginHook::WriteBundle)
        .with(PluginHook::Config)
        .with(PluginHook::Transform);

    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        vec![
            PluginHook::Config,
            PluginHook::Transform,
            PluginHook::WriteBundle
        ]
    );
}

#[test]
fn collecting_hooks_deduplicates_them() {
    let set: HookSet = [
        PluginHook::Load,
        PluginHook::Load,
        PluginHook::Transform,
        PluginHook::Load,
    ]
    .into_iter()
    .collect();

    assert_eq!(set.len(), 2);
    assert_eq!(
        set,
        HookSet::of(PluginHook::Load).with(PluginHook::Transform)
    );
}

#[test]
fn collecting_nothing_gives_the_empty_set() {
    let set: HookSet = std::iter::empty().collect();

    assert_eq!(set, HookSet::EMPTY);
}

#[test]
fn a_set_serializes_as_hook_ids() {
    let set = HookSet::of(PluginHook::Transform).with(PluginHook::ResolveId);

    assert_eq!(
        serde_json::to_value(set).expect("serializes"),
        serde_json::json!(["resolve-id", "transform"])
    );
}

#[test]
fn an_empty_set_serializes_as_an_empty_list() {
    assert_eq!(
        serde_json::to_string(&HookSet::EMPTY).expect("serializes"),
        "[]"
    );
}

#[test]
fn a_set_round_trips_through_json() {
    for set in [
        HookSet::EMPTY,
        HookSet::ALL,
        HookSet::of(PluginHook::Load).with(PluginHook::WriteBundle),
    ] {
        let json = serde_json::to_string(&set).expect("serializes");
        let back: HookSet = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, set, "{json}");
    }
}

#[test]
fn a_hook_round_trips_through_json() {
    for hook in PluginHook::ALL {
        let json = serde_json::to_string(&hook).expect("serializes");
        assert_eq!(json, format!("\"{}\"", hook.as_str()));
        let back: PluginHook = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, hook);
    }
}

#[test]
fn into_iterator_matches_iter() {
    let set = HookSet::ALL.without(PluginHook::Config);

    assert_eq!(
        set.into_iter().collect::<Vec<_>>(),
        set.iter().collect::<Vec<_>>()
    );
}
