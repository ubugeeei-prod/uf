//! Every contract violation the graph reports, and how it is ordered.

use super::*;

#[test]
fn a_client_module_importing_a_server_only_package_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("@uniflowed/server"));
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/server-only-import-in-client"
    );
    assert!(graph.has_errors());
}

#[test]
fn a_client_module_importing_a_server_only_subpath_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("@uniflowed/server/db"));
    assert_eq!(builder.build().diagnostics().len(), 1);
}

#[test]
fn a_client_module_importing_a_dot_server_file_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/Counter.js").with_import("./secrets.server.js"));
    builder.add_module(server("app/secrets.server.js"));
    builder.add_entry("app/Counter.js", EntryKind::Client);
    let graph = builder.build();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.rule() == "rsc/server-only-import-in-client")
    );
}

#[test]
fn a_module_pulled_into_the_client_graph_may_not_import_server_only_code() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./shared.js"));
    builder.add_module(server("app/shared.js").with_import("@uniflowed/server"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.rule() == "rsc/server-only-import-in-client")
    );
}

#[test]
fn a_server_module_may_import_server_only_code() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("@uniflowed/server"));
    builder.add_entry("app/page.js", EntryKind::Server);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_server_module_calling_a_client_only_api_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/page.js",
        "export default function Page() {\n const [a] = useState(1);\n return a;\n}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/client-only-api-in-server"
    );
}

#[test]
fn a_client_module_calling_a_client_only_api_is_fine() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/Counter.js",
        "\"use client\";\nexport default function Counter() {\n const [a] = useState(1);\n return a;\n}\n",
    );
    builder.add_entry("app/Counter.js", EntryKind::Client);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_shared_module_reached_only_from_the_client_may_use_hooks() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "import Counter from \"./Counter.js\";\n");
    builder.add_source(
        "app/Counter.js",
        "\"use client\";\nimport { useCounter } from \"./useCounter.js\";\n",
    );
    builder.add_source(
        "app/useCounter.js",
        "export function useCounter() {\n return useState(0);\n}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn an_unreachable_server_module_using_hooks_is_not_reported() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/orphan.js", "const a = useState(1);\n");
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_sync_server_action_export_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport function refresh() {}\n",
    );
    let graph = builder.build();
    assert_eq!(graph.diagnostics()[0].rule(), "rsc/server-action-not-async");
}

#[test]
fn a_non_function_server_action_export_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport const limit = 5;\n",
    );
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/server-action-not-a-function"
    );
}

#[test]
fn an_exported_class_in_a_server_actions_module_is_an_error() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("server/actions.js", "\"use server\";\nexport class Db {}\n");
    assert_eq!(
        builder.build().diagnostics()[0].rule(),
        "rsc/server-action-not-a-function"
    );
}

#[test]
fn an_async_server_action_export_is_accepted() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function refresh() {}\n",
    );
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn a_server_action_factory_export_is_accepted() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport const refresh = serverAction(async () => {});\n",
    );
    assert!(builder.build().diagnostics().is_empty());
}

#[test]
fn directive_issues_are_lifted_into_graph_diagnostics() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source("app/page.js", "const a = 1;\n\"use client\";\n");
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/directive-not-in-prologue"
    );
}

#[test]
fn diagnostics_are_ordered_deterministically() {
    let build = || {
        let mut builder = RscGraphBuilder::new();
        builder.add_source("b.js", "\"use client\";\nimport \"@uniflowed/server\";\n");
        builder.add_source("a.js", "\"use client\";\nimport \"server-only\";\n");
        builder.build()
    };
    let first: Vec<_> = build()
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect();
    let second: Vec<_> = build()
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect();
    assert_eq!(first, second);
    assert!(first[0].contains("a.js"));
}

/// A hook the name lists do not know, called by a module the server runs, is
/// reported as unanswered rather than passed over.
///
/// This is ubugeeei-prod/uf#348: `rsc/client-only-api-in-server` matches
/// identifiers against two sorted name lists, so a Server Component calling
/// `useState` is caught and one calling a hook *built on* `useState` is
/// invisible. `docs/app/_uf.layout.js` is that module in this repository. The
/// graph cannot decide it — the answer is in another module's body — and
/// deciding it wrongly is worse than saying so, so it says so.
#[test]
fn a_hook_the_name_lists_do_not_know_is_an_unanswered_question() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/_uf.layout.js",
        "function Masthead() { const { pathname } = useRoute(); }",
    );
    builder.add_entry("app/_uf.layout.js", EntryKind::Server);
    let graph = builder.build();

    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.rule() == "rsc/unclassified-hook-in-server")
        .unwrap_or_else(|| panic!("nothing was reported: {:#?}", graph.diagnostics()));
    assert_eq!(diagnostic.severity(), RscSeverity::Warn);
    assert!(diagnostic.to_string().contains("useRoute"), "{diagnostic}");
    assert!(
        !graph.has_errors(),
        "not knowing is not a contract violation: {:#?}",
        graph.diagnostics()
    );
}

/// And it is a question about the *server*. A module in the client bundle runs
/// in a browser, where every hook is legal, so there is nothing to ask.
#[test]
fn a_hook_in_a_client_module_is_not_a_question() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/Masthead.js",
        "\"use client\";\nfunction Masthead() { useRoute(); }",
    );
    builder.add_entry("app/Masthead.js", EntryKind::Client);
    let graph = builder.build();

    assert!(graph.diagnostics().is_empty(), "{:#?}", graph.diagnostics());
}
