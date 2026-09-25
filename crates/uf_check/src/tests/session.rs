//! [`Session`]: positional questions over a batch kept warm.

use super::*;

/// A project of two files: a model, and an app that imports it — enough for
/// every question to cross a file boundary.
fn project() -> Vec<OwnedSource> {
    vec![
        OwnedSource::new(
            "src/user.js",
            "// @flow\n\
             export type User = { name: string, age: number };\n\
             export function greet(user: User): string {\n  return user.name;\n}\n",
        ),
        OwnedSource::new(
            "src/app.js",
            "// @flow\n\
             import { greet, type User } from './user.js';\n\
             const user: User = { name: 'Ada', age: 36 };\n\
             const greeting = greet(user);\n\
             export const count = 1;\n",
        ),
    ]
}

fn at(line: u32, column: u32) -> Position {
    Position { line, column }
}

fn session() -> Session {
    let session = Session::start(Vec::new(), CheckLimits::default()).expect("a session starts");
    session.load(project()).expect("the project loads");
    session
}

#[test]
fn a_build_without_a_checker_cannot_start_a_session() {
    if is_available() {
        return;
    }
    let error = Session::start(Vec::new(), CheckLimits::default()).expect_err("no checker");
    assert!(error.is_unavailable());
}

#[test]
fn the_type_of_a_binding_is_printed_as_its_declaration() {
    require_checker!();
    let session = session();

    // `greeting`, on line 4 of `src/app.js`.
    let found = session
        .type_at("src/app.js", at(4, 7))
        .expect("the query runs")
        .expect("a binding has a type");

    assert_eq!(found.printed, "const greeting: string");
    assert_eq!(found.span.path, "src/app.js");
    assert_eq!((found.span.start, found.span.end), (at(4, 7), at(4, 15)));
}

#[test]
fn the_type_of_an_imported_function_comes_from_the_file_that_exports_it() {
    require_checker!();
    let session = session();

    // `greet` in the call on line 4.
    let found = session
        .type_at("src/app.js", at(4, 18))
        .expect("the query runs")
        .expect("the callee has a type");

    assert!(
        found.printed.contains("(user: User) => string"),
        "{}",
        found.printed
    );
}

#[test]
fn a_member_access_is_typed_through_the_imported_type() {
    require_checker!();
    let session = session();
    session
        .edit(
            "src/app.js",
            "// @flow\n\
             import { type User } from './user.js';\n\
             declare const user: User;\n\
             const years = user.age;\n"
                .to_owned(),
        )
        .expect("the edit applies");

    // `age` in `user.age`.
    let found = session
        .type_at("src/app.js", at(4, 21))
        .expect("the query runs")
        .expect("a property has a type");

    assert!(found.printed.ends_with("number"), "{}", found.printed);
}

#[test]
fn an_unannotated_export_is_diagnosed_in_the_editor() {
    require_checker!();

    let session = Session::start(Vec::new(), CheckLimits::default()).expect("starts");
    session
        .load(vec![
            OwnedSource::new(
                "src/model.js",
                "// @flow\nfunction make(): { age: number } { return { age: 18 }; }\nexport const Signup = make();\n",
            ),
            OwnedSource::new(
                "src/app.js",
                "// @flow\nimport { Signup } from './model.js';\ntype SignupType = typeof Signup;\nconst wrong: SignupType = { age: '36' };\n",
            ),
        ])
        .expect("loads");

    let diagnostics = session
        .diagnostics("src/model.js")
        .expect("checks")
        .expect("checked file");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == Some("signature-verification-failure") })
    );
}

#[test]
fn nothing_typed_under_the_cursor_is_no_answer() {
    require_checker!();
    let session = session();

    // Inside the `// @flow` comment.
    assert_eq!(session.type_at("src/app.js", at(1, 4)).expect("runs"), None);
    // A file the batch does not have.
    assert_eq!(
        session.type_at("src/missing.js", at(1, 1)).expect("runs"),
        None
    );
}

#[test]
fn a_definition_across_files_lands_on_the_exporting_declaration() {
    require_checker!();
    let session = session();

    // `greet` in the call on line 4.
    let found = session
        .definition("src/app.js", at(4, 18))
        .expect("the query runs");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].origin, Origin::Source);
    assert_eq!(found[0].span.path, "src/user.js");
    assert_eq!(found[0].span.start.line, 3);
}

