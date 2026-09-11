//! What the pass writes, checked without a Deno.
//!
//! Deno is the thing these cannot check: whether the runtime accepts the tree
//! and the map is `crates/uf_cli/tests/deno_host.rs`'s question, and it needs a
//! binary CI installs. What *is* checkable here is everything up to that point
//! — that the Flow is gone, that the map names what it has to name, and that
//! the entry which keeps a compiled module from being redirected a second time
//! is present — and it is checked here so that a failure on CI is a failure
//! about Deno rather than about a `format!`.

use camino::{Utf8Path, Utf8PathBuf};
use uf_project::{ProjectFile, SourceKind};

use super::{DenoLoader, IMPORT_MAP, build, directory_url, file_url};

/// A project root and an `@uniflowed` scope beside it, both thrown away after.
struct Fixture {
    _temporary: tempfile::TempDir,
    root: Utf8PathBuf,
    scope: Utf8PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("a temp directory");
        let base =
            Utf8PathBuf::from_path_buf(temporary.path().to_path_buf()).expect("a UTF-8 temp path");
        let fixture = Self {
            root: base.join("project"),
            scope: base.join("node_modules/@uniflowed"),
            _temporary: temporary,
        };
        // The one package every run needs, because it is the worker.
        fixture.package(
            "test",
            r#"{
              "name": "@uniflowed/test",
              "type": "module",
              "exports": { ".": "./index.js", "./worker": "./worker.js" }
            }"#,
        );
        fixture.write(
            &fixture.scope.join("test/index.js"),
            "// @flow\nexport const answer: number = 42;\n",
        );
        fixture.write(
            &fixture.scope.join("test/worker.js"),
            "// @flow\nimport { answer } from \"./index.js\";\nconsole.log(answer);\n",
        );
        fixture
    }

    fn package(&self, name: &str, manifest: &str) {
        self.write(&self.scope.join(name).join("package.json"), manifest);
    }

    fn write(&self, path: &Utf8Path, contents: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, contents).expect("a written file");
    }

    /// A project source file, written to disk and described the way
    /// `uf_project`'s scan describes one.
    fn source(&self, relative: &str, contents: &str) -> ProjectFile {
        let absolute = self.root.join(relative);
        self.write(&absolute, contents);
        ProjectFile {
            absolute_path: absolute,
            relative_path: relative.to_owned(),
            source: contents.to_owned(),
            kind: SourceKind::JavaScript,
        }
    }

    fn build(&self, sources: &[ProjectFile]) -> DenoLoader {
        build(
            &self.root,
            &uf_config::UniflowedConfig::default(),
            &self.scope,
            sources,
            true,
        )
        .expect("the pass runs")
    }

    fn map(&self, loader: &DenoLoader) -> serde_json::Value {
        let text = std::fs::read_to_string(&loader.import_map).expect("the map was written");
        serde_json::from_str(&text).expect("the map is JSON")
    }
}

/// The obstacle the whole pass exists for: Flow syntax Deno cannot parse.
#[test]
fn a_project_module_is_compiled_and_no_longer_flow() {
    let fixture = Fixture::new();
    let sources = vec![fixture.source(
        "src/answer.test.js",
        "// @flow\nimport \"@uniflowed/test\";\nconst answer: number = 1;\nexport default answer;\n",
    )];

    let loader = fixture.build(&sources);

    let compiled = std::fs::read_to_string(loader.directory.join("src/answer.test.js"))
        .expect("the module was written");
    assert!(
        !compiled.contains("const answer: number"),
        "the annotation Deno stops at is still there:\n{compiled}"
    );
    // The map is the reason a stack frame names the file the author wrote
    // rather than the copy under `.uf/deno`.
    assert!(
        compiled.contains("//# sourceMappingURL=data:application/json;base64,"),
        "no source map was appended:\n{compiled}"
    );
    assert!(loader.compiled >= 1, "{loader:?}");
}

