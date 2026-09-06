//! What `uf dev` says about the server/client contract, and when it says it.

use camino::Utf8PathBuf;

use super::{RscReport, RscUpdate};

/// A helper a Server Component imports, with nothing client-only in it.
const CLEAN: &str = "export function greeting() {\n  return \"hello\";\n}\n";

/// The same helper, reaching for a browser global. `_uf.page.js` is a server
/// entry and imports it, so the graph says the server runs `localStorage`.
const TOUCHES_THE_BROWSER: &str =
    "export function greeting() {\n  return localStorage.getItem(\"greeting\");\n}\n";

/// A hook the name lists have never heard of — ubugeeei-prod/uf#348's case.
const CALLS_AN_UNKNOWN_HOOK: &str =
    "export function greeting() {\n  return useRoute().pathname;\n}\n";

/// A project whose only page imports one helper.
fn project(helper: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(
        root.join("app/_uf.page.js"),
        "import { greeting } from \"./greeting.js\";\nexport default function Page() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("app/greeting.js"), helper).unwrap();
    (dir, root)
}

fn rules(update: &RscUpdate) -> Vec<&'static str> {
    match update {
        RscUpdate::Reported(diagnostics) => {
            diagnostics.iter().map(|d| d.rule()).collect::<Vec<_>>()
        }
        _ => panic!("expected diagnostics, got {update:?}"),
    }
}

/// A clean project says nothing, and goes on saying nothing.
///
/// The first half is the one worth stating: `uf dev` prints a banner and then
/// belongs to the application. A report that greets every project on start-up
/// is a report people learn to scroll past.
#[test]
fn a_project_with_no_violations_is_quiet() {
    let (_dir, root) = project(CLEAN);
    let mut report = RscReport::new(&root);

    assert_eq!(report.refresh(), RscUpdate::Unchanged);
    assert_eq!(report.refresh(), RscUpdate::Unchanged);
}

/// A violation that appears while the server runs is reported, once.
///
/// This is ubugeeei-prod/uf#347: the analysis existed, `uf build` counted it,
/// and `uf dev` never called it — so a Server Component that reached for the
/// browser worked in development, because nothing is split yet, and failed in
/// review. Recomputing is what makes the answer true rather than merely early:
/// the graph is a whole-project property and goes stale on any edit.
#[test]
fn a_violation_that_appears_is_reported_and_then_not_repeated() {
    let (_dir, root) = project(CLEAN);
    let mut report = RscReport::new(&root);
    assert_eq!(report.refresh(), RscUpdate::Unchanged);

    std::fs::write(root.join("app/greeting.js"), TOUCHES_THE_BROWSER).unwrap();
    let update = report.refresh();
    assert_eq!(rules(&update), ["rsc/client-only-api-in-server"]);
    let RscUpdate::Reported(diagnostics) = &update else {
        unreachable!()
    };
    assert_eq!(diagnostics[0].module(), "app/greeting.js");
    assert!(diagnostics[0].to_string().contains("localStorage"));

    // The same answer is not the same message twice: a rescan happens on every
    // save, and most saves change nothing this cares about.
    assert_eq!(report.refresh(), RscUpdate::Unchanged);
}

/// And it goes away again.
///
/// Without this a reader cannot tell a fixed violation from a report that
/// simply stopped being printed.
#[test]
fn a_violation_that_is_fixed_is_cleared_exactly_once() {
    let (_dir, root) = project(TOUCHES_THE_BROWSER);
    let mut report = RscReport::new(&root);
    assert_eq!(rules(&report.refresh()), ["rsc/client-only-api-in-server"]);

    std::fs::write(root.join("app/greeting.js"), CLEAN).unwrap();
    assert_eq!(report.refresh(), RscUpdate::Cleared);
    assert_eq!(report.refresh(), RscUpdate::Unchanged);
}

/// A hook the name lists cannot classify is reported as a question rather than
/// passed over in silence — ubugeeei-prod/uf#348, in the place a person reads.
#[test]
fn a_hook_the_check_cannot_classify_is_reported_as_a_warning() {
    let (_dir, root) = project(CALLS_AN_UNKNOWN_HOOK);
    let mut report = RscReport::new(&root);

    let update = report.refresh();
    assert_eq!(rules(&update), ["rsc/unclassified-hook-in-server"]);
    let RscUpdate::Reported(diagnostics) = &update else {
        unreachable!()
    };
    assert_eq!(diagnostics[0].severity(), uf_rsc::RscSeverity::Warn);
    assert!(diagnostics[0].to_string().contains("useRoute"));
}

/// A scan failure sits in the middle of the ordinary recovery sequence, and it
/// must not swallow the line that says the violation is gone.
///
/// A violation is on screen; the next save leaves a file the scan cannot read,
/// which is what a half-written file looks like from here; the save after that
/// fixes both. What the reader sees at the end is the old violation, and the
/// only thing that replaces it is `Cleared`. Remembering what was *rendered*
/// separately from the last successful scan is what makes that possible: the
/// failure is entitled to forget the scan, so that the next success is drawn in
/// full, and not entitled to forget the screen.
#[test]
fn a_violation_fixed_across_a_failed_scan_is_still_cleared() {
    let (_dir, root) = project(TOUCHES_THE_BROWSER);
    let mut report = RscReport::new(&root);
    assert_eq!(rules(&report.refresh()), ["rsc/client-only-api-in-server"]);

    std::fs::write(root.join("app/half-written.js"), [0xff, 0xfe, 0x00]).unwrap();
    let update = report.refresh();
    assert!(matches!(update, RscUpdate::Failed(_)), "{update:?}");

    std::fs::remove_file(root.join("app/half-written.js")).unwrap();
    std::fs::write(root.join("app/greeting.js"), CLEAN).unwrap();
    assert_eq!(report.refresh(), RscUpdate::Cleared);
    assert_eq!(report.refresh(), RscUpdate::Unchanged);
}

/// And a project that was clean before the failure still says nothing after
/// it: `Cleared` replaces a violation on screen, and there was none.
#[test]
fn a_clean_project_across_a_failed_scan_stays_quiet() {
    let (_dir, root) = project(CLEAN);
    let mut report = RscReport::new(&root);
    assert_eq!(report.refresh(), RscUpdate::Unchanged);

    std::fs::write(root.join("app/half-written.js"), [0xff, 0xfe, 0x00]).unwrap();
    let update = report.refresh();
    assert!(matches!(update, RscUpdate::Failed(_)), "{update:?}");

    std::fs::remove_file(root.join("app/half-written.js")).unwrap();
    assert_eq!(report.refresh(), RscUpdate::Unchanged);
}
