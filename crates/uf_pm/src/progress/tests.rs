//! The classifier is checked against lines npm actually printed.
//!
//! Every `npm http` line below was copied out of a real `npm install
//! --loglevel=http` run rather than written from the documentation, because
//! the documentation does not describe this format at all and a line invented
//! to match the parser proves only that the parser matches itself.

use super::*;
use crate::detect::YarnEdition;

/// A time base that tests can move by hand.
fn clock() -> Instant {
    Instant::now()
}

fn resolved(line: &str) -> String {
    match Reader::Npm.classify(line) {
        ManagerEvent::Resolved { package } => package,
        other => panic!("expected a manifest read, got {other:?} for {line:?}"),
    }
}

fn fetched(line: &str) -> (String, bool) {
    match Reader::Npm.classify(line) {
        ManagerEvent::Fetched { package, cached } => (package, cached),
        other => panic!("expected an archive, got {other:?} for {line:?}"),
    }
}

#[test]
fn a_manifest_request_names_the_package() {
    assert_eq!(
        resolved("npm http fetch GET 200 https://registry.npmjs.org/lodash 168ms (cache miss)"),
        "lodash"
    );
}

#[test]
fn a_scoped_manifest_request_is_unescaped() {
    // npm percent-escapes the scope separator, and `@types%2fnode` is not a
    // package name anybody would recognise on screen.
    assert_eq!(
        resolved("npm http fetch GET 200 https://registry.npmjs.org/@types%2fnode 40ms"),
        "@types/node"
    );
}

#[test]
fn an_archive_request_carries_the_version() {
    assert_eq!(
        fetched(
            "npm http fetch GET 200 https://registry.npmjs.org/chalk/-/chalk-5.3.0.tgz 71ms (cache miss)"
        ),
        ("chalk@5.3.0".to_owned(), false)
    );
}

#[test]
fn a_scoped_archive_keeps_its_scope_and_loses_it_from_the_version() {
    assert_eq!(
        fetched(
            "npm http fetch GET 200 https://registry.npmjs.org/@esbuild%2fdarwin-arm64/-/darwin-arm64-0.21.5.tgz 300ms"
        ),
        ("@esbuild/darwin-arm64@0.21.5".to_owned(), false)
    );
}

#[test]
fn an_archive_served_from_the_cache_says_so() {
    assert_eq!(
        fetched(
            "npm http cache lodash@https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz 0ms (cache hit)"
        ),
        ("lodash@4.17.21".to_owned(), true)
    );
}

#[test]
fn an_archive_that_breaks_the_naming_convention_keeps_its_name() {
    // A registry is free to serve an archive under any file name; losing the
    // version is better than printing a name that is not the package's.
    assert_eq!(
        fetched("npm http fetch GET 200 https://registry.npmjs.org/left-pad/-/whatever.tgz 5ms"),
        ("left-pad".to_owned(), false)
    );
}

#[test]
fn the_advisory_request_is_the_audit() {
    assert_eq!(
        Reader::Npm.classify(
            "npm http fetch POST 200 https://registry.npmjs.org/-/npm/v1/security/advisories/bulk 154ms"
        ),
        ManagerEvent::Audited
    );
}

#[test]
fn everything_npm_would_have_printed_anyway_is_passed_through() {
    for line in [
        "npm warn deprecated glob@7.2.3: Glob versions prior to v9 are no longer supported",
        "npm error code EPERM",
        "",
        "added 16 packages, and audited 17 packages in 5s",
        "2 vulnerabilities (1 moderate, 1 high)",
        "  run `npm fund` for details",
    ] {
        assert_eq!(
            Reader::Npm.classify(line),
            ManagerEvent::Passthrough,
            "{line:?} is npm's own line and must reach the terminal"
        );
    }
}

#[test]
fn an_http_line_uf_cannot_read_is_still_npm_s_to_print() {
    // The rule is not "swallow anything starting with `npm http`": it is
    // "swallow the request lines uf asked for". A line that is neither is a
    // line uf has no business hiding.
    assert_eq!(
        Reader::Npm.classify("npm http something new npm started printing"),
        ManagerEvent::Passthrough
    );
}

#[test]
fn a_package_name_cannot_smuggle_an_escape_sequence_onto_the_terminal() {
    // The name comes from a registry and is about to be drawn into a region
    // uf steers with cursor movements. `\x1b[2A` inside it would move that
    // cursor and the region would start eating the scrollback.
    let hostile = "npm http fetch GET 200 https://registry.npmjs.org/ev\u{1b}[2Ail 5ms";
    assert_eq!(resolved(hostile), "ev[2Ail");
    assert!(!resolved(hostile).contains('\u{1b}'));
}

#[test]
fn a_label_is_bounded_however_long_the_registry_answers() {
    let long = "x".repeat(MAX_LABEL * 4);
    assert_eq!(safe_label(&long).chars().count(), MAX_LABEL);
}

