#![allow(clippy::disallowed_macros)]

//! Golden tests over `tests/sqlc/cases`: sqlc's own examples (authors,
//! booktest, jets, ondeck, batch) for every engine sqlc ships them for, and
//! uf's cases for types, commands and options.
//!
//! Each case holds the request sqlc sent (`request.json`, captured by
//! `tests/sqlc/capture.mjs`) and the files `uf` generated from it (`gen/`).
//! The same `gen/` is what `tests/sqlc` type-checks, lints and runs against
//! real databases, so a change here is a change those tests see.
//!
//! `UF_SQLC_BLESS=1 cargo test -p uf_sqlc --test golden` rewrites `gen/`.

use std::fs;
use std::path::{Path, PathBuf};

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/sqlc/cases")
}

fn format(source: &str) -> Result<String, String> {
    let mut config = uf_config::FmtConfig::default();
    config.align = false;
    uf_fmt::format_source(source, &config)
        .map(|result| result.output)
        .map_err(|error| error.to_string())
}

#[test]
fn every_case_matches_its_golden_output() {
    let bless = std::env::var_os("UF_SQLC_BLESS").is_some();
    let mut failures = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(cases())
        .expect("tests/sqlc/cases")
        .flatten()
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    assert!(entries.len() >= 15, "the cases went missing");
    for entry in entries {
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let request = fs::read(dir.join("request.json")).expect("request.json");
        let request = uf_sqlc::decode(&request).unwrap_or_else(|error| panic!("{name}: {error}"));
        let files =
            uf_sqlc::generate(&request, &format).unwrap_or_else(|error| panic!("{name}: {error}"));
        let gen_dir = dir.join("gen");
        if bless {
            let _ = fs::remove_dir_all(&gen_dir);
            fs::create_dir_all(&gen_dir).expect("create gen");
            for file in &files {
                fs::write(gen_dir.join(&file.name), &file.contents).expect("write golden");
            }
            continue;
        }
        let mut on_disk: Vec<String> = fs::read_dir(&gen_dir)
            .map(|read| {
                read.flatten()
                    .map(|file| file.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        on_disk.sort();
        let mut generated: Vec<String> = files.iter().map(|file| file.name.clone()).collect();
        generated.sort();
        if on_disk != generated {
            failures.push(format!(
                "{name}: files {generated:?}, golden has {on_disk:?}"
            ));
            continue;
        }
        for file in &files {
            let golden = fs::read_to_string(gen_dir.join(&file.name)).unwrap_or_default();
            if golden != file.contents {
                failures.push(format!(
                    "{name}/{}:\n{}",
                    file.name,
                    similar_asserts::SimpleDiff::from_str(
                        &golden,
                        &file.contents,
                        "golden",
                        "generated"
                    )
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "rerun with UF_SQLC_BLESS=1 if intended:\n{}",
        failures.join("\n")
    );
}

#[test]
fn generated_files_are_already_formatted_and_signed() {
    for entry in fs::read_dir(cases()).expect("cases").flatten() {
        let request =
            uf_sqlc::decode(&fs::read(entry.path().join("request.json")).expect("request"))
                .expect("decode");
        for file in uf_sqlc::generate(&request, &format).expect("generate") {
            assert!(
                file.contents.contains("@generated SignedSource<<"),
                "{} is not signed",
                file.name
            );
            // The signature is over the file with the token in place, so
            // putting the token back and signing again gives the same file.
            let start = file.contents.find("SignedSource<<").expect("signature");
            let end = start + file.contents[start..].find(">>").expect("end") + 2;
            let unsigned = format!(
                "{}{}{}",
                &file.contents[..start],
                uf_sqlc::signed::TOKEN,
                &file.contents[end..]
            );
            assert_eq!(uf_sqlc::signed::sign(&unsigned), file.contents);
            // And `uf fmt` would leave it alone even without the signature.
            assert_eq!(
                format(&unsigned).expect("format"),
                unsigned,
                "{} is not formatted",
                file.name
            );
        }
    }
}

#[test]
fn protobuf_and_json_requests_decode_to_the_same_thing() {
    for name in ["types-sqlite", "types-mysql"] {
        let dir = cases().join(name);
        let json = uf_sqlc::decode(&fs::read(dir.join("request.json")).expect("json"))
            .expect("decode json");
        let protobuf =
            uf_sqlc::decode(&fs::read(dir.join("request.pb")).expect("pb")).expect("decode pb");
        assert_eq!(protobuf, json, "{name}");
    }
}

#[test]
fn a_protobuf_response_round_trips_through_the_plugin_entry_point() {
    let input = fs::read(cases().join("types-sqlite/request.pb")).expect("pb");
    let output = uf_sqlc::run_plugin(&input, &format).expect("run");
    // A response is `repeated File files = 1`: every top-level field is
    // field 1, length-delimited.
    assert_eq!(output.first(), Some(&0x0a));
    let json_input = fs::read(cases().join("types-sqlite/request.json")).expect("json");
    let json_output = uf_sqlc::run_plugin(&json_input, &format).expect("run json");
    let parsed: serde_json::Value = serde_json::from_slice(&json_output).expect("json response");
    assert!(
        parsed["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
    );
}

#[test]
fn unknown_options_are_refused() {
    let mut request =
        uf_sqlc::decode(&fs::read(cases().join("authors-sqlite/request.json")).expect("json"))
            .expect("decode");
    request.plugin_options = br#"{"timestampz":"string"}"#.to_vec();
    let error = uf_sqlc::generate(&request, &format).expect_err("a misspelt option");
    assert!(error.contains("timestampz"), "{error}");
}

fn generated(case: &str, options: &str) -> String {
    let mut request =
        uf_sqlc::decode(&fs::read(cases().join(case).join("request.json")).expect("json"))
            .expect("decode");
    request.plugin_options = options.as_bytes().to_vec();
    uf_sqlc::generate(&request, &format)
        .expect("generate")
        .into_iter()
        .find(|file| file.name.ends_with("query.sql.js"))
        .expect("query file")
        .contents
}

#[test]
fn a_model_named_queryable_keeps_one_binding() {
    let query = generated("authors-sqlite", r#"{"rename":{"authors":"Queryable"}}"#);
    assert!(
        query.contains("import type { Queryable } from \"./models.js\";"),
        "{query}"
    );
    assert!(query.contains("Queryable as QueryableRuntime"), "{query}");
    assert!(query.contains("db: QueryableRuntime"), "{query}");
    assert!(
        !query.contains("import type { Queryable } from \"@uniflowed/sql\";")
            && !query.contains("import type { ExecResult, Queryable }"),
        "{query}"
    );
    let bare = query.matches("import type { Queryable }").count();
    assert_eq!(bare, 1, "{query}");

    let sync = generated(
        "authors-sqlite",
        r#"{"sync":true,"rename":{"authors":"SyncQueryable"}}"#,
    );
    assert!(
        sync.contains("import type { SyncQueryable } from \"./models.js\";"),
        "{sync}"
    );
    assert!(
        sync.contains("SyncQueryable as SyncQueryableRuntime"),
        "{sync}"
    );
    assert!(sync.contains("db: SyncQueryableRuntime"), "{sync}");
    assert_eq!(
        sync.matches("import type { SyncQueryable }").count(),
        1,
        "{sync}"
    );
}

#[test]
fn sync_option_emits_synchronous_functions() {
    let query = generated("authors-sqlite", r#"{"sync":true}"#);
    assert!(query.contains("SyncQueryable"), "{query}");
    assert!(query.contains(".oneSync("), "{query}");
    assert!(query.contains(".execSync("), "{query}");
    assert!(!query.contains("Promise<"), "{query}");
    assert!(!query.contains(".one("), "{query}");
    assert!(!query.contains(": Queryable"), "{query}");

    let batch = generated("types-sqlite", r#"{"sync":true}"#);
    assert!(batch.contains(".copyFromSync("), "{batch}");
    assert!(batch.contains(".batchSync("), "{batch}");
    assert!(batch.contains(".manySync("), "{batch}");
    assert!(!batch.contains("Promise<"), "{batch}");
    assert!(!batch.contains(".then("), "{batch}");
}
