//! Which release of uf a project runs on: `uf: "0.3.0"`.
//!
//! # Why a project pins uf
//!
//! `runtime: "node@26"` and `packageManager: "pnpm@12.0.0"` say which Node and
//! which pnpm a project runs on, and nothing said which uf. Two people on the
//! same commit ran whatever `~/.local/bin/uf` each had last updated to, and a
//! CI job ran whatever the installer called `latest` that morning — so a lint
//! rule, a formatter's output or a build's shape changed under a project that
//! had changed nothing. `rust-toolchain.toml` is the answer Rust gave the same
//! question, and this is that answer for uf:
//!
//! ```js
//! export default defineConfig({
//!   uf: "0.3.0",
//! });
//! ```
//!
//! Any uf started inside the project — `uf`, `ufr` or `ufx`, whichever release
//! is on `PATH` — hands the command line to `uf@0.3.0` from the installer's
//! store, installing it first if the machine does not have it. The switch is
//! per invocation: nothing global moves, and a directory outside the project
//! keeps the machine's own uf. See `uf_cli`'s `commands::toolchain::pin`.
//!
//! # Exact, and only exact
//!
//! A prefix — `0.3`, the newest 0.3.x — is what `runtime` accepts and locks in
//! `uf.lock`. uf does not accept one for itself yet: resolving it needs the
//! list of every release, which the installer does not read, and a pin whose
//! meaning waits on a network call before any command can start is a slower
//! `uf` everywhere for the sake of a shorter string. So a prefix is refused
//! with the release to write instead, and a range for the reason every range
//! is refused: it can mean a different uf tomorrow.
//!
//! # Read twice
//!
//! Once by [`pinned_uf`] before any command line is parsed, and once more with
//! the rest of the config by [`crate::load_config`]. The first read is narrow
//! on purpose. The release being pinned may be newer than the one reading the
//! file, with keys this one has never heard of and would refuse, and the pin
//! is the one key that has to be read *before* deciding whose schema the rest
//! of the file is held to.

use std::fs;

use camino::Utf8Path;
use compact_str::CompactString;
use serde::Deserialize;

use crate::tools::{is_exact, is_prefix};
use crate::{ConfigError, discover_config, discover_root, extract_config_object};

/// Refuse a `uf` pin that is not an exact release.
pub(crate) fn check(path: &Utf8Path, written: Option<&str>) -> Result<(), ConfigError> {
    let Some(written) = written else {
        return Ok(());
    };
    match refusal(written) {
        None => Ok(()),
        Some(reason) => Err(ConfigError::UfVersion {
            path: path.to_path_buf(),
            written: written.to_owned(),
            reason,
        }),
    }
}

/// Why `written` is not a release uf can pin itself to, or `None` when it is.
fn refusal(written: &str) -> Option<String> {
    if is_exact(written) {
        return None;
    }
    if let Some(version) = written.strip_prefix("uf@") {
        return Some(if is_exact(version) {
            uf_infra::into_string(compact_str::format_compact!(
                "and the key already names uf. Write the version alone: `uf: \"{version}\"`"
            ))
        } else {
            String::from("and the key already names uf. Write the version alone, such as `0.3.0`")
        });
    }
    if is_prefix(written) {
        return Some(uf_infra::into_string(compact_str::format_compact!(
            "which is a prefix, and uf pins itself to an exact release. Write the release, \
             such as `{written}{}`",
            if written.contains('.') { ".0" } else { ".0.0" }
        )));
    }
    Some(String::from(
        "which is not a release. uf pins itself to an exact version, such as `0.3.0`",
    ))
}

/// Only the key [`pinned_uf`] reads; every other key is somebody else's.
#[derive(Deserialize)]
struct Pin {
    #[serde(default)]
    uf: Option<serde_json::Value>,
}

