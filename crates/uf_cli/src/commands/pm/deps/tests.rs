//! What the four screens look like, and what the manifest reader reads.
//!
//! Rendered here rather than driven through npm, the way `install/tests.rs`
//! does it and for the same reason: a test that runs a package manager asserts
//! on whatever that manager resolved today. What is pinned below is the layout
//! and the arithmetic. `crates/uf_cli/tests/dependencies.rs` is the other half,
//! and it does run npm, over a fixture no registry is needed for.

use super::*;
use std::fs;
use uf_pm::delta::{ChangeKind, PackageChange};
use uf_term::Capabilities;

/// A renderer with nothing to draw with: no colour, ASCII glyphs, nobody
/// watching. The same capability a redirected stream gets.
fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

fn added(field: &'static str, name: &str, range: &str) -> ManifestChange {
    ManifestChange {
        member: None,
        field,
        name: name.to_owned(),
        range: range.to_owned(),
        kind: ManifestChangeKind::Added,
    }
}

fn unchanged_tree() -> LockfileDelta {
    LockfileDelta {
        detailed: true,
        ..LockfileDelta::default()
    }
}

fn changed_tree() -> LockfileDelta {
    LockfileDelta {
        changes: vec![PackageChange {
            name: "date-fns".into(),
            kind: ChangeKind::Added,
            before: "".into(),
            after: "4.1.0".into(),
        }],
        detailed: true,
        ..LockfileDelta::default()
    }
}

fn report(heading: &'static str, manifest: Vec<ManifestChange>, tree: LockfileDelta) -> DepsReport {
    DepsReport {
        heading,
        continued: false,
        manager: "npm".to_owned(),
        chosen_by: "package-lock.json".to_owned(),
        commands: vec!["npm install --ignore-scripts date-fns".to_owned()],
        lockfile: "package-lock.json · 17 packages · 12.40 kB".to_owned(),
        manifest,
        tree,
        link: None,
        elapsed: Duration::from_millis(2100),
    }
}

/// What `uf link` printed after npm had linked a package: the manifest and the
/// tree had not moved, so it said nothing had (ubugeeei-prod/uf#976).
#[test]
fn a_link_names_the_package_and_where_it_leads_rather_than_saying_nothing_changed() {
    let mut report = report("uf link", Vec::new(), unchanged_tree());
    report.commands = vec!["npm link --ignore-scripts ../ui".to_owned()];
    report.link = Some(LinkReport {
        name: "@acme/ui".to_owned(),
        to: "../ui".to_owned(),
        recorded: false,
    });

    let mut out = String::new();
    render_summary(&plain(), &mut out, &report);

    assert!(!out.contains("already up to date"), "{out}");
    assert!(out.lines().any(|line| line.trim() == "linked"), "{out}");
    let row = out
        .lines()
        .find(|line| line.trim_start().starts_with("@acme/ui"))
        .unwrap_or_else(|| panic!("no row for the link:\n{out}"));
    assert!(row.trim_end().ends_with("../ui"), "{out}");
    assert!(out.contains("linked @acme/ui in 2.1s"), "{out}");
}

/// Yarn 2+ records a link it cannot make yet, and the report says that rather
/// than claiming either a link or a failure.
#[test]
fn a_link_yarn_only_recorded_is_a_warning_that_says_why() {
    let mut report = report("uf link", Vec::new(), unchanged_tree());
    report.link = Some(LinkReport {
        name: "ui".to_owned(),
        to: "portal:/work/ui".to_owned(),
        recorded: true,
    });

    let mut out = String::new();
    render_summary(&plain(), &mut out, &report);

    assert!(out.lines().any(|line| line.trim() == "recorded"), "{out}");
    assert!(out.contains("portal:/work/ui"), "{out}");
    assert!(out.contains("nothing here depends on it yet"), "{out}");
    assert!(!out.contains("linked ui"), "{out}");
}

