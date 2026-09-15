//! What uf is about to spawn, without spawning it.
//!
//! [`run_operation`] is a process spawn and cannot be asserted on in a unit
//! test. [`invocation_for`] is the whole of its argument handling, which is the
//! part with the decisions in it: which flag uf adds, where it goes, and which
//! operands never reach a command line at all.

use super::*;
use crate::command::{DependencyKind, PROGRAMS};
use crate::detect::YarnEdition;

/// A root with nothing in it, for the assertions the root does not decide.
///
/// Only [`workspace_root_argument`] reads the directory, and it reads one
/// file; a path that does not exist is a project that is not a workspace root,
/// which is what every test below except the pnpm ones is about.
fn nowhere() -> &'static Utf8Path {
    Utf8Path::new("/uf-tests/not-a-project")
}

fn operands(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn args(
    manager: PackageManager,
    operation: Operation<'_>,
    list: &[&str],
    allow_scripts: bool,
) -> Vec<String> {
    args_in(nowhere(), manager, operation, list, allow_scripts)
}

fn args_in(
    root: &Utf8Path,
    manager: PackageManager,
    operation: Operation<'_>,
    list: &[&str],
    allow_scripts: bool,
) -> Vec<String> {
    let invocation = invocation_for(root, manager, operation, &operands(list), allow_scripts)
        .expect("these operands are passable");
    std::iter::once(invocation.program.to_owned())
        .chain(invocation.args.iter().map(ToString::to_string))
        .collect()
}

/// A range is part of the specifier, not a second argument.
///
/// `react@^18.2.0` split on the `@` would be two packages, one of which does
/// not exist; `"react@>=18 <20"` split on the space would be two more. Neither
/// ever goes through a shell, so neither is ever split.
#[test]
fn a_version_range_stays_inside_one_specifier() {
    for spec in [
        "react",
        "react@^18.2.0",
        "react@>=18 <20",
        "@uniflowed/vite@0.0.0-alpha.7",
        "react@npm:preact@^10",
        "./packages/ui",
        "file:../shared",
    ] {
        let invocation = invocation_for(
            nowhere(),
            PackageManager::Npm,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &operands(&[spec]),
            true,
        )
        .expect("a specifier is passable");

        assert_eq!(
            invocation.args.last().map(std::borrow::Cow::as_ref),
            Some(spec),
            "npm was handed something other than {spec:?}"
        );
    }
}

/// Every specifier is its own argument, in the order they were typed.
#[test]
fn several_specifiers_stay_several_arguments() {
    assert_eq!(
        args(
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Dev,
            },
            &["eslint", "prettier@^3"],
            true,
        ),
        ["pnpm", "add", "--save-dev", "eslint", "prettier@^3"]
    );
}

/// uf's own flag comes before the specifiers, so the `command` row reads as
/// "what uf added" then "what you asked for".
#[test]
fn the_scripts_refusal_precedes_the_specifiers() {
    assert_eq!(
        args(
            PackageManager::Npm,
            Operation::Add {
                kind: DependencyKind::Dev,
            },
            &["eslint"],
            false,
        ),
        ["npm", "install", "--save-dev", "--ignore-scripts", "eslint"]
    );
}

/// A project that allows lifecycle scripts is not told to ignore them.
#[test]
fn allowing_lifecycle_scripts_leaves_the_flag_off() {
    assert_eq!(
        args(PackageManager::Bun, Operation::Install, &[], true),
        ["bun", "install"]
    );
    assert_eq!(
        args(PackageManager::Bun, Operation::Install, &[], false),
        ["bun", "install", "--ignore-scripts"]
    );
}

/// `--ignore-scripts` is an install flag. `pnpm why --ignore-scripts` is an
/// unknown option, and a query that fails on a flag uf added is uf's bug.
#[test]
fn a_read_only_query_is_never_told_to_ignore_scripts() {
    for manager in PackageManager::ALL {
        let invocation = invocation_for(
            nowhere(),
            manager,
            Operation::Why,
            &operands(&["react"]),
            false,
        )
        .expect("a package name is passable");
        assert!(
            !invocation.args.iter().any(|arg| arg == "--ignore-scripts"),
            "{manager} why carried an install flag: {invocation}"
        );
        assert_eq!(
            invocation.args.last().map(std::borrow::Cow::as_ref),
            Some("react")
        );
    }
}

