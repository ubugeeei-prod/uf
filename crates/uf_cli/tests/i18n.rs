//! `uf i18n extract` and `uf i18n merge`, over a project on disk.
//!
//! The unit tests in `uf_i18n` read one module's text and compare two values in
//! memory; nothing there runs discovery, chooses a path, or decides an exit
//! code. This does: it is the round trip a translation actually makes — extract
//! the project, edit the file the way a vendor does, merge it back — plus the
//! two refusals that are the whole reason the commands are allowed to fail.
//!
//! ubugeeei-prod/uf#569.

mod support;

use std::fs;

use serde_json::Value;
use support::{Project, uf};

/// A project whose messages are the three shapes the walk has to read: a
/// message with a parameter, one written from a module-level constant, and one
/// that takes nothing.
fn project() -> Project {
    Project::new(&[(
        "src/messages.js",
        r#"// @flow
import { defineCatalogue, message, number, string } from "@uniflowed/i18n";

const UNREAD = `.input {$count :number}
.match $count
one {{You have {$count} unread message.}}
*   {{You have {$count} unread messages.}}`;

const messages = {
  greeting: message("Hello, {$name}!", { name: string }),
  unread: message(UNREAD, { count: number }),
  cartEmpty: message("Your cart is empty.", {}),
};

export const en = defineCatalogue("en-US", messages);
"#,
    )])
}

fn run(project: &Project, args: &[&str]) -> (i32, String, String) {
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["--color", "never"])
        .args(args)
        .output()
        .expect("uf started");
    (
        output.status.code().expect("uf exited"),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn extract_writes_the_catalogue_the_locale_names() {
    let project = project();
    let (code, stdout, stderr) = run(&project, &["i18n", "extract"]);
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");

    // The locale came from `defineCatalogue("en-US", …)`, so the path did too:
    // nothing on the command line said `en-US`.
    let written = project.path().join("i18n/en-US.json");
    let text = fs::read_to_string(&written).expect("the catalogue was written");
    assert!(stdout.contains("i18n/en-US.json"), "{stdout}");

    let catalogue: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(catalogue["format"], "uf-i18n-catalogue/1");
    assert_eq!(catalogue["sourceLocale"], "en-US");
    assert_eq!(catalogue["locale"], "en-US");

    let greeting = &catalogue["messages"]["greeting"];
    assert_eq!(greeting["source"], "Hello, {$name}!");
    // A fresh extraction gives a translator the source to edit rather than an
    // empty box, and that is how `merge` tells a translated entry from one
    // that came back untouched.
    assert_eq!(greeting["translation"], "Hello, {$name}!");
    assert_eq!(greeting["parameters"]["name"], "string");
    // The `message(…)` call's own line, which is the line a translator asking
    // "where does this appear" wants.
    assert_eq!(greeting["declaredAt"], "src/messages.js:10");

    // The constant was resolved rather than reported.
    let unread = catalogue["messages"]["unread"]["source"]
        .as_str()
        .expect("a source");
    assert!(unread.starts_with(".input {$count :number}"), "{unread}");
    assert_eq!(
        catalogue["messages"]["unread"]["parameters"]["count"],
        "number"
    );

    // And the file is the reviewable kind: sorted, indented, newline-ended.
    assert!(text.ends_with("}\n"), "{text}");
    let keys: Vec<&str> = catalogue["messages"]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["cartEmpty", "greeting", "unread"]);
}