/// For every manager, a link is believed only when `node_modules` has one,
/// and every other state is a failure that says what is there instead.
#[test]
fn a_link_is_believed_only_when_node_modules_has_one() {
    use uf_pm::links::LinkState;
    use uf_pm::{PackageManager, YarnEdition};

    let base = Utf8Path::new("/work/app");
    for manager in PackageManager::ALL {
        let linked = link_report(
            base,
            manager,
            "ui",
            Some(LinkState::Linked("/work/ui".into())),
            None,
        )
        .unwrap_or_else(|why| panic!("{manager}: {why}"));
        assert_eq!(linked.to, "../ui", "{manager}");
        assert!(!linked.recorded, "{manager}");

        for (state, says) in [
            (
                Some(LinkState::Installed),
                "is an installed package, not a link",
            ),
            (Some(LinkState::Absent), "there is no node_modules/ui"),
            (
                Some(LinkState::Broken("../gone".into())),
                "is a link to ../gone, which does not exist",
            ),
        ] {
            let why = link_report(base, manager, "ui", state.clone(), None)
                .expect_err(&format!("{manager}: {state:?} is not a link"));
            assert!(
                why.contains(says) && why.ends_with("nothing was linked"),
                "{manager}: {why}"
            );
        }

        let recorded = link_report(
            base,
            manager,
            "ui",
            Some(LinkState::Absent),
            Some("portal:/work/ui".to_owned()),
        );
        if manager == PackageManager::Yarn(YarnEdition::Berry) {
            let recorded = recorded.unwrap_or_else(|why| panic!("{why}"));
            assert!(recorded.recorded);
            assert_eq!(recorded.to, "portal:/work/ui");
        } else {
            assert!(
                recorded.is_err(),
                "only Yarn 2+ records a link in resolutions, not {manager}"
            );
        }
    }
}

fn unlink_request_for(target: LinkTarget, directory: Option<&str>) -> UnlinkRequest {
    UnlinkRequest {
        target,
        name: "ui".to_owned(),
        directory: directory.map(Utf8PathBuf::from),
        operands: vec!["ui".to_owned()],
    }
}

/// `uf unlink NAME` runs only where there is a link to take out, for every
/// manager, and otherwise says what is there instead.
#[test]
fn unlink_runs_only_where_node_modules_or_yarn_has_a_link() {
    use uf_pm::{PackageManager, YarnEdition};

    let root = Utf8Path::new("/work/app");
    let request = unlink_request_for(LinkTarget::Package, None);
    let plan = |manager, before: Option<&LinkState>, resolution: Option<&str>| {
        unlink_plan(root, manager, &request, before, resolution, None, false)
            .map(|undone| undone.shown)
    };
    for manager in PackageManager::ALL {
        let linked = plan(manager, Some(&LinkState::Linked("/work/ui".into())), None);
        let resolved = plan(manager, Some(&LinkState::Absent), Some("portal:/work/ui"));
        if manager == PackageManager::Yarn(YarnEdition::Berry) {
            assert!(linked.is_err(), "Yarn 2+ records every link in resolutions");
            assert_eq!(resolved.as_deref(), Ok("portal:/work/ui"));
            continue;
        }
        assert_eq!(linked.as_deref(), Ok("../ui"), "{manager}");
        assert!(
            resolved.is_err(),
            "{manager} does not link through resolutions"
        );
        assert_eq!(
            plan(manager, Some(&LinkState::Broken("../gone".into())), None).as_deref(),
            Ok("../gone"),
            "{manager}: a link to nothing is still a link"
        );
        for (state, says) in [
            (
                Some(LinkState::Installed),
                "is an installed package, not a link",
            ),
            (Some(LinkState::Absent), "there is no node_modules/ui"),
            (None, "there is no node_modules/ui"),
        ] {
            let why =
                plan(manager, state.as_ref(), None).expect_err(&format!("{manager}: {state:?}"));
            assert!(why.contains(says), "{manager}: {why}");
        }
    }
}