/// Every operation, every manager, with an operand: the operand is last and
/// the program is still one uf is allowed to spawn.
#[test]
fn an_operand_is_always_the_last_argument() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let invocation = match invocation_for(
                nowhere(),
                manager,
                operation,
                &operands(&["lodash"]),
                false,
            ) {
                Ok(invocation) => invocation,
                // The manager has no such command, which is what
                // `search_is_unsupported_where_the_manager_has_none` asserts.
                Err(ManagerRunError::Unsupported { .. }) => continue,
                Err(error) => panic!("{manager} {operation:?}: {error}"),
            };
            assert!(PROGRAMS.contains(&invocation.program));
            assert_eq!(
                invocation.args.last().map(std::borrow::Cow::as_ref),
                Some("lodash"),
                "{manager} {operation:?} dropped or reordered the operand"
            );
        }
    }
}

/// An operand starting with `-` is a flag to every manager there is, so it is
/// refused here rather than passed on to mean something.
#[test]
fn an_operand_that_would_be_read_as_a_flag_is_refused() {
    for operand in ["--global", "-g", "--save-dev", "-"] {
        let error = invocation_for(
            nowhere(),
            PackageManager::Npm,
            Operation::Remove,
            &operands(&[operand]),
            false,
        )
        .expect_err("uf does not pass this on");

        let message = error.to_string();
        assert!(message.contains(operand), "{message}");
        assert!(
            message.contains("would read as a flag"),
            "the message has to say why: {message}"
        );
        assert!(
            message.contains("./"),
            "and what to write instead: {message}"
        );
    }
}

/// An empty operand is a package with no name.
#[test]
fn an_empty_operand_is_refused() {
    let error = invocation_for(
        nowhere(),
        PackageManager::Bun,
        Operation::Add {
            kind: DependencyKind::Prod,
        },
        &operands(&[""]),
        true,
    )
    .expect_err("uf does not pass this on");

    assert!(error.to_string().contains("it is empty"), "{error}");
}

/// A refusal is a refusal for the whole command: one bad operand does not get
/// a half-run that installed the others.
#[test]
fn one_refused_operand_refuses_the_whole_invocation() {
    assert!(
        invocation_for(
            nowhere(),
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Add {
                kind: DependencyKind::Peer,
            },
            &operands(&["react", "--force", "react-dom"]),
            true,
        )
        .is_err()
    );
}

/// A scoped name starts with `@`, not with `-`, and is ordinary.
#[test]
fn a_scoped_name_is_an_ordinary_operand() {
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Remove,
            &["@uniflowed/vite"],
            false,
        ),
        ["yarn", "remove", "--ignore-scripts", "@uniflowed/vite"]
    );
}

/// The frozen install is the one CI runs, and it takes no operands.
#[test]
fn the_frozen_install_is_the_managers_own_immutable_install() {
    assert_eq!(
        args(PackageManager::Npm, Operation::InstallFrozen, &[], false),
        ["npm", "ci", "--ignore-scripts"]
    );
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::InstallFrozen,
            &[],
            true,
        ),
        ["yarn", "install", "--immutable"]
    );
}

/// A directory shaped like a pnpm workspace root: the one file pnpm looks for.
fn pnpm_workspace() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::write(
        root.join("pnpm-workspace.yaml"),
        "packages:\n  - packages/*\n",
    )
    .expect("a workspace file");
    (dir, root)
}