/// The second half of ubugeeei-prod/uf#246, answered by the same artefact.
#[test]
fn the_map_names_every_uniflowed_export() {
    let fixture = Fixture::new();
    let sources = vec![fixture.source("src/a.test.js", "// @flow\nimport \"@uniflowed/test\";\n")];

    let loader = fixture.build(&sources);
    let map = fixture.map(&loader);
    let imports = &map["imports"];

    let into = loader.directory.join("packages/@uniflowed/test");
    assert_eq!(imports["@uniflowed/test"], file_url(&into.join("index.js")));
    assert_eq!(
        imports["@uniflowed/test/worker"],
        file_url(&into.join("worker.js"))
    );
    // And a trailing-slash key, so a deep path no `exports` entry names still
    // lands in the compiled tree rather than in the installed one.
    assert_eq!(imports["@uniflowed/test/"], directory_url(&into));
}

#[test]
fn package_imports_are_scoped_to_the_package_that_declares_them() {
    let fixture = Fixture::new();
    fixture.package(
        "test",
        r##"{
          "name": "@uniflowed/test",
          "type": "module",
          "imports": {
            "#worker": "./worker.js",
            "#internal/*": "./internal/*",
            "#external": "@uniflowed/host"
          },
          "exports": { ".": "./index.js", "./worker": "./worker.js" }
        }"##,
    );
    fixture.write(
        &fixture.scope.join("test/internal/log.js"),
        "// @flow\nexport const label: string = \"log\";\n",
    );
    let sources = vec![fixture.source("src/a.test.js", "// @flow\nimport \"@uniflowed/test\";\n")];

    let loader = fixture.build(&sources);
    let map = fixture.map(&loader);
    let into = loader.directory.join("packages/@uniflowed/test");
    let scope = directory_url(&into);
    let entries = &map["scopes"][scope.as_str()];

    assert_eq!(entries["#worker"], file_url(&into.join("worker.js")));
    assert_eq!(entries["#internal/"], directory_url(&into.join("internal")));
    assert!(
        entries["#external"].is_null(),
        "bare package targets are omitted rather than written as invalid import-map addresses"
    );
    assert!(
        map["imports"]["#worker"].is_null(),
        "package-local imports must not become process-wide import-map entries"
    );
}

/// The entry a reader stops at, and the one whose absence breaks everything.
///
/// `.uf/deno` is under the project root, so the prefix that redirects the
/// project would redirect the output of that redirect a second time. The
/// identity entry is longer, and an import map resolves the longest matching
/// prefix.
#[test]
fn the_output_directory_maps_to_itself() {
    let fixture = Fixture::new();
    let sources = vec![fixture.source("src/a.test.js", "// @flow\nexport const a: number = 1;\n")];

    let loader = fixture.build(&sources);
    let imports = fixture.map(&loader)["imports"].clone();

    let root = directory_url(&fixture.root);
    let out = directory_url(&loader.directory);
    assert_eq!(imports[&root], out, "the project is not redirected");
    assert_eq!(
        imports[&out], out,
        "a compiled module would be redirected a second time"
    );
    assert!(
        out.len() > root.len(),
        "the identity entry has to be the longer prefix or it never wins: {out} vs {root}"
    );
}

/// A compiled module's relative imports resolve against its *new* home, so
/// what it imports has to be there too — even when uf does not compile it.
#[test]
fn a_file_that_is_not_flow_is_copied_rather_than_skipped() {
    let fixture = Fixture::new();
    let sources = vec![
        fixture.source("src/a.test.js", "// @flow\nexport const a: number = 1;\n"),
        ProjectFile {
            absolute_path: fixture.root.join("src/fixture.json"),
            relative_path: String::from("src/fixture.json"),
            source: String::from("{\"kept\": true}"),
            kind: SourceKind::Json,
        },
    ];
    fixture.write(&fixture.root.join("src/fixture.json"), "{\"kept\": true}");

    let loader = fixture.build(&sources);

    let copied = std::fs::read_to_string(loader.directory.join("src/fixture.json"))
        .expect("the fixture was copied");
    assert_eq!(copied, "{\"kept\": true}");
}

/// An ahead-of-time pass runs on every command, so the second one has to be
/// able to say it had nothing to do.
#[test]
fn a_second_pass_over_unchanged_source_compiles_nothing() {
    let fixture = Fixture::new();
    let sources = vec![fixture.source("src/a.test.js", "// @flow\nexport const a: number = 1;\n")];

    let first = fixture.build(&sources);
    assert!(first.compiled > 0, "{first:?}");
    let second = fixture.build(&sources);

    assert_eq!(second.compiled, 0, "{second:?}");
    assert_eq!(second.copied, 0, "{second:?}");
    assert!(second.import_map.ends_with(IMPORT_MAP));
}

