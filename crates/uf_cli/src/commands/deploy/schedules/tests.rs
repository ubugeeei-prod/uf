use super::*;

/// Read one module's `schedule` export, without a project around it.
fn read(source: &str) -> Result<Option<String>> {
    read_schedule(source, Utf8Path::new("app/api/_uf.route.js"))
}

/// The same, for a module that declares a schedule and exports the `GET` a
/// trigger calls — which is what a real one has to do, so most cases below
/// would otherwise be testing the method refusal by accident.
fn read_with_handler(declaration: &str) -> Result<Option<String>> {
    read(&format!("{declaration}export function GET() {{}}\n"))
}

#[test]
fn a_string_export_is_the_expression() {
    let found = read_with_handler("export const schedule = \"*/15 * * * *\";\n")
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
        read_with_handler("export const schedule = \"99 * * * *\";\n").expect("five fields"),
        Some("99 * * * *".to_owned())
    );
}

#[test]
fn a_let_export_is_read_the_same_way_a_const_is() {
    // The declaration kind is not the point — being written in the file is.
    assert_eq!(
        read_with_handler("export let schedule = \"0 0 * * *\";\n").expect("parses"),
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
            "export const schedule = \"*/15 * * * *\";\nexport function GET() {}\n",
        ),
        (
            "app/api/digest/_uf.route.js",
            "export const schedule = \"0 6 * * 1\";\nexport function GET() {}\n",
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
        read_with_handler("const schedule = \"*/15 * * * *\";\nexport { schedule };\n")
            .expect("parses"),
        Some("*/15 * * * *".to_owned())
    );
}

