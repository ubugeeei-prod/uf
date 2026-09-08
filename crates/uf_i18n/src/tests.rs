//! Unit tests for the walk, the digest and the two files.

use std::collections::BTreeMap;

use super::extract::{ProblemKind, read_module};
use super::merge::{compare, render_locale_module};
use super::{
    CATALOGUE_FORMAT, ExtractReport, MessageCatalogue, MessageEntry, ParamKind, digest_of,
};

/// A module that imports the package the ordinary way.
fn module(body: &str) -> String {
    format!(
        "// @flow\nimport {{ boolean, date, defineCatalogue, message, number, string }} from \"@uniflowed/i18n\";\n\n{body}\n"
    )
}

fn read(body: &str) -> super::extract::FileMessages {
    read_module(&module(body), "src/messages.js").expect("the parser accepted the source")
}

fn keys(found: &super::extract::FileMessages) -> Vec<&str> {
    found.entries.iter().map(|(key, _)| key.as_str()).collect()
}

#[test]
fn reads_a_message_declared_in_an_object() {
    let found = read(
        "const messages = {\n  greeting: message(\"Hello, {$name}!\", { name: string }),\n};\n",
    );
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(keys(&found), ["greeting"]);
    let (_, entry) = &found.entries[0];
    assert_eq!(entry.source, "Hello, {$name}!");
    assert_eq!(entry.translation, entry.source);
    assert_eq!(
        entry.parameters,
        BTreeMap::from([("name".to_owned(), ParamKind::String)])
    );
    assert_eq!(entry.declared_at, "src/messages.js:5");
}

#[test]
fn reads_a_message_declared_as_a_constant() {
    let found = read("const farewell = message(\"Bye.\", {});\n");
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(keys(&found), ["farewell"]);
    assert!(found.entries[0].1.parameters.is_empty());
}

#[test]
fn resolves_a_message_written_from_a_module_level_constant() {
    // The shape the package's own documentation writes: a `.match` message is
    // several lines and does not belong inline.
    let found = read(
        "const UNREAD = `.input {$count :number}\n.match $count\n*   {{{$count} unread}}`;\n\
         const messages = { unread: message(UNREAD, { count: number }) };\n",
    );
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert!(found.entries[0].1.source.starts_with(".input {$count"));
    assert_eq!(
        found.entries[0].1.parameters,
        BTreeMap::from([("count".to_owned(), ParamKind::Number)])
    );
}

#[test]
fn resolves_a_constant_declared_below_the_message() {
    // Two passes over the file, so source order is not a semantic.
    let found = read(
        "const messages = { unread: message(UNREAD, { count: number }) };\nconst UNREAD = \"{$count}\";\n",
    );
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(found.entries[0].1.source, "{$count}");
}

#[test]
fn follows_an_aliased_import() {
    let source = "// @flow\nimport { message as msg, string as text } from \"@uniflowed/i18n\";\n\
                  const messages = { hi: msg(\"Hi, {$who}\", { who: text }) };\n";
    let found = read_module(source, "src/a.js").expect("parsed");
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(keys(&found), ["hi"]);
    assert_eq!(
        found.entries[0].1.parameters,
        BTreeMap::from([("who".to_owned(), ParamKind::String)])
    );
}

#[test]
fn follows_a_namespace_import() {
    let source = "// @flow\nimport * as i18n from \"@uniflowed/i18n\";\n\
                  const messages = { hi: i18n.message(\"Hi, {$who}\", { who: i18n.string }) };\n\
                  const en = i18n.defineCatalogue(\"en-US\", messages);\n";
    let found = read_module(source, "src/a.js").expect("parsed");
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(keys(&found), ["hi"]);
    assert_eq!(
        found.locales.iter().next().map(String::as_str),
        Some("en-US")
    );
}

#[test]
fn reads_the_subpath_import_too() {
    let source = "// @flow\nimport { message, string } from \"@uniflowed/i18n/catalogue\";\n\
                  const messages = { hi: message(\"Hi, {$who}\", { who: string }) };\n";
    let found = read_module(source, "src/a.js").expect("parsed");
    assert_eq!(keys(&found), ["hi"]);
}