/// The whole of ubugeeei-prod/uf#484.
///
/// `pnpm add` in a workspace root answers `ERR_PNPM_ADDING_TO_ROOT` and tells
/// you to run the command again with `-w` — advice nobody could follow through
/// uf, which never passed it. So `uf add --dev @uniflowed/test` in a pnpm
/// monorepo failed and recommended the command that had just failed.
#[test]
fn adding_at_a_pnpm_workspace_root_says_the_root_is_meant() {
    let (_guard, root) = pnpm_workspace();

    assert_eq!(
        args_in(
            &root,
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Dev,
            },
            &["@uniflowed/test@0.0.0-alpha.10"],
            false,
        ),
        [
            "pnpm",
            "add",
            "--save-dev",
            "--workspace-root",
            "--ignore-scripts",
            "@uniflowed/test@0.0.0-alpha.10",
        ],
        "uf's own additions come together, scope first, and the specifier last"
    );
}

/// Inside a member package it is not passed, and that is the point.
///
/// pnpm's refusal exists because a shell can be in the workspace root by
/// accident. uf's root is the nearest directory with a `package.json`, so
/// `uf add` in a member resolves to the member — and `--workspace-root` there
/// would move the dependency somewhere nobody asked for it to go.
#[test]
fn adding_inside_a_workspace_member_targets_the_member() {
    let (_guard, root) = pnpm_workspace();
    let member = root.join("packages/ui");
    std::fs::create_dir_all(&member).expect("a member directory");

    assert_eq!(
        args_in(
            &member,
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &["react"],
            true,
        ),
        ["pnpm", "add", "react"]
    );
}

/// One manager and one operation, because that is where pnpm's check is.
///
/// The other managers add to a workspace root without being asked twice, and
/// `--workspace-root` is not a flag any of them has: passing it to npm would
/// turn a working `uf add` into an unknown option. pnpm's own check lives in
/// its `add` handler alone, so `remove` and `update` are left as they were.
#[test]
fn no_other_manager_or_operation_is_told_about_the_workspace_root() {
    let (_guard, root) = pnpm_workspace();

    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let Ok(invocation) = invocation_for(&root, manager, operation, &[], false) else {
                // The manager has no such command; that refusal is asserted on
                // by `search_is_unsupported_where_the_manager_has_none`.
                continue;
            };
            let named_the_root = invocation.args.iter().any(|arg| arg == "--workspace-root");
            let expected =
                manager == PackageManager::Pnpm && matches!(operation, Operation::Add { .. });
            assert_eq!(
                named_the_root, expected,
                "{manager} {operation:?}: {invocation}"
            );
        }
    }
}

/// A project that is not a workspace root is not told it is one.
///
/// `pnpm add` in an ordinary pnpm project has never needed the flag, and one
/// added anyway would be uf changing what a command means on every project to
/// fix it on some.
#[test]
fn an_ordinary_pnpm_project_is_left_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").expect("a lockfile");

    assert_eq!(
        args_in(
            &root,
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &["react"],
            true,
        ),
        ["pnpm", "add", "react"]
    );
}

/// A `pnpm-workspace.yaml` that is a directory is not a workspace root.
///
/// The marker is repository content and the check is `symlink_metadata`, the
/// same rule detection applies to a manifest: a directory, a device, or a
/// symlink pointing out of the checkout does not get to decide which project a
/// dependency lands in.
#[test]
fn a_workspace_marker_that_is_not_a_regular_file_does_not_count() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::create_dir(root.join("pnpm-workspace.yaml")).expect("a directory in its place");

    assert!(!crate::detect::is_pnpm_workspace_root(&root));
    assert_eq!(
        args_in(
            &root,
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &["react"],
            true,
        ),
        ["pnpm", "add", "react"]
    );
}

/// A directory shaped like a Yarn 1 workspace root: `workspaces` in its own
/// `package.json`.
fn yarn_workspace() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::write(
        root.join("package.json"),
        r#"{ "name": "root", "private": true, "workspaces": ["packages/*"] }"#,
    )
    .expect("a manifest");
    (dir, root)
}

