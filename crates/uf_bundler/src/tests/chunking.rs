//! Which module lands in which chunk, and what a browser is allowed to load.

use super::fixture::{Fixture, assert_chunks_parse, chunk_named};
use crate::chunk::{ChunkEnvironment, ChunkKind};

/// A project with two routes, a shared helper and a client boundary.
fn app() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.write("shared.js", "export const shared = () => 1;\n");
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\nimport { shared } from \"../../shared.js\";\nexport default function Counter() {\n  return shared();\n}\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "import { shared } from \"../shared.js\";\nimport Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return [shared(), Counter];\n}\n",
    );
    fixture.entry(
        "app/about/_uf.page.js",
        "import { shared } from \"../../shared.js\";\nexport default function About() {\n  return shared();\n}\n",
    );
    fixture
}

#[test]
fn every_route_entry_gets_its_own_chunk() {
    let fixture = app();

    let output = fixture.bundle();

    let names = output
        .chunks
        .iter()
        .filter(|chunk| matches!(chunk.kind, ChunkKind::Entry { .. }))
        .count();
    assert_eq!(names, 2);
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_client_boundary_becomes_its_own_chunk() {
    let fixture = app();

    let output = fixture.bundle();

    let client = output
        .chunks
        .iter()
        .find(|chunk| matches!(chunk.kind, ChunkKind::Client { .. }))
        .expect("a client chunk");
    assert_eq!(client.environment, ChunkEnvironment::Client);
    assert!(
        client
            .modules
            .iter()
            .any(|path| path.as_str() == "app/client/Counter.js")
    );
    fixture.keep();
}

#[test]
fn a_module_two_roots_reach_goes_in_a_shared_chunk() {
    let fixture = app();

    let output = fixture.bundle();

    let shared = output
        .chunks
        .iter()
        .find(|chunk| chunk.kind == ChunkKind::Shared)
        .expect("a shared chunk");
    assert!(
        shared
            .modules
            .iter()
            .any(|path| path.as_str() == "shared.js")
    );
    fixture.keep();
}

#[test]
fn a_client_chunk_never_holds_a_server_only_module() {
    let mut fixture = Fixture::new();
    fixture.write(
        "server/db.server.js",
        "export const query = () => \"secret\";\n",
    );
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\nexport default function Counter() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "import { query } from \"../server/db.server.js\";\nimport Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return [query(), Counter];\n}\n",
    );

    let output = fixture.bundle();

    for chunk in &output.chunks {
        if chunk.environment != ChunkEnvironment::Client {
            continue;
        }
        assert!(
            !chunk
                .modules
                .iter()
                .any(|path| path.as_str() == "server/db.server.js"),
            "server-only module reached the client chunk {}",
            chunk.file_name
        );
        assert!(!chunk.code.contains("secret"), "{}", chunk.code);
    }
    fixture.keep();
}

#[test]
fn a_server_only_module_stays_in_a_server_chunk() {
    let mut fixture = Fixture::new();
    fixture.write("server/db.server.js", "export const query = () => 1;\n");
    fixture.entry(
        "app/_uf.page.js",
        "import { query } from \"../server/db.server.js\";\nexport default function Page() {\n  return query();\n}\n",
    );

    let output = fixture.bundle();

    let chunk = output
        .chunk_of(camino::Utf8Path::new("server/db.server.js"))
        .expect("the module is in a chunk");
    assert_eq!(chunk.environment, ChunkEnvironment::Server);
    fixture.keep();
}

#[test]
fn a_module_only_one_route_reaches_stays_in_that_route_chunk() {
    let mut fixture = Fixture::new();
    fixture.write("only.js", "export const only = 1;\n");
    fixture.entry(
        "app/_uf.page.js",
        "import { only } from \"../only.js\";\nexport default function Page() {\n  return only;\n}\n",
    );
    fixture.entry(
        "app/about/_uf.page.js",
        "export default function About() {\n  return 2;\n}\n",
    );

    let output = fixture.bundle();

    assert!(
        output
            .chunks
            .iter()
            .all(|chunk| chunk.kind != ChunkKind::Shared)
    );
    let chunk = output
        .chunk_of(camino::Utf8Path::new("only.js"))
        .expect("the module is in a chunk");
    assert!(
        chunk
            .modules
            .iter()
            .any(|path| path.as_str() == "app/_uf.page.js")
    );
    fixture.keep();
}

#[test]
fn a_chunk_imports_the_chunks_it_depends_on() {
    let fixture = app();

    let output = fixture.bundle();

    let entry = chunk_named(&output, "entry-app-_uf");
    assert!(!entry.imports.is_empty());
    for name in &entry.imports {
        assert!(
            entry.code.contains(name.rsplit('/').next().unwrap()),
            "{}",
            entry.code
        );
    }
    fixture.keep();
}

#[test]
fn the_closure_of_a_route_lists_every_chunk_it_needs() {
    let fixture = app();

    let output = fixture.bundle();

    let closure = output.closure_of(camino::Utf8Path::new("app/_uf.page.js"));
    assert!(closure.len() >= 2, "{closure:?}");
    assert!(closure.windows(2).all(|pair| pair[0] <= pair[1]));
    fixture.keep();
}

#[test]
fn modules_inside_a_chunk_come_after_the_modules_they_import() {
    let mut fixture = Fixture::new();
    fixture.write("deep.js", "export const deep = 1;\n");
    fixture.write(
        "mid.js",
        "import { deep } from \"./deep.js\";\nexport const mid = deep + 1;\n",
    );
    fixture.entry(
        "app.js",
        "import { mid } from \"./mid.js\";\nexport const top = mid + 1;\n",
    );

    let output = fixture.bundle();

    let modules: Vec<&str> = output.chunks[0]
        .modules
        .iter()
        .map(|path| path.as_str())
        .collect();
    assert_eq!(modules, vec!["deep.js", "mid.js", "app.js"]);
    fixture.keep();
}

#[test]
fn chunks_come_back_sorted_by_file_name() {
    let fixture = app();

    let output = fixture.bundle();

    let names: Vec<&str> = output
        .chunks
        .iter()
        .map(|chunk| chunk.file_name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    fixture.keep();
}

#[test]
fn a_chunk_name_is_derived_from_its_root_module_path() {
    let mut fixture = Fixture::new();
    fixture.entry("app/deep/page.js", "export const a = 1;\n");

    let output = fixture.bundle();

    assert!(
        output.chunks[0]
            .file_name
            .starts_with("assets/entry-app-deep-page-js-"),
        "{}",
        output.chunks[0].file_name
    );
    fixture.keep();
}

#[test]
fn a_client_root_reached_from_two_routes_is_still_one_chunk() {
    let mut fixture = Fixture::new();
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\nexport default function Counter() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "import Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return Counter;\n}\n",
    );
    fixture.entry(
        "app/about/_uf.page.js",
        "import Counter from \"../client/Counter.js\";\nexport default function About() {\n  return Counter;\n}\n",
    );

    let output = fixture.bundle();

    let client = output
        .chunks
        .iter()
        .filter(|chunk| matches!(chunk.kind, ChunkKind::Client { .. }))
        .count();
    assert_eq!(client, 1);
    assert_chunks_parse(&output);
    fixture.keep();
}