#[test]
fn a_type_definition_names_the_declared_type() {
    require_checker!();
    let session = session();

    // `user`, declared as a `User`, on line 3.
    let found = session
        .type_definition("src/app.js", at(3, 7))
        .expect("the query runs");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].span.path, "src/user.js");
    assert_eq!(found[0].span.start.line, 2);
}

#[test]
fn a_definition_in_a_library_definition_says_which_kind() {
    require_checker!();
    let libs = vec![OwnedSource::new(
        "flow-typed/clock.js",
        "declare function tick(): number;\n",
    )];
    let session = Session::start(libs, CheckLimits::default()).expect("a session starts");
    session
        .load(vec![OwnedSource::new(
            "app.js",
            "// @flow\nconst now = tick();\nconst items = [1].map(n => n);\n",
        )])
        .expect("loads");

    let project = session.definition("app.js", at(2, 13)).expect("runs");
    assert_eq!(project.len(), 1, "{project:?}");
    assert_eq!(project[0].origin, Origin::Library);
    assert_eq!(project[0].span.path, "flow-typed/clock.js");

    // `map`, which Flow's own `core.js` declares.
    let builtin = session.definition("app.js", at(3, 19)).expect("runs");
    assert!(
        builtin.iter().all(|found| found.origin == Origin::Builtin),
        "{builtin:?}"
    );
    assert!(!builtin.is_empty());
}

#[test]
fn a_member_completion_offers_the_properties_with_their_types() {
    require_checker!();
    let session = session();
    session
        .edit(
            "src/app.js",
            "// @flow\n\
             import { type User } from './user.js';\n\
             declare const user: User;\n\
             user.\n"
                .to_owned(),
        )
        .expect("the edit applies");

    let found = session
        .completion("src/app.js", at(4, 6))
        .expect("the query runs")
        .expect("the service answers");

    let offered: Vec<(&str, Option<&str>)> = found
        .items
        .iter()
        .map(|item| (item.label.as_str(), item.detail.as_deref()))
        .collect();
    assert!(offered.contains(&("age", Some("number"))), "{offered:?}");
    assert!(offered.contains(&("name", Some("string"))), "{offered:?}");
}

#[test]
fn an_edit_is_seen_by_the_next_question() {
    require_checker!();
    let session = session();
    assert_eq!(
        session
            .type_at("src/app.js", at(5, 14))
            .expect("runs")
            .map(|found| found.printed),
        Some("const count: 1".to_owned())
    );

    session
        .edit(
            "src/app.js",
            "// @flow\nexport const count = 'many';\n".to_owned(),
        )
        .expect("applies");

    assert_eq!(
        session
            .type_at("src/app.js", at(2, 14))
            .expect("runs")
            .map(|found| found.printed),
        Some("const count: \"many\"".to_owned())
    );
}

#[test]
fn an_edit_to_a_dependency_reaches_the_file_that_imports_it() {
    require_checker!();
    let session = session();
    // Inferred, with `user.js` merged into it.
    let before = session.type_at("src/app.js", at(4, 7)).expect("runs");
    assert_eq!(
        before.map(|found| found.printed),
        Some("const greeting: string".to_owned())
    );

    session
        .edit(
            "src/user.js",
            "// @flow\n\
             export type User = { name: string, age: number };\n\
             export function greet(user: User): number {\n  return user.age;\n}\n"
                .to_owned(),
        )
        .expect("applies");

    let after = session.type_at("src/app.js", at(4, 7)).expect("runs");
    assert_eq!(
        after.map(|found| found.printed),
        Some("const greeting: number".to_owned())
    );
}

#[test]
fn an_edit_to_a_file_outside_the_batch_is_refused_rather_than_guessed() {
    require_checker!();
    let session = session();

    assert!(
        !session
            .edit("src/new.js", "// @flow\n".to_owned())
            .expect("runs")
    );
    assert!(!session.contains("src/new.js").expect("runs"));
    assert!(session.contains("src/app.js").expect("runs"));
}

#[test]
fn a_file_that_does_not_parse_has_no_answer_and_does_not_stop_the_session() {
    require_checker!();
    let session = session();
    session
        .edit("src/app.js", "// @flow\nconst = ;\n".to_owned())
        .expect("applies");

    assert_eq!(session.type_at("src/app.js", at(2, 1)).expect("runs"), None);
    // The other file still answers.
    assert!(
        session
            .type_at("src/user.js", at(3, 17))
            .expect("runs")
            .is_some()
    );
}

