use super::*;

fn rendered(manager: PackageManager, operation: Operation<'_>) -> String {
    supported(manager, operation).to_string()
}

/// The invocation, or a panic naming the pair that has none.
///
/// Most of this file is about *which* command a manager runs, and every pair
/// it asserts on has one. The pairs that do not are
/// `search_is_unsupported_where_the_manager_has_none`'s, which asks the
/// opposite question.
fn supported(manager: PackageManager, operation: Operation<'_>) -> Invocation {
    command_for(manager, operation)
        .unwrap_or_else(|| panic!("{manager} has no command for {operation:?}"))
}

const fn add(kind: DependencyKind) -> Operation<'static> {
    Operation::Add { kind }
}

const YARN_CLASSIC: PackageManager = PackageManager::Yarn(YarnEdition::Classic);
const YARN_BERRY: PackageManager = PackageManager::Yarn(YarnEdition::Berry);

#[test]
fn uf_maps_every_operation() {
    assert_eq!(
        rendered(PackageManager::Uf, Operation::Install),
        "uf install"
    );
    assert_eq!(
        rendered(PackageManager::Uf, Operation::InstallFrozen),
        "uf install --frozen-lockfile"
    );
    assert_eq!(
        rendered(PackageManager::Uf, add(DependencyKind::Prod)),
        "uf add"
    );
    assert_eq!(
        rendered(PackageManager::Uf, add(DependencyKind::Dev)),
        "uf add --dev"
    );
    assert_eq!(
        rendered(PackageManager::Uf, add(DependencyKind::Optional)),
        "uf add --optional"
    );
    assert_eq!(
        rendered(PackageManager::Uf, add(DependencyKind::Peer)),
        "uf add --peer"
    );
    assert_eq!(rendered(PackageManager::Uf, Operation::Remove), "uf remove");
    assert_eq!(
        rendered(PackageManager::Uf, Operation::Run { task: "build" }),
        "uf run build"
    );
    assert_eq!(rendered(PackageManager::Uf, Operation::Exec), "uf exec");
    assert_eq!(rendered(PackageManager::Uf, Operation::DlxExec), "uf exec");
    assert_eq!(rendered(PackageManager::Uf, Operation::Update), "uf update");
    assert_eq!(rendered(PackageManager::Uf, Operation::Why), "uf why");
}

#[test]
fn npm_maps_every_operation() {
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Install),
        "npm install"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::InstallFrozen),
        "npm ci"
    );
    assert_eq!(
        rendered(PackageManager::Npm, add(DependencyKind::Prod)),
        "npm install"
    );
    assert_eq!(
        rendered(PackageManager::Npm, add(DependencyKind::Dev)),
        "npm install --save-dev"
    );
    assert_eq!(
        rendered(PackageManager::Npm, add(DependencyKind::Optional)),
        "npm install --save-optional"
    );
    assert_eq!(
        rendered(PackageManager::Npm, add(DependencyKind::Peer)),
        "npm install --save-peer"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Remove),
        "npm uninstall"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Run { task: "build" }),
        "npm run build"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Exec),
        "npm exec --"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::DlxExec),
        "npx --yes"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Update),
        "npm update"
    );
    assert_eq!(rendered(PackageManager::Npm, Operation::Why), "npm explain");
}

#[test]
fn pnpm_maps_every_operation() {
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::Install),
        "pnpm install"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::InstallFrozen),
        "pnpm install --frozen-lockfile"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, add(DependencyKind::Prod)),
        "pnpm add"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, add(DependencyKind::Dev)),
        "pnpm add --save-dev"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, add(DependencyKind::Optional)),
        "pnpm add --save-optional"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, add(DependencyKind::Peer)),
        "pnpm add --save-peer"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::Remove),
        "pnpm remove"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::Run { task: "build" }),
        "pnpm run build"
    );
    assert_eq!(rendered(PackageManager::Pnpm, Operation::Exec), "pnpm exec");
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::DlxExec),
        "pnpm dlx"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::Update),
        "pnpm update"
    );
    assert_eq!(rendered(PackageManager::Pnpm, Operation::Why), "pnpm why");
}