#[test]
fn extract_json_reports_and_writes_nothing() {
    let project = project();
    let (code, stdout, stderr) = run(&project, &["i18n", "extract", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let report: Value = serde_json::from_str(&stdout).expect("pure JSON on stdout");
    assert_eq!(report["command"], "uf i18n extract");
    assert_eq!(report["out"], "i18n/en-US.json");
    assert_eq!(report["report"]["modules"], 1);
    assert_eq!(
        report["report"]["catalogue"]["messages"]["cartEmpty"]["source"],
        "Your cart is empty."
    );
    assert!(
        !project.path().join("i18n/en-US.json").exists(),
        "`--json` is the dry run: it must not write the file"
    );
}

#[test]
fn a_message_uf_cannot_read_fails_the_command_and_writes_nothing() {
    // The one failure that costs a release: a message missing from the file a
    // vendor is sent is invisible in the extraction and invisible in review.
    let project = project();
    project.write(
        "src/dynamic.js",
        r#"// @flow
import { message, string } from "@uniflowed/i18n";

export const messages = {
  chosen: message(pick(), { who: string }),
};
"#,
    );

    let (code, stdout, stderr) = run(&project, &["i18n", "extract"]);
    assert_eq!(code, 1, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("src/dynamic.js:5"), "{stdout}");
    assert!(stdout.contains("source is not a literal"), "{stdout}");
    assert!(
        !project.path().join("i18n/en-US.json").exists(),
        "a catalogue missing a message is worse than no catalogue"
    );
}

#[test]
fn merge_writes_the_locale_module_and_reports_the_rest() {
    let project = project();
    assert_eq!(run(&project, &["i18n", "extract"]).0, 0);

    // What a vendor sends back: the target locale named, two messages
    // translated, one left in English.
    let path = project.path().join("i18n/en-US.json");
    let mut catalogue: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("JSON");
    catalogue["locale"] = Value::from("ja-JP");
    catalogue["messages"]["greeting"]["translation"] = Value::from("こんにちは、{$name}!");
    catalogue["messages"]["cartEmpty"]["translation"] = Value::from("カートは空です。");
    let returned = project.path().join("i18n/ja-JP.json");
    fs::write(
        &returned,
        format!("{}\n", serde_json::to_string_pretty(&catalogue).unwrap()),
    )
    .expect("write");

    let (code, stdout, stderr) = run(&project, &["i18n", "merge", "i18n/ja-JP.json"]);
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");

    // Beside the file it read, named for the locale, which is where
    // `defineLocales`' `() => import("./ja-JP.js")` points.
    let module = fs::read_to_string(project.path().join("i18n/ja-JP.js")).expect("the module");
    assert!(module.starts_with("// @flow\n"), "{module}");
    assert!(
        module.contains("greeting: \"こんにちは、{$name}!\","),
        "{module}"
    );
    assert!(
        module.contains("cartEmpty: \"カートは空です。\","),
        "{module}"
    );
    // The message that came back in English is not claimed as Japanese: it is
    // left out, so the catalogue reports it as `untranslated` at run time.
    assert!(!module.contains("unread:"), "{module}");
    assert!(stdout.contains("still in the source locale"), "{stdout}");
}

#[test]
fn a_message_that_changed_while_the_file_was_out_is_refused() {
    let project = project();
    assert_eq!(run(&project, &["i18n", "extract"]).0, 0);

    let path = project.path().join("i18n/en-US.json");
    let mut catalogue: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("JSON");
    catalogue["locale"] = Value::from("ja-JP");
    catalogue["messages"]["greeting"]["translation"] = Value::from("こんにちは、{$name}!");
    let returned = project.path().join("i18n/ja-JP.json");
    fs::write(&returned, serde_json::to_string_pretty(&catalogue).unwrap()).expect("write");

    // And meanwhile the English moved on, which is what a translation round
    // taking weeks means.
    project.write(
        "src/messages.js",
        &fs::read_to_string(project.path().join("src/messages.js"))
            .expect("read")
            .replace("Hello, {$name}!", "Welcome back, {$name}!"),
    );

    let (code, stdout, stderr) = run(&project, &["i18n", "merge", "i18n/ja-JP.json"]);
    assert_eq!(code, 1, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("greeting"), "{stdout}");
    assert!(stdout.contains("its text changed"), "{stdout}");
    assert!(
        !project.path().join("i18n/ja-JP.js").exists(),
        "a translation of a sentence that no longer exists is not a translation \
         of the one that replaced it"
    );
}

#[test]
fn the_source_catalogue_is_not_a_translation_of_itself() {
    // The mistake anybody makes once: merging the file that was sent out
    // instead of the one that came back. Merging it would write the English
    // into `en-US.js` and call it a locale.
    let project = project();
    assert_eq!(run(&project, &["i18n", "extract"]).0, 0);

    let (code, stdout, stderr) = run(&project, &["i18n", "merge", "i18n/en-US.json"]);
    assert_eq!(code, 1, "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stderr.contains("is the source catalogue, not a translation"),
        "{stderr}"
    );
}

#[test]
fn a_project_that_names_no_locale_asks_rather_than_inventing_one() {
    let project = Project::new(&[(
        "src/messages.js",
        r#"// @flow
import { message, string } from "@uniflowed/i18n";

export const messages = {
  greeting: message("Hello, {$name}!", { name: string }),
};
"#,
    )]);

    let (code, _, stderr) = run(&project, &["i18n", "extract"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("--locale"), "{stderr}");

    // And with the flag it is an ordinary run.
    let (code, stdout, stderr) = run(&project, &["i18n", "extract", "--locale", "en-GB"]);
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");
    assert!(project.path().join("i18n/en-GB.json").exists(), "{stdout}");
}
