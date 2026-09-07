//! What the listing says, including the sentence npm gets.

use super::*;
use uf_pm::builds::Buildable;
use uf_term::{Capabilities, Renderer};

fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

fn buildable(name: &str, version: &str, scripts: &[&'static str], approved: bool) -> Buildable {
    Buildable {
        name: name.into(),
        version: version.into(),
        scripts: scripts.to_vec(),
        approved,
    }
}

fn drawn(manager: &str, approvals: Approvals, configured: bool, waiting: &[Buildable]) -> String {
    let mut out = String::new();
    render(&plain(), &mut out, manager, approvals, configured, waiting);
    out
}

#[test]
fn a_waiting_package_is_shown_with_what_it_would_run() {
    let out = drawn(
        "pnpm",
        Approvals::OnlyBuilt,
        false,
        &[buildable("sharp", "0.33.5", &["postinstall"], false)],
    );

    assert!(out.contains("sharp"), "{out}");
    assert!(out.contains("0.33.5"), "{out}");
    assert!(out.contains("postinstall"), "{out}");
    assert!(out.contains("pnpm.onlyBuiltDependencies"), "{out}");
    assert!(
        out.contains("1 package would run code at install time and is not approved"),
        "{out}"
    );
    assert!(out.contains("uf pm approve-builds <name>"), "{out}");
}

#[test]
fn an_approved_package_is_marked_and_not_counted_as_waiting() {
    let out = drawn(
        "pnpm",
        Approvals::OnlyBuilt,
        false,
        &[buildable("sharp", "0.33.5", &["postinstall"], true)],
    );

    assert!(out.contains("every one of them is approved"), "{out}");
    assert!(!out.contains("not approved"), "{out}");
}

/// The sentence #495 asked for: npm's "runs everything" stated rather than
/// papered over.
#[test]
fn npm_is_told_it_cannot_approve_one_and_not_another() {
    let out = drawn(
        "npm",
        Approvals::AllOrNothing,
        false,
        &[buildable("sharp", "0.33.5", &["postinstall"], false)],
    );

    assert!(out.contains("cannot approve one and not another"), "{out}");
    assert!(out.contains("all of them or none"), "{out}");
    assert!(out.contains("pm.allowLifecycleScripts"), "{out}");
    // And it must not offer a command that would do nothing for this manager.
    assert!(!out.contains("uf pm approve-builds <name>"), "{out}");
    assert!(out.contains("nothing: it is all or none"), "{out}");
}

/// With the flag on, the table is beside the point and saying anything else
/// would be a report that flatters the project.
#[test]
fn the_flag_being_on_is_said_before_the_table_rather_than_after() {
    let out = drawn(
        "pnpm",
        Approvals::OnlyBuilt,
        true,
        &[buildable("sharp", "0.33.5", &["postinstall"], false)],
    );

    let warning = out
        .find("pm.allowLifecycleScripts is on")
        .expect("the warning");
    let table = out.find("sharp").expect("the table");
    assert!(warning < table, "{out}");
    assert!(
        out.contains("approved or not"),
        "the warning does not say the approval is moot: {out}"
    );
    // No "so uf does not let them run" underneath, because it does.
    assert!(!out.contains("uf does not let"), "{out}");
}

#[test]
fn a_tree_that_runs_nothing_says_so() {
    let out = drawn("pnpm", Approvals::OnlyBuilt, false, &[]);

    assert!(
        out.contains("nothing in this project's tree runs code at install time"),
        "{out}"
    );
}

/// Plural, because the sentence is read by somebody deciding whether to worry.
#[test]
fn several_waiting_packages_read_as_several() {
    let out = drawn(
        "bun",
        Approvals::Trusted,
        false,
        &[
            buildable("esbuild", "0.24.0", &["postinstall"], false),
            buildable("sharp", "0.33.5", &["install", "postinstall"], false),
        ],
    );

    assert!(
        out.contains("2 packages would run code at install time and are not approved"),
        "{out}"
    );
    assert!(out.contains("trustedDependencies"), "{out}");
    assert!(out.contains("install, postinstall"), "{out}");
}