/// A link that leads somewhere other than the directory named, or that is how
/// the manifest's own path dependency is installed, is not `uf link`'s to undo.
#[test]
fn unlink_leaves_a_link_it_did_not_make() {
    use uf_pm::{PackageManager, YarnEdition};

    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join("vendor/ui")).unwrap();
    fs::create_dir_all(root.join("ui")).unwrap();

    let other = root.join("other");
    let elsewhere = unlink_plan(
        &app,
        PackageManager::Npm,
        &unlink_request_for(LinkTarget::Package, Some(other.as_str())),
        Some(&LinkState::Linked(root.join("ui"))),
        None,
        None,
        false,
    )
    .expect_err("a link to another checkout");
    assert!(
        elsewhere.contains("links to ../ui, not to ../other"),
        "{elsewhere}"
    );

    let vendored = LinkState::Linked(app.join("vendor/ui"));
    let plan = |manager, range, pnpm_override| {
        unlink_plan(
            &app,
            manager,
            &unlink_request_for(LinkTarget::Package, None),
            Some(&vendored),
            None,
            Some(range),
            pnpm_override,
        )
        .map(|undone| undone.shown)
    };
    for manager in [
        PackageManager::Npm,
        PackageManager::Bun,
        PackageManager::Yarn(YarnEdition::Classic),
        PackageManager::Pnpm,
    ] {
        let why = plan(manager, "link:vendor/ui", false).expect_err(&format!("{manager}"));
        assert!(
            why.contains("declares ui as link:vendor/ui") && why.contains("uf remove ui"),
            "{manager}: {why}"
        );
    }
    // pnpm writes a `link:` dependency and an override for one link, and the
    // override is what makes it a link to undo.
    assert_eq!(
        plan(PackageManager::Pnpm, "link:vendor/ui", true).as_deref(),
        Ok("vendor/ui")
    );

    let afterwards = Afterwards {
        state: Some(vendored.clone()),
        version: Some("1.0.0".to_owned()),
        ..Afterwards::default()
    };
    let undone = |target: Utf8PathBuf| Undone {
        shown: "../ui".to_owned(),
        target: Some(target),
    };
    // The same link, back because the manifest declares it: what pnpm 12
    // leaves when it linked a package nothing had declared.
    assert_eq!(
        unlink_outcome(
            &app,
            PackageManager::Pnpm,
            "ui",
            &undone(app.join("vendor/ui")),
            &afterwards,
            Some("link:vendor/ui"),
        ),
        Ok(Unlinked::StillDeclared("link:vendor/ui".to_owned()))
    );
    // A different link, and the one the manifest declares: the release put
    // back, which npm and pnpm install as a link to its directory.
    for manager in [PackageManager::Npm, PackageManager::Pnpm] {
        assert_eq!(
            unlink_outcome(
                &app,
                manager,
                "ui",
                &undone(root.join("ui")),
                &afterwards,
                Some("file:vendor/ui"),
            ),
            Ok(Unlinked::Reinstalled("1.0.0".to_owned())),
            "{manager}"
        );
    }
}

