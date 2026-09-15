use camino::Utf8Path;

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
                // `pnpm patch` extracts into a temporary directory; the commit
                // is the half that installs.
                | Operation::Patch
                // A registry read.
                | Operation::Info
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
            // Berry's production install is `yarn workspaces focus`, which has
            // no frozen form.
            Operation::InstallProd,
            Operation::InstallFrozenProd,
            Operation::Exec,
            Operation::DlxExec,
            Operation::Update,
            // Yarn 1's `yarn list` became `yarn info` in Berry, and Berry put
            // the registry commands under `yarn npm`.
            Operation::List,
            Operation::Audit,
            // `yarn patch` is Berry's, and Yarn 1 never had one.
            Operation::Patch,
            Operation::PatchCommit,
            // Yarn 1's `dedupe` only says it is unnecessary.
            Operation::Dedupe,
            // Yarn 1 links by name, Berry by path.
            Operation::Link {
                target: LinkTarget::Register,
            },
            Operation::Link {
                target: LinkTarget::Package,
            },
            Operation::Link {
                target: LinkTarget::Directory,
            },
            Operation::Info,
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

/// Every operation but the ones named here, so a manager uf supports can
/// answer the rest of the table without uf substituting anybody.
#[test]
fn only_the_operations_named_here_can_be_missing_from_a_manager() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            if matches!(
                operation,
                Operation::Search
                    | Operation::Patch
                    | Operation::PatchCommit
                    | Operation::InstallFrozenProd
                    | Operation::Dedupe
                    | Operation::Link { .. }
            ) {
                continue;
            }
            assert!(
                command_for(manager, operation).is_some(),
                "{manager} has no command for {operation:?}"
            );
        }
    }
}

/// The everyday verbs of ubugeeei-prod/uf#976, one manager's row at a time:
/// `install --prod`, `dedupe`, the three `link`s and `info`, each as the
/// command that manager's users already type, or as nothing where it has none.
#[test]
fn every_manager_maps_the_everyday_verbs_or_has_none() {
    let link = |target| Operation::Link { target };
    let rows: [(PackageManager, [Option<&str>; 7]); 6] = [
        (
            PackageManager::Uf,
            [
                Some("uf install --prod"),
                Some("uf install --frozen-lockfile --prod"),
                Some("uf dedupe"),
                Some("uf link"),
                Some("uf link"),
                Some("uf link"),
                Some("uf info"),
            ],
        ),
        (
            PackageManager::Npm,
            [
                Some("npm install --omit=dev"),
                Some("npm ci --omit=dev"),
                Some("npm dedupe"),
                Some("npm link"),
                Some("npm link"),
                Some("npm link"),
                Some("npm view"),
            ],
        ),
        (
            PackageManager::Pnpm,
            [
                Some("pnpm install --prod"),
                Some("pnpm install --frozen-lockfile --prod"),
                Some("pnpm dedupe"),
                Some("pnpm link"),
                Some("pnpm link"),
                Some("pnpm link"),
                Some("pnpm view"),
            ],
        ),
        (
            YARN_CLASSIC,
            [
                Some("yarn install --production"),
                Some("yarn install --frozen-lockfile --production"),
                None,
                Some("yarn link"),
                Some("yarn link"),
                None,
                Some("yarn info"),
            ],
        ),
        (
            YARN_BERRY,
            [
                Some("yarn workspaces focus --all --production"),
                None,
                Some("yarn dedupe"),
                None,
                None,
                Some("yarn link"),
                Some("yarn npm info"),
            ],
        ),
        (
            PackageManager::Bun,
            [
                Some("bun install --omit=dev"),
                Some("bun install --frozen-lockfile --production"),
                None,
                Some("bun link"),
                Some("bun link"),
                None,
                Some("bun info"),
            ],
        ),
    ];

    for (manager, expected) in rows {
        let operations = [
            Operation::InstallProd,
            Operation::InstallFrozenProd,
            Operation::Dedupe,
            link(LinkTarget::Register),
            link(LinkTarget::Package),
            link(LinkTarget::Directory),
            Operation::Info,
        ];
        for (operation, expected) in operations.into_iter().zip(expected) {
            assert_eq!(
                command_for(manager, operation)
                    .map(|invocation| invocation.to_string())
                    .as_deref(),
                expected,
                "{manager} {operation:?}"
            );
        }
    }
}

