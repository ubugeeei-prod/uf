use camino::Utf8PathBuf;

use super::*;

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn versions(releases: &[Release]) -> Vec<&str> {
    releases
        .iter()
        .map(|release| release.version.as_str())
        .collect()
}

fn releases(versions: &[&str]) -> Vec<Release> {
    versions.iter().copied().map(Release::new).collect()
}

/// A directory of fixtures served the way the publishers serve the real lists,
/// and the bases that point at it.
fn publisher(files: &[(&str, &str)]) -> (tempfile::TempDir, Bases) {
    let (guard, root) = temp();
    for (path, body) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    (guard, Bases::at(&format!("file://{root}")))
}

/// Four rows in the shape nodejs.org writes them, out of order on purpose.
const NODE_INDEX: &str = r#"[
  {"version":"v26.8.2","date":"2026-09-09","files":["osx-arm64-tar","linux-x64"],"lts":false},
  {"version":"v24.14.0","date":"2026-08-20","files":["osx-arm64-tar"],"lts":"Krypton"},
  {"version":"v27.0.0-rc.1","date":"2026-10-02","files":[],"lts":false},
  {"version":"v26.10.0","date":"2026-10-01","files":[],"lts":false}
]"#;

/// Node's index is read without the `v`, newest first, with its LTS lines.
#[test]
fn nodes_index_is_read_newest_first_with_its_lts_lines() {
    let parsed = parse_node_index(NODE_INDEX).expect("an index");

    // 26.10.0 before 26.8.2: ordered as numbers, which a string sort gets wrong.
    assert_eq!(
        versions(&parsed),
        ["27.0.0-rc.1", "26.10.0", "26.8.2", "24.14.0"]
    );
    assert_eq!(parsed[3].lts.as_deref(), Some("Krypton"));
    assert_eq!(parsed[2].lts, None, "`lts: false` is no line at all");
    assert_eq!(parsed[2].date.as_deref(), Some("2026-09-09"));
}

/// A packument's version list, prereleases kept and ordered the way semver
/// orders them.
#[test]
fn a_packument_is_its_version_list() {
    let parsed = parse_packument(
        r#"{
          "name": "bun",
          "dist-tags": { "latest": "1.4.2" },
          "versions": {
            "1.3.14": {},
            "1.4.2-canary.9": {},
            "1.4.2": {},
            "1.4.10": {},
            "1.4.2-canary.10": {}
          }
        }"#,
    )
    .expect("a packument");

    assert_eq!(
        versions(&parsed),
        [
            "1.4.10",
            "1.4.2",
            // A release outranks its own prereleases, and `canary.10` is
            // newer than `canary.9`.
            "1.4.2-canary.10",
            "1.4.2-canary.9",
            "1.3.14"
        ]
    );
    assert!(parsed[2].is_prerelease());
    assert!(!parsed[1].is_prerelease());
    assert_eq!(
        parsed[0].date, None,
        "an abbreviated packument has no dates"
    );
}

