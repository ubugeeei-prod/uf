use super::*;

/// Read one module's `schedule` export, without a project around it.
fn read(source: &str) -> Result<Option<String>> {
    read_schedule(source, Utf8Path::new("app/api/_uf.route.js"))
}

#[test]
fn a_string_export_is_the_expression() {
    let found = read("export const schedule = \"*/15 * * * *\";\nexport function GET() {}\n")
        .expect("a module that parses");
    assert_eq!(found.as_deref(), Some("*/15 * * * *"));
}

#[test]
fn a_module_without_one_declares_none() {
    assert_eq!(
        read("export function GET() {}\n").expect("a module that parses"),
        None
    );
}

/// A local `const schedule` is not an export, and neither is a different name.
#[test]
fn only_the_exported_binding_of_that_name_counts() {
    assert_eq!(
        read("const schedule = \"* * * * *\";\nexport function GET() {}\n").expect("parses"),
        None
    );
    assert_eq!(
        read("export const schedules = \"* * * * *\";\n").expect("parses"),
        None
    );
}

/// The refusal that matters: this build runs no project code, so an expression
/// it would have to evaluate is one it cannot write into `wrangler.json`.
#[test]
fn a_computed_expression_is_refused_by_name_rather_than_skipped() {
    let message = read("export const schedule = everyMinutes(15);\n")
        .expect_err("a computed schedule is refused")
        .to_string();
    assert!(message.contains("cannot read"), "{message}");
    assert!(message.contains("*/15 * * * *"), "{message}");
    assert!(message.contains("_uf.route.js"), "{message}");
}

#[test]
fn a_template_literal_is_refused_for_the_same_reason() {
    let message = read("export const schedule = `*/${n} * * * *`;\n")
        .expect_err("a template literal is computed")
        .to_string();
    assert!(message.contains("cannot read"), "{message}");
}

#[test]
fn an_expression_that_is_not_five_fields_is_refused_here() {
    let message = read("export const schedule = \"* * * *\";\n")
        .expect_err("four fields is not a cron expression")
        .to_string();
    assert!(message.contains("not five fields"), "{message}");
    assert!(message.contains("day-of-week"), "{message}");
}

/// What each field *contains* is `packages/server/internal/cron.js`'s to say,
/// and a second matcher here would be two implementations of one rule. So this
/// passes something that module refuses, deliberately — loudly at start-up
/// rather than quietly in both places.
#[test]
fn what_a_field_holds_is_not_this_builds_question() {
    assert_eq!(
        read("export const schedule = \"99 * * * *\";\n").expect("five fields"),
        Some("99 * * * *".to_owned())
    );
}

#[test]
fn a_let_export_is_read_the_same_way_a_const_is() {
    // The declaration kind is not the point — being written in the file is.
    assert_eq!(
        read("export let schedule = \"0 0 * * *\";\n").expect("parses"),
        Some("0 0 * * *".to_owned())
    );
}

/// A project on disk, so `discover_server_modules`' walk is exercised rather
/// than assumed: the string tests above never touch the router root, and the
/// question "does it find the file" is a different one from "does it read it".
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (path, contents) in files {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
        std::fs::write(&file, contents).expect("the file");
    }
    dir
}

fn discovered(dir: &tempfile::TempDir) -> Vec<DeclaredSchedule> {
    let root = Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
    discover_schedules(root, &UniflowedConfig::default()).expect("a project that reads")
}

#[test]
fn a_route_handler_that_declares_one_is_found_at_its_path() {
    let dir = project(&[
        (
            "app/api/sweep/_uf.route.js",
            "export const schedule = \"*/15 * * * *\";\nexport function GET() {}\n",
        ),
        ("app/api/health/_uf.route.js", "export function GET() {}\n"),
    ]);

    let found = discovered(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "/api/sweep");
    assert_eq!(found[0].cron, "*/15 * * * *");
    assert!(
        found[0].file.as_str().ends_with("_uf.route.js"),
        "{:?}",
        found[0].file
    );
}

#[test]
fn several_come_back_in_path_order() {
    let dir = project(&[
        (
            "app/api/sweep/_uf.route.js",
            "export const schedule = \"*/15 * * * *\";\n",
        ),
        (
            "app/api/digest/_uf.route.js",
            "export const schedule = \"0 6 * * 1\";\n",
        ),
    ]);

    let paths: Vec<_> = discovered(&dir).into_iter().map(|s| s.path).collect();
    // Sorted, so a message built from this does not depend on the order the
    // filesystem hands entries back.
    assert_eq!(paths, ["/api/digest", "/api/sweep"]);
}

/// A middleware runs per request by definition, so a schedule on one would be
/// a schedule on something that already has a trigger.
#[test]
fn a_middleware_is_not_asked() {
    let dir = project(&[(
        "app/dashboard/_uf.middleware.js",
        "export const schedule = \"* * * * *\";\nexport function middleware() {}\n",
    )]);
    assert_eq!(discovered(&dir), Vec::new());
}

#[test]
fn a_project_with_no_router_root_declares_none() {
    let dir = project(&[("package.json", "{}\n")]);
    assert_eq!(discovered(&dir), Vec::new());
}

/// The refusal reaches out of a real project, not only out of a string.
#[test]
fn a_computed_expression_in_a_real_project_fails_the_walk() {
    let dir = project(&[(
        "app/api/sweep/_uf.route.js",
        "export const schedule = everyMinutes(15);\n",
    )]);
    let root = Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
    let message = discover_schedules(root, &UniflowedConfig::default())
        .expect_err("a computed schedule is refused")
        .to_string();
    assert!(message.contains("cannot read"), "{message}");
}

/// `export { schedule }` is the same declaration to everything else that reads
/// this module, so a build that saw only `export const` would miss a schedule
/// silently — the one failure this file exists to refuse.
#[test]
fn a_specifier_export_is_read_the_same_way_a_declaration_is() {
    assert_eq!(
        read("const schedule = \"*/15 * * * *\";\nexport { schedule };\n").expect("parses"),
        Some("*/15 * * * *".to_owned())
    );
}

#[test]
fn a_renamed_specifier_export_is_read_under_the_name_it_takes() {
    assert_eq!(
        read("const every = \"0 6 * * 1\";\nexport { every as schedule };\n").expect("parses"),
        Some("0 6 * * 1".to_owned())
    );
    // And the local name is not what is looked for.
    assert_eq!(
        read("const schedule = \"0 6 * * 1\";\nexport { schedule as other };\n").expect("parses"),
        None
    );
}

#[test]
fn a_specifier_export_of_something_unreadable_is_refused() {
    let message = read("const schedule = everyMinutes(15);\nexport { schedule };\n")
        .expect_err("a computed binding is refused however it is exported")
        .to_string();
    assert!(message.contains("cannot read"), "{message}");
}

/// The value is in another module, and following imports to find it is a much
/// larger question than reading this file. Refused rather than skipped.
#[test]
fn a_re_export_is_refused_rather_than_missed() {
    let message = read("export { schedule } from \"./elsewhere.js\";\n")
        .expect_err("a re-exported schedule is refused")
        .to_string();
    assert!(message.contains("re-exports"), "{message}");
    assert!(
        message.contains("elsewhere") || message.contains("another module"),
        "{message}"
    );
}

#[test]
fn a_specifier_export_is_found_in_a_real_project_too() {
    let dir = project(&[(
        "app/api/sweep/_uf.route.js",
        "const schedule = \"*/15 * * * *\";\nexport { schedule };\nexport function GET() {}\n",
    )]);
    let found = discovered(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].cron, "*/15 * * * *");
}
