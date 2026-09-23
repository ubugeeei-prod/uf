//! The decisions behind `uf editor`, without a process or an editor.
//!
//! `crates/uf_cli/tests/editor.rs` runs the command end to end, with fake
//! editor launchers and a release served from a directory. These are the
//! parts it is built from — the JSONC splicing above all, because a settings
//! file that comes back with a comment missing or a value changed is the
//! failure a reader would never forgive.

use serde_json::{Value, json};

use super::install::{self, Placed};
use super::jsonc::{self, Outcome, Want};
use super::setup::{self, FilePlan};
use crate::cli::Editor;

/// Read JSONC back as a value, the way an editor would.
fn read(text: &str) -> Value {
    json5::from_str(text).unwrap_or_else(|error| panic!("not JSONC ({error}):\n{text}"))
}

#[test]
fn a_missing_file_is_written_whole_in_the_order_asked() {
    let wants = [
        Want::value(&["javascript.validate.enable"], json!(false)),
        Want::value(&["[javascript]", "js/ts.validate.enabled"], json!(false)),
        Want::value(
            &["[javascript]", "editor.defaultFormatter"],
            json!("uniflowed.uf"),
        ),
    ];
    let edit = jsonc::edit(None, &wants).unwrap();
    assert_eq!(
        edit.text,
        "{\n  \"javascript.validate.enable\": false,\n  \"[javascript]\": {\n    \
         \"js/ts.validate.enabled\": false,\n    \"editor.defaultFormatter\": \"uniflowed.uf\"\n  }\n}\n"
    );
    assert_eq!(edit.outcomes, vec![Outcome::Added; 3]);
}

#[test]
fn an_existing_file_keeps_its_comments_its_order_and_its_values() {
    let before = r#"{
  // The team's choice: keep TypeScript's errors in this repository.
  "javascript.validate.enable": true,
  /* spacing */
  "editor.tabSize": 2,
  "[javascript]": {
    "editor.formatOnSave": false, // not yet
  },
}
"#;
    let wants = [
        Want::value(&["javascript.validate.enable"], json!(false)),
        Want::value(
            &["[javascript]", "editor.defaultFormatter"],
            json!("uniflowed.uf"),
        ),
        Want::value(&["[javascript]", "editor.formatOnSave"], json!(true)),
        Want::value(
            &["[javascriptreact]", "editor.defaultFormatter"],
            json!("uniflowed.uf"),
        ),
    ];
    let edit = jsonc::edit(Some(before), &wants).unwrap();

    assert_eq!(
        edit.outcomes,
        vec![
            Outcome::Kept {
                current: json!(true)
            },
            Outcome::Added,
            Outcome::Kept {
                current: json!(false)
            },
            Outcome::Added,
        ]
    );
    // Every comment and every original line survives, untouched.
    for line in before.lines() {
        assert!(
            edit.text.contains(line),
            "lost {line:?} from:\n{}",
            edit.text
        );
    }
    let after = read(&edit.text);
    assert_eq!(after["javascript.validate.enable"], json!(true));
    assert_eq!(after["editor.tabSize"], json!(2));
    assert_eq!(after["[javascript]"]["editor.formatOnSave"], json!(false));
    assert_eq!(
        after["[javascript]"]["editor.defaultFormatter"],
        json!("uniflowed.uf")
    );
    assert_eq!(
        after["[javascriptreact]"]["editor.defaultFormatter"],
        json!("uniflowed.uf")
    );
}

#[test]
fn a_second_run_adds_nothing() {
    let wants = setup::targets(Editor::Vscode)
        .into_iter()
        .find(|target| target.path == ".vscode/settings.json")
        .map(|target| match target.kind {
            setup::TargetKind::Jsonc(wants) => wants,
            setup::TargetKind::Whole { .. } => unreachable!(),
        })
        .unwrap();
    let first = jsonc::edit(Some("{\n  \"files.eol\": \"\\n\"\n}\n"), &wants).unwrap();
    assert!(first.changes());
    let second = jsonc::edit(Some(&first.text), &wants).unwrap();
    assert!(!second.changes(), "second run changed:\n{}", second.text);
    assert_eq!(second.text, first.text);
    assert!(
        second
            .outcomes
            .iter()
            .all(|outcome| *outcome == Outcome::Already)
    );
}

#[test]
fn an_empty_object_gets_members_without_a_dangling_comma() {
    let edit = jsonc::edit(
        Some("{}\n"),
        &[Want::value(&["a"], json!(1)), Want::value(&["b"], json!(2))],
    )
    .unwrap();
    assert_eq!(edit.text, "{\n  \"a\": 1,\n  \"b\": 2\n}\n");
}