/// Anything that is not a release list is refused rather than read as an empty
/// one, which would resolve nothing and say nothing about why.
#[test]
fn anything_but_a_release_list_is_refused() {
    assert_eq!(parse_node_index("<html>captive portal</html>"), None);
    assert_eq!(parse_node_index(r#"[{"date":"2026-09-09"}]"#), None);
    assert_eq!(parse_node_index(r#"{"version":"v26.8.2"}"#), None);
    assert_eq!(parse_packument("{}"), None);
    assert_eq!(parse_packument(r#"{"versions":["1.0.0"]}"#), None);
}

/// A prefix is matched a component at a time.
#[test]
fn a_prefix_is_matched_component_by_component() {
    let list = releases(&["26.0.0", "2.0.0", "1.40.0", "1.4.2", "1.4.1"]);
    let found = |prefix| resolve(prefix, &list).map(|release| release.version.as_str());

    assert_eq!(found("1.4"), Some("1.4.2"), "1.4 is 1.4.x, never 1.40.0");
    assert_eq!(found("2"), Some("2.0.0"), "2 is 2.x, never 26.0.0");
    assert_eq!(found("26"), Some("26.0.0"));
    assert_eq!(found("1"), Some("1.40.0"));
    assert_eq!(
        found("1.4.1"),
        Some("1.4.1"),
        "three numbers name one release"
    );
    assert_eq!(found("3"), None);
}

/// `node@27` is the newest 27 that was released, not a candidate for one.
#[test]
fn a_prerelease_is_never_what_a_prefix_names() {
    let list = releases(&["27.0.0-rc.1", "26.8.2", "4.9.2+sha.20260910"]);
    let found = |prefix| resolve(prefix, &list).map(|release| release.version.as_str());

    assert_eq!(found("27"), None);
    assert_eq!(found("26"), Some("26.8.2"));
    // Build metadata is a release saying which commit it came from.
    assert_eq!(found("4.9"), Some("4.9.2+sha.20260910"));
}

/// What is not a prefix names nothing, rather than something.
#[test]
fn a_prefix_that_is_not_numbers_names_nothing() {
    let list = releases(&["26.8.2"]);
    for prefix in ["", "lts", "v26", "26.x", "26.8.2.1", "^26"] {
        assert_eq!(resolve(prefix, &list), None, "{prefix:?}");
    }
}

/// A refresh writes the cache a reader reads, and the reader needs nothing
/// but the file.
#[test]
fn a_refresh_writes_the_cache_a_reader_reads() {
    let (_publisher, bases) = publisher(&[("index.json", NODE_INDEX)]);
    let (_cache, cache) = temp();

    assert_eq!(cached_in(&cache, Tool::Node), None, "nothing cached yet");

    let fetched = refresh_in(&cache, Tool::Node, &bases).expect("the fixture is served");
    assert_eq!(fetched.tool, Tool::Node);
    assert_eq!(fetched.format, FORMAT);
    assert_eq!(fetched.sources, [format!("{}/index.json", bases.nodejs)]);
    assert_eq!(
        fetched
            .resolve("26")
            .map(|release| release.version.as_str()),
        Some("26.10.0")
    );
    assert!(
        fetched.age().is_some_and(|age| age.as_secs() < 60),
        "{:?}",
        fetched.age()
    );

    assert_eq!(cached_in(&cache, Tool::Node), Some(fetched));
    assert!(cache.join("node.json").is_file());
    let leftovers: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "node.json")
        .collect();
    assert!(leftovers.is_empty(), "nothing but the cache: {leftovers:?}");
}

/// Yarn is two packages, and its list is both of theirs.
#[test]
fn yarns_list_is_both_of_its_packages() {
    let (_publisher, bases) = publisher(&[
        ("yarn", r#"{"versions":{"1.22.22":{},"1.22.19":{}}}"#),
        (
            "@yarnpkg/cli-dist",
            r#"{"versions":{"4.18.0":{},"3.8.7":{}}}"#,
        ),
    ]);
    let (_cache, cache) = temp();

    let fetched = refresh_in(&cache, Tool::Yarn, &bases).unwrap();

    assert_eq!(
        versions(&fetched.releases),
        ["4.18.0", "3.8.7", "1.22.22", "1.22.19"]
    );
    assert_eq!(fetched.sources.len(), 2);
    assert!(fetched.sources[1].ends_with("/@yarnpkg/cli-dist"));
}

/// A failed refresh leaves the previous cache where it was: the list a
/// completion was offering yesterday is still better than none.
#[test]
fn a_failed_refresh_leaves_the_cache_as_it_was() {
    let (publisher_dir, bases) = publisher(&[("bun", r#"{"versions":{"1.4.2":{}}}"#)]);
    let (_cache, cache) = temp();
    let first = refresh_in(&cache, Tool::Bun, &bases).unwrap();

    std::fs::write(
        publisher_dir.path().join("bun"),
        "<html>rate limited</html>",
    )
    .unwrap();
    let error = refresh_in(&cache, Tool::Bun, &bases).unwrap_err();

    assert!(
        matches!(&error, EnvError::MalformedIndex { url } if url.ends_with("/bun")),
        "{error:?}"
    );
    assert_eq!(cached_in(&cache, Tool::Bun), Some(first));
}

/// A list that is not there is a download error naming the list.
#[test]
fn a_list_that_cannot_be_fetched_names_its_url() {
    let (_publisher, bases) = publisher(&[]);
    let (_cache, cache) = temp();

    let error = refresh_in(&cache, Tool::Pnpm, &bases).unwrap_err();

    assert!(
        matches!(&error, EnvError::Download { url, .. } if url.ends_with("/pnpm")),
        "{error:?}"
    );
    assert!(error.to_string().contains("/pnpm"), "{error}");
    assert_eq!(cached_in(&cache, Tool::Pnpm), None);
}

/// A cache in another shape, for another tool, or not JSON at all is a miss,
/// and the next refresh replaces it.
#[test]
fn a_cache_uf_cannot_trust_is_a_miss() {
    let (_publisher, bases) = publisher(&[("deno", r#"{"versions":{"2.9.6":{}}}"#)]);
    let (_cache, cache) = temp();
    let fetched = refresh_in(&cache, Tool::Deno, &bases).unwrap();

    let mut other_shape = serde_json::to_value(&fetched).unwrap();
    other_shape["format"] = serde_json::json!(FORMAT + 1);
    std::fs::write(cache.join("deno.json"), other_shape.to_string()).unwrap();
    assert_eq!(cached_in(&cache, Tool::Deno), None);

    std::fs::write(cache.join("deno.json"), "not json").unwrap();
    assert_eq!(cached_in(&cache, Tool::Deno), None);

    // Deno's list, filed under Node's name.
    std::fs::write(
        cache.join("node.json"),
        serde_json::to_string(&fetched).unwrap(),
    )
    .unwrap();
    assert_eq!(cached_in(&cache, Tool::Node), None);
}

/// One override puts both publishers' lists under a fixture directory.
#[test]
fn one_base_serves_every_list() {
    let bases = Bases::at("file:///fixtures/");
    assert_eq!(bases.nodejs, "file:///fixtures");
    assert_eq!(bases.registry, "file:///fixtures");

    let urls = |tool| {
        bases
            .lists(tool)
            .into_iter()
            .map(|(url, _)| url)
            .collect::<Vec<_>>()
    };
    assert_eq!(urls(Tool::Node), ["file:///fixtures/index.json"]);
    assert_eq!(urls(Tool::Deno), ["file:///fixtures/deno"]);
    assert_eq!(
        urls(Tool::Yarn),
        [
            "file:///fixtures/yarn",
            "file:///fixtures/@yarnpkg/cli-dist"
        ]
    );

    let real = Bases::default();
    assert_eq!(
        real.lists(Tool::Node)[0].0,
        "https://nodejs.org/dist/index.json"
    );
    assert_eq!(real.lists(Tool::Bun)[0].0, "https://registry.npmjs.org/bun");
}

/// The real publishers answer in the shapes this reads, and every list resolves
/// a prefix a project would write.
///
/// Ignored, because it asks nodejs.org and the npm registry, and a check that
/// fails when somebody else's host is slow is a check people learn to re-run.
/// Run it by hand after changing where a list comes from or how one is read:
///
/// ```sh
/// cargo test -p uf_env the_real_publishers -- --ignored
/// ```
#[test]
#[ignore = "reads nodejs.org and registry.npmjs.org"]
fn the_real_publishers_answer_in_the_shapes_this_reads() {
    let (_cache, cache) = temp();
    let bases = Bases::default();
    for (tool, prefixes) in [
        (Tool::Node, &["24", "26"][..]),
        (Tool::Bun, &["1"][..]),
        (Tool::Deno, &["2"][..]),
        (Tool::Npm, &["11"][..]),
        (Tool::Pnpm, &["10"][..]),
        // Both halves of Yarn: Classic from `yarn`, Berry from its dist.
        (Tool::Yarn, &["1", "4"][..]),
    ] {
        let index =
            refresh_in(&cache, tool, &bases).unwrap_or_else(|error| panic!("{tool}: {error}"));
        for prefix in prefixes {
            let release = index
                .resolve(prefix)
                .unwrap_or_else(|| panic!("{tool}@{prefix} resolves to nothing"));
            assert!(
                release.version.starts_with(&format!("{prefix}.")),
                "{tool}@{prefix} resolved to {}",
                release.version
            );
        }
        assert_eq!(cached_in(&cache, tool).as_ref(), Some(&index));
    }
}