/// What unlinking left, for every manager: the declared release back, nothing
/// at all, or a failure that says what is still there.
#[test]
fn unlink_says_what_node_modules_has_afterwards() {
    use uf_pm::PackageManager;

    let root = Utf8Path::new("/work/app");
    let undone = Undone {
        shown: "../ui".to_owned(),
        target: Some("/work/ui".into()),
    };
    let after = |state, resolution: Option<&str>, version: Option<&str>| Afterwards {
        state,
        resolution: resolution.map(ToOwned::to_owned),
        version: version.map(ToOwned::to_owned),
    };
    for manager in PackageManager::ALL {
        let outcome = |afterwards: Afterwards, declared: Option<&str>| {
            unlink_outcome(root, manager, "ui", &undone, &afterwards, declared)
        };
        assert_eq!(
            outcome(
                after(Some(LinkState::Installed), None, Some("1.0.0")),
                Some("^1.0.0")
            ),
            Ok(Unlinked::Reinstalled("1.0.0".to_owned())),
            "{manager}"
        );
        assert_eq!(
            outcome(after(Some(LinkState::Installed), None, Some("1.0.0")), None),
            Ok(Unlinked::Transitive("1.0.0".to_owned())),
            "{manager}"
        );
        assert_eq!(
            outcome(after(Some(LinkState::Absent), None, None), None),
            Ok(Unlinked::Gone),
            "{manager}"
        );
        let missing = outcome(after(Some(LinkState::Absent), None, None), Some("^1.0.0"))
            .expect_err("declared, and not there");
        assert!(missing.contains("run `uf install`"), "{manager}: {missing}");
        let still = outcome(
            after(Some(LinkState::Linked("/work/ui".into())), None, None),
            None,
        )
        .expect_err("still a link");
        assert!(still.contains("still links to ../ui"), "{manager}: {still}");
        assert_eq!(
            still.contains("pnpm-workspace.yaml"),
            manager == PackageManager::Pnpm,
            "{manager}: {still}"
        );
        let resolved = outcome(
            after(Some(LinkState::Absent), Some("portal:/work/ui"), None),
            None,
        )
        .expect_err("still resolved");
        assert!(
            resolved.contains("still resolves ui to portal:/work/ui"),
            "{manager}: {resolved}"
        );
    }
}

/// Unregistering removes a link to this package and nothing else, and says
/// what is there when it is not one.
#[test]
fn unregistering_removes_only_a_link_to_this_package() {
    use uf_pm::PackageManager;

    let entry = Utf8PathBuf::from("/usr/lib/node_modules/ui");
    assert_eq!(
        unregister_plan(
            PackageManager::Npm,
            "ui",
            &Registration::Linked(entry.clone())
        ),
        Ok(entry.clone())
    );
    let elsewhere = unregister_plan(
        PackageManager::Npm,
        "ui",
        &Registration::Elsewhere {
            entry: entry.clone(),
            to: "/work/other".into(),
        },
    )
    .unwrap_err();
    assert!(
        elsewhere.contains("links to /work/other, not to this package"),
        "{elsewhere}"
    );
    let installed =
        unregister_plan(PackageManager::Npm, "ui", &Registration::Installed(entry)).unwrap_err();
    assert!(
        installed.contains("installed from a registry"),
        "{installed}"
    );
    assert_eq!(
        unregister_plan(PackageManager::Bun, "ui", &Registration::Absent).unwrap_err(),
        "ui is not registered with bun"
    );
}

