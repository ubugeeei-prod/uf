//! What `uf dev` says about the server/client contract, and when it says it.

use camino::Utf8PathBuf;

use super::{BundleMove, RscReport, RscUpdate};

/// A helper a Server Component imports, with nothing client-only in it.
const CLEAN: &str = "export function greeting() {\n  return \"hello\";\n}\n";

/// The same helper, reaching for a browser global. `$page.js` is a server
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
        root.join("app/$page.js"),
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

/// The counter the page renders, before anybody makes it the browser's.
const A_SERVER_COMPONENT: &str = "export function Counter() {\n  return null;\n}\n";

/// And after: one directive, and the module is a client bundle root.
const A_CLIENT_COMPONENT: &str =
    "\"use client\";\nexport function Counter() {\n  return null;\n}\n";

/// A project whose page imports a component through one module in between.
///
/// Three modules deep on purpose: the chain a reader is shown is the whole
/// point of the report, and a chain of two hops proves nothing about the walk
/// that builds it.
fn project_with_a_component(counter: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(
        root.join("app/$page.js"),
        "import { Section } from \"./section.js\";\nexport default function Page() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("app/section.js"),
        "import { Counter } from \"./Counter.js\";\nexport function Section() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("app/Counter.js"), counter).unwrap();
    (dir, root)
}

/// The lines the report would print, in the order it prints them.
fn moved(report: &RscReport) -> Vec<String> {
    report.moved.moves.iter().map(BundleMove::line).collect()
}

/// Nothing moved on the first scan, because there was nothing to move from.
///
/// The alternative is a list of every client module in the project every time
/// `uf dev` starts, which is the banner `a_project_with_no_violations_is_quiet`
/// exists to keep off the screen — the same argument, about the other half of
/// what this module reports.
#[test]
fn the_first_scan_reports_no_movement() {
    let (_dir, root) = project_with_a_component(A_CLIENT_COMPONENT);
    let mut report = RscReport::new(&root);

    report.refresh();
    assert_eq!(moved(&report), Vec::<String>::new());
    assert_eq!(report.moved.total, 0);
}

/// One directive moves three modules, and each is told why it moved.
///
/// This is ubugeeei-prod/uf#520's question — *why is this in the client
/// bundle* — asked at the moment the answer changes. The page and the section
/// are in the bundle because uf's client re-renders the matched tree from the
/// same modules the server did, so a module above a boundary is one React
/// needs in order to reach the boundary at all; the chain is what says so
/// without the reader having to know that.
#[test]
fn adding_use_client_reports_the_module_and_everything_above_it() {
    let (_dir, root) = project_with_a_component(A_SERVER_COMPONENT);
    let mut report = RscReport::new(&root);
    report.refresh();
    assert_eq!(report.moved.total, 0);

    std::fs::write(root.join("app/Counter.js"), A_CLIENT_COMPONENT).unwrap();
    report.refresh();

    assert_eq!(
        moved(&report),
        [
            "app/$page.js is now in the client bundle — app/$page.js imports \
             app/section.js imports app/Counter.js, which declares `\"use client\"`",
            "app/Counter.js is now in the client bundle — it declares `\"use client\"`",
            "app/section.js is now in the client bundle — app/section.js imports \
             app/Counter.js, which declares `\"use client\"`",
        ]
    );
    assert_eq!(report.moved.total, 3);
}

/// And taking it away again moves them back out, once.
///
/// The second `refresh` is the half that makes this a report rather than a
/// log: a save that changes nothing about the bundle says nothing about it.
#[test]
fn removing_use_client_reports_the_modules_leaving_and_then_stays_quiet() {
    let (_dir, root) = project_with_a_component(A_CLIENT_COMPONENT);
    let mut report = RscReport::new(&root);
    report.refresh();

    std::fs::write(root.join("app/Counter.js"), A_SERVER_COMPONENT).unwrap();
    report.refresh();
    assert_eq!(
        moved(&report),
        [
            "app/$page.js is out of the client bundle",
            "app/Counter.js is out of the client bundle",
            "app/section.js is out of the client bundle",
        ]
    );

    std::fs::write(
        root.join("app/section.js"),
        "export function Section() {}\n",
    )
    .unwrap();
    report.refresh();
    assert_eq!(moved(&report), Vec::<String>::new());
}

/// A failed scan does not make the next one report the whole bundle as new.
///
/// A failure recomputes nothing, so it knows nothing about what the browser
/// gets — which is why the remembered bundle survives one, where the
/// remembered *diagnostics* deliberately do not. Forgetting it here would
/// answer every half-written file with the entire client bundle arriving.
#[test]
fn a_failed_scan_does_not_reintroduce_the_whole_bundle() {
    let (_dir, root) = project_with_a_component(A_CLIENT_COMPONENT);
    let mut report = RscReport::new(&root);
    report.refresh();

    std::fs::write(root.join("app/half-written.js"), [0xff, 0xfe, 0x00]).unwrap();
    let update = report.refresh();
    assert!(matches!(update, RscUpdate::Failed(_)), "{update:?}");
    assert_eq!(report.moved.total, 0);

    std::fs::remove_file(root.join("app/half-written.js")).unwrap();
    report.refresh();
    assert_eq!(moved(&report), Vec::<String>::new());
}

/// The number is exact and the list is not, which is the right way round.
///
/// A directive added deep in a tree can move a great many modules at once. The
/// count is what tells a reader how big the change was; the chains are what
/// they read, and a screen of them is a screen nobody reads. `MAX_MOVES` is
/// the ceiling `docs/security.md`'s "no unbounded anything" asks of a report
/// as much as of a parser.
#[test]
fn a_large_move_is_counted_in_full_and_listed_in_part() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("app")).unwrap();
    // A chain of pages, each importing the next, ending at the component.
    let depth = super::MAX_MOVES + 5;
    std::fs::write(
        root.join("app/$page.js"),
        "import { Step } from \"./step-0.js\";\nexport default function Page() {}\n",
    )
    .unwrap();
    for step in 0..depth {
        let next = if step + 1 == depth {
            "./Counter.js".to_owned()
        } else {
            format!("./step-{}.js", step + 1)
        };
        std::fs::write(
            root.join(format!("app/step-{step}.js")),
            format!("import {{ Step }} from \"{next}\";\nexport function Step() {{}}\n"),
        )
        .unwrap();
    }
    std::fs::write(root.join("app/Counter.js"), A_SERVER_COMPONENT).unwrap();

    let mut report = RscReport::new(&root);
    report.refresh();
    std::fs::write(root.join("app/Counter.js"), A_CLIENT_COMPONENT).unwrap();
    report.refresh();

    // The page, the component and every step between them.
    assert_eq!(report.moved.total, depth + 2);
    assert_eq!(report.moved.moves.len(), super::MAX_MOVES);
}