#[test]
fn nested_objects_are_created_once_and_indented_where_they_go() {
    let before = "{\n  \"languages\": {\n    \"Python\": {}\n  }\n}\n";
    let wants = [
        Want::value(&["languages", "JavaScript", "format_on_save"], json!("on")),
        Want::value(
            &["languages", "JavaScript", "formatter"],
            json!({ "language_server": { "name": "uf" } }),
        ),
    ];
    let edit = jsonc::edit(Some(before), &wants).unwrap();
    assert_eq!(edit.outcomes, vec![Outcome::Added, Outcome::Added]);
    let after = read(&edit.text);
    assert_eq!(after["languages"]["Python"], json!({}));
    assert_eq!(
        after["languages"]["JavaScript"]["format_on_save"],
        json!("on")
    );
    assert_eq!(
        after["languages"]["JavaScript"]["formatter"]["language_server"]["name"],
        json!("uf")
    );
    assert!(
        edit.text
            .contains("\n    \"JavaScript\": {\n      \"format_on_save\": \"on\","),
        "{}",
        edit.text
    );
}

#[test]
fn a_recommendation_is_appended_to_the_projects_own() {
    let want = [Want::contains(&["recommendations"], json!("uniflowed.uf"))];
    let edit = jsonc::edit(
        Some("{ \"recommendations\": [\"dbaeumer.vscode-eslint\"] }"),
        &want,
    )
    .unwrap();
    assert_eq!(
        read(&edit.text)["recommendations"],
        json!(["uniflowed.uf", "dbaeumer.vscode-eslint"])
    );
    let again = jsonc::edit(Some(&edit.text), &want).unwrap();
    assert_eq!(again.outcomes, vec![Outcome::Already]);

    let empty = jsonc::edit(Some("{ \"recommendations\": [] }"), &want).unwrap();
    assert_eq!(
        read(&empty.text)["recommendations"],
        json!(["uniflowed.uf"])
    );
}

#[test]
fn something_else_where_an_object_is_needed_is_kept() {
    // The old array form of `editor.codeActionsOnSave`.
    let edit = jsonc::edit(
        Some("{ \"[javascript]\": { \"editor.codeActionsOnSave\": [\"source.fixAll\"] } }"),
        &[Want::value(
            &[
                "[javascript]",
                "editor.codeActionsOnSave",
                "source.fixAll.uf",
            ],
            json!("explicit"),
        )],
    )
    .unwrap();
    assert_eq!(
        edit.outcomes,
        vec![Outcome::Kept {
            current: json!(["source.fixAll"])
        }]
    );
    assert!(!edit.changes());
}

#[test]
fn text_that_is_not_jsonc_is_refused() {
    for broken in ["{ \"a\": }", "[1, 2]", "{ \"a\": 1 } trailing", "{ /* open"] {
        assert!(
            jsonc::edit(Some(broken), &[Want::value(&["b"], json!(1))]).is_err(),
            "{broken} was accepted"
        );
    }
}

#[test]
fn strings_with_braces_and_escapes_do_not_confuse_the_reader() {
    let before = r#"{ "a\"}": "{[//not a comment]}", "b": 1 }"#;
    let edit = jsonc::edit(Some(before), &[Want::value(&["b"], json!(1))]).unwrap();
    assert_eq!(edit.outcomes, vec![Outcome::Already]);
    assert_eq!(edit.text, before);
}

#[test]
fn vscode_gets_both_validation_names_and_cursor_only_the_one_it_has() {
    let keys = |editor| -> Vec<String> {
        setup::targets(editor)
            .into_iter()
            .flat_map(|target| match target.kind {
                setup::TargetKind::Jsonc(wants) => wants,
                setup::TargetKind::Whole { .. } => Vec::new(),
            })
            .map(|want| want.path.join(" "))
            .collect()
    };
    let vscode = keys(Editor::Vscode);
    assert!(vscode.contains(&"javascript.validate.enable".to_owned()));
    assert!(vscode.contains(&"[javascript] js/ts.validate.enabled".to_owned()));
    let cursor = keys(Editor::Cursor);
    assert!(cursor.contains(&"javascript.validate.enable".to_owned()));
    assert!(!cursor.iter().any(|key| key.contains("js/ts.")));
    // Nothing about TypeScript files.
    assert!(
        !vscode
            .iter()
            .any(|key| key.starts_with("typescript") || key.contains("[typescript"))
    );
}

#[test]
fn zed_turns_vtsls_off_and_keeps_every_other_server() {
    let target = &setup::targets(Editor::Zed)[0];
    let plan = setup::plan(target, None).unwrap();
    let FilePlan::Create { text } = plan else {
        panic!("expected a new file");
    };
    let servers = &read(&text)["languages"]["JavaScript"]["language_servers"];
    assert_eq!(
        servers,
        &json!(["uf", "!vtsls", "!typescript-language-server", "..."])
    );
}