#[test]
fn ignores_a_message_function_that_is_not_the_packages() {
    // The reason the import is checked rather than the name: `message` is an
    // ordinary word, and a walk that matched on it would put a WebSocket frame
    // in a translation file.
    let source = "// @flow\n// @uniflowed/i18n is mentioned here and imported nowhere.\n\
                  import { message } from \"./socket.js\";\n\
                  const frames = { ping: message(\"ping\", {}) };\n";
    let found = read_module(source, "src/socket-user.js").expect("parsed");
    assert!(found.entries.is_empty());
    assert!(found.problems.is_empty(), "{:?}", found.problems);
}

#[test]
fn ignores_a_type_only_import() {
    let source = "// @flow\nimport type { Message } from \"@uniflowed/i18n\";\n\
                  import { message } from \"./elsewhere.js\";\n\
                  const messages = { hi: message(\"Hi\", {}) };\n\
                  export type Held = Message<{}>;\n";
    let found = read_module(source, "src/a.js").expect("parsed");
    assert!(found.entries.is_empty(), "{:?}", found.entries);
}

#[test]
fn reports_a_message_under_no_key() {
    let found = read("register(message(\"Hi\", {}));\n");
    assert!(found.entries.is_empty());
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert_eq!(found.problems[0].kind, ProblemKind::NoKey);
    // The header `module` writes is three lines, so the body starts on line 4.
    assert_eq!(found.problems[0].at, "src/messages.js:4");
}

#[test]
fn reports_a_source_that_is_not_a_literal() {
    let found = read("const messages = { hi: message(pick(), { name: string }) };\n");
    assert!(found.entries.is_empty());
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert_eq!(found.problems[0].kind, ProblemKind::SourceNotALiteral);
}

#[test]
fn reports_a_template_literal_with_a_substitution() {
    let found = read("const messages = { hi: message(`Hi ${who}`, { who: string }) };\n");
    assert!(found.entries.is_empty());
    assert_eq!(found.problems[0].kind, ProblemKind::SourceNotALiteral);
}

#[test]
fn reports_a_named_source_that_is_not_a_module_constant() {
    let found = read(
        "function build() {\n  const INNER = \"Hi\";\n  return message(INNER, {});\n}\n\
         const messages = { hi: message(INNER, {}) };\n",
    );
    assert!(found.entries.is_empty());
    assert!(
        found
            .problems
            .iter()
            .any(|problem| problem.kind == ProblemKind::SourceNotALiteral),
        "{:?}",
        found.problems
    );
}

#[test]
fn reports_a_parameter_that_is_not_one_of_the_kinds() {
    let found = read("const messages = { hi: message(\"Hi, {$who}\", { who: anything }) };\n");
    assert!(found.entries.is_empty());
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert_eq!(found.problems[0].kind, ProblemKind::ParametersNotReadable);
    assert!(found.problems[0].detail.contains("`who`"));
}

#[test]
fn reports_spread_parameters() {
    let found = read("const messages = { hi: message(\"Hi, {$who}\", { ...shared }) };\n");
    assert!(found.entries.is_empty());
    assert_eq!(found.problems[0].kind, ProblemKind::ParametersNotReadable);
}

#[test]
fn reads_every_kind_the_package_exports() {
    let found = read(
        "const messages = {\n  all: message(\"{$a} {$b} {$c} {$d}\", { a: string, b: number, c: boolean, d: date }),\n};\n",
    );
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(
        found.entries[0].1.parameters,
        BTreeMap::from([
            ("a".to_owned(), ParamKind::String),
            ("b".to_owned(), ParamKind::Number),
            ("c".to_owned(), ParamKind::Boolean),
            ("d".to_owned(), ParamKind::Date),
        ])
    );
}

#[test]
fn a_file_that_does_not_parse_is_a_problem_and_not_a_failure() {
    let found = read_module(
        "// @flow\nimport { message } from \"@uniflowed/i18n\";\nconst = ;\n",
        "src/broken.js",
    )
    .expect("a syntax error is not a parser refusal");
    assert_eq!(found.problems.len(), 1);
    assert_eq!(found.problems[0].kind, ProblemKind::Unparsable);
    assert_eq!(found.problems[0].at, "src/broken.js");
}

