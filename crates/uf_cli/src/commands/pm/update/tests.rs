//! What the report says, drawn against a renderer with nothing to draw with.
//!
//! Every one of these is about the layout and the wording. Nothing here reaches
//! a registry: [`uf_pm::registry`] is tested on its own inputs, and a test that
//! asked npm what it published today would assert on the news.

use super::*;
use uf_term::{Capabilities, Renderer};

fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

fn version(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

fn row(name: &str, declared: &str, newest: &str, step: Level) -> Row {
    let range = Range::parse(declared).expect("a range");
    let newest = version(newest);
    Row {
        manifest: ".".to_owned(),
        name: name.to_compact_string(),
        declared: declared.to_compact_string(),
        rewritten: range.rewritten_to(&newest),
        newest,
        step,
    }
}

fn report(level: Option<Level>, rows: Vec<Row>) -> Report {
    Report {
        level,
        dry_run: false,
        asked_about: rows.len().max(1),
        manifests: 1,
        rows,
        undecidable: 0,
        unreachable: Vec::new(),
        deprecation: None,
    }
}

fn drawn(report: &Report) -> String {
    let mut out = String::new();
    render(&plain(), &mut out, report);
    out
}

#[test]
fn the_plain_report_names_the_newest_and_the_flag_that_takes_it() {
    let out = drawn(&report(
        None,
        vec![row("react", "^18.2.0", "19.2.0", Level::Major)],
    ));

    assert!(out.contains("outside the range"), "{out}");
    assert!(out.contains("react"), "{out}");
    assert!(out.contains("^18.2.0"), "{out}");
    assert!(out.contains("19.2.0"), "{out}");
    assert!(out.contains("major"), "{out}");
    assert!(out.contains("uf update --latest"), "{out}");
    // The plain form reports; it does not claim to have written anything.
    assert!(!out.contains("rewrite ranges"), "{out}");
}

/// Singular, because "1 dependencies are newer than their range allows" is the
/// sentence every tool gets wrong.
#[test]
fn one_dependency_reads_as_one() {
    let out = drawn(&report(
        None,
        vec![row("react", "^18.2.0", "19.2.0", Level::Major)],
    ));

    assert!(
        out.contains("1 dependency is newer than its range allows"),
        "{out}"
    );
}

#[test]
fn several_read_as_several() {
    let out = drawn(&report(
        None,
        vec![
            row("react", "^18.2.0", "19.2.0", Level::Major),
            row("vitest", "~1.0.0", "1.1.0", Level::Minor),
        ],
    ));

    assert!(
        out.contains("2 dependencies are newer than their range allows"),
        "{out}"
    );
}

/// With a level the column is what the range *becomes*, because that is the
/// thing about to be written into a file.
#[test]
fn a_level_report_shows_the_range_it_writes() {
    let out = drawn(&report(
        Some(Level::Major),
        vec![row("react", "^18.2.0", "19.2.0", Level::Major)],
    ));

    assert!(out.contains("ranges to rewrite"), "{out}");
    assert!(
        out.contains("^19.2.0"),
        "the rewritten range is missing: {out}"
    );
    assert!(out.contains("read what changed"), "no major warning: {out}");
}

#[test]
fn a_dry_run_says_it_wrote_nothing() {
    let mut report = report(
        Some(Level::Major),
        vec![row("react", "^18.2.0", "19.2.0", Level::Major)],
    );
    report.dry_run = true;

    let out = drawn(&report);

    assert!(out.contains("would be rewritten"), "{out}");
    assert!(out.contains("--dry-run"), "{out}");
}

/// A minor-only run has nothing to warn about, and should not warn.
#[test]
fn without_a_major_there_is_no_changelog_warning() {
    let out = drawn(&report(
        Some(Level::Minor),
        vec![row("vitest", "~1.0.0", "1.1.0", Level::Minor)],
    ));

    assert!(!out.contains("read what changed"), "{out}");
}

#[test]
fn nothing_to_do_says_which_nothing_it_is() {
    assert!(
        drawn(&report(None, Vec::new()))
            .contains("every dependency's newest version is inside its declared range")
    );
    assert!(
        drawn(&report(Some(Level::Patch), Vec::new()))
            .contains("no range would move at the patch level")
    );
}

/// A project of nothing but `workspace:*` should not be told that everything is
/// up to date — uf never asked.
#[test]
fn a_project_uf_could_not_ask_about_is_not_told_it_is_current() {
    let mut report = report(None, Vec::new());
    report.asked_about = 0;
    report.undecidable = 4;

    let out = drawn(&report);

    assert!(
        out.contains("no dependency declares a range uf can compare"),
        "{out}"
    );
    assert!(out.contains("4 ranges left alone"), "{out}");
    assert!(out.contains("workspace:"), "{out}");
}

/// A registry that answered for forty packages and not for one is a different
/// situation from one that answered for none, and only the name tells you.
#[test]
fn an_unreachable_package_is_named_rather_than_counted() {
    let mut report = report(None, Vec::new());
    report.unreachable =
        vec!["left-pad: could not read left-pad from the registry: 404".to_owned()];

    let out = drawn(&report);

    assert!(out.contains("1 package could not be read"), "{out}");
    assert!(out.contains("left-pad"), "{out}");
}

/// In a single-package project every row would say `.`, so the column is not
/// drawn at all.
#[test]
fn the_manifest_column_appears_only_in_a_workspace() {
    let one = drawn(&report(
        None,
        vec![row("react", "^18.2.0", "19.2.0", Level::Major)],
    ));
    assert!(!one.contains("manifest"), "{one}");

    let mut many = report(None, vec![row("react", "^18.2.0", "19.2.0", Level::Major)]);
    many.manifests = 3;
    let many = drawn(&many);
    assert!(many.contains("manifest"), "{many}");
}

/// A major has to be read about; a patch does not. Sorting by step puts the
/// rows that need a decision at the top.
#[test]
fn the_rows_that_need_a_decision_are_first() {
    let mut rows = vec![
        row("a-patch", "^1.0.0", "1.0.1", Level::Patch),
        row("z-major", "^1.0.0", "2.0.0", Level::Major),
        row("m-minor", "^1.0.0", "1.1.0", Level::Minor),
    ];
    rows.sort_by(|left, right| {
        right
            .step
            .cmp(&left.step)
            .then_with(|| left.name.cmp(&right.name))
    });

    let out = drawn(&report(None, rows));
    let major = out.find("z-major").expect("the major row");
    let minor = out.find("m-minor").expect("the minor row");
    let patch = out.find("a-patch").expect("the patch row");
    assert!(major < minor && minor < patch, "{out}");
}

#[test]
fn naming_packages_narrows_the_run() {
    let declared = vec![
        Declaration {
            manifest: Utf8PathBuf::from("/p/package.json"),
            field: "dependencies",
            name: "react".to_compact_string(),
            range: "^18.2.0".to_compact_string(),
        },
        Declaration {
            manifest: Utf8PathBuf::from("/p/package.json"),
            field: "devDependencies",
            name: "vitest".to_compact_string(),
            range: "~1.0.0".to_compact_string(),
        },
    ];

    assert_eq!(requested(&declared, &[]).len(), 2);
    assert_eq!(
        requested(&declared, &["react".to_owned()])
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<Vec<_>>(),
        vec!["react"]
    );
    // A name nothing declares contributes no rows rather than failing.
    assert!(requested(&declared, &["nope".to_owned()]).is_empty());
}

#[test]
fn the_root_manifest_is_a_dot_and_a_package_is_its_directory() {
    let root = Utf8Path::new("/p");
    assert_eq!(relative(root, Utf8Path::new("/p/package.json")), ".");
    assert_eq!(
        relative(root, Utf8Path::new("/p/packages/ui/package.json")),
        "packages/ui"
    );
}

/// ubugeeei-prod/uf#540: a project still reading through `publish.registry` is
/// told which key to move to, beside the answer rather than instead of it.
#[test]
fn a_project_on_the_old_registry_key_is_told_where_the_new_one_is() {
    let mut report = report(None, vec![row("react", "^18.2.0", "19.2.0", Level::Major)]);
    report.deprecation = Some(
        uf_config::RegistrySource::PublishFallback
            .deprecation()
            .expect("a deprecation")
            .to_owned(),
    );

    let out = drawn(&report);
    assert!(out.contains("pm.registry"), "{out}");
    assert!(out.contains("publish.registry"), "{out}");
    // The report it came for is still there.
    assert!(out.contains("19.2.0"), "{out}");
}
