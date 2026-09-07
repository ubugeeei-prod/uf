//! What a build says, and what it ships, when a module has styles or the
//! React Compiler has an opinion about it.
//!
//! Both of these are the same class of defect: the build lost something and
//! said nothing. `stylex.create` compiled to class names and the stylesheet
//! that declares them was never written, so the pages shipped a `class`
//! attribute pointing at rules that do not exist; and every React Compiler
//! finding printed as `a function:` with no file and no position, so a reader
//! could not tell their own module from a dependency's. Neither failed a
//! build, which is why neither was noticed by one.
//!
//! Separate from `vite.rs` because the fixtures are: that file builds this
//! repository's docs site and an app with a route handler, and neither uses
//! StyleX or contains code the compiler declines. These two projects exist to
//! be the thing that was missing, and they are written out here rather than
//! kept under `tests/fixtures` because a fixture whose whole point is a
//! deliberate defect is one uf's own lint and format tasks would report as
//! this repository's.
//!
//! The assertions go all the way to the artefact. "A stylesheet was emitted"
//! would have passed on a sheet with the wrong rules in it, so the test takes
//! the class names out of the prerendered HTML and requires a rule for each,
//! takes the custom properties out of those rules and requires a declaration
//! for each, and requires the dark theme's overrides to be inside the media
//! query that makes them a dark theme. See ubugeeei-prod/uf#306 and #307.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use support::{Project, uf};

/// Whether a Vite build can run here: Node on PATH and the workspace
/// installed.
///
/// A missing fixture is a failure, not a skip, for the reason `vite.rs` gives:
/// cargo hides a passing test's output, so a suite that quietly stopped
/// building anything still prints "ok". `UF_ALLOW_FIXTURE_SKIP=1` opts out on
/// a machine that genuinely cannot; CI sets nothing and so can never skip.
fn fixture_ready() -> bool {
    let mut missing = Vec::new();
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        missing.push("`node` is not on PATH".to_owned());
    }
    let driver = support::repo_root().join("node_modules/@uniflowed/vite/driver.js");
    if !driver.is_file() {
        missing.push(format!("{} does not exist; run `npm ci`", driver.display()));
    }

    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "a Vite build cannot run here, so this test would prove nothing: {}",
        missing.join("; ")
    );
    eprintln!("skipping: {}", missing.join("; "));
    false
}

/// The files every one of these projects starts from: a router entry and a
/// config that names it.
fn app_scaffold() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "package.json",
            "{ \"name\": \"uf-vite-styles\", \"private\": true, \"type\": \"module\" }\n",
        ),
        (
            "uf.config.js",
            r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: { router: { entry: "app.js", root: "app" } },
  build: { entries: ["app.js"], outDir: "dist" },
});
"#,
        ),
        (
            "app.js",
            r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#,
        ),
    ]
}