// --- the digest -------------------------------------------------------------

#[test]
fn the_digest_answers_only_the_question_it_is_for() {
    let parameters = BTreeMap::from([("count".to_owned(), ParamKind::Number)]);
    let base = digest_of("{$count} unread", &parameters);

    // The text moved.
    assert_ne!(base, digest_of("{$count} new", &parameters));
    // The kind moved.
    assert_ne!(
        base,
        digest_of(
            "{$count} unread",
            &BTreeMap::from([("count".to_owned(), ParamKind::String)])
        )
    );
    // A parameter was added.
    assert_ne!(
        base,
        digest_of(
            "{$count} unread",
            &BTreeMap::from([
                ("count".to_owned(), ParamKind::Number),
                ("who".to_owned(), ParamKind::String),
            ])
        )
    );
    // And the same message twice is the same digest, so a message that only
    // moved to another file does not invalidate its translations.
    assert_eq!(base, digest_of("{$count} unread", &parameters));
    assert_eq!(base.len(), 16);
}

#[test]
fn the_separator_keeps_two_readings_of_one_string_apart() {
    // `{ab: string }` and `{ a: string, b: … }` would hash alike if the names
    // and kinds were concatenated without one.
    assert_ne!(
        digest_of("x", &BTreeMap::from([("ab".to_owned(), ParamKind::String)])),
        digest_of(
            "x",
            &BTreeMap::from([
                ("a".to_owned(), ParamKind::String),
                ("b".to_owned(), ParamKind::String),
            ])
        )
    );
}

// --- the file ---------------------------------------------------------------

fn entry(source: &str, translation: &str, parameters: &[(&str, ParamKind)]) -> MessageEntry {
    let parameters: BTreeMap<String, ParamKind> = parameters
        .iter()
        .map(|(name, kind)| ((*name).to_owned(), *kind))
        .collect();
    MessageEntry {
        digest: digest_of(source, &parameters),
        source: source.to_owned(),
        translation: translation.to_owned(),
        parameters,
        declared_at: "src/messages.js:3".to_owned(),
    }
}

fn catalogue(locale: &str, messages: &[(&str, MessageEntry)]) -> MessageCatalogue {
    MessageCatalogue {
        format: CATALOGUE_FORMAT.to_owned(),
        source_locale: "en-US".to_owned(),
        locale: locale.to_owned(),
        messages: messages
            .iter()
            .map(|(key, entry)| ((*key).to_owned(), entry.clone()))
            .collect(),
    }
}

fn extraction(messages: &[(&str, MessageEntry)]) -> ExtractReport {
    ExtractReport {
        files_seen: 1,
        files_parsed: 1,
        modules: 1,
        catalogue: catalogue("en-US", messages),
        problems: Vec::new(),
        unreadable: Vec::new(),
    }
}

#[test]
fn a_catalogue_round_trips_through_json() {
    let written = catalogue(
        "en-US",
        &[(
            "greeting",
            entry("Hi, {$who}", "Hi, {$who}", &[("who", ParamKind::String)]),
        )],
    );
    let text = written.to_json().expect("serialised");
    assert!(text.ends_with('\n'), "the file is newline-terminated");
    assert_eq!(
        MessageCatalogue::from_json(&text).expect("read back"),
        written
    );
}

#[test]
fn a_file_from_another_uf_is_named_rather_than_misread() {
    let text = "{\"format\":\"uf-i18n-catalogue/2\",\"sourceLocale\":\"en\",\"locale\":\"ja\",\"messages\":{}}";
    let error = MessageCatalogue::from_json(text).expect_err("refused");
    assert!(error.to_string().contains("uf-i18n-catalogue/2"), "{error}");
}

// --- the merge --------------------------------------------------------------

