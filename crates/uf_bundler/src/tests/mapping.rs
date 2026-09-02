//! Source maps: shape, contents, and the line table behind them.

use serde_json::Value;

use super::fixture::Fixture;
use crate::sourcemap::{LineOrigin, SourceMapBuilder};

#[test]
fn no_source_map_is_emitted_when_the_build_does_not_ask_for_one() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const a = 1;\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].source_map.is_none());
    assert!(!output.chunks[0].code.contains("sourceMappingURL"));
    fixture.keep();
}

#[test]
fn a_source_map_is_emitted_when_the_build_asks_for_one() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry("app.js", "export const a = 1;\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].source_map.is_some());
    assert!(output.chunks[0].code.contains("//# sourceMappingURL="));
    fixture.keep();
}

#[test]
fn the_source_map_names_the_chunk_it_belongs_to() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry("app.js", "export const a = 1;\n");

    let output = fixture.bundle();

    let map: Value =
        serde_json::from_str(output.chunks[0].source_map.as_ref().expect("map")).expect("json");
    let base = output.chunks[0]
        .file_name
        .rsplit('/')
        .next()
        .expect("base name");
    assert_eq!(map["version"], 3);
    assert_eq!(map["file"], base);
    fixture.keep();
}

#[test]
fn the_source_map_carries_every_module_source() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.write("util.js", "export const helper = 1;\n");
    fixture.entry(
        "app.js",
        "import { helper } from \"./util.js\";\nexport const a = helper;\n",
    );

    let output = fixture.bundle();

    let map: Value =
        serde_json::from_str(output.chunks[0].source_map.as_ref().expect("map")).expect("json");
    let sources: Vec<&str> = map["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|value| value.as_str().expect("string"))
        .collect();
    assert_eq!(sources, vec!["util.js", "app.js"]);
    assert!(
        map["sourcesContent"][0]
            .as_str()
            .expect("content")
            .contains("export const helper")
    );
    fixture.keep();
}

#[test]
fn the_source_map_holds_the_flow_source_not_the_stripped_one() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry("app.js", "// @flow\nexport const a: number = 1;\n");

    let output = fixture.bundle();

    let map: Value =
        serde_json::from_str(output.chunks[0].source_map.as_ref().expect("map")).expect("json");
    assert!(
        map["sourcesContent"][0]
            .as_str()
            .expect("content")
            .contains(": number")
    );
    fixture.keep();
}

#[test]
fn the_mappings_string_has_one_entry_per_output_line() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry("app.js", "export const a = 1;\nexport const b = 2;\n");

    let output = fixture.bundle();

    let map: Value =
        serde_json::from_str(output.chunks[0].source_map.as_ref().expect("map")).expect("json");
    let mappings = map["mappings"].as_str().expect("mappings");
    let code_lines = output.chunks[0]
        .code
        .lines()
        .filter(|line| !line.starts_with("//# sourceMappingURL"))
        .count();
    assert_eq!(mappings.split(';').count(), code_lines);
    fixture.keep();
}

#[test]
fn an_empty_builder_produces_an_empty_mapping() {
    let builder = SourceMapBuilder::new();

    let map: Value = serde_json::from_str(&builder.finish("chunk.js")).expect("json");

    assert_eq!(map["mappings"], "");
    assert_eq!(map["sources"].as_array().expect("sources").len(), 0);
}

#[test]
fn a_generated_line_maps_to_nothing() {
    let mut builder = SourceMapBuilder::new();
    builder.generated_line();
    builder.generated_line();

    let map: Value = serde_json::from_str(&builder.finish("chunk.js")).expect("json");

    assert_eq!(map["mappings"], ";");
}

#[test]
fn the_first_mapped_line_encodes_a_zero_delta() {
    let mut builder = SourceMapBuilder::new();
    let source = builder.add_source("a.js", "const a = 1;\n");
    builder.mapped_line(LineOrigin { source, line: 0 });

    let map: Value = serde_json::from_str(&builder.finish("chunk.js")).expect("json");

    assert_eq!(map["mappings"], "AAAA");
}

#[test]
fn consecutive_source_lines_encode_a_delta_of_one() {
    let mut builder = SourceMapBuilder::new();
    let source = builder.add_source("a.js", "const a = 1;\nconst b = 2;\n");
    builder.mapped_line(LineOrigin { source, line: 0 });
    builder.mapped_line(LineOrigin { source, line: 1 });

    let map: Value = serde_json::from_str(&builder.finish("chunk.js")).expect("json");

    assert_eq!(map["mappings"], "AAAA;AACA");
}

#[test]
fn a_second_source_encodes_a_source_delta() {
    let mut builder = SourceMapBuilder::new();
    let first = builder.add_source("a.js", "const a = 1;\n");
    let second = builder.add_source("b.js", "const b = 2;\n");
    builder.mapped_line(LineOrigin {
        source: first,
        line: 0,
    });
    builder.mapped_line(LineOrigin {
        source: second,
        line: 0,
    });

    let map: Value = serde_json::from_str(&builder.finish("chunk.js")).expect("json");

    assert_eq!(map["mappings"], "AAAA;ACAA");
    assert_eq!(builder_line_count(), 0);
}

fn builder_line_count() -> usize {
    SourceMapBuilder::new().line_count()
}

#[test]
fn writing_a_bundle_puts_the_map_beside_the_chunk() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry("app.js", "export const a = 1;\n");

    let output = fixture.bundle();
    let written = crate::write_bundle(&fixture.options(), &output, &fixture.container())
        .expect("write succeeds");

    assert_eq!(written.len(), 2);
    assert!(
        written
            .iter()
            .any(|path| path.as_str().ends_with(".js.map"))
    );
    fixture.keep();
}

#[test]
fn a_module_line_maps_back_to_the_same_line_after_erasure() {
    let mut fixture = Fixture::new();
    fixture.with_sourcemap();
    fixture.entry(
        "app.js",
        "// @flow\ntype Id = string;\nexport const a: Id = \"x\";\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    let map: Value =
        serde_json::from_str(output.chunks[0].source_map.as_ref().expect("map")).expect("json");
    let mappings: Vec<&str> = map["mappings"]
        .as_str()
        .expect("mappings")
        .split(';')
        .collect();
    let body = code
        .lines()
        .position(|line| line.contains("= \"x\";"))
        .expect("the declaration line");
    // The third source line, one line on from the mapping before it.
    assert_eq!(mappings[body], "AACA");
    fixture.keep();
}
