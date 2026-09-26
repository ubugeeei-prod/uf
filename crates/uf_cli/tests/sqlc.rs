#![allow(clippy::disallowed_macros)]

//! `uf` as sqlc's process plugin, and `uf sqlc`.
//!
//! The plugin half is exercised the way sqlc calls it: the argument
//! `/plugin.CodegenService/Generate`, a request sqlc really sent (captured in
//! `tests/sqlc/cases`) on stdin, and the response read back and compared with
//! the golden files `crates/uf_sqlc/tests/golden.rs` also checks. So what a
//! user's `sqlc generate` writes is what the goldens hold, byte for byte.
//!
//! `uf sqlc` is exercised against a stand-in `sqlc` that records how it was
//! started; `tools/ci/sqlc.sh` runs the real one over every case.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::uf;

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/sqlc/cases")
}

/// Read a `GenerateResponse`: `repeated File files = 1`, each `{ name = 1, contents = 2 }`.
fn files(response: &[u8]) -> Vec<(String, String)> {
    fn varint(bytes: &[u8], at: &mut usize) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = bytes[*at];
            *at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    fn fields(bytes: &[u8]) -> Vec<(u64, &[u8])> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let key = varint(bytes, &mut at);
            assert_eq!(key & 7, 2, "every field of a response is length-delimited");
            let len = usize::try_from(varint(bytes, &mut at)).expect("length");
            out.push((key >> 3, &bytes[at..at + len]));
            at += len;
        }
        out
    }
    fields(response)
        .into_iter()
        .map(|(number, file)| {
            assert_eq!(number, 1);
            let mut name = String::new();
            let mut contents = String::new();
            for (field, value) in fields(file) {
                let text = String::from_utf8(value.to_vec()).expect("utf-8");
                match field {
                    1 => name = text,
                    2 => contents = text,
                    other => panic!("unexpected field {other}"),
                }
            }
            (name, contents)
        })
        .collect()
}

#[test]
fn answers_sqlcs_protobuf_call_with_the_golden_files() {
    let case = cases().join("types-sqlite");
    let output = uf()
        .arg("/plugin.CodegenService/Generate")
        .write_stdin(fs::read(case.join("request.pb")).expect("request.pb"))
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let generated = files(&output.stdout);
    assert!(!generated.is_empty());
    for (name, contents) in generated {
        let golden = fs::read_to_string(case.join("gen").join(&name)).expect("golden");
        assert_eq!(contents, golden, "{name} differs from the golden file");
    }
}

#[test]
fn answers_a_json_call_in_json() {
    let output = uf()
        .arg("/plugin.CodegenService/Generate")
        .write_stdin(fs::read(cases().join("authors-postgresql/request.json")).expect("request"))
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let names: Vec<&str> = response["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["models.js", "query.sql.js"]);
}

#[test]
fn a_request_it_cannot_generate_is_an_error_on_stderr_and_nothing_on_stdout() {
    let mut request: serde_json::Value = serde_json::from_slice(
        &fs::read(cases().join("authors-sqlite/request.json")).expect("request"),
    )
    .expect("json");
    // `timestampz`: the misspelling the options refuse rather than ignore.
    request["plugin_options"] = serde_json::Value::String("eyJ0aW1lc3RhbXB6IjoiRGF0ZSJ9".into());
    let output = uf()
        .arg("/plugin.CodegenService/Generate")
        .write_stdin(serde_json::to_vec(&request).expect("encode"))
        .output()
        .expect("run uf");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timestampz"), "{stderr}");
}

#[cfg(unix)]
fn fake_sqlc(dir: &Path, exit: i32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-sqlc");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{log}\"\ncommand -v uf >> \"{log}\"\nexit {exit}\n",
            log = dir.join("log").display()
        ),
    )
    .expect("write fake sqlc");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    script
}

#[cfg(unix)]
#[test]
fn generate_runs_sqlc_with_this_uf_first_on_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlc = fake_sqlc(dir.path(), 0);
    let output = uf()
        .args(["sqlc", "generate", "-f", "db/sqlc.yaml"])
        .current_dir(dir.path())
        .env("SQLC", &sqlc)
        // An older `uf` earlier on PATH is what the prefix is for.
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(dir.path().join("log")).expect("log");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines[0], "generate");
    assert_eq!(lines[1], "-f");
    assert!(lines[2].ends_with("db/sqlc.yaml") && Path::new(lines[2]).is_absolute());
    let this_uf = assert_cmd::cargo::cargo_bin("uf");
    assert_eq!(
        fs::canonicalize(lines[3]).expect("uf on PATH"),
        fs::canonicalize(this_uf).expect("this uf")
    );
}

#[cfg(unix)]
#[test]
fn diff_fails_when_sqlc_says_the_files_are_stale() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlc = fake_sqlc(dir.path(), 1);
    let output = uf()
        .args(["sqlc", "diff"])
        .current_dir(dir.path())
        .env("SQLC", &sqlc)
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("out of date"));
    assert_eq!(
        fs::read_to_string(dir.path().join("log"))
            .expect("log")
            .lines()
            .next(),
        Some("diff")
    );
}

#[test]
fn says_how_to_get_sqlc_when_there_is_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env("SQLC", dir.path().join("no-such-sqlc"))
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("sqlc is not installed"));
}
