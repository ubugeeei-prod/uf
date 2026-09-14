use std::cell::RefCell;
use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use uf_config::{extract_config_object, parse_config_object};

use super::*;
use crate::index::{FORMAT, Release};
use crate::tool::{Arch, Os};

const MAC: Platform = Platform {
    os: Os::Darwin,
    arch: Arch::Arm64,
};

/// A project whose `uf.config.js` is `defineConfig(<body>)`, read the way every
/// command reads it.
fn project(body: &str) -> (tempfile::TempDir, Utf8PathBuf, UniflowedConfig) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let path = root.join("uf.config.js");
    let source = format!("export default defineConfig({body});");
    std::fs::write(&path, &source).unwrap();
    let object = extract_config_object(&source).expect("an object literal");
    let config = parse_config_object(&path, &object).unwrap_or_else(|error| panic!("{error}"));
    (dir, root, config)
}

fn list(tool: Tool, versions: &[&str]) -> Index {
    Index {
        format: FORMAT,
        tool,
        fetched_at: 0,
        sources: vec![format!("the {tool} fixture")],
        releases: versions.iter().copied().map(Release::new).collect(),
    }
}

/// Release lists held in memory, recording every fetch.
#[derive(Default)]
struct Lists {
    published: BTreeMap<Tool, Vec<&'static str>>,
    last_fetched: BTreeMap<Tool, Vec<&'static str>>,
    offline: bool,
    fetches: RefCell<Vec<Tool>>,
}