#[test]
fn merges_the_translations_and_reports_the_rest() {
    let unchanged = entry(
        "Hi, {$who}",
        "こんにちは、{$who}",
        &[("who", ParamKind::String)],
    );
    let untouched = entry("Bye.", "Bye.", &[]);
    let mut stale = entry(
        "{$count} unread",
        "{$count} 件",
        &[("count", ParamKind::Number)],
    );
    stale.digest = digest_of("{$count} new", &stale.parameters);
    let gone = entry("Removed.", "消えた", &[]);

    let returned = catalogue(
        "ja-JP",
        &[
            ("greeting", unchanged.clone()),
            ("cartEmpty", untouched),
            ("unread", stale),
            ("gone", gone),
        ],
    );
    let project = extraction(&[
        ("greeting", unchanged),
        ("cartEmpty", entry("Bye.", "Bye.", &[])),
        (
            "unread",
            entry(
                "{$count} unread",
                "{$count} unread",
                &[("count", ParamKind::Number)],
            ),
        ),
        ("nobodyTranslated", entry("New.", "New.", &[])),
    ]);

    let report = compare(&returned, project);
    assert_eq!(report.translated, 1);
    assert_eq!(report.untranslated, ["cartEmpty"]);
    assert_eq!(report.unknown, ["gone"]);
    assert_eq!(report.missing, ["nobodyTranslated"]);
    assert_eq!(report.stale.len(), 1);
    assert_eq!(report.stale[0].key, "unread");
    assert!(report.has_problems(), "a stale message fails the command");

    // The stale message is left out rather than merged against the old source.
    assert!(report.module.contains("greeting"));
    assert!(!report.module.contains("unread"));
    assert!(!report.module.contains("cartEmpty"));
}

#[test]
fn says_which_half_of_a_message_moved() {
    // The file went out when the parameter was called `name`; the message now
    // calls it `who`. Both halves of the sentence are the report's job: a
    // translator reading "its parameters changed" and nothing else has to go
    // and diff two files to learn what to do about it.
    let renamed = entry("Hi, {$who}", "やあ", &[("name", ParamKind::String)]);
    let returned = catalogue("ja-JP", &[("greeting", renamed)]);
    let project = extraction(&[(
        "greeting",
        entry("Hi, {$who}", "Hi, {$who}", &[("who", ParamKind::String)]),
    )]);

    let report = compare(&returned, project);
    assert_eq!(report.stale.len(), 1);
    assert!(
        report.stale[0].what.contains("parameters changed"),
        "{}",
        report.stale[0].what
    );
    assert!(
        report.stale[0]
            .what
            .contains("from (name: string) to (who: string)"),
        "{}",
        report.stale[0].what
    );
    assert_eq!(report.stale[0].declared_at, "src/messages.js:3");
}

#[test]
fn says_so_when_only_the_digest_disagrees() {
    // Nothing but a hand-edited digest reaches this arm, and the sentence for
    // it says that rather than naming a change that did not happen. Worth a
    // test because it is the one branch of `changed` a real project cannot
    // produce, and an empty sentence here would be a report that says nothing.
    let mut edited = entry("Hi.", "やあ", &[]);
    edited.digest = "0000000000000000".to_owned();
    let returned = catalogue("ja-JP", &[("greeting", edited)]);
    let project = extraction(&[("greeting", entry("Hi.", "Hi.", &[]))]);

    let report = compare(&returned, project);
    assert_eq!(report.stale.len(), 1);
    assert!(
        report.stale[0].what.contains("digest does not match"),
        "{}",
        report.stale[0].what
    );
}

#[test]
fn the_module_is_a_flow_module_the_loader_can_import() {
    let messages = BTreeMap::from([
        ("greeting", "こんにちは、{$name}!"),
        ("multi line", ".match $count\n*   {{a \"quoted\" line}}"),
    ]);
    let module = render_locale_module("ja-JP", &messages);

    assert!(module.starts_with("// @flow\n"));
    assert!(module.contains("export default {"));
    // An identifier key is bare, anything else is quoted, which is what a
    // formatter would leave behind.
    assert!(
        module.contains("\n  greeting: \"こんにちは、{$name}!\",\n"),
        "{module}"
    );
    assert!(module.contains("\n  \"multi line\": "), "{module}");
    // The newline and the quotes are encoded rather than written through.
    assert!(module.contains("\\n"), "{module}");
    assert!(module.contains("\\\"quoted\\\""), "{module}");
}
