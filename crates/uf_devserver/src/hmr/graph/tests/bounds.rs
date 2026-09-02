//! Module paths, and the bounds that turn a hostile tree into a typed
//! error instead of an allocation.

use super::*;

#[test]
fn module_paths_are_normalized_to_forward_slashes() {
    let mut graph = DevGraph::new();
    graph
        .insert("app\\nested\\page.js", SERVER_HELPER)
        .expect("inserts");

    assert_eq!(
        module(&graph, "app/nested/page.js").path().as_str(),
        "app/nested/page.js"
    );
}

#[test]
fn a_dot_segment_is_dropped_from_a_module_path() {
    let mut graph = DevGraph::new();
    graph
        .insert("./app/./page.js", SERVER_HELPER)
        .expect("inserts");

    assert!(graph.find("app/page.js").is_some());
}

#[test]
fn an_absolute_module_path_is_refused() {
    let mut graph = DevGraph::new();
    let error = graph.insert("/etc/passwd", SERVER_HELPER).unwrap_err();

    assert!(matches!(error, GraphError::NotProjectRelative { .. }));
    assert!(graph.is_empty());
}

#[test]
fn a_windows_absolute_module_path_is_refused() {
    let mut graph = DevGraph::new();

    assert!(matches!(
        graph.insert("\\app\\page.js", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
    assert!(matches!(
        graph.insert("C:/app/page.js", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
}

#[test]
fn a_module_path_that_climbs_out_of_the_project_is_refused() {
    let mut graph = DevGraph::new();

    assert!(matches!(
        graph.insert("../.env", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
    assert!(matches!(
        graph.insert("app/../../.env", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
}

#[test]
fn an_empty_module_path_is_refused() {
    let mut graph = DevGraph::new();

    assert!(matches!(
        graph.insert("", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
    assert!(matches!(
        graph.insert("./", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
}

#[test]
fn a_module_path_with_a_nul_byte_is_refused() {
    let mut graph = DevGraph::new();

    assert!(matches!(
        graph.insert("app/page\0.js", SERVER_HELPER).unwrap_err(),
        GraphError::NotProjectRelative { .. }
    ));
}

#[test]
fn a_module_path_deeper_than_the_bound_is_refused() {
    let mut graph = DevGraph::new();
    let deep = (0..=MAX_MODULE_DEPTH)
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>()
        .join("/");

    let error = graph.insert(&deep, SERVER_HELPER).unwrap_err();

    assert!(matches!(error, GraphError::TooDeep { .. }));
}

#[test]
fn a_module_path_exactly_at_the_depth_bound_is_accepted() {
    let mut graph = DevGraph::new();
    let deep = (0..MAX_MODULE_DEPTH)
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>()
        .join("/");

    assert!(graph.insert(&deep, SERVER_HELPER).is_ok());
}

#[test]
fn a_source_over_the_byte_bound_is_refused() {
    let mut graph = DevGraph::new();
    let source = "a".repeat(MAX_MODULE_BYTES + 1);

    let error = graph.insert("app/huge.js", &source).unwrap_err();

    assert!(matches!(error, GraphError::SourceTooLarge { .. }));
    assert!(graph.is_empty());
}

#[test]
fn a_module_naming_more_imports_than_the_bound_is_refused() {
    let mut graph = DevGraph::new();
    let mut source = String::from("// @flow\n");
    for index in 0..=MAX_MODULE_IMPORTS {
        source.push_str(&format!("import m{index} from \"./m{index}.js\";\n"));
    }

    let error = graph.insert("app/barrel.js", &source).unwrap_err();

    assert!(matches!(error, GraphError::TooManyImports { .. }));
}

#[test]
fn a_non_ascii_module_path_is_kept_verbatim() {
    let mut graph = DevGraph::new();
    graph
        .insert("app/café.js", CLIENT_COMPONENT)
        .expect("inserts");

    assert!(graph.find("app/café.js").is_some());
}

#[test]
fn find_normalizes_before_it_looks_up() {
    let graph = graph_with(&[("app/page.js", SERVER_HELPER)]);

    assert!(graph.find("./app/page.js").is_some());
    assert!(graph.find("app\\page.js").is_some());
    assert!(graph.find("app/nested/../page.js").is_some());
    assert!(graph.find("/app/page.js").is_none());
}

#[test]
fn graph_errors_name_the_module_they_refused() {
    let mut graph = DevGraph::new();
    let error = graph.insert("../secret.js", SERVER_HELPER).unwrap_err();

    assert!(error.to_string().contains("../secret.js"));
}