/// The tree is one generation. A different build of `uf` compiles it again.
///
/// The failure this prevents is `packages/host/internal/node-hooks.js`'s: a
/// suite that passes, or fails, for the previous build's reasons, with nothing
/// about the run looking wrong.
#[test]
fn a_different_build_of_uf_throws_the_tree_away() {
    let fixture = Fixture::new();
    let sources = vec![fixture.source("src/a.test.js", "// @flow\nexport const a: number = 1;\n")];

    let first = fixture.build(&sources);
    std::fs::write(first.directory.join(super::STAMP), "0\u{0}another build\n")
        .expect("the stamp is writable");
    let second = fixture.build(&sources);

    assert!(
        second.compiled > 0,
        "a tree written by another build was kept: {second:?}"
    );
}

/// The packages the worker is actually made of, compiled for real.
///
/// Everything above runs against a two-file fixture, which checks the pass and
/// not the thing the pass has to survive: `@uniflowed/test` and
/// `@uniflowed/host` are about thirty modules of the repository's own Flow,
/// with an `exports` map each and a `browser` field on one of them. A syntax
/// the transform cannot take, or a manifest this module's `exports` reader
/// mishandles, is a failure CI would otherwise first meet inside Deno — where
/// it arrives as "the suite did not run" rather than as the module that could
/// not be compiled.
///
/// The scope is built out of symlinks rather than by installing, so this needs
/// no `node_modules`: `reachable_packages` canonicalises, which is what a
/// workspace install would have produced anyway.
#[cfg(unix)]
#[test]
fn the_packages_the_worker_is_made_of_compile() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repository = Utf8PathBuf::from_path_buf(repository.canonicalize().expect("a repository"))
        .expect("a UTF-8 repository path");

    let fixture = Fixture::new();
    // Over the two-file stand-in `Fixture::new` wrote: the real package is
    // what this test is about.
    std::fs::remove_dir_all(fixture.scope.join("test")).expect("the stand-in is removable");
    for name in ["test", "host"] {
        std::os::unix::fs::symlink(
            repository.join("packages").join(name),
            fixture.scope.join(name),
        )
        .expect("a linked package");
    }
    let sources = vec![fixture.source(
        "a.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\n",
    )];

    let loader = fixture.build(&sources);

    let worker = std::fs::read_to_string(&loader.worker).expect("the worker was compiled");
    assert!(
        !worker.contains("type Request = {|"),
        "the worker's Flow survived the transform"
    );
    let imports = fixture.map(&loader)["imports"].clone();
    // `@uniflowed/host` has no `.` export, only subpaths — the case a reader of
    // `exports` most easily gets wrong.
    assert!(imports["@uniflowed/host"].is_null(), "{imports}");
    assert!(
        imports["@uniflowed/host/transform"]
            .as_str()
            .unwrap_or_default()
            .ends_with("/packages/@uniflowed/host/transform.js"),
        "{imports}"
    );
    assert!(
        imports["@uniflowed/test/worker"]
            .as_str()
            .unwrap_or_default()
            .ends_with("/packages/@uniflowed/test/worker.js"),
        "{imports}"
    );
}

/// A key that does not parse as a URL is a *bare specifier* to an import map,
/// so this is the difference between an entry that matches and one that is
/// silently inert.
#[test]
fn a_path_becomes_a_file_url_with_the_awkward_bytes_escaped() {
    assert_eq!(file_url(Utf8Path::new("/a/b.js")), "file:///a/b.js");
    assert_eq!(
        file_url(Utf8Path::new("/a b/c#d.js")),
        "file:///a%20b/c%23d.js"
    );
    // `?` above all: the worker appends `?uf-run=<n>` to the URL it imports,
    // and a path that carried one unescaped would split the key.
    assert_eq!(file_url(Utf8Path::new("/a?b.js")), "file:///a%3Fb.js");
    assert_eq!(directory_url(Utf8Path::new("/a/b")), "file:///a/b/");
    assert_eq!(directory_url(Utf8Path::new("/a/b/")), "file:///a/b/");
}