#[test]
fn yarn_classic_maps_every_operation() {
    assert_eq!(rendered(YARN_CLASSIC, Operation::Install), "yarn install");
    assert_eq!(
        rendered(YARN_CLASSIC, Operation::InstallFrozen),
        "yarn install --frozen-lockfile"
    );
    assert_eq!(
        rendered(YARN_CLASSIC, add(DependencyKind::Prod)),
        "yarn add"
    );
    assert_eq!(
        rendered(YARN_CLASSIC, add(DependencyKind::Dev)),
        "yarn add --dev"
    );
    assert_eq!(
        rendered(YARN_CLASSIC, add(DependencyKind::Optional)),
        "yarn add --optional"
    );
    assert_eq!(
        rendered(YARN_CLASSIC, add(DependencyKind::Peer)),
        "yarn add --peer"
    );
    assert_eq!(rendered(YARN_CLASSIC, Operation::Remove), "yarn remove");
    assert_eq!(
        rendered(YARN_CLASSIC, Operation::Run { task: "build" }),
        "yarn run build"
    );
    assert_eq!(rendered(YARN_CLASSIC, Operation::Exec), "yarn run");
    assert_eq!(rendered(YARN_CLASSIC, Operation::DlxExec), "npx --yes");
    assert_eq!(rendered(YARN_CLASSIC, Operation::Update), "yarn upgrade");
    assert_eq!(rendered(YARN_CLASSIC, Operation::Why), "yarn why");
}

#[test]
fn yarn_berry_maps_every_operation() {
    assert_eq!(rendered(YARN_BERRY, Operation::Install), "yarn install");
    assert_eq!(
        rendered(YARN_BERRY, Operation::InstallFrozen),
        "yarn install --immutable"
    );
    assert_eq!(rendered(YARN_BERRY, add(DependencyKind::Prod)), "yarn add");
    assert_eq!(
        rendered(YARN_BERRY, add(DependencyKind::Dev)),
        "yarn add --dev"
    );
    assert_eq!(
        rendered(YARN_BERRY, add(DependencyKind::Optional)),
        "yarn add --optional"
    );
    assert_eq!(
        rendered(YARN_BERRY, add(DependencyKind::Peer)),
        "yarn add --peer"
    );
    assert_eq!(rendered(YARN_BERRY, Operation::Remove), "yarn remove");
    assert_eq!(
        rendered(YARN_BERRY, Operation::Run { task: "build" }),
        "yarn run build"
    );
    assert_eq!(rendered(YARN_BERRY, Operation::Exec), "yarn exec");
    assert_eq!(rendered(YARN_BERRY, Operation::DlxExec), "yarn dlx");
    assert_eq!(rendered(YARN_BERRY, Operation::Update), "yarn up");
    assert_eq!(rendered(YARN_BERRY, Operation::Why), "yarn why");
}

#[test]
fn bun_maps_every_operation() {
    assert_eq!(
        rendered(PackageManager::Bun, Operation::Install),
        "bun install"
    );
    assert_eq!(
        rendered(PackageManager::Bun, Operation::InstallFrozen),
        "bun install --frozen-lockfile"
    );
    assert_eq!(
        rendered(PackageManager::Bun, add(DependencyKind::Prod)),
        "bun add"
    );
    assert_eq!(
        rendered(PackageManager::Bun, add(DependencyKind::Dev)),
        "bun add --dev"
    );
    assert_eq!(
        rendered(PackageManager::Bun, add(DependencyKind::Optional)),
        "bun add --optional"
    );
    assert_eq!(
        rendered(PackageManager::Bun, add(DependencyKind::Peer)),
        "bun add --peer"
    );
    assert_eq!(
        rendered(PackageManager::Bun, Operation::Remove),
        "bun remove"
    );
    assert_eq!(
        rendered(PackageManager::Bun, Operation::Run { task: "build" }),
        "bun run build"
    );
    assert_eq!(rendered(PackageManager::Bun, Operation::Exec), "bun run");
    assert_eq!(rendered(PackageManager::Bun, Operation::DlxExec), "bunx");
    assert_eq!(
        rendered(PackageManager::Bun, Operation::Update),
        "bun update"
    );
    assert_eq!(rendered(PackageManager::Bun, Operation::Why), "bun why");
}

#[test]
fn every_manager_and_operation_pair_maps_to_an_allowlisted_program() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let Some(invocation) = command_for(manager, operation) else {
                continue;
            };
            assert!(
                PROGRAMS.contains(&invocation.program),
                "{manager} {operation:?} escaped the program allowlist"
            );
        }
    }
}

#[test]
fn frozen_installs_differ_from_plain_installs_for_every_manager() {
    for manager in PackageManager::ALL {
        let install = supported(manager, Operation::Install);
        let frozen = supported(manager, Operation::InstallFrozen);
        assert_ne!(install, frozen, "{manager} has no distinct frozen install");
    }
}

/// Four dependency maps, four distinct commands, for every manager.
///
/// `uf add --peer react` that quietly writes `dependencies` is worse than one
/// that refuses: the manifest would say something nobody meant, and the next
/// install would honour it.
#[test]
fn every_dependency_kind_maps_to_its_own_command_for_every_manager() {
    for manager in PackageManager::ALL {
        let mut seen: Vec<(DependencyKind, Invocation)> = Vec::new();
        for kind in DependencyKind::ALL {
            let invocation = supported(manager, add(kind));
            for (earlier, previous) in &seen {
                assert_ne!(
                    *previous, invocation,
                    "{manager} spells {earlier:?} and {kind:?} the same way"
                );
            }
            seen.push((kind, invocation));
        }
    }
}