#[test]
fn only_npm_is_narrated() {
    assert_eq!(Reader::for_manager(PackageManager::Npm), Some(Reader::Npm));
    for manager in [
        PackageManager::Pnpm,
        PackageManager::Yarn(YarnEdition::Classic),
        PackageManager::Yarn(YarnEdition::Berry),
        PackageManager::Bun,
        PackageManager::Uf,
    ] {
        assert_eq!(
            Reader::for_manager(manager),
            None,
            "{manager} draws its own install and must keep its terminal"
        );
    }
}

#[test]
fn the_ladder_starts_resolving() {
    let watch = InstallWatch::start(clock());
    assert_eq!(watch.current(), InstallPhase::Resolve);
    assert_eq!(watch.phases()[0].state, PhaseState::Running);
    assert!(
        watch.phases()[1..]
            .iter()
            .all(|phase| phase.state == PhaseState::Waiting)
    );
}

#[test]
fn an_archive_moves_the_ladder_on_and_a_late_manifest_does_not_move_it_back() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Resolved {
            package: "react".to_owned(),
        },
        start,
    );
    watch.observe(
        &ManagerEvent::Fetched {
            package: "react@18.3.1".to_owned(),
            cached: false,
        },
        start + Duration::from_millis(100),
    );
    assert_eq!(watch.current(), InstallPhase::Fetch);

    // npm keeps reading metadata while it downloads; the ladder must not
    // bounce between two phases for the rest of the install.
    watch.observe(
        &ManagerEvent::Resolved {
            package: "scheduler".to_owned(),
        },
        start + Duration::from_millis(150),
    );
    assert_eq!(watch.current(), InstallPhase::Fetch);
    assert_eq!(watch.phases()[0].count, 2);
    assert_eq!(watch.phases()[0].state, PhaseState::Done);
}

#[test]
fn a_quiet_registry_means_the_manager_is_writing_node_modules() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Fetched {
            package: "react@18.3.1".to_owned(),
            cached: false,
        },
        start,
    );

    watch.idle(start + LINKING_IDLE / 2);
    assert_eq!(
        watch.current(),
        InstallPhase::Fetch,
        "a gap shorter than the threshold is just a slow response"
    );

    watch.idle(start + LINKING_IDLE);
    assert_eq!(watch.current(), InstallPhase::Link);
}

#[test]
fn one_more_archive_takes_the_ladder_back_off_linking() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Fetched {
            package: "a@1".to_owned(),
            cached: false,
        },
        start,
    );
    watch.idle(start + LINKING_IDLE);
    assert_eq!(watch.current(), InstallPhase::Link);

    watch.observe(
        &ManagerEvent::Fetched {
            package: "b@2".to_owned(),
            cached: false,
        },
        start + LINKING_IDLE + Duration::from_millis(10),
    );
    assert_eq!(watch.current(), InstallPhase::Fetch);
    // Exactly one phase runs, however far the ladder has stepped backwards.
    assert_eq!(
        watch
            .phases()
            .iter()
            .filter(|phase| phase.state == PhaseState::Running)
            .count(),
        1
    );
}

#[test]
fn linking_is_never_inferred_before_anything_has_been_fetched() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.idle(start + LINKING_IDLE * 10);
    assert_eq!(
        watch.current(),
        InstallPhase::Resolve,
        "a slow first manifest is not a finished download"
    );
}

#[test]
fn the_audit_waits_for_the_fetching_to_be_over() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Fetched {
            package: "a@1".to_owned(),
            cached: false,
        },
        start,
    );
    // npm posts its advisory request while it is still unpacking, so seeing
    // one is not evidence that the install has moved on.
    watch.observe(&ManagerEvent::Audited, start + Duration::from_millis(10));
    assert_eq!(watch.current(), InstallPhase::Fetch);
    assert_eq!(watch.phases()[3].count, 1);

    watch.idle(start + LINKING_IDLE + Duration::from_millis(10));
    watch.observe(&ManagerEvent::Audited, start + Duration::from_secs(2));
    assert_eq!(watch.current(), InstallPhase::Audit);
}

#[test]
fn time_is_charged_to_the_phase_that_was_running() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Resolved {
            package: "react".to_owned(),
        },
        start,
    );
    watch.observe(
        &ManagerEvent::Fetched {
            package: "react@18.3.1".to_owned(),
            cached: true,
        },
        start + Duration::from_secs(2),
    );
    watch.finish(start + Duration::from_secs(5));

    assert_eq!(watch.phases()[0].elapsed, Duration::from_secs(2));
    assert_eq!(watch.phases()[1].elapsed, Duration::from_secs(3));
    assert_eq!(watch.cached(), 1);
    assert_eq!(watch.fetched(), 1);
    assert!(
        watch
            .phases()
            .iter()
            .all(|phase| phase.state != PhaseState::Running),
        "nothing is still running once the manager has exited"
    );
}

#[test]
fn a_passthrough_line_changes_nothing_about_the_ladder() {
    let start = clock();
    let mut watch = InstallWatch::start(start);
    let before = watch.phases().to_vec();
    watch.observe(&ManagerEvent::Passthrough, start);
    assert_eq!(watch.phases(), before.as_slice());
}