impl Lists {
    fn publishing(mut self, tool: Tool, versions: &[&'static str]) -> Self {
        self.published.insert(tool, versions.to_vec());
        self
    }

    fn with_cache(mut self, tool: Tool, versions: &[&'static str]) -> Self {
        self.last_fetched.insert(tool, versions.to_vec());
        self
    }

    fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    fn fetches(&self) -> Vec<Tool> {
        self.fetches.borrow().clone()
    }
}

impl Releases for Lists {
    fn fetch(&self, tool: Tool) -> Result<Index, EnvError> {
        self.fetches.borrow_mut().push(tool);
        let unreachable = || EnvError::Download {
            url: format!("https://publisher.invalid/{tool}"),
            detail: "Could not resolve host".to_owned(),
        };
        if self.offline {
            return Err(unreachable());
        }
        self.published
            .get(&tool)
            .map(|versions| list(tool, versions))
            .ok_or_else(unreachable)
    }

    fn cached(&self, tool: Tool) -> Option<Index> {
        self.last_fetched
            .get(&tool)
            .map(|versions| list(tool, versions))
    }
}

fn lock_text(root: &Utf8Path) -> Option<String> {
    std::fs::read_to_string(root.join("uf.lock")).ok()
}

/// A project that declares nothing has nothing to resolve, fetch or lock.
#[test]
fn a_project_that_declares_nothing_has_an_empty_toolchain() {
    let (_guard, root, config) = project("{}");
    let lists = Lists::default();

    let toolchain = resolve(&root, &config, Lookup::Missing, &lists).unwrap();

    assert!(toolchain.tools.is_empty());
    assert!(!toolchain.lock_changed);
    assert_eq!(lock_text(&root), None);
    assert!(lists.fetches().is_empty());
}

/// A prefix is resolved to the newest release once, locked, and read from the
/// lock after that — even once something newer is published.
#[test]
fn a_prefix_is_resolved_once_and_then_read_from_the_lock() {
    let (_guard, root, config) = project(r#"{ runtime: "node@26" }"#);
    let lists =
        Lists::default().publishing(Tool::Node, &["27.0.0-rc.1", "26.10.0", "26.8.2", "24.14.0"]);

    let toolchain = resolve(&root, &config, Lookup::Missing, &lists).unwrap();

    let [node] = toolchain.tools.as_slice() else {
        panic!("one tool: {:?}", toolchain.tools);
    };
    assert_eq!(
        node.resolution,
        Resolution::Resolved {
            prefix: "26".to_owned(),
            version: "26.10.0".to_owned(),
            was: None
        }
    );
    assert_eq!(node.spec(), "node@26");
    // `runtime` is what the build and the tests fall back to, and the one pin
    // says so rather than appearing three times.
    assert_eq!(node.roles(), "runtime, build runtime, test runtime");
    assert!(node.uses.iter().all(|used| used.key == "runtime"));
    assert!(toolchain.lock_changed);
    assert_eq!(
        lock_text(&root).as_deref(),
        Some("{\n  \"toolchain\": {\n    \"node@26\": \"26.10.0\"\n  }\n}\n")
    );

    let newer = Lists::default().publishing(Tool::Node, &["26.11.0", "26.10.0"]);
    let again = resolve(&root, &config, Lookup::Missing, &newer).unwrap();
    assert_eq!(
        again.tools[0].resolution,
        Resolution::Locked {
            prefix: "26".to_owned(),
            version: "26.10.0".to_owned()
        }
    );
    assert!(!again.lock_changed);
    assert!(newer.fetches().is_empty(), "a locked prefix needs no list");
}

/// A listing reads the lock and nothing else: no fetch, no write.
#[test]
fn lock_only_reads_and_writes_nothing() {
    let (_guard, root, config) = project(r#"{ runtime: "node@26" }"#);
    let lists = Lists::default().publishing(Tool::Node, &["26.10.0"]);

    let toolchain = resolve(&root, &config, Lookup::LockOnly, &lists).unwrap();

    assert_eq!(
        toolchain.tools[0].resolution,
        Resolution::Unlocked {
            prefix: "26".to_owned()
        }
    );
    assert_eq!(toolchain.tools[0].pin(MAC), None);
    assert!(lists.fetches().is_empty());
    assert_eq!(lock_text(&root), None);
}

/// An update asks the publisher again and moves the lock, saying from where.
#[test]
fn latest_moves_the_lock_and_says_from_where() {
    let (_guard, root, config) = project(r#"{ runtime: "node@26" }"#);
    let mut lock = ToolchainLock::default();
    lock.insert(Tool::Node, "26", "26.8.2");
    lock::write(&root.join("uf.lock"), &lock).unwrap();

    let lists = Lists::default().publishing(Tool::Node, &["26.10.0", "26.8.2"]);
    let toolchain = resolve(&root, &config, Lookup::Latest, &lists).unwrap();

    assert_eq!(
        toolchain.tools[0].resolution,
        Resolution::Resolved {
            prefix: "26".to_owned(),
            version: "26.10.0".to_owned(),
            was: Some("26.8.2".to_owned())
        }
    );
    assert!(toolchain.lock_changed);
    assert!(
        lock_text(&root)
            .unwrap()
            .contains("\"node@26\": \"26.10.0\"")
    );

    // Already the newest: nothing moves and nothing is written.
    let unchanged = resolve(&root, &config, Lookup::Latest, &lists).unwrap();
    assert!(matches!(
        unchanged.tools[0].resolution,
        Resolution::Locked { .. }
    ));
    assert!(!unchanged.lock_changed);
}

/// Without a network, an install locks the newest release uf knows of, and an
/// update — which is a question about now — refuses.
#[test]
fn offline_an_install_uses_the_last_list_and_an_update_does_not() {
    let (_guard, root, config) = project(r#"{ runtime: "bun@1.4" }"#);
    let lists = Lists::default()
        .with_cache(Tool::Bun, &["1.4.1", "1.3.14"])
        .offline();

    let toolchain = resolve(&root, &config, Lookup::Missing, &lists).unwrap();
    assert_eq!(toolchain.tools[0].resolution.version(), Some("1.4.1"));

    std::fs::remove_file(root.join("uf.lock")).unwrap();
    let error = resolve(&root, &config, Lookup::Latest, &lists).unwrap_err();
    assert!(matches!(error, EnvError::Download { .. }), "{error:?}");
    assert_eq!(lock_text(&root), None);
}

/// With no network and no list, the refusal names the spec and what to do.
#[test]
fn a_prefix_with_no_list_to_resolve_it_names_the_spec() {
    let (_guard, root, config) = project(r#"{ packageManager: "pnpm@10" }"#);
    let lists = Lists::default().offline();

    let error = resolve(&root, &config, Lookup::Missing, &lists).unwrap_err();

    assert!(matches!(error, EnvError::Unresolvable { .. }), "{error:?}");
    let message = error.to_string();
    assert!(
        message.starts_with("pnpm@10 is not locked in "),
        "{message}"
    );
    assert!(message.contains("uf.lock"), "{message}");
    assert!(message.contains("Could not resolve host"), "{message}");
    assert!(message.contains("write an exact version"), "{message}");
}

/// A prefix the list has nothing for says which list, and for an old Deno, why
/// that list is the wrong place to look.
#[test]
fn a_prefix_with_no_release_says_which_list_was_asked() {
    let (_guard, root, config) = project(r#"{ runtime: "node@13" }"#);
    let lists = Lists::default().publishing(Tool::Node, &["26.10.0"]);
    let message = resolve(&root, &config, Lookup::Missing, &lists)
        .unwrap_err()
        .to_string();
    assert_eq!(message, "node@13 names no release in the node fixture");

    let (_guard, root, config) = project(r#"{ runtime: "deno@1.40" }"#);
    let lists = Lists::default().publishing(Tool::Deno, &["2.9.6", "1.46.3"]);
    let message = resolve(&root, &config, Lookup::Missing, &lists)
        .unwrap_err()
        .to_string();
    assert!(message.contains("from 1.46 on"), "{message}");
    assert!(message.contains("`deno@1.40.0`"), "{message}");
}

/// A spec with no version, or an exact one, needs no list and no lock.
#[test]
fn exact_and_unversioned_specs_need_no_list() {
    let (_guard, root, config) = project(r#"{ runtime: "node", packageManager: "pnpm@12.0.0" }"#);
    let lists = Lists::default();

    let toolchain = resolve(&root, &config, Lookup::Missing, &lists).unwrap();

    assert_eq!(
        toolchain
            .tools
            .iter()
            .map(|declared| (declared.spec(), declared.resolution.version()))
            .collect::<Vec<_>>(),
        [
            ("node".to_owned(), None),
            ("pnpm@12.0.0".to_owned(), Some("12.0.0"))
        ]
    );
    assert_eq!(
        toolchain
            .pins(MAC)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["pnpm@12.0.0"]
    );
    assert!(lists.fetches().is_empty());
    assert_eq!(lock_text(&root), None);
}

/// A Bun runner is the test runtime, and says which key made it so.
#[test]
fn a_bun_runner_is_the_test_runtime_it_implies() {
    let (_guard, root, config) = project(r#"{ runtime: "node@26", test: { runner: "bun@1.4" } }"#);
    let lists = Lists::default()
        .publishing(Tool::Node, &["26.10.0"])
        .publishing(Tool::Bun, &["1.4.2"]);

    let toolchain = resolve(&root, &config, Lookup::Missing, &lists).unwrap();

    let summary: Vec<(String, String, Vec<String>)> = toolchain
        .tools
        .iter()
        .map(|declared| {
            (
                declared.spec(),
                declared.roles(),
                declared.uses.iter().map(|used| used.key.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "node@26".to_owned(),
                "runtime, build runtime".to_owned(),
                vec!["runtime".to_owned(), "runtime".to_owned()]
            ),
            (
                "bun@1.4".to_owned(),
                "test runtime".to_owned(),
                vec!["test.runner".to_owned()]
            ),
        ]
    );
    assert_eq!(
        toolchain
            .linked(MAC)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["node@26.10.0", "bun@1.4.2"]
    );
}

/// Two releases of one tool are two pins, and the one on `PATH` is the
/// `runtime`'s.
#[test]
fn two_releases_of_one_tool_link_the_one_every_command_falls_back_to() {
    let (_guard, root, config) =
        project(r#"{ runtime: "node@26.8.2", build: { runtime: "node@24.14.0" } }"#);

    let toolchain = resolve(&root, &config, Lookup::Missing, &Lists::default()).unwrap();

    let pins: Vec<String> = toolchain
        .pins(MAC)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(pins, ["node@24.14.0", "node@26.8.2"]);
    let linked: Vec<String> = toolchain
        .linked(MAC)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(linked, ["node@26.8.2"]);
}

/// `env.toolchain` and exact `engines` still pin, with the key that did, and
/// `engines` gives way to any key that names the same tool.
#[test]
fn the_spellings_that_came_before_still_pin_with_their_keys() {
    let (_guard, root, config) =
        project(r#"{ env: { toolchain: { node: "24.14.0", pnpm: "9.15.0" } } }"#);
    std::fs::write(
        root.join("package.json"),
        r#"{ "engines": { "node": "22.9.0", "bun": "1.2.19", "npm": ">=10" } }"#,
    )
    .unwrap();

    let toolchain = resolve(&root, &config, Lookup::Missing, &Lists::default()).unwrap();

    let summary: Vec<(String, Vec<String>)> = toolchain
        .tools
        .iter()
        .map(|declared| {
            (
                declared.spec(),
                declared.uses.iter().map(|used| used.key.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "node@24.14.0".to_owned(),
                vec!["env.toolchain.node".to_owned()]
            ),
            (
                "bun@1.2.19".to_owned(),
                vec!["package.json#engines.bun".to_owned()]
            ),
            (
                "pnpm@9.15.0".to_owned(),
                vec!["env.toolchain.pnpm".to_owned()]
            ),
        ]
    );

    let (_guard, root, config) = project(r#"{ runtime: "node@26.8.2" }"#);
    std::fs::write(
        root.join("package.json"),
        r#"{ "engines": { "node": "24.14.0" } }"#,
    )
    .unwrap();
    let toolchain = resolve(&root, &config, Lookup::Missing, &Lists::default()).unwrap();
    assert_eq!(
        toolchain
            .tools
            .iter()
            .map(Declared::spec)
            .collect::<Vec<_>>(),
        ["node@26.8.2"]
    );
}

/// A pin in `env.toolchain` is still held to being exact.
#[test]
fn an_env_toolchain_pin_that_is_not_exact_is_still_refused() {
    let (_guard, root, config) = project(r#"{ env: { toolchain: { node: "^24" } } }"#);

    let error = resolve(&root, &config, Lookup::LockOnly, &Lists::default()).unwrap_err();

    assert!(
        matches!(error, EnvError::NotAnExactVersion { .. }),
        "{error:?}"
    );
}

/// A lock entry for a prefix nothing declares any more goes on the next write,
/// and a listing leaves it alone.
#[test]
fn a_lock_entry_nothing_declares_goes_on_the_next_write() {
    let (_guard, root, config) = project(r#"{ runtime: "node@26" }"#);
    let mut lock = ToolchainLock::default();
    lock.insert(Tool::Node, "26", "26.8.2");
    lock.insert(Tool::Bun, "1.4", "1.4.2");
    lock::write(&root.join("uf.lock"), &lock).unwrap();

    resolve(&root, &config, Lookup::LockOnly, &Lists::default()).unwrap();
    assert!(lock_text(&root).unwrap().contains("bun@1.4"));

    let toolchain = resolve(&root, &config, Lookup::Missing, &Lists::default()).unwrap();
    assert!(toolchain.lock_changed);
    assert_eq!(
        lock_text(&root).as_deref(),
        Some("{\n  \"toolchain\": {\n    \"node@26\": \"26.8.2\"\n  }\n}\n")
    );
}