#[test]
fn helix_is_appended_to_unless_javascript_is_already_configured() {
    let target = &setup::targets(Editor::Helix)[0];
    assert!(matches!(
        setup::plan(target, Some("[[language]]\nname = \"rust\"\n")).unwrap(),
        FilePlan::Append { .. }
    ));
    assert!(matches!(
        setup::plan(target, Some("[[language]]\nname = \"javascript\"\n")).unwrap(),
        FilePlan::Conflict { .. }
    ));
    let written = setup::plan(target, None).unwrap().result(None).unwrap();
    assert_eq!(
        setup::plan(target, Some(&written)).unwrap(),
        FilePlan::Already
    );
}

#[test]
fn an_emacs_dir_locals_is_never_merged_into() {
    let target = &setup::targets(Editor::Emacs)[0];
    assert!(matches!(
        setup::plan(target, Some("((nil . ((indent-tabs-mode . nil))))\n")).unwrap(),
        FilePlan::Conflict { .. }
    ));
}

#[test]
fn jetbrains_and_vim_have_no_file_uf_writes() {
    assert!(setup::targets(Editor::Jetbrains).is_empty());
    assert!(setup::targets(Editor::Vim).is_empty());
}

#[test]
fn a_release_asset_is_on_github_unless_a_base_replaces_the_host() {
    assert_eq!(
        install::asset_url(None, "0.2.0", "uf-vscode-0.2.0.vsix"),
        "https://github.com/ubugeeei-prod/uf/releases/download/uf@0.2.0/uf-vscode-0.2.0.vsix"
    );
    assert_eq!(
        install::asset_url(Some("file:///tmp/r/"), "0.2.0", "uf-vscode-0.2.0.vsix"),
        "file:///tmp/r/0.2.0/uf-vscode-0.2.0.vsix"
    );
    assert_eq!(install::normalize_version("uf@0.2.0"), "0.2.0");
    assert_eq!(install::normalize_version(" v0.2.0 "), "0.2.0");
}

#[test]
fn a_checksum_file_has_to_state_a_digest() {
    let digest = install::sha256_hex(b"abc");
    assert_eq!(
        digest,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        install::stated_digest(&format!("{digest}  uf-vscode-0.2.0.vsix\n")),
        Some(digest.clone())
    );
    assert_eq!(install::stated_digest(&digest.to_uppercase()), Some(digest));
    assert_eq!(install::stated_digest("Not Found"), None);
    assert_eq!(install::stated_digest(""), None);
}

#[test]
fn a_file_someone_else_wrote_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("nvim/lua/uf.lua")).unwrap();
    let ours = "-- uf for Neovim: start\nlocal M = {}\n";

    assert_eq!(install::place(&path, ours, false).unwrap(), Placed::Written);
    assert_eq!(
        install::place(&path, ours, false).unwrap(),
        Placed::Unchanged
    );
    // An older copy uf wrote starts with the same line, and is brought up to date.
    std::fs::write(&path, "-- uf for Neovim: start\nlocal old = 1\n").unwrap();
    assert_eq!(install::place(&path, ours, false).unwrap(), Placed::Updated);
    // Somebody's own file of the same name is refused, then replaced on --force.
    std::fs::write(&path, "-- my own uf helpers\n").unwrap();
    let refused = install::place(&path, ours, false).unwrap_err().to_string();
    assert!(refused.contains("--force"), "{refused}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "-- my own uf helpers\n"
    );
    assert_eq!(install::place(&path, ours, true).unwrap(), Placed::Updated);
}

#[test]
fn the_embedded_integrations_are_the_files_under_editors() {
    // A file moved under `editors/` fails to compile `assets.rs`; this holds
    // that what is compiled in is what `place` recognises as uf's own.
    assert!(super::assets::NEOVIM.starts_with("-- uf for Neovim"));
    assert!(super::assets::VIM.starts_with("\" uf for Vim"));
    assert!(super::assets::EMACS.starts_with(";;; uf.el"));
    assert!(super::assets::HELIX.contains("[language-server.uf]"));
    assert!(
        super::assets::ZED
            .iter()
            .any(|asset| asset.path == "extension.toml"
                && asset.contents.contains("[language_servers.uf]"))
    );
}

#[test]
fn directories_follow_xdg_and_fall_back_to_home() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    };
    let xdg = env(&[
        ("XDG_DATA_HOME", "/x/data"),
        ("XDG_CONFIG_HOME", "/x/config"),
        ("HOME", "/h"),
    ]);
    assert_eq!(install::data_dir(&xdg).unwrap(), "/x/data/uf/editors");
    assert_eq!(install::config_dir(&xdg).unwrap(), "/x/config");
    let home = env(&[("HOME", "/h")]);
    assert_eq!(
        install::data_dir(&home).unwrap(),
        "/h/.local/share/uf/editors"
    );
    assert_eq!(install::config_dir(&home).unwrap(), "/h/.config");
    assert!(install::data_dir(&env(&[])).is_err());
}
