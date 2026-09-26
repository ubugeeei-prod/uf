//! Read the live local dev channel. This never starts or executes an application.
use crate::ui::Ui;
use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use serde_json::{Value, json};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn read(cwd: &Utf8Path, ui: &mut Ui, tool: &str, id: Option<&str>) -> Result<()> {
    let root = uf_config::discover_root(cwd);
    let file = root.join(".uf/dev-state.json");
    let metadata = fs::symlink_metadata(&file)
        .context("no running uf dev diagnostic channel; start uf dev in this project")?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 4 * 1024 * 1024,
        uf_infra::cstr!("invalid dev diagnostic snapshot")
    );
    let canonical = fs::canonicalize(&file)?;
    ensure!(
        canonical.starts_with(fs::canonicalize(&root)?),
        uf_infra::cstr!("dev diagnostic channel leaves this project")
    );
    let state: Value = serde_json::from_str(&fs::read_to_string(&file)?)?;
    ensure!(
        state["schema"] == 1,
        uf_infra::cstr!("unsupported dev diagnostic schema")
    );
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let updated = state["updatedAt"]
        .as_u64()
        .context("missing dev snapshot timestamp")?;
    ensure!(
        u128::from(updated) <= now + 5_000 && now.saturating_sub(u128::from(updated)) < 30_000,
        uf_infra::cstr!("dev diagnostic snapshot is stale; restart uf dev")
    );
    let result = match tool {
        "uf_dev_errors" => {
            json!({ "session":state["session"], "generation":state["generation"], "updatedAt":updated, "errors":state["errors"], "metadataError":state["metadataError"] })
        }
        "uf_dev_logs" => {
            json!({ "session":state["session"], "updatedAt":updated, "logs":state["logs"] })
        }
        "uf_dev_routes" => {
            json!({ "routes":state["routes"], "truncated":state["truncated"], "metadataError":state["metadataError"] })
        }
        "uf_dev_action" => {
            let id = id.context("action id is required")?;
            ensure!(
                id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
                uf_infra::cstr!("action id must be 64 hexadecimal characters")
            );
            let action = state["actions"]
                .as_array()
                .context("missing action table")?
                .iter()
                .find(|a| a["id"] == id)
                .context(
                    "action id is not present in this dev generation; rebuild the client reference",
                )?;
            json!({ "action": action, "generation":state["generation"] })
        }
        _ => anyhow::bail!(uf_infra::cstr!("unknown dev tool {tool}")),
    };
    ui.json(&result)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::OutputMode;

    #[test]
    fn reads_generation_and_action_but_refuses_stale_or_external_state() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::create_dir(root.join(".uf")).unwrap();
        let file = root.join(".uf/dev-state.json");
        let id = "a".repeat(64);
        let mut state = json!({"schema":1,"updatedAt":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),"generation":3,"session":"test","errors":[{"kind":"runtime","requestId":"request"}],"actions":[{"id":id,"module":"app/action.js","export":"save"}]});
        fs::write(&file, state.to_string()).unwrap();
        let mut ui = Ui::capturing(OutputMode::Json);
        read(root, &mut ui, "uf_dev_errors", None).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&ui.take_captured()).unwrap()["generation"],
            3
        );
        read(root, &mut ui, "uf_dev_action", Some(&id)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&ui.take_captured()).unwrap()["action"]["export"],
            "save"
        );
        assert!(read(root, &mut ui, "uf_dev_action", Some("bad")).is_err());
        assert!(read(root, &mut ui, "uf_dev_action", Some(&"b".repeat(64))).is_err());
        state["updatedAt"] = json!(0);
        fs::write(&file, state.to_string()).unwrap();
        assert!(
            read(root, &mut ui, "uf_dev_errors", None)
                .unwrap_err()
                .to_string()
                .contains("stale")
        );
        #[cfg(unix)]
        {
            fs::remove_file(&file).unwrap();
            std::os::unix::fs::symlink(root.join("package.json"), &file).unwrap();
            assert!(read(root, &mut ui, "uf_dev_logs", None).is_err());
        }
    }
}
