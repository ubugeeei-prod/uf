use super::*;

fn rendered(manager: PackageManager, operation: Operation<'_>) -> String {
    command_for(manager, operation).to_string()
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
        rendered(PackageManager::Uf, Operation::Add { dev: false }),
        "uf add"
    );
    assert_eq!(
        rendered(PackageManager::Uf, Operation::Add { dev: true }),
        "uf add --dev"
    );
    assert_eq!(rendered(PackageManager::Uf, Operation::Remove), "uf remove");
    assert_eq!(
        rendered(PackageManager::Uf, Operation::Run { task: "build" }),
        "uf run build"
    );
    assert_eq!(rendered(PackageManager::Uf, Operation::Exec), "uf exec");
    assert_eq!(rendered(PackageManager::Uf, Operation::DlxExec), "uf exec");
    assert_eq!(
        rendered(PackageManager::Uf, Operation::Update),
        "uf upgrade"
    );
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
        rendered(PackageManager::Npm, Operation::Add { dev: false }),
        "npm install"
    );
    assert_eq!(
        rendered(PackageManager::Npm, Operation::Add { dev: true }),
        "npm install --save-dev"
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
        rendered(PackageManager::Pnpm, Operation::Add { dev: false }),
        "pnpm add"
    );
    assert_eq!(
        rendered(PackageManager::Pnpm, Operation::Add { dev: true }),
        "pnpm add --save-dev"
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
        rendered(YARN_CLASSIC, Operation::Add { dev: false }),
        "yarn add"
    );
    assert_eq!(
        rendered(YARN_CLASSIC, Operation::Add { dev: true }),
        "yarn add --dev"
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
    assert_eq!(
        rendered(YARN_BERRY, Operation::Add { dev: false }),
        "yarn add"
    );
    assert_eq!(
        rendered(YARN_BERRY, Operation::Add { dev: true }),
        "yarn add --dev"
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
        rendered(PackageManager::Bun, Operation::Add { dev: false }),
        "bun add"
    );
    assert_eq!(
        rendered(PackageManager::Bun, Operation::Add { dev: true }),
        "bun add --dev"
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
            let invocation = command_for(manager, operation);
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
        let install = command_for(manager, Operation::Install);
        let frozen = command_for(manager, Operation::InstallFrozen);
        assert_ne!(install, frozen, "{manager} has no distinct frozen install");
    }
}

#[test]
fn dev_adds_differ_from_production_adds_for_every_manager() {
    for manager in PackageManager::ALL {
        let production = command_for(manager, Operation::Add { dev: false });
        let development = command_for(manager, Operation::Add { dev: true });
        assert_ne!(production, development, "{manager} ignores dev adds");
    }
}

#[test]
fn run_appends_the_task_as_the_final_argument() {
    for manager in PackageManager::ALL {
        let invocation = command_for(manager, Operation::Run { task: "test:unit" });
        assert_eq!(
            invocation.args.last().map(Cow::as_ref),
            Some("test:unit"),
            "{manager} dropped the task name"
        );
    }
}

#[test]
fn a_hostile_task_name_stays_a_single_argument() {
    let invocation = command_for(
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
    let invocation = command_for(PackageManager::Npm, Operation::Run { task: "" });

    assert_eq!(invocation.args.as_slice(), ["run", ""]);
}

#[test]
fn a_non_ascii_task_name_survives_intact() {
    let invocation = command_for(PackageManager::Bun, Operation::Run { task: "ビルド" });

    assert_eq!(invocation.args.last().map(Cow::as_ref), Some("ビルド"));
}

#[test]
fn mapped_invocations_never_allocate_beyond_the_inline_capacity() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let invocation = command_for(manager, operation);
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
        ]
    );
}