/// And each one names the field it will write, which is the fact a caller
/// reports and a reader checks.
#[test]
fn a_dependency_kind_names_its_manifest_field() {
    assert_eq!(DependencyKind::Prod.manifest_field(), "dependencies");
    assert_eq!(DependencyKind::Dev.manifest_field(), "devDependencies");
    assert_eq!(
        DependencyKind::Optional.manifest_field(),
        "optionalDependencies"
    );
    assert_eq!(DependencyKind::Peer.manifest_field(), "peerDependencies");
}

/// The read-only operations are the ones that must not be told to ignore
/// scripts, because they install nothing and the flag is not their vocabulary.
#[test]
fn only_the_operations_that_install_can_run_scripts() {
    for operation in Operation::ALL {
        let installs = operation.installs_packages();
        let expected = !matches!(
            operation,
            Operation::Run { .. }
                | Operation::Exec
                | Operation::DlxExec
                | Operation::Why
                | Operation::List
                | Operation::Audit
                | Operation::Search
        );
        assert_eq!(installs, expected, "{operation:?}");
    }
}

#[test]
fn run_appends_the_task_as_the_final_argument() {
    for manager in PackageManager::ALL {
        let invocation = supported(manager, Operation::Run { task: "test:unit" });
        assert_eq!(
            invocation.args.last().map(Cow::as_ref),
            Some("test:unit"),
            "{manager} dropped the task name"
        );
    }
}

#[test]
fn a_hostile_task_name_stays_a_single_argument() {
    let invocation = supported(
        PackageManager::Pnpm,
        Operation::Run {
            task: "build; rm -rf /",
        },
    );

    assert_eq!(invocation.program, "pnpm");
    assert_eq!(invocation.args.len(), 2);
    assert_eq!(invocation.args[1], "build; rm -rf /");
}

#[test]
fn an_empty_task_name_is_still_one_argument() {
    let invocation = supported(PackageManager::Npm, Operation::Run { task: "" });

    assert_eq!(invocation.args.as_slice(), ["run", ""]);
}

#[test]
fn a_non_ascii_task_name_survives_intact() {
    let invocation = supported(PackageManager::Bun, Operation::Run { task: "ビルド" });

    assert_eq!(invocation.args.last().map(Cow::as_ref), Some("ビルド"));
}

#[test]
fn mapped_invocations_never_allocate_beyond_the_inline_capacity() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let Some(invocation) = command_for(manager, operation) else {
                continue;
            };
            assert!(
                !invocation.args.spilled(),
                "{manager} {operation:?} spilled"
            );
        }
    }
}

#[test]
fn mapping_is_deterministic() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            assert_eq!(
                command_for(manager, operation),
                command_for(manager, operation)
            );
        }
    }
}

#[test]
fn invocations_serialize_for_the_cli() {
    let json = serde_json::to_string(&command_for(PackageManager::Pnpm, Operation::InstallFrozen))
        .unwrap();

    assert_eq!(
        json,
        r#"{"program":"pnpm","args":["install","--frozen-lockfile"]}"#
    );
}

#[test]
fn yarn_editions_disagree_exactly_where_yarn_changed() {
    let differing = Operation::ALL
        .into_iter()
        .filter(|operation| {
            command_for(YARN_CLASSIC, *operation) != command_for(YARN_BERRY, *operation)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        differing,
        [
            Operation::InstallFrozen,
            Operation::Exec,
            Operation::DlxExec,
            Operation::Update,
            // Yarn 1's `yarn list` became `yarn info` in Berry, and Berry put
            // the registry commands under `yarn npm`.
            Operation::List,
            Operation::Audit,
        ]
    );
}

/// The pairs the table cannot answer, named rather than counted.
///
/// A manager that gains the command is a change to this list, which is the
/// point: "yarn has no search" should stop being true the day it stops being
/// true, and nothing else in the crate would notice.
#[test]
fn search_is_unsupported_where_the_manager_has_none() {
    let without = PackageManager::ALL
        .into_iter()
        .filter(|manager| command_for(*manager, Operation::Search).is_none())
        .collect::<Vec<_>>();

    // In `PackageManager::ALL`'s own order.
    assert_eq!(without, [PackageManager::Bun, YARN_BERRY, YARN_CLASSIC]);
}

/// And search is the only one, so every other operation is answerable by every
/// manager uf supports.
#[test]
fn search_is_the_only_operation_a_manager_can_lack() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            if matches!(operation, Operation::Search) {
                continue;
            }
            assert!(
                command_for(manager, operation).is_some(),
                "{manager} has no command for {operation:?}"
            );
        }
    }
}