/// Yarn 1 refuses `yarn add` *and* `yarn remove` at a workspace root without
/// `-W`, which through uf was a command that failed and told you to add a flag
/// uf never passed — the Yarn 1 half of ubugeeei-prod/uf#484.
#[test]
fn yarn_1_is_told_the_workspace_root_is_meant_on_add_and_remove() {
    let (_guard, root) = yarn_workspace();

    for operation in [
        Operation::Add {
            kind: DependencyKind::Prod,
        },
        Operation::Remove,
    ] {
        let args = args_in(
            &root,
            PackageManager::Yarn(YarnEdition::Classic),
            operation,
            &["left-pad"],
            true,
        );
        assert_eq!(
            args.get(2).map(String::as_str),
            Some("--ignore-workspace-root-check"),
            "{operation:?}: {args:?}"
        );
    }
    // `yarn upgrade` has no such check, and a flag it does not need is a flag
    // that can only be wrong later.
    assert_eq!(
        args_in(
            &root,
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Update,
            &[],
            true,
        ),
        ["yarn", "upgrade"]
    );
    // And no other manager is told Yarn 1's flag.
    for manager in [
        PackageManager::Npm,
        PackageManager::Pnpm,
        PackageManager::Yarn(YarnEdition::Berry),
        PackageManager::Bun,
    ] {
        let invocation = invocation_for(&root, manager, Operation::Remove, &[], true)
            .expect("every manager removes");
        assert!(
            !invocation
                .args
                .iter()
                .any(|arg| arg == "--ignore-workspace-root-check"),
            "{manager}: {invocation}"
        );
    }
}

/// A member of that workspace is not its root, and is left alone.
#[test]
fn yarn_1_inside_a_member_is_not_told_about_the_root() {
    let (_guard, root) = yarn_workspace();
    let member = root.join("packages/ui");
    std::fs::create_dir_all(&member).expect("a member directory");
    std::fs::write(member.join("package.json"), r#"{ "name": "ui" }"#).expect("a manifest");

    assert_eq!(
        args_in(
            &member,
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &["left-pad"],
            true,
        ),
        ["yarn", "add", "left-pad"]
    );
}

/// Yarn 2+ answers `--ignore-scripts` with "Unsupported option name" on every
/// command, so uf's refusal is the setting Yarn reads from the environment,
/// and no flag at all.
#[test]
fn yarn_2_refuses_scripts_through_its_setting_rather_than_a_flag_it_rejects() {
    for operation in Operation::ALL {
        let Ok(invocation) = invocation_for(
            nowhere(),
            PackageManager::Yarn(YarnEdition::Berry),
            operation,
            &[],
            false,
        ) else {
            continue;
        };
        assert!(
            !invocation.args.iter().any(|arg| arg == "--ignore-scripts"),
            "{operation:?}: {invocation}"
        );
        let refused = invocation.env.contains(&("YARN_ENABLE_SCRIPTS", "false"));
        assert_eq!(
            refused,
            operation.installs_packages(),
            "{operation:?}: {invocation}"
        );
    }
    // And allowed scripts leave the setting alone.
    let allowed = invocation_for(
        nowhere(),
        PackageManager::Yarn(YarnEdition::Berry),
        Operation::Install,
        &[],
        true,
    )
    .expect("berry installs");
    assert!(allowed.env.is_empty(), "{allowed}");
}

/// `pnpm link --ignore-scripts` is "Unknown option: 'ignore-scripts'"; the
/// setting spelled as pnpm's `--config.` is accepted by every command.
#[test]
fn pnpm_link_is_told_about_scripts_in_the_spelling_it_accepts() {
    for target in crate::LinkTarget::ALL {
        let args = args(
            PackageManager::Pnpm,
            Operation::Link { target },
            &["../lib"],
            false,
        );
        assert_eq!(
            args,
            ["pnpm", "link", "--config.ignore-scripts=true", "../lib"],
            "{target:?}"
        );
    }
    // `pnpm remove --ignore-scripts` is the same refusal — which every
    // `uf remove` in a pnpm project hit — and `patch-commit` does not declare
    // the flag either.
    assert_eq!(
        args(
            PackageManager::Pnpm,
            Operation::Remove,
            &["left-pad"],
            false
        ),
        ["pnpm", "remove", "--config.ignore-scripts=true", "left-pad"]
    );
    assert_eq!(
        args(
            PackageManager::Pnpm,
            Operation::PatchCommit,
            &["/tmp/p"],
            false
        ),
        [
            "pnpm",
            "patch-commit",
            "--config.ignore-scripts=true",
            "/tmp/p"
        ]
    );
    // Everywhere else pnpm keeps the flag it documents.
    assert_eq!(
        args(PackageManager::Pnpm, Operation::Dedupe, &[], false),
        ["pnpm", "dedupe", "--ignore-scripts"]
    );
}

/// `uf info <package> <field>` puts the field where each manager reads it.
#[test]
fn an_info_field_is_a_positional_everywhere_but_yarn_2() {
    assert_eq!(
        args(
            PackageManager::Npm,
            Operation::Info,
            &["react", "version"],
            false
        ),
        ["npm", "view", "react", "version"]
    );
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Info,
            &["react", "version"],
            false,
        ),
        ["yarn", "info", "react", "version"]
    );
    assert_eq!(
        args(
            PackageManager::Bun,
            Operation::Info,
            &["react", "version"],
            false
        ),
        ["bun", "info", "react", "version"]
    );
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Info,
            &["react", "version"],
            false,
        ),
        ["yarn", "npm", "info", "react", "--fields", "version"]
    );
    // With no field there is nothing to spell differently.
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Info,
            &["react"],
            false,
        ),
        ["yarn", "npm", "info", "react"]
    );
}