/// Build the project and return its stdout, insisting that it succeeded.
fn build(root: &Path) -> String {
    let output = uf().arg("--cwd").arg(root).arg("build").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "the build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// The one stylesheet a build emitted.
fn stylesheet(dist: &Path) -> String {
    let mut sheets: Vec<PathBuf> = fs::read_dir(dist.join("assets"))
        .expect("the build wrote an assets directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "css"))
        .collect();
    sheets.sort();
    assert_eq!(
        sheets.len(),
        1,
        "expected exactly one stylesheet in {}, found {sheets:?}",
        dist.display()
    );
    fs::read_to_string(&sheets[0]).unwrap()
}

/// The classes on the first `tag` element in a document.
fn classes_on(html: &str, tag: &str) -> Vec<String> {
    let at = html
        .find(tag)
        .unwrap_or_else(|| panic!("no {tag} element in:\n{html}"));
    let rest = &html[at..];
    let start = rest
        .find("class=\"")
        .unwrap_or_else(|| panic!("no class attribute on {tag} in:\n{html}"))
        + "class=\"".len();
    let end = start
        + rest[start..]
            .find('"')
            .expect("an unterminated class attribute");
    rest[start..end]
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The body of the first block that opens with `prefix`, brace-matched.
///
/// Brace-matched rather than "everything after it": an at-rule's body is where
/// the rules that belong to it are, and a test that searched the rest of the
/// file would pass on a dark override that had fallen *out* of the media
/// query, which is exactly the shape of the bug next door.
fn block_after(css: &str, prefix: &str) -> String {
    let at = css
        .find(prefix)
        .unwrap_or_else(|| panic!("no {prefix:?} in the stylesheet:\n{css}"));
    let open = at
        + css[at..]
            .find('{')
            .expect("a block opens with a brace after its prelude");
    let mut depth = 0usize;
    for (offset, character) in css[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return css[open + 1..open + offset].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("{prefix:?} is never closed in:\n{css}");
}

/// The custom properties a chunk of CSS reads, as `--name`.
fn variables_read(css: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("var(--") {
        rest = &rest[at + "var(".len()..];
        let end = rest
            .find([')', ','])
            .expect("a var() reference that is never closed");
        names.push(rest[..end].trim().to_owned());
        rest = &rest[end..];
    }
    names.sort();
    names.dedup();
    names
}

/// A `stylex.create` in an application module reaches `dist/` as CSS.
///
/// The whole chain, because every link of it was broken at once and only the
/// last one showed: `uf transform` returned the rules, the host's service
/// dropped them on the floor, so the plugin never asked Vite for a stylesheet;
/// and once it did, the `:root` block declaring the custom properties those
/// rules read was tree-shaken away with the token module nothing referenced
/// any more. The build exited 0 through all of it. See ubugeeei-prod/uf#306.
#[test]
fn a_stylex_application_ships_the_stylesheet_it_compiled() {
    if !fixture_ready() {
        return;
    }
    let mut files = app_scaffold();
    files.push(("app/plain.css", ".plain-heading { color: #663399; }\n"));
    files.push((
        "app/_uf.page.js",
        r#"// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { ufAutoTheme } from "@uniflowed/stylex/theme";

import "./plain.css";

const styles = stylex.create({
  button: {
    backgroundColor: ufTokens.accent,
    color: ufTokens.accentInk,
    borderRadius: ufTokens.radiusPill,
    padding: ufTokens.space3,
  },
});

export default component Home() {
  return (
    <main {...props(ufAutoTheme)}>
      <h1 className="plain-heading">styled</h1>
      <button type="button" {...props(styles.button)}>
        press
      </button>
    </main>
  );
}
"#,
    ));
    let project = Project::new(&files);

    build(project.path());

    let dist = project.path().join("dist");
    let index = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");
    let css = stylesheet(&dist);

    // The document says it has a stylesheet, and it is the one that was
    // written. A build that emitted the CSS and linked nothing is the same
    // unstyled page.
    assert!(
        index.contains("rel=\"stylesheet\" href=\"/assets/"),
        "the page links no stylesheet:\n{index}"
    );

    // Every class the compiler minted for the button resolves to a rule.
    let button = classes_on(&index, "<button");
    assert!(!button.is_empty(), "the button has no classes:\n{index}");
    for class in &button {
        assert!(
            css.contains(&format!(".{class}{{")),
            "the document wears .{class} and the stylesheet does not declare it:\n{css}"
        );
    }

    // …and every custom property those rules read is declared on `:root`,
    // which is the half that survived the first fix and still shipped colours
    // that resolve to nothing.
    let root = block_after(&css, ":root{");
    let used = variables_read(&css);
    assert!(
        !used.is_empty(),
        "the token-backed rules read no custom properties at all:\n{css}"
    );
    for name in &used {
        assert!(
            root.contains(&format!("{name}:")),
            "{name} is read by a rule and declared nowhere on :root:\n{root}"
        );
    }

    // `ufAutoTheme` follows the reader's system, so its overrides belong
    // inside the media query and nowhere else.
    let dark = block_after(&css, "@media (prefers-color-scheme");
    let main = classes_on(&index, "<main");
    assert!(
        main.iter().any(|class| dark.contains(&format!(".{class}"))),
        "no class on <main> is themed inside the dark media query:\n{dark}"
    );

    // A plain stylesheet import in the same build is still extracted: the two
    // paths are independent, and neither is a fallback for the other.
    assert!(
        css.contains(".plain-heading{"),
        "the plain CSS import was lost:\n{css}"
    );
}

/// A React Compiler finding names its module, its own position and its
/// component, and is said once per build.
///
/// The fixture holds findings of the application's own and the same findings
/// inside a package it depends on, because the difference between them is one
/// decision under test: the first are the reader's to act on and are printed
/// with a position; the second are not theirs to fix and are counted rather
/// than listed. See ubugeeei-prod/uf#307.
///
/// The other decision is how many lines three ref accesses in one component
/// are worth. Three, one each — this used to be one, because a finding carried
/// its function's position rather than its own and the plugin's
/// de-duplication could not tell them apart (ubugeeei-prod/uf#371).
#[test]
fn a_react_compiler_finding_names_its_file_and_line() {
    if !fixture_ready() {
        return;
    }
    // A ref written during render — the compiler declines the function and
    // says so, at a position and with the name of the `component` it is about,
    // which is the case the old message reduced to the word "a function".
    let refs_during_render = r#"// @flow
import { useRef } from "react";

export component Counter() {
  const seen = useRef<number>(0);
  seen.current = seen.current + 1;
  return <p>{seen.current}</p>;
}
"#;
    let mut files = app_scaffold();
    files.push((
        "node_modules/@uniflowed/probe/package.json",
        r#"{ "name": "@uniflowed/probe", "version": "0.0.0", "type": "module", "exports": { ".": "./index.js" } }
"#,
    ));
    files.push(("node_modules/@uniflowed/probe/index.js", refs_during_render));
    files.push((
        "app/_uf.page.js",
        r#"// @flow
import { useRef } from "react";
import { Counter } from "@uniflowed/probe";

export default component Home() {
  const box = useRef<number>(0);
  box.current = box.current + 1;
  return (
    <main>
      <p>{box.current}</p>
      <Counter />
    </main>
  );
}
"#,
    ));
    let project = Project::new(&files);

    let stdout = build(project.path());

    assert!(
        !stdout.contains("a function"),
        "a finding still names no one:\n{stdout}"
    );

    // The application's own findings: their module, their lines, their
    // columns.
    let mine: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("Cannot access refs during render"))
        .collect();
    let mut positions: Vec<&str> = mine
        .iter()
        .filter_map(|line| line.split("app/_uf.page.js:").nth(1))
        .filter_map(|rest| rest.split(further_colon).next())
        .map(|position| position.trim_end_matches(':'))
        .collect();
    assert_eq!(
        positions.len(),
        mine.len(),
        "a finding does not name the module it is about:\n{stdout}"
    );
    assert!(
        positions
            .iter()
            .all(|p| p.contains(':') && p.split(':').all(|part| part.parse::<u32>().is_ok())),
        "a finding names no line and column:\n{stdout}"
    );
    assert!(
        mine.iter().all(|line| line.contains("(in Home)")),
        "a finding does not name the component it is about:\n{stdout}"
    );

    // `box.current` is assigned on one line, read on the right of that same
    // assignment, and read again in the JSX: three places to look, so three
    // findings, each at its own column. Before ubugeeei-prod/uf#371 every
    // finding carried the position of the `component` keyword rather than its
    // own, so all three printed as the same line and two were discarded as
    // duplicates of the first.
    assert_eq!(
        positions.len(),
        3,
        "the three ref accesses did not arrive as three findings:\n{stdout}"
    );

    // A build runs Vite twice, over the client bundle and the server's, and
    // one access is still one thing to fix — so no position may repeat.
    let listed = positions.len();
    positions.sort_unstable();
    positions.dedup();
    assert_eq!(
        positions.len(),
        listed,
        "a build runs Vite twice and must still report each finding once:\n{stdout}"
    );

    // The dependency's identical finding is not listed — it is not the
    // application author's to fix — but the build still says it happened.
    assert!(
        !stdout.contains("node_modules/@uniflowed/probe/index.js:"),
        "a dependency's finding was printed as if the reader could act on it:\n{stdout}"
    );
    assert!(
        stdout.contains("@uniflowed/probe"),
        "the build hid a dependency's findings instead of counting them:\n{stdout}"
    );
    assert!(
        stdout.contains("not this application's to fix"),
        "the summary does not say why those findings are not listed:\n{stdout}"
    );
}

/// Where `line:column` stops and the message begins.
///
/// The message is separated from the position by `": "`, and the position is
/// itself made of colons, so the space is the only thing that ends it — the
/// colon it leaves behind is trimmed by the caller.
fn further_colon(character: char) -> bool {
    character == ' '
}