#[test]
fn a_position_past_the_text_is_no_answer_and_not_a_crash() {
    require_checker!();
    let session = session();

    // Past the end of `// @flow`, past the last line, and at column zero:
    // the port panics on the first, and an editor sends it whenever the
    // pointer rests beyond a short line.
    for at in [at(1, 40), at(40, 1), at(2, 0)] {
        assert_eq!(session.type_at("src/app.js", at).expect("runs"), None);
        assert!(
            session
                .definition("src/app.js", at)
                .expect("runs")
                .is_empty()
        );
        assert_eq!(session.completion("src/app.js", at).expect("runs"), None);
    }
    // And the session is still the same session.
    assert!(
        session
            .type_at("src/app.js", at(4, 7))
            .expect("runs")
            .is_some()
    );
}

/// `project()` with a type error in `src/app.js`: a `User` whose `age` is a
/// string, on line 3.
fn broken() -> Vec<OwnedSource> {
    let mut sources = project();
    sources[1].source = sources[1].source.replace("age: 36", "age: '36'");
    sources
}

fn codes(found: &[TypeDiagnostic]) -> Vec<Option<&'static str>> {
    found.iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn a_files_diagnostics_are_the_ones_uf_check_reports_for_it() {
    require_checker!();
    let batch = broken();
    let session = Session::start(Vec::new(), CheckLimits::default()).expect("starts");
    session.load(batch.clone()).expect("loads");

    let found = session
        .diagnostics("src/app.js")
        .expect("runs")
        .expect("the file is inferred");

    let sources: Vec<Source<'_>> = batch
        .iter()
        .map(|source| Source::new(&source.path, &source.source))
        .collect();
    let report = check_sources(&sources, &[], &CheckLimits::default()).expect("checks");
    let expected: Vec<TypeDiagnostic> = report
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.primary.path == "src/app.js")
        .collect();
    assert!(!expected.is_empty(), "the fixture has an error to find");
    assert_eq!(found, expected);
    assert_eq!(found[0].primary.start.line, 3);
    // And the file without one has nothing to say.
    assert_eq!(
        session.diagnostics("src/user.js").expect("runs"),
        Some(Vec::new())
    );
}

#[test]
fn a_suppression_comment_hides_a_diagnostic_from_the_editor_as_from_uf_check() {
    require_checker!();
    let session = Session::start(Vec::new(), CheckLimits::default()).expect("starts");
    let mut batch = broken();
    batch[1].source = batch[1].source.replace(
        "const user: User",
        "// $FlowFixMe[incompatible-type]\nconst user: User",
    );
    session.load(batch).expect("loads");

    assert_eq!(
        session.diagnostics("src/app.js").expect("runs"),
        Some(Vec::new())
    );
}

#[test]
fn an_edit_that_fixes_the_error_clears_it_and_one_that_breaks_a_dependency_reports_it() {
    require_checker!();
    let session = Session::start(Vec::new(), CheckLimits::default()).expect("starts");
    session.load(broken()).expect("loads");
    let before = session
        .diagnostics("src/app.js")
        .expect("runs")
        .expect("inferred");
    assert_eq!(codes(&before), [Some("incompatible-type")]);

    // Fixed in the editor.
    session
        .edit("src/app.js", project()[1].source.clone())
        .expect("applies");
    assert_eq!(
        session.diagnostics("src/app.js").expect("runs"),
        Some(Vec::new())
    );

    // Then the file it imports changes under it: `greet` now wants a number,
    // and the call in `src/app.js` is what is wrong. Asked about `app.js`,
    // which was not the file edited.
    session
        .edit(
            "src/user.js",
            "// @flow\n\
             export type User = { name: string, age: number };\n\
             export function greet(user: number): string {\n  return String(user);\n}\n"
                .to_owned(),
        )
        .expect("applies");
    let after = session
        .diagnostics("src/app.js")
        .expect("runs")
        .expect("inferred");
    assert!(!after.is_empty(), "the call is now wrong");
    assert!(
        after
            .iter()
            .all(|diagnostic| diagnostic.primary.start.line == 4),
        "{after:?}"
    );
}

#[test]
fn a_file_that_does_not_parse_leaves_its_syntax_error_to_the_linter() {
    require_checker!();
    let session = session();
    session
        .edit("src/app.js", "// @flow\nconst = ;\n".to_owned())
        .expect("applies");

    assert_eq!(session.diagnostics("src/app.js").expect("runs"), None);
    assert_eq!(session.diagnostics("src/missing.js").expect("runs"), None);
}