/// A refusal says what to run instead, for each everyday verb a manager lacks.
#[test]
fn every_missing_everyday_verb_is_refused_with_what_to_run_instead() {
    for (manager, operation, advice) in [
        (
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Dedupe,
            "uf install",
        ),
        (PackageManager::Bun, Operation::Dedupe, "uf update"),
        (
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Link {
                target: crate::LinkTarget::Directory,
            },
            "uf link <its name>",
        ),
        (
            PackageManager::Bun,
            Operation::Link {
                target: crate::LinkTarget::Directory,
            },
            "uf link <its name>",
        ),
        (
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Link {
                target: crate::LinkTarget::Package,
            },
            "uf link <path to the package>",
        ),
        (
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Link {
                target: crate::LinkTarget::Register,
            },
            "uf link <path to the package>",
        ),
        (
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::InstallFrozenProd,
            "uf install --prod",
        ),
    ] {
        let Err(ManagerRunError::Unsupported {
            operation: name,
            hint,
            ..
        }) = invocation_for(nowhere(), manager, operation, &[], false)
        else {
            panic!("{manager} {operation:?} was not refused");
        };
        assert_eq!(name, operation.name());
        assert!(hint.contains(advice), "{manager} {operation:?}: {hint}");
    }
}

/// `yarn up` is project-wide wherever it runs, so it is the one member-scoped
/// operation uf refuses; everything else runs per member.
#[test]
fn only_yarn_2s_update_cannot_be_scoped_to_members() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let refused = check_member_operation(manager, operation).is_err();
            let expected = manager == PackageManager::Yarn(YarnEdition::Berry)
                && operation == Operation::Update;
            assert_eq!(refused, expected, "{manager} {operation:?}");
        }
    }
}

/// A Yarn 2 or 3 production install that fails is told about the plugin
/// `yarn workspaces focus` lives in, and nothing else gets a guess.
#[test]
fn a_failed_yarn_2_production_install_names_the_plugin_it_needs() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let hint = failure_hint(manager, operation);
            if manager == PackageManager::Yarn(YarnEdition::Berry)
                && operation == Operation::InstallProd
            {
                let hint = hint.expect("the production install names the plugin");
                assert!(
                    hint.contains("yarn plugin import workspace-tools"),
                    "{hint}"
                );
            } else {
                assert_eq!(hint, None, "{manager} {operation:?}");
            }
        }
    }
}