/// Each manager is given the package the way its own unlink takes it: npm and
/// pnpm a name to unregister, Yarn 1 and bun nothing, Yarn 2+ the path it
/// linked, and bun nothing to run at all for a link in the project.
#[test]
fn unlink_names_the_package_the_way_each_manager_takes_it() {
    use uf_pm::{PackageManager, YarnEdition};

    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(root.join("ui")).unwrap();
    fs::write(app.join("package.json"), r#"{ "name": "acme-app" }"#).unwrap();
    fs::write(root.join("ui/package.json"), r#"{ "name": "@acme/ui" }"#).unwrap();

    let register = |manager| unlink_request(&app, manager, None).unwrap();
    assert_eq!(register(PackageManager::Npm).operands, ["acme-app"]);
    assert_eq!(register(PackageManager::Pnpm).operands, ["acme-app"]);
    assert!(
        register(PackageManager::Yarn(YarnEdition::Classic))
            .operands
            .is_empty()
    );
    assert!(register(PackageManager::Bun).operands.is_empty());

    let by_path = unlink_request(&app, PackageManager::Npm, Some("../ui")).unwrap();
    assert_eq!(by_path.target, LinkTarget::Package);
    assert_eq!(by_path.name, "@acme/ui");
    assert_eq!(by_path.operands, ["@acme/ui"]);
    assert_eq!(
        by_path.directory.as_deref(),
        Some(root.join("ui").as_path())
    );

    let berry = PackageManager::Yarn(YarnEdition::Berry);
    let by_path = unlink_request(&app, berry, Some("../ui")).unwrap();
    assert_eq!(by_path.target, LinkTarget::Directory);
    assert_eq!(by_path.operands, ["../ui"]);
    assert_eq!(
        by_path.operation(berry),
        Some(Operation::Unlink {
            target: LinkTarget::Directory
        })
    );
    assert_eq!(
        unlink_request(&app, PackageManager::Bun, Some("@acme/ui"))
            .unwrap()
            .operation(PackageManager::Bun),
        None,
        "bun has no unlink <name>, so uf removes the link itself"
    );

    assert!(unlink_request(&root.join("nowhere"), PackageManager::Npm, None).is_err());
    assert!(unlink_request(&app, PackageManager::Npm, Some("../nowhere")).is_err());
}

/// The first `uf add`: both files moved, and the screen says which fields.
#[test]
fn an_add_that_changed_something_names_the_field_and_the_tree() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(
            "uf add",
            vec![added("dependencies", "date-fns", "^4.1.0")],
            changed_tree(),
        ),
    );

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts date-fns",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "",
            "  manifest",
            "",
            "       field         package   range",
            "    +  dependencies  date-fns  ^4.1.0",
            "",
            "  dependency tree",
            "    added  1",
            "",
            "       package   version",
            "    +  date-fns  4.1.0",
            "",
            "+ 1 package recorded in dependencies in 2.1s",
        ]
    );
    assert!(out.ends_with('\n'));
}

/// The second `uf add` of the same thing, which is the run that has to stay
/// cheap to read.
#[test]
fn an_add_that_changed_nothing_says_so_in_one_line() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", Vec::new(), unchanged_tree()),
    );

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts date-fns",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "",
            "+ already up to date in 2.1s",
        ]
    );
    assert!(
        !out.contains("manifest") && !out.contains("dependency tree"),
        "a report about nothing happening is noise:\n{out}"
    );
}

/// A range that was already satisfied, resolved to something newer: the
/// manifest did not move and the tree did, and both halves are said.
#[test]
fn a_tree_that_moved_without_the_manifest_is_not_called_an_add() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", Vec::new(), changed_tree()),
    );

    assert!(
        out.contains("+ the manifest already said so; 1 change in the tree"),
        "{out}"
    );
}

/// Each command's own verb, so the last line is not the same sentence four
/// times.
#[test]
fn each_command_reports_in_its_own_words() {
    for (heading, expected) in [
        ("uf add", "1 package recorded in dependencies"),
        ("uf remove", "1 package taken out of dependencies"),
        ("uf update", "1 package re-ranged in dependencies"),
    ] {
        let mut out = String::new();
        render_summary(
            &plain(),
            &mut out,
            &report(
                heading,
                vec![added("dependencies", "date-fns", "^4.1.0")],
                unchanged_tree(),
            ),
        );
        assert!(out.contains(expected), "{heading}:\n{out}");
    }
}

/// `uf dedupe` and `uf link` never promised a manifest change, so a tree that
/// moved is the whole sentence rather than a denial of one.
#[test]
fn a_command_that_is_not_about_the_manifest_reports_the_tree_alone() {
    for heading in ["uf dedupe", "uf link"] {
        let mut out = String::new();
        render_summary(
            &plain(),
            &mut out,
            &report(heading, Vec::new(), changed_tree()),
        );
        assert!(out.contains("+ 1 change in the tree"), "{heading}:\n{out}");
        assert!(
            !out.contains("the manifest already said so"),
            "{heading}:\n{out}"
        );
    }
}