/// A manifest that could not be written keeps the move for the next write.
///
/// `persist` used to swallow the failure and `refresh` reported the move
/// anyway, which advanced the bundle it compares against. Vite went on reading
/// the old manifest, and the *next* successful write compared against a
/// baseline that had already moved and said nothing had changed — so the edit
/// landed silently and the one report that exists to catch it never came.
#[test]
fn a_manifest_that_cannot_be_written_keeps_the_move_for_the_next_write() {
    use uf_rsc::{RSC_MANIFEST_BUILD_DIR, RSC_MANIFEST_FILE_NAME};

    let (_dir, root) = project_with_a_component(A_SERVER_COMPONENT);
    let mut report = RscReport::new(&root);
    report.refresh();
    assert_eq!(report.moved.total, 0);

    // A directory where the manifest file goes: `create_dir_all` still
    // succeeds and `fs::write` does not, which is the failure being modelled
    // — a full disk, a read-only checkout, a `.uf` somebody chowned.
    let manifest = root
        .join(RSC_MANIFEST_BUILD_DIR)
        .join(RSC_MANIFEST_FILE_NAME);
    std::fs::remove_file(&manifest).unwrap();
    std::fs::create_dir(&manifest).unwrap();

    std::fs::write(root.join("app/Counter.js"), A_CLIENT_COMPONENT).unwrap();
    report.refresh();
    assert_eq!(
        moved(&report),
        Vec::<String>::new(),
        "nothing to report while the browser is still being served the old manifest"
    );

    // And once it can be written, the move is still there to report — three
    // modules, not none.
    std::fs::remove_dir(&manifest).unwrap();
    report.refresh();
    assert_eq!(report.moved.total, 3, "{:?}", moved(&report));
    assert!(manifest.exists(), "and the manifest landed this time");
}