#[test]
fn a_renamed_specifier_export_is_read_under_the_name_it_takes() {
    assert_eq!(
        read_with_handler("const every = \"0 6 * * 1\";\nexport { every as schedule };\n")
            .expect("parses"),
        Some("0 6 * * 1".to_owned())
    );
    // And the local name is not what is looked for.
    assert_eq!(
        read_with_handler("const schedule = \"0 6 * * 1\";\nexport { schedule as other };\n")
            .expect("parses"),
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

/// A trigger fires a `GET` at the route's own path, so a module that declares
/// a schedule and exports no `GET` is a trigger that would answer 405.
#[test]
fn a_schedule_without_the_handler_it_would_call_is_refused() {
    let message = read("export const schedule = \"*/15 * * * *\";\nexport function POST() {}\n")
        .expect_err("a schedule with no GET is refused")
        .to_string();
    assert!(message.contains("exports no `GET`"), "{message}");
    assert!(message.contains("405"), "{message}");
}

#[test]
fn the_handler_counts_in_any_spelling_a_module_may_use() {
    for handler in [
        "export function GET() {}\n",
        "export const GET = () => new Response(\"\");\n",
        "function handler() {}\nexport { handler as GET };\n",
    ] {
        let source = format!("export const schedule = \"0 0 * * *\";\n{handler}");
        assert_eq!(
            read(&source).expect("a module with a handler"),
            Some("0 0 * * *".to_owned()),
            "{handler}"
        );
    }
}

/// And a route handler that declares nothing is never asked about its methods.
#[test]
fn a_module_with_no_schedule_may_export_whatever_it_answers() {
    assert_eq!(read("export function POST() {}\n").expect("parses"), None);
}

/// The annotated spelling, which is the one a Flow project actually writes.
///
/// `export const schedule: string = "…"` puts a type annotation on the
/// identifier, and this reads the declarator rather than the token after the
/// name — so both spellings are one declaration. Asserted rather than assumed
/// because the fixture in `crates/uf_cli/tests/vite.rs` is written this way,
/// and a build that silently found no schedule there would assert nothing.
#[test]
fn a_type_annotation_does_not_hide_the_declaration() {
    let found = read_with_handler("export const schedule: string = \"*/15 * * * *\";\n")
        .expect("a module that parses");
    assert_eq!(found.as_deref(), Some("*/15 * * * *"));
}

// # The artefact's own two halves
//
// Everything above is about reading a declaration out of a project. What
// follows is about the other end: whether the directory `uf build --adapter`
// just wrote says the same thing in both of the places it says it.
//
// The sources below are *linked* files rather than the entries
// `@uniflowed/vite`'s `driver.js` writes, and that is the point of every shape
// here: what lands in an artefact has been through Rolldown, so the
// identifiers, the statement forms and the whitespace are a third program's
// and the check must not depend on any of them. Two spellings of each shape
// are driven — one the way a reader would write it, one renamed and reflowed
// the way a linker may — because a check that only passed the readable one
// would be a check that goes red the first time the linker changes its mind.

/// A declaration, as [`discover_schedules`] would have returned it.
fn declared(path: &str, cron: &str) -> DeclaredSchedule {
    DeclaredSchedule {
        path: path.into(),
        file: Utf8PathBuf::from(format!("app{path}/_uf.route.js")),
        cron: cron.to_owned(),
    }
}

/// A directory holding `files`, as an artefact directory.
fn artefact(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("writing an artefact file");
    }
    dir
}

/// That directory's path, as the check takes it.
fn at(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path")
}

/// `wrangler.json`, with `crons` when there are any and no `triggers` at all
/// when there are none — which is what `wrangler_config` writes.
fn wrangler(crons: &[&str]) -> String {
    if crons.is_empty() {
        return "{ \"main\": \"./worker.js\" }\n".to_owned();
    }
    let listed = crons
        .iter()
        .map(|cron| format!("\"{cron}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ \"main\": \"./worker.js\", \"triggers\": {{ \"crons\": [{listed}] }} }}\n")
}

/// A linked `worker.js`, as a reader would find it in the output.
fn worker_entry(routes: &[(&str, &str)], scheduled: bool) -> String {
    let mut source =
        String::from("import { c as createWorkerFetch } from \"./chunks/edge.js\";\n\n");
    if !routes.is_empty() {
        let map = routes
            .iter()
            .map(|(cron, path)| format!("  \"{cron}\": \"{path}\""))
            .collect::<Vec<_>>()
            .join(",\n");
        source.push_str(&format!("const routes = {{\n{map}\n}};\n\n"));
    }
    source.push_str("export default {\n  fetch: createWorkerFetch({ handle, beginRequest }),\n");
    if scheduled {
        source.push_str("  scheduled: createWorkerScheduled({ handle, beginRequest, routes }),\n");
    }
    source.push_str("};\n");
    source
}

/// The same worker after a linker that renames and reflows.
///
/// Nothing a runtime reads has changed: the property names are still there
/// because the runtime reads them, and the strings are still there because
/// they are data. Everything else has.
fn worker_entry_linked(routes: &[(&str, &str)], scheduled: bool) -> String {
    let map = routes
        .iter()
        .map(|(cron, path)| format!("\"{cron}\":\"{path}\""))
        .collect::<Vec<_>>()
        .join(",");
    let mut source = String::from("import{a as n$3,b as s$2}from\"./chunks/edge-a1b2.js\";");
    if !routes.is_empty() {
        source.push_str(&format!("var r$4={{{map}}};"));
    }
    source.push_str("var w$1={fetch:n$3({handle:h$2,beginRequest:b$1})");
    if scheduled {
        source.push_str(",scheduled:s$2({handle:h$2,beginRequest:b$1,routes:r$4})");
    }
    source.push_str("};export{w$1 as default};\n");
    source
}

/// A linked `server.js`, as a reader would find it in the output.
fn process_entry(wired: &[(&str, &str)]) -> String {
    let mut source =
        String::from("import { s as serve, r as routeSchedule } from \"./chunks/n.js\";\n");
    if !wired.is_empty() {
        source.push_str("\nconst schedules = [\n");
        for (path, cron) in wired {
            source.push_str(&format!(
                "  routeSchedule({{ handle: fetch, beginRequest, path: \"{path}\", \
                 cron: \"{cron}\" }}),\n"
            ));
        }
        source.push_str("];\n");
    }
    source.push_str("\nserve({ handle: fetch, staticDir, beginRequest, schedules });\n");
    source
}

/// The same after a linker that renames and reflows.
fn process_entry_linked(wired: &[(&str, &str)]) -> String {
    let mut source = String::from("import{s as v$2,r as q$3}from\"./chunks/n-c3d4.js\";");
    if !wired.is_empty() {
        let built = wired
            .iter()
            .map(|(path, cron)| {
                format!("q$3({{handle:f$1,beginRequest:b$1,path:\"{path}\",cron:\"{cron}\"}})")
            })
            .collect::<Vec<_>>()
            .join(",");
        source.push_str(&format!("var z$5=[{built}];"));
    }
    source.push_str("v$2({handle:f$1,staticDir:d$1,beginRequest:b$1,schedules:z$5});\n");
    source
}

/// An artefact whose halves agree is an artefact this says nothing about.
#[test]
fn a_worker_that_declares_what_it_runs_is_accepted() {
    for written in [worker_entry, worker_entry_linked] {
        let dir = artefact(&[
            (
                "worker.js",
                &written(&[("*/15 * * * *", "/api/sweep")], true),
            ),
            ("wrangler.json", &wrangler(&["*/15 * * * *"])),
        ]);
        assert_wired(
            DeployAdapter::Edge,
            &at(&dir),
            &[declared("/api/sweep", "*/15 * * * *")],
        )
        .expect("both halves name the same expression, however it was linked");
    }
}

/// The bug itself: ubugeeei-prod/uf#712, taken back by #717.
///
/// A `triggers.crons` beside a `worker.js` that exports no `scheduled()` is a
/// Worker Cloudflare accepts, deploys, and then fails on every invocation of.
#[test]
fn a_trigger_with_no_handler_to_call_is_refused() {
    let dir = artefact(&[
        ("worker.js", &worker_entry(&[], false)),
        ("wrangler.json", &wrangler(&["*/15 * * * *"])),
    ]);
    let message = assert_wired(
        DeployAdapter::Edge,
        &at(&dir),
        &[declared("/api/sweep", "*/15 * * * *")],
    )
    .expect_err("a trigger with nothing to call is refused")
    .to_string();
    assert!(message.contains("exports no `scheduled()`"), "{message}");
    assert!(message.contains("uf#712"), "{message}");
    // And the file itself, so the fault can be read rather than reproduced.
    assert!(message.contains("export default {"), "{message}");
}

/// And the other direction, which is the half a one-way check would miss: a
/// `scheduled()` nothing will ever call.
#[test]
fn a_handler_no_trigger_would_call_is_refused() {
    let dir = artefact(&[
        (
            "worker.js",
            &worker_entry(&[("*/15 * * * *", "/api/sweep")], true),
        ),
        ("wrangler.json", &wrangler(&[])),
    ]);
    let message = assert_wired(DeployAdapter::Edge, &at(&dir), &[])
        .expect_err("a scheduled export nothing fires is refused")
        .to_string();
    assert!(message.contains("exports a `scheduled()`"), "{message}");
}

/// A trigger and an export together are still not enough: Cloudflare fires the
/// expression, so an expression the routes map does not hold is an invocation
/// with nowhere to go.
#[test]
fn a_trigger_the_worker_routes_nowhere_is_refused() {
    let dir = artefact(&[
        (
            "worker.js",
            &worker_entry(&[("0 6 * * 1", "/api/digest")], true),
        ),
        ("wrangler.json", &wrangler(&["*/15 * * * *"])),
    ]);
    let message = assert_wired(
        DeployAdapter::Edge,
        &at(&dir),
        &[declared("/api/sweep", "*/15 * * * *")],
    )
    .expect_err("an expression with no route is refused")
    .to_string();
    assert!(
        message.contains("does not route it to /api/sweep"),
        "{message}"
    );
}

/// And one routed to a path the project never declared it on.
#[test]
fn a_schedule_routed_to_another_path_is_refused() {
    for written in [worker_entry, worker_entry_linked] {
        let dir = artefact(&[
            (
                "worker.js",
                &written(&[("*/15 * * * *", "/api/digest")], true),
            ),
            ("wrangler.json", &wrangler(&["*/15 * * * *"])),
        ]);
        let message = assert_wired(
            DeployAdapter::Edge,
            &at(&dir),
            &[declared("/api/sweep", "*/15 * * * *")],
        )
        .expect_err("a schedule on the wrong route is refused")
        .to_string();
        assert!(message.contains("/api/sweep"), "{message}");
    }
}

/// A project with none gets the worker it has always had.
#[test]
fn a_worker_with_no_schedules_declares_and_runs_none() {
    let dir = artefact(&[
        ("worker.js", &worker_entry(&[], false)),
        ("wrangler.json", &wrangler(&[])),
    ]);
    assert_wired(DeployAdapter::Edge, &at(&dir), &[]).expect("nothing declared and nothing run");
}

/// The three targets with no configuration file to make a disagreement
/// visible: what has to be true is that the entry carries the declaration.
#[test]
fn a_process_entry_that_carries_what_was_declared_is_accepted() {
    for adapter in [
        DeployAdapter::Node,
        DeployAdapter::Bun,
        DeployAdapter::Container,
    ] {
        for written in [process_entry, process_entry_linked] {
            let dir = artefact(&[("server.js", &written(&[("/api/sweep", "*/15 * * * *")]))]);
            assert_wired(
                adapter,
                &at(&dir),
                &[declared("/api/sweep", "*/15 * * * *")],
            )
            .unwrap_or_else(|error| panic!("`{}`: {error}", adapter.as_str()));
        }
    }
}

/// An entry that lost one. On these targets a schedule the entry does not
/// carry is scheduled work that never happens, with nothing anywhere saying
/// so — the same failure as #712 with no `wrangler.json` to make it visible.
#[test]
fn a_process_entry_that_lost_a_declaration_is_refused() {
    for adapter in [
        DeployAdapter::Node,
        DeployAdapter::Bun,
        DeployAdapter::Container,
    ] {
        let dir = artefact(&[("server.js", &process_entry(&[]))]);
        let message = assert_wired(
            adapter,
            &at(&dir),
            &[declared("/api/sweep", "*/15 * * * *")],
        )
        .expect_err("a declaration nothing carries is refused")
        .to_string();
        assert!(message.contains("runs no schedule with"), "{message}");
        assert!(message.contains(adapter.as_str()), "{message}");
    }
}

/// The expression, and not merely a schedule on that route.
#[test]
fn a_process_entry_that_carries_a_different_expression_is_refused() {
    let dir = artefact(&[("server.js", &process_entry(&[("/api/sweep", "0 6 * * 1")]))]);
    let message = assert_wired(
        DeployAdapter::Node,
        &at(&dir),
        &[declared("/api/sweep", "*/15 * * * *")],
    )
    .expect_err("a schedule on a different expression is refused")
    .to_string();
    assert!(message.contains("*/15 * * * *"), "{message}");
}

/// And the route, and not merely the expression.
#[test]
fn a_process_entry_that_carries_a_different_route_is_refused() {
    let dir = artefact(&[(
        "server.js",
        &process_entry(&[("/api/digest", "*/15 * * * *")]),
    )]);
    let message = assert_wired(
        DeployAdapter::Node,
        &at(&dir),
        &[declared("/api/sweep", "*/15 * * * *")],
    )
    .expect_err("a schedule on a different route is refused")
    .to_string();
    assert!(message.contains("/api/sweep"), "{message}");
}

/// Several, each of which has to be found beside its own property name rather
/// than anywhere in the file.
#[test]
fn every_declaration_is_matched_against_its_own_property() {
    let dir = artefact(&[(
        "server.js",
        &process_entry(&[("/api/sweep", "*/15 * * * *"), ("/api/digest", "0 6 * * 1")]),
    )]);
    assert_wired(
        DeployAdapter::Node,
        &at(&dir),
        &[
            declared("/api/digest", "0 6 * * 1"),
            declared("/api/sweep", "*/15 * * * *"),
        ],
    )
    .expect("two declarations, both carried");
}

/// The scheduler's own code is not a schedule.
///
/// `serve` reaches `@uniflowed/server/schedule` for every project, so
/// `schedule.js`'s `cron: parseCron(options.cron)` is linked into every one of
/// these entries. A check that read a bare `cron:` as a declaration would
/// refuse every `node` build there has ever been — which is why the reverse
/// direction is not stated on these three, and why this test exists to hold
/// that line.
#[test]
fn the_schedulers_own_cron_property_is_not_read_as_a_declaration() {
    let dir = artefact(&[(
        "server.js",
        "import{p as parseCron}from\"./chunks/n.js\";\
         function defineSchedule(o){return{name:o.name,cron:parseCron(o.cron),run:o.run}}\
         v$2({handle:f$1,staticDir:d$1,beginRequest:b$1});\n",
    )]);
    assert_wired(DeployAdapter::Node, &at(&dir), &[])
        .expect("a project that declared none, in an entry that links the scheduler");
}

#[test]
fn a_process_entry_with_no_schedules_is_the_file_it_has_always_been() {
    let dir = artefact(&[("server.js", &process_entry(&[]))]);
    assert_wired(DeployAdapter::Node, &at(&dir), &[]).expect("nothing declared and nothing run");
}

/// The three that run none. Reaching here with one means [`refuse_unrunnable`]
/// was bypassed, which is a fault in uf and is reported as one rather than
/// quietly accepted.
#[test]
fn a_target_that_runs_no_schedule_never_carries_one() {
    let dir = artefact(&[]);
    for adapter in [
        DeployAdapter::Serverless,
        DeployAdapter::Static,
        DeployAdapter::Deno,
    ] {
        assert_wired(adapter, &at(&dir), &[]).expect("a project that declared none");
        let message = assert_wired(adapter, &at(&dir), &[declared("/api/sweep", "* * * * *")])
            .expect_err("a schedule this target would not run")
            .to_string();
        assert!(message.contains("runs no schedule"), "{message}");
        assert!(message.contains("refuse_unrunnable"), "{message}");
    }
}

/// A property name is a whole name, not a substring of a longer one.
///
/// The reason [`property_value`] exists rather than a `contains("scheduled:")`:
/// a linker is free to produce `unscheduled:` or `rescheduled:`, and a check
/// that read one of those as the export would pass an artefact with no
/// `scheduled()` in it at all.
#[test]
fn a_longer_identifier_ending_in_the_name_is_not_that_property() {
    assert_eq!(property_value("{ unscheduled: 1 }", "scheduled"), None);
    assert_eq!(property_value("{ scheduled: 1 }", "scheduled"), Some("1 }"));
    // And the whitespace between the two is the linker's, so it is skipped.
    assert_eq!(
        property_value("{scheduled  :\n  2}", "scheduled"),
        Some("2}")
    );
}