/// A command that chose workspace members ran once per member, says so a row
/// at a time, and says which member each manifest change is in.
#[test]
fn changes_in_chosen_members_name_the_member() {
    let mut in_api = added("dependencies", "zod", "^4.1.8");
    in_api.member = Some("api".to_owned());
    let mut in_web = added("dependencies", "zod", "^4.1.8");
    in_web.member = Some("web".to_owned());
    let mut summary = report("uf add", vec![in_api, in_web], unchanged_tree());
    summary.commands = vec![
        "npm install --ignore-scripts zod  (api)".to_owned(),
        "npm install --ignore-scripts zod  (web)".to_owned(),
    ];

    let mut out = String::new();
    render_summary(&plain(), &mut out, &summary);

    assert_eq!(out.matches("command").count(), 2, "{out}");
    assert!(out.contains("workspace"), "{out}");
    let rows = out
        .lines()
        .filter(|line| line.contains("zod") && line.contains("^4.1.8"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows[0].contains("api") && rows[1].contains("web"), "{out}");
    assert!(out.contains("2 packages recorded in dependencies"), "{out}");
}

/// A run over the project alone has no member to name, and no column spent on
/// one.
#[test]
fn an_unscoped_change_has_no_workspace_column() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(
            "uf add",
            vec![added("dependencies", "date-fns", "^4.1.0")],
            unchanged_tree(),
        ),
    );

    assert!(!out.contains("workspace"), "{out}");
}

/// `--filter` and `-w` are one choice, which clap has already kept apart.
#[test]
fn the_two_workspace_flags_are_one_scope() {
    assert_eq!(Scope::from_flags(Vec::new(), false), Scope::Project);
    assert_eq!(Scope::from_flags(Vec::new(), true), Scope::WorkspaceRoot);
    assert_eq!(
        Scope::from_flags(vec!["ui".to_owned()], false),
        Scope::Members(vec!["ui".to_owned()])
    );
}

/// A lockfile uf does not parse gets no tree section rather than an empty one,
/// the way `uf install` does it.
#[test]
fn a_lockfile_uf_cannot_read_gets_no_empty_tree_section() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(
            "uf add",
            vec![added("dependencies", "date-fns", "^4.1.0")],
            LockfileDelta::default(),
        ),
    );

    assert!(out.contains("manifest"), "{out}");
    assert!(!out.contains("dependency tree"), "{out}");
}

/// Nothing drawn here needs a terminal.
#[test]
fn nothing_the_summary_draws_needs_an_escape_sequence() {
    for (manifest, tree) in [
        (Vec::new(), unchanged_tree()),
        (
            vec![added("peerDependencies", "react", "^19")],
            changed_tree(),
        ),
    ] {
        let mut out = String::new();
        render_summary(&plain(), &mut out, &report("uf add", manifest, tree));
        assert!(!out.contains('\u{1b}'), "{out}");
    }
}

/// A long list is cut, and says how much it cut.
#[test]
fn a_long_manifest_list_is_cut_and_says_how_much_it_cut() {
    let manifest: Vec<ManifestChange> = (0..MANIFEST_CHANGES_SHOWN + 4)
        .map(|index| added("dependencies", &format!("package-{index}"), "^1.0.0"))
        .collect();
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", manifest, unchanged_tree()),
    );

    assert!(out.contains("and 4 more"), "{out}");
    assert!(out.contains("package-0"), "{out}");
    assert!(!out.contains("package-16"), "{out}");
}

// --- the manifest reader -------------------------------------------------

fn manifest(source: &str) -> BTreeMap<(&'static str, String), String> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path =
        camino::Utf8PathBuf::from_path_buf(dir.path().join("package.json")).expect("a UTF-8 path");
    fs::write(&path, source).expect("a manifest");
    dependency_entries(&path)
}

/// All four fields are read, and nothing else is.
#[test]
fn every_dependency_field_is_read_and_only_those() {
    let entries = manifest(
        r#"{
          "name": "app",
          "dependencies": { "react": "^19.0.0" },
          "devDependencies": { "eslint": "^9" },
          "optionalDependencies": { "fsevents": "^2" },
          "peerDependencies": { "react-dom": "^19" },
          "bundledDependencies": ["nope"],
          "scripts": { "build": "vite build" }
        }"#,
    );

    assert_eq!(entries.len(), 4);
    assert_eq!(entries[&("dependencies", "react".to_owned())], "^19.0.0");
    assert_eq!(entries[&("devDependencies", "eslint".to_owned())], "^9");
    assert_eq!(
        entries[&("optionalDependencies", "fsevents".to_owned())],
        "^2"
    );
    assert_eq!(
        entries[&("peerDependencies", "react-dom".to_owned())],
        "^19"
    );
}