/// A production install is never the plain install under another name: the
/// flag that leaves `devDependencies` out is the whole point of it.
#[test]
fn a_production_install_differs_from_the_plain_install_wherever_it_exists() {
    for manager in PackageManager::ALL {
        let install = supported(manager, Operation::Install);
        assert_ne!(
            install,
            supported(manager, Operation::InstallProd),
            "{manager}"
        );
        if let Some(frozen) = command_for(manager, Operation::InstallFrozenProd) {
            assert_ne!(
                frozen,
                supported(manager, Operation::InstallFrozen),
                "{manager}"
            );
        }
    }
}

/// What installs is what gets told to leave scripts alone, and `info`, which
/// only reads a registry, is not.
#[test]
fn the_everyday_verbs_that_install_are_the_ones_that_can_run_scripts() {
    for operation in [
        Operation::InstallProd,
        Operation::InstallFrozenProd,
        Operation::Dedupe,
        Operation::Link {
            target: LinkTarget::Register,
        },
        Operation::Link {
            target: LinkTarget::Package,
        },
        Operation::Link {
            target: LinkTarget::Directory,
        },
    ] {
        assert!(operation.installs_packages(), "{operation:?}");
    }
    assert!(!Operation::Info.installs_packages());
}

/// A path is written as a path, and everything else is a name — including a
/// name that looks like a path to a person skimming it.
#[test]
fn a_link_target_is_a_path_only_when_it_is_written_as_one() {
    assert_eq!(LinkTarget::of(None), LinkTarget::Register);
    // Windows spells a drive and a share differently, and only Windows reads
    // them as absolute.
    #[cfg(windows)]
    for path in [r"C:\work\ui", r"\\server\share\ui"] {
        assert_eq!(LinkTarget::of(Some(path)), LinkTarget::Directory, "{path}");
    }
    for path in [
        ".",
        "..",
        "./ui",
        "../ui",
        "/work/ui",
        "../../packages/ui",
        r".\ui",
        r"..\ui",
    ] {
        assert_eq!(LinkTarget::of(Some(path)), LinkTarget::Directory, "{path}");
    }
    for name in ["ui", "@acme/ui", "left-pad", ".bin-tools", "..."] {
        let expected = if name == "..." {
            // Three dots is not `..`, and not `../` either.
            LinkTarget::Package
        } else {
            LinkTarget::Package
        };
        assert_eq!(LinkTarget::of(Some(name)), expected, "{name}");
    }
}

/// An invocation with a variable renders it in front, where a reader would
/// type it, and serializes it only when there is one.
#[test]
fn a_variable_is_rendered_before_the_program_and_serialized_only_when_set() {
    let mut invocation = supported(YARN_BERRY, Operation::Install);
    assert_eq!(
        serde_json::to_string(&invocation).unwrap(),
        r#"{"program":"yarn","args":["install"]}"#
    );

    invocation.env.push(("YARN_ENABLE_SCRIPTS", "false"));
    assert_eq!(
        invocation.to_string(),
        "YARN_ENABLE_SCRIPTS=false yarn install"
    );
    assert_eq!(
        serde_json::to_string(&invocation).unwrap(),
        r#"{"program":"yarn","args":["install"],"env":[["YARN_ENABLE_SCRIPTS","false"]]}"#
    );
}

/// Two managers have `patch`, three do not, and the two that do have both
/// halves — an `Operation::Patch` a manager could open but not commit would be
/// a command that leaves an edit nowhere.
#[test]
fn patch_is_unsupported_where_the_manager_has_none() {
    let without = PackageManager::ALL
        .into_iter()
        .filter(|manager| command_for(*manager, Operation::Patch).is_none())
        .collect::<Vec<_>>();

    // In `PackageManager::ALL`'s own order.
    assert_eq!(
        without,
        [PackageManager::Bun, YARN_CLASSIC, PackageManager::Npm]
    );

    for manager in PackageManager::ALL {
        assert_eq!(
            command_for(manager, Operation::Patch).is_some(),
            command_for(manager, Operation::PatchCommit).is_some(),
            "{manager} has one half of patch and not the other"
        );
    }
}

/// And the refusal names what to do instead, because "your package manager
/// cannot" is half an answer.
#[test]
fn the_patch_refusal_names_the_ecosystems_answer() {
    let refusal = crate::invocation_for(
        Utf8Path::new("/nowhere"),
        PackageManager::Npm,
        Operation::Patch,
        &[],
        false,
    )
    .expect_err("npm has no patch");

    let crate::ManagerRunError::Unsupported {
        manager,
        operation,
        hint,
    } = refusal
    else {
        panic!("npm's patch was a passthrough: {refusal:?}");
    };
    assert_eq!(manager, "npm");
    assert_eq!(operation, "patch");
    assert!(hint.contains("patch-package"), "{hint}");
    assert!(hint.contains("pnpm and yarn 2+"), "{hint}");
}