/// The release of uf the project around `start` pins, if it pins one.
///
/// Reads nothing but `uf`, and answers `Ok(None)` for a config file it cannot
/// read at all: that file is the full load's to explain, in the release the
/// reader already has, rather than a reason for no uf to be able to say so.
///
/// # Errors
///
/// When `uf` is present and not an exact release.
pub fn pinned_uf(start: &Utf8Path) -> Result<Option<CompactString>, ConfigError> {
    let root = discover_root(start);
    let Some(path) = discover_config(&root) else {
        return Ok(None);
    };
    let Ok(source) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Some(object) = extract_config_object(&source) else {
        return Ok(None);
    };
    let Ok(pin) = json5::from_str::<Pin>(&object) else {
        return Ok(None);
    };
    match pin.uf {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(written)) => {
            check(&path, Some(&written))?;
            Ok(Some(written.into()))
        }
        Some(other) => Err(ConfigError::UfVersion {
            path,
            written: other.to_string(),
            reason: String::from(
                "which is not a string. uf pins itself to an exact version, such as `\"0.3.0\"`",
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_release_is_a_pin() {
        for written in ["0.3.0", "1.0.0", "0.0.0-alpha.35", "1.2.3+build.7"] {
            assert_eq!(refusal(written), None, "{written}");
        }
    }

    #[test]
    fn a_prefix_is_refused_with_the_release_to_write() {
        let reason = refusal("0.3").unwrap();
        assert!(reason.contains("prefix"), "{reason}");
        assert!(reason.contains("`0.3.0`"), "{reason}");
        assert!(refusal("1").unwrap().contains("`1.0.0`"));
    }

    #[test]
    fn a_spec_is_refused_with_the_version_alone() {
        let reason = refusal("uf@0.3.0").unwrap();
        assert!(reason.contains("`uf: \"0.3.0\"`"), "{reason}");
    }

    #[test]
    fn ranges_and_tags_are_refused() {
        for written in ["^0.3.0", "~0.3", ">=0.3.0", "latest", "*", "", "0.3.x"] {
            assert!(refusal(written).is_some(), "{written:?} was accepted");
        }
    }

    fn project(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("uf.config.js"), config).unwrap();
        dir
    }

    fn read(dir: &tempfile::TempDir) -> Result<Option<CompactString>, ConfigError> {
        pinned_uf(Utf8Path::from_path(dir.path()).unwrap())
    }

    #[test]
    fn the_pin_is_read_from_the_config() {
        let dir = project("export default defineConfig({ uf: \"0.3.0\", runtime: \"node@26\" });");
        assert_eq!(read(&dir).unwrap().as_deref(), Some("0.3.0"));
    }

    #[test]
    fn keys_this_release_does_not_know_do_not_stop_the_pin() {
        let dir = project(
            "export default defineConfig({\n  uf: \"9.0.0\",\n  aSectionFromTheFuture: { on: true },\n});",
        );
        assert_eq!(read(&dir).unwrap().as_deref(), Some("9.0.0"));
    }

    #[test]
    fn no_pin_is_none() {
        let dir = project("export default defineConfig({ runtime: \"node@26\" });");
        assert_eq!(read(&dir).unwrap(), None);
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(read(&empty).unwrap(), None);
    }

    #[test]
    fn a_config_that_cannot_be_read_statically_is_left_to_the_full_load() {
        let dir = project("const version = \"0.3.0\";\nexport default defineConfig(make());");
        assert_eq!(read(&dir).unwrap(), None);
    }

    #[test]
    fn a_bad_pin_is_refused_naming_the_file() {
        let dir = project("export default defineConfig({ uf: \"^0.3.0\" });");
        let message = read(&dir).unwrap_err().to_string();
        assert!(message.contains("uf.config.js"), "{message}");
        assert!(message.contains("`^0.3.0`"), "{message}");
        let dir = project("export default defineConfig({ uf: 3 });");
        assert!(read(&dir).unwrap_err().to_string().contains("not a string"));
    }
}
