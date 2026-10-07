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
    fake_sqlc_output(dir, exit, "", "")
}

#[cfg(unix)]
fn fake_sqlc_output(dir: &Path, exit: i32, stdout: &str, stderr: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let out_path = dir.join("fake-stdout");
    let err_path = dir.join("fake-stderr");
    fs::write(&out_path, stdout).expect("write stdout");
    fs::write(&err_path, stderr).expect("write stderr");
    let script = dir.join("fake-sqlc");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{log}\"\ncommand -v uf >> \"{log}\"\ncat \"{out}\"\ncat \"{err}\" >&2\nexit {exit}\n",
            log = dir.join("log").display(),
            out = out_path.display(),
            err = err_path.display(),
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

#[cfg(unix)]
#[test]
fn diff_reports_a_sqlc_failure_that_is_not_a_diff() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlc = fake_sqlc_output(
        dir.path(),
        1,
        "",
        "ERROR: syntax error at or near \"SELEC\"\n",
    );
    let output = uf()
        .args(["sqlc", "diff"])
        .current_dir(dir.path())
        .env("SQLC", &sqlc)
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sqlc diff failed"), "{stderr}");
    assert!(!stderr.contains("out of date"), "{stderr}");
    assert!(stderr.contains("syntax error"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn diff_reports_an_exit_other_than_1_as_a_sqlc_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlc = fake_sqlc_output(dir.path(), 2, "", "");
    let output = uf()
        .args(["sqlc", "diff"])
        .current_dir(dir.path())
        .env("SQLC", &sqlc)
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sqlc diff failed"), "{stderr}");
    assert!(!stderr.contains("out of date"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn diff_treats_a_unified_diff_as_stale_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sqlc = fake_sqlc_output(
        dir.path(),
        1,
        "--- a/query.sql.js\n+++ b/query.sql.js\n",
        "note: checked 1 file\n",
    );
    let output = uf()
        .args(["sqlc", "diff"])
        .current_dir(dir.path())
        .env("SQLC", &sqlc)
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("out of date"), "{stderr}");
    assert!(stderr.contains("checked 1 file"), "{stderr}");
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

#[test]
fn generate_names_uf_sqlc_install_when_sqlc_is_nowhere() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env_remove("SQLC")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("uf sqlc install"), "{stderr}");
}

#[test]
fn install_refuses_an_unknown_version_without_a_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = uf()
        .args(["sqlc", "install", "--version", "9.9.9"])
        .current_dir(dir.path())
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--sha256"), "{stderr}");
    assert!(!dir.path().join(".uf/sqlc/sqlc").exists());
}

#[test]
fn install_refuses_a_digest_that_is_not_the_pins() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = uf()
        .args(["sqlc", "install", "--sha256"])
        .arg("ab".repeat(32))
        .current_dir(dir.path())
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Omit `--sha256`"), "{stderr}");
    assert!(!dir.path().join(".uf/sqlc/sqlc").exists());
}

/// A stand-in sqlc at `.uf/sqlc/sqlc`, with `version` naming `version`.
#[cfg(unix)]
fn place_managed(dir: &Path, version: &str) -> PathBuf {
    let managed = dir.join(".uf").join("sqlc");
    fs::create_dir_all(&managed).expect("mkdir");
    let script = fake_sqlc(&managed, 0);
    let binary = managed.join("sqlc");
    fs::rename(&script, &binary).expect("rename");
    fs::write(managed.join("version"), format!("{version}\n")).expect("version");
    binary
}

#[cfg(unix)]
#[test]
fn generate_uses_the_pinned_copy_under_dot_uf_when_nothing_else_is_installed() {
    let dir = tempfile::tempdir().expect("tempdir");
    place_managed(dir.path(), "1.31.1");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env_remove("SQLC")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(dir.path().join(".uf/sqlc/log")).expect("log");
    assert!(log.starts_with("generate\n"), "{log}");
}

#[cfg(unix)]
#[test]
fn a_managed_sqlc_of_another_version_is_not_used() {
    let dir = tempfile::tempdir().expect("tempdir");
    place_managed(dir.path(), "1.30.0");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env_remove("SQLC")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("uf sqlc install"), "{stderr}");
    assert!(stderr.contains("1.30.0"), "{stderr}");
    assert!(!dir.path().join(".uf/sqlc/log").exists());
}

#[cfg(unix)]
#[test]
fn sqlc_on_path_wins_over_the_pinned_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    place_managed(dir.path(), "1.31.1");
    let path_dir = dir.path().join("bin");
    fs::create_dir_all(&path_dir).expect("mkdir");
    let script = fake_sqlc(&path_dir, 0);
    fs::rename(&script, path_dir.join("sqlc")).expect("rename");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env_remove("SQLC")
        .env("PATH", format!("{}:/usr/bin:/bin", path_dir.display()))
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(path_dir.join("log")).expect("log");
    assert!(log.starts_with("generate\n"), "{log}");
    assert!(!dir.path().join(".uf/sqlc/log").exists());
}

#[cfg(unix)]
#[test]
fn sqlc_env_wins_even_when_the_pinned_copy_is_installed() {
    let dir = tempfile::tempdir().expect("tempdir");
    place_managed(dir.path(), "1.31.1");
    let output = uf()
        .args(["sqlc", "generate"])
        .current_dir(dir.path())
        .env("SQLC", dir.path().join("missing-sqlc"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run uf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sqlc is not installed"), "{stderr}");
    assert!(!stderr.contains("uf sqlc install"), "{stderr}");
    assert!(!dir.path().join(".uf/sqlc/log").exists());
}

#[cfg(unix)]
#[test]
fn install_prints_the_path_and_skips_a_download_when_the_version_matches() {
    let dir = tempfile::tempdir().expect("tempdir");
    let binary = place_managed(dir.path(), "1.31.1");
    let before = fs::read(&binary).expect("script");
    let output = uf()
        .args(["sqlc", "install"])
        .current_dir(dir.path())
        .env_remove("SQLC")
        .output()
        .expect("run uf");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(stdout.lines().count(), 1, "{stdout:?}");
    assert!(stdout.ends_with('\n'));
    let printed = PathBuf::from(stdout.trim_end());
    assert!(printed.is_absolute(), "{printed:?}");
    assert_eq!(fs::read(&printed).expect("installed"), before);
    assert!(String::from_utf8_lossy(&output.stderr).contains("already installed"));
}