/// A manifest is repository content. It gets read before anything validates it,
/// so the pollution keys go the way they go everywhere else uf walks JSON.
#[test]
fn prototype_pollution_keys_are_never_dependency_names() {
    let entries = manifest(
        r#"{"dependencies": {"__proto__": "1", "constructor": "1", "prototype": "1", "ok": "^1"}}"#,
    );

    assert_eq!(entries.len(), 1);
    assert!(entries.contains_key(&("dependencies", "ok".to_owned())));
}

/// A manifest uf cannot read is no entries, not an error: the manager is what
/// gets to refuse a manifest, and it says so far better than this could.
#[test]
fn a_manifest_that_is_not_readable_json_is_no_entries() {
    assert!(manifest("this is not json").is_empty());
    assert!(manifest("[]").is_empty());
    assert!(manifest(r#"{"dependencies": ["react"]}"#).is_empty());
    assert!(manifest(r#"{"dependencies": {"react": 19}}"#).is_empty());
    assert!(dependency_entries(camino::Utf8Path::new("/nowhere/package.json")).is_empty());
}

/// The three ways an entry can differ, and the one way it cannot.
#[test]
fn a_manifest_diff_reports_arrivals_departures_and_new_ranges() {
    let before = manifest(
        r#"{"dependencies": {"react": "^18", "lodash": "^4"},
            "devDependencies": {"eslint": "^9"}}"#,
    );
    let after = manifest(
        r#"{"dependencies": {"react": "^19", "date-fns": "^4"},
            "devDependencies": {"eslint": "^9"}}"#,
    );

    let mut changes = manifest_changes(&before, &after);
    changes.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(
        changes,
        [
            added("dependencies", "date-fns", "^4"),
            ManifestChange {
                member: None,
                field: "dependencies",
                name: "lodash".to_owned(),
                range: "^4".to_owned(),
                kind: ManifestChangeKind::Removed,
            },
            ManifestChange {
                member: None,
                field: "dependencies",
                name: "react".to_owned(),
                range: "^19".to_owned(),
                kind: ManifestChangeKind::Reranged,
            },
        ],
        "eslint did not move, so it is not a change"
    );
}

/// The same package in two fields is two entries: `uf add --peer react` in a
/// project that already had `react` in `dependencies` really did add one.
#[test]
fn the_same_name_in_two_fields_is_two_entries() {
    let before = manifest(r#"{"dependencies": {"react": "^19"}}"#);
    let after =
        manifest(r#"{"dependencies": {"react": "^19"}, "peerDependencies": {"react": "^19"}}"#);

    assert_eq!(
        manifest_changes(&before, &after),
        [added("peerDependencies", "react", "^19")]
    );
}

/// The retry line is the command as it was typed, so it can be pasted back.
#[test]
fn the_retry_line_is_the_command_that_was_typed() {
    assert_eq!(
        retry_line("uf add", &["react".to_owned(), "react-dom@^19".to_owned()]),
        "uf add react react-dom@^19"
    );
    assert_eq!(retry_line("uf update", &[]), "uf update");
}

/// `uf update --latest` rewrites the manifests itself and says so; the install
/// that follows must not answer that with "the manifest already said so".
#[test]
fn a_continued_command_does_not_deny_the_line_above_it() {
    let mut report = report("uf update", Vec::new(), changed_tree());
    assert!(
        headline(&report).contains("the manifest already said so"),
        "{}",
        headline(&report)
    );

    report.continued = true;
    assert_eq!(headline(&report), "1 change in the tree");
}
