//! Every rule's Good and Bad examples, run as tests, and the reference pages
//! built from them.
//!
//! # What this module owns
//!
//! Each rule in [`crate::rules`] has one example file,
//! `crates/uf_lint/rules/<namespace>/<name>.md`, holding at least one `## Bad`
//! and at least one `## Good` example. This module:
//!
//! 1. fails when a rule has no example file, or a file lacks a Bad or a Good
//!    example, or a file names no rule at all;
//! 2. lints every example with that rule alone switched on. A Bad example must
//!    produce exactly the findings written in its `diagnostics` block, with
//!    the same paths, lines, columns and messages. A Good example must produce
//!    none;
//! 3. renders `docs/app/reference/lint/$page.mdx` from the files and fails
//!    when the checked-in page is not what it renders.
//!
//! So the documentation cannot drift from the linter. An example that stops
//! being true fails `cargo test`, a message that changes fails it until the
//! example is updated, and the pages are output, not something a person keeps
//! in step by hand.
//!
//! # Updating
//!
//! `UF_BLESS=1 cargo test -p uf_lint --lib rule_docs` rewrites each Bad
//! example's `diagnostics` block with what the linter actually reports, then
//! regenerates the pages. Read the diff: blessing records behaviour, it does
//! not judge it. A Good example that reports something is never blessed.
//!
//! # The example format
//!
//! ````markdown
//! One or two paragraphs: what the rule catches and why.
//!
//! ## Bad
//!
//! Optional prose about this example.
//!
//! ```js
//! // @flow
//! type Props = { value: any };
//! ```
//!
//! ```diagnostics
//! app/example.js:2:23 avoid `any`; …
//! ```
//!
//! ## Good
//!
//! ```js
//! // @flow
//! type Props = { value: mixed };
//! ```
//! ````
//!
//! Each example runs with its own rule switched on and nothing else, except
//! the rules its `uf-lint-disable` comments name. Those are switched on too, so
//! the suppression rules can be shown working, and their findings count like
//! any other.
//!
//! One example is one small project. Every code fence in a section is a file
//! of it. A fence's info string may name the file, as in ```` ```json path=package.json ````.
//! Without a path, a `js` fence is `app/example.js` and a `json` fence is
//! `package.json`, and a section may hold only one unnamed fence of each.
//! A rule that looks across files, such as `import/no-cycle`, gets its second
//! file this way.
//!
//! # Rules that do not run yet
//!
//! A rule that needs Flow type inference ([`crate::rules::RuleRequirement::TypeChecker`])
//! is reported as unavailable instead of running. Its examples are still
//! required, because they are the documentation, but they cannot be executed.
//! So a Bad example of such a rule must not carry a `diagnostics` block: uf
//! would be claiming output it has never produced. The page says the examples
//! are not checked yet.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::*;
use crate::rules::RuleDescriptor;

/// Rules that run but that the compiler uf ships never answers, with the
/// reason. Their Bad example shows the code the category is about, but it
/// cannot carry a `diagnostics` block, and the page says why.
///
/// This list is a claim about `react_compiler` 0.1.0 and has to be checked,
/// not trusted. [`every_rule_has_good_and_bad_examples_that_do_what_they_say`]
/// fails the moment one of these examples starts to report something, so an
/// entry cannot outlive the compiler behaviour it describes.
const NOT_PRODUCED: [(&str, &str); 2] = [
    (
        "react-compiler/fbt",
        "`react_compiler` 0.1.0 never files a diagnostic under `FBT`. A malformed `fbt` tag is \
         reported as `Invariant` or `Todo` instead, so this rule has nothing to report yet",
    ),
    (
        "react-compiler/preserve-manual-memoization",
        "under the options uf and `eslint-plugin-react-hooks` run the compiler with, the shapes \
         that break a manual memo are reported first as `react-compiler/memo-dependencies` or \
         `react-compiler/immutability`, and compilation of that function stops before this \
         check runs",
    ),
];

/// Why `id` can have no recorded findings, when it is in [`NOT_PRODUCED`].
fn not_produced(id: &str) -> Option<&'static str> {
    NOT_PRODUCED
        .iter()
        .find(|(rule, _)| *rule == id)
        .map(|(_, why)| *why)
}

/// Where the example files live: `crates/uf_lint/rules`.
fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("rules")
}

/// Where the generated reference pages live: `docs/app/reference/lint`.
fn pages_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/app/reference/lint")
}

/// Whether this run should rewrite files instead of comparing against them.
fn blessing() -> bool {
    std::env::var_os("UF_BLESS").is_some_and(|value| value == "1")
}

/// `UF_RULES=a11y/` limits a run to the rules whose id starts with it, so
/// whoever is writing one namespace's examples can check and bless those
/// without touching anyone else's files. Unset, every rule is checked, which is
/// what CI does.
fn selected(id: &str) -> bool {
    std::env::var("UF_RULES").map_or(true, |prefix| id.starts_with(prefix.as_str()))
}

/// The example file for a rule id: `flow/unclear-type` →
/// `rules/flow/unclear-type.md`.
fn example_path(id: &str) -> PathBuf {
    examples_root().join(format!("{id}.md"))
}

/// Whether an example is one the rule must report or one it must accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bad,
    Good,
}

impl Kind {
    fn heading(self) -> &'static str {
        match self {
            Self::Bad => "Bad",
            Self::Good => "Good",
        }
    }
}

/// One file of an example project.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExampleFile {
    /// The fence's language, which decides the default path and how the page
    /// highlights it.
    lang: String,
    /// The path the fence named, when it named one.
    named: Option<String>,
    /// The file's text, without the fence.
    code: String,
}

impl ExampleFile {
    /// The path the file is linted at.
    fn path(&self) -> String {
        if let Some(named) = &self.named {
            return named.clone();
        }
        match self.lang.as_str() {
            "json" => "package.json".to_owned(),
            _ => "app/example.js".to_owned(),
        }
    }
}

/// One `## Bad` or `## Good` section.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Example {
    kind: Kind,
    /// Prose between the heading and the first fence, trimmed.
    prose: String,
    files: Vec<ExampleFile>,
    /// The `diagnostics` block, one finding per line, when there is one.
    expected: Option<Vec<String>>,
}

/// A parsed example file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RuleDoc {
    /// Prose before the first example: what the rule catches and why.
    intro: String,
    examples: Vec<Example>,
}

/// Parse an example file, or say what is wrong with it.
///
/// Strict on purpose. A fence that never closes, text after the last fence of
/// a section, or a section heading other than `## Bad` and `## Good` is an
/// error, because each of those would otherwise go silently missing from the
/// page or from the test.
fn parse(text: &str) -> Result<RuleDoc, String> {
    let mut intro = String::new();
    let mut examples: Vec<Example> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            let kind = match heading.trim() {
                "Bad" => Kind::Bad,
                "Good" => Kind::Good,
                other => {
                    return Err(format!(
                        "unknown section `## {other}`; use `## Bad` or `## Good`"
                    ));
                }
            };
            examples.push(Example {
                kind,
                prose: String::new(),
                files: Vec::new(),
                expected: None,
            });
            continue;
        }
        if let Some(info) = line.strip_prefix("```") {
            let mut body = Vec::new();
            let mut closed = false;
            for inner in lines.by_ref() {
                if inner == "```" {
                    closed = true;
                    break;
                }
                body.push(inner);
            }
            if !closed {
                return Err(format!("the fence ```{info} never closes"));
            }
            let Some(example) = examples.last_mut() else {
                return Err("a code fence before the first `## Bad` or `## Good`".to_owned());
            };
            let mut words = info.split_whitespace();
            let lang = words.next().unwrap_or_default().to_owned();
            if lang == "diagnostics" {
                if example.expected.is_some() {
                    return Err("two `diagnostics` blocks in one section".to_owned());
                }
                example.expected = Some(body.iter().map(|line| (*line).to_owned()).collect());
                continue;
            }
            if example.expected.is_some() {
                return Err(
                    "a file after the `diagnostics` block; put the findings last".to_owned(),
                );
            }
            let mut named = None;
            for word in words {
                match word.strip_prefix("path=") {
                    Some(path) if !path.is_empty() => named = Some(path.to_owned()),
                    _ => return Err(format!("unknown fence option `{word}`")),
                }
            }
            if lang.is_empty() {
                return Err("a code fence with no language".to_owned());
            }
            example.files.push(ExampleFile {
                lang,
                named,
                code: body.join("\n") + "\n",
            });
            continue;
        }
        match examples.last_mut() {
            None => {
                intro.push_str(line);
                intro.push('\n');
            }
            Some(example) if example.files.is_empty() => {
                example.prose.push_str(line);
                example.prose.push('\n');
            }
            Some(_) if line.trim().is_empty() => {}
            Some(_) => {
                return Err(format!(
                    "prose after a section's first fence: `{line}`; write it above the code"
                ));
            }
        }
    }
    for example in &mut examples {
        example.prose = example.prose.trim().to_owned();
        let mut paths: Vec<String> = example.files.iter().map(ExampleFile::path).collect();
        paths.sort();
        let before = paths.len();
        paths.dedup();
        if paths.len() != before {
            return Err(format!(
                "two files at the same path in one `## {}` section; name them with `path=`",
                example.kind.heading()
            ));
        }
        if example.files.is_empty() {
            return Err(format!(
                "a `## {}` section with no code",
                example.kind.heading()
            ));
        }
    }
    Ok(RuleDoc {
        intro: intro.trim().to_owned(),
        examples,
    })
}

/// Render a parsed file back to its canonical text, which is what blessing
/// writes.
fn render_example_file(doc: &RuleDoc) -> String {
    let mut out = String::new();
    if !doc.intro.is_empty() {
        out.push_str(&doc.intro);
        out.push_str("\n\n");
    }
    for example in &doc.examples {
        let _ = writeln!(out, "## {}\n", example.kind.heading());
        if !example.prose.is_empty() {
            out.push_str(&example.prose);
            out.push_str("\n\n");
        }
        for file in &example.files {
            match &file.named {
                Some(path) => {
                    let _ = writeln!(out, "```{} path={path}", file.lang);
                }
                None => {
                    let _ = writeln!(out, "```{}", file.lang);
                }
            }
            out.push_str(&file.code);
            out.push_str("```\n\n");
        }
        if let Some(expected) = &example.expected {
            out.push_str("```diagnostics\n");
            for line in expected {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("```\n\n");
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

/// What linting an example with `rule` alone reports, one finding per line, as
/// `path:line:column message`, in the order the report sorts them.
fn findings(rule: &RuleDescriptor, example: &Example) -> Vec<String> {
    let files: Vec<SourceFile> = example
        .files
        .iter()
        .map(|file| at(&file.path(), &file.code))
        .collect();
    let mut config = only(rule.id);
    // A rule a suppression comment names is switched on too. Otherwise the
    // suppression rules' examples could not be written at all: an unused
    // suppression is only judged when its rule runs, and a used one only
    // silences something when its rule reports. Those rules' findings are
    // compared like the example's own, so an example cannot hide one.
    for file in &example.files {
        for named in suppressed_rules(&file.code) {
            if crate::rules::canonical_rule_id(named).is_some() {
                config
                    .lint
                    .rules
                    .insert(CompactString::from(named), RuleLevel::Error);
            }
        }
    }
    let report = lint_sources(&files, &config).expect("lint");
    report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{}:{} {}",
                diagnostic.path.as_deref().unwrap_or("?"),
                diagnostic.line,
                diagnostic.column,
                diagnostic.message
            )
        })
        .collect()
}

/// The rule ids `uf-lint-disable` and `uf-lint-disable-next-line` comments in
/// `code` name, as written.
fn suppressed_rules(code: &str) -> Vec<&str> {
    code.lines()
        .filter_map(|line| {
            let comment = line.trim_start().strip_prefix("//")?.trim_start();
            comment
                .strip_prefix("uf-lint-disable-next-line")
                .or_else(|| comment.strip_prefix("uf-lint-disable"))
        })
        .flat_map(|names| {
            names
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|name| !name.is_empty())
        })
        .collect()
}

/// Every rule with its parsed example file, or the problems that stop one
/// being read. Problems are collected rather than panicking on the first, so
/// one run lists every rule that needs work.
fn load_all() -> (Vec<(&'static RuleDescriptor, RuleDoc)>, Vec<String>) {
    let mut docs = Vec::new();
    let mut problems = Vec::new();
    for rule in crate::rules().iter().filter(|rule| selected(rule.id)) {
        let path = example_path(rule.id);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => {
                problems.push(format!(
                    "{}: no example file at {}",
                    rule.id,
                    path.display()
                ));
                continue;
            }
        };
        match parse(&text) {
            Ok(doc) => docs.push((rule, doc)),
            Err(problem) => problems.push(format!("{}: {problem}", rule.id)),
        }
    }
    (docs, problems)
}

/// Every example file names a rule that exists: a file left behind by a
/// renamed or removed rule is documentation for nothing.
#[test]
fn every_example_file_belongs_to_a_rule() {
    let mut stray = Vec::new();
    let root = examples_root();
    let Ok(namespaces) = std::fs::read_dir(&root) else {
        panic!("no example directory at {}", root.display());
    };
    for namespace in namespaces.flatten() {
        let Ok(files) = std::fs::read_dir(namespace.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let id = format!("{}/{name}", namespace.file_name().to_string_lossy());
            if crate::rules::rule(&id).is_none() {
                stray.push(path.display().to_string());
            }
        }
    }
    assert!(
        stray.is_empty(),
        "example files for rules that do not exist:\n{}",
        stray.join("\n")
    );
}

/// Every rule's example file, parsed and run, with everything wrong with them.
///
/// Under `UF_BLESS=1` a Bad example's recorded findings are replaced by what the
/// linter reports, in memory and on disk, so the pages rendered from the
/// returned docs are the blessed ones. Both tests call this rather than one
/// reading what the other wrote, because the test harness runs them at the
/// same time.
fn checked() -> (Vec<(&'static RuleDescriptor, RuleDoc)>, Vec<String>) {
    let (mut docs, mut problems) = load_all();
    let bless = blessing();
    for (rule, doc) in &mut docs {
        let runs = rule.requirement.is_available();
        let silent = not_produced(rule.id).is_some();
        if !doc.examples.iter().any(|example| example.kind == Kind::Bad) {
            problems.push(format!("{}: no `## Bad` example", rule.id));
        }
        if !doc
            .examples
            .iter()
            .any(|example| example.kind == Kind::Good)
        {
            problems.push(format!("{}: no `## Good` example", rule.id));
        }
        if doc.intro.is_empty() {
            problems.push(format!(
                "{}: no introduction: say what the rule catches and why",
                rule.id
            ));
        }
        let mut changed = false;
        for (index, example) in doc.examples.iter_mut().enumerate() {
            let label = format!(
                "{} {} example #{}",
                rule.id,
                example.kind.heading(),
                index + 1
            );
            if !runs {
                if example.expected.is_some() {
                    problems.push(format!(
                        "{label}: the rule needs type inference and does not run, so it cannot \
                         have a `diagnostics` block"
                    ));
                }
                continue;
            }
            let actual = findings(rule, example);
            match example.kind {
                Kind::Good => {
                    if !actual.is_empty() {
                        problems.push(format!("{label} reports:\n  {}", actual.join("\n  ")));
                    }
                    if example.expected.is_some() {
                        problems.push(format!(
                            "{label}: a Good example has no `diagnostics` block"
                        ));
                    }
                }
                Kind::Bad if silent => {
                    if !actual.is_empty() {
                        problems.push(format!(
                            "{label} reports something now, so the rule no longer belongs in \
                             NOT_PRODUCED:\n  {}",
                            actual.join("\n  ")
                        ));
                    }
                    if example.expected.is_some() {
                        problems.push(format!(
                            "{label}: the compiler never reports this rule, so there is nothing \
                             to record in a `diagnostics` block"
                        ));
                    }
                }
                Kind::Bad => {
                    if actual.is_empty() {
                        problems.push(format!("{label} reports nothing"));
                        continue;
                    }
                    if example.expected.as_ref() != Some(&actual) {
                        if bless {
                            example.expected = Some(actual);
                            changed = true;
                        } else {
                            problems.push(format!(
                                "{label}: expected\n  {}\nbut the rule reports\n  {}\n\
                                 (UF_BLESS=1 records what it reports)",
                                example.expected.as_deref().unwrap_or_default().join("\n  "),
                                actual.join("\n  ")
                            ));
                        }
                    }
                }
            }
        }
        if changed {
            std::fs::write(example_path(rule.id), render_example_file(doc)).expect("bless");
        }
    }
    (docs, problems)
}

/// Every rule has an example file with a Bad and a Good example, every Bad
/// example reports exactly what its `diagnostics` block says, and every Good
/// example reports nothing.
#[test]
fn every_rule_has_good_and_bad_examples_that_do_what_they_say() {
    let (_, problems) = checked();
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The reference pages are exactly what the example files render to.
#[test]
fn the_rule_reference_pages_are_current() {
    if std::env::var_os("UF_RULES").is_some() {
        // A partial run renders partial pages; only a whole run may judge them.
        return;
    }
    let (docs, problems) = checked();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let pages = render_pages(&docs);
    let root = pages_root();
    let mut stale = Vec::new();
    for (relative, text) in &pages {
        let path = root.join(relative);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != *text {
            if blessing() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("create page directory");
                }
                std::fs::write(&path, text).expect("write page");
            } else {
                stale.push(relative.clone());
            }
        }
    }
    // Anything else in the directory is a page nothing renders any more.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if entry.path().is_dir() {
                format!("{name}/$page.mdx")
            } else {
                name
            };
            if !pages.contains_key(&relative) {
                stale.push(relative);
            }
        }
    }
    assert!(
        stale.is_empty(),
        "docs/app/reference/lint is out of date: {}\n\
         UF_BLESS=1 cargo test -p uf_lint --lib rule_docs regenerates it",
        stale.join(", ")
    );
}

// --- Rendering ----------------------------------------------------------------

/// A namespace's page title and one-line summary, in the order the index lists
/// them. A namespace missing here is a test failure rather than a page with no
/// title.
const NAMESPACES: [(&str, &str, &str); 14] = [
    (
        "flow",
        "Flow",
        "Flow's own lints and the parse check, configured beside uf's rules.",
    ),
    (
        "react",
        "React",
        "Component and hook syntax, JSX shape, and memoization the compiler already does.",
    ),
    (
        "react-compiler",
        "React Compiler",
        "The official React Compiler's diagnostics, one rule per category.",
    ),
    (
        "a11y",
        "Accessibility",
        "JSX that a screen reader, a keyboard or a switch cannot use.",
    ),
    (
        "markup",
        "Markup",
        "HTML the browser's parser would rewrite or ignore.",
    ),
    (
        "import",
        "Imports",
        "The static import graph: cycles, undeclared packages and paths.",
    ),
    (
        "server",
        "Server",
        "Server components, server actions and the client boundary.",
    ),
    (
        "router",
        "Router",
        "File-system routes the router would not serve.",
    ),
    (
        "security",
        "Security",
        "Known vulnerability classes kept out of the code.",
    ),
    (
        "fetch",
        "Fetch",
        "The instrumented `fetch` the toolchain is built around.",
    ),
    ("react-native", "React Native", "Platform-specific modules."),
    ("vite", "Vite", "`@uniflowed/vite` configuration."),
    (
        "package",
        "package.json",
        "Manifests that shell out instead of declaring tasks.",
    ),
    (
        "uniflowed",
        "House rules",
        "Whitespace, tasks and the suppression comments themselves.",
    ),
];

/// A heading's anchor as the docs site computes it: lower case, anything but
/// letters, digits, `-`, `_` and spaces dropped, spaces to `-`. So
/// `flow/unclear-type` is `#flowunclear-type`.
fn anchor(heading: &str) -> String {
    heading
        .chars()
        .filter_map(|ch| match ch {
            ' ' => Some('-'),
            '-' | '_' => Some(ch),
            _ if ch.is_alphanumeric() => Some(ch.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

/// `description` with its first letter capitalised and a full stop, so a
/// catalogue line reads as a sentence on the page.
fn sentence(description: &str) -> String {
    let mut chars = description.chars();
    let mut out: String = chars
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default();
    out.push_str(chars.as_str());
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

fn level_name(level: RuleLevel) -> &'static str {
    match level {
        RuleLevel::Off => "off",
        RuleLevel::Warn => "warn",
        RuleLevel::Error => "error",
    }
}

/// Every page, by path relative to `docs/app/reference/lint`.
///
/// One page: the manual's navigation names every page that exists
/// (`tests/library/docs-nav.test.js`), and one entry for the whole catalogue
/// reads better than fourteen. The page opens with a table per namespace, each
/// row linking to the rule's own section further down.
fn render_pages(docs: &[(&'static RuleDescriptor, RuleDoc)]) -> BTreeMap<String, String> {
    let mut by_namespace: BTreeMap<&str, Vec<&(&'static RuleDescriptor, RuleDoc)>> =
        BTreeMap::new();
    for entry in docs {
        let namespace = entry
            .0
            .id
            .split_once('/')
            .map_or(entry.0.id, |(namespace, _)| namespace);
        by_namespace.entry(namespace).or_default().push(entry);
    }
    for namespace in by_namespace.keys() {
        assert!(
            NAMESPACES.iter().any(|(name, _, _)| name == namespace),
            "rule namespace `{namespace}` has no entry in NAMESPACES"
        );
    }

    let mut page = String::from(
        "---\n\
         title: \"Lint rules · uf\"\n\
         description: \"Every rule uf lint runs, with an example it reports and one it accepts.\"\n\
         readiness: \"Implemented\"\n\
         ---\n\n\
         {/* Generated from crates/uf_lint/rules by `UF_BLESS=1 cargo test -p uf_lint --lib rule_docs`. Edit the examples there, not this page. */}\n\n\
         <p className=\"eyebrow\">Reference</p>\n\n\
         # Lint rules\n\n\
         <div className=\"lede\">\n\
         Every rule `uf lint` knows, with an example it reports and an example it\n\
         accepts. Each example is run by uf's test suite, and the findings shown\n\
         under a Bad example are the findings `uf lint` prints for it.\n\
         </div>\n\n\
         The level is the default. `uf lint --rules` prints the level each rule runs\n\
         at in your project, and whether a fix applies. Rules marked *needs type\n\
         inference* are listed but do not run yet: uf reports them as unavailable,\n\
         and their examples show what they will check.\n\n",
    );

    // The contents: one table per namespace.
    for (namespace, title, summary) in NAMESPACES {
        let Some(entries) = by_namespace.get(namespace) else {
            continue;
        };
        let _ = writeln!(page, "## {title}\n\n{summary}\n");
        page.push_str("| Rule | Default | What it checks |\n| --- | --- | --- |\n");
        for (rule, _) in entries {
            let needs = if rule.requirement.is_available() {
                ""
            } else {
                " · needs type inference"
            };
            let _ = writeln!(
                page,
                "| [`{id}`](#{anchor}) | `{level}`{needs} | {description} |",
                id = rule.id,
                anchor = anchor(rule.id),
                level = level_name(rule.default_level),
                description = rule.description,
            );
        }
        page.push('\n');
    }

    // Then every rule, in the same order.
    page.push_str("## Every rule, with examples\n\n");
    for (namespace, _, _) in NAMESPACES {
        let Some(entries) = by_namespace.get(namespace) else {
            continue;
        };
        for (rule, doc) in entries {
            let runs = rule.requirement.is_available();
            let level = level_name(rule.default_level);
            let _ = writeln!(page, "### {}\n", rule.id);
            if let Some(why) = not_produced(rule.id) {
                let _ = writeln!(
                    page,
                    "**uf does not report this yet.** It runs, but {why}. The Bad example \
                     below is what the category is about.\n"
                );
            }
            let _ = writeln!(
                page,
                "Default: `{level}`{}. {}\n",
                if runs {
                    ""
                } else {
                    " · needs Flow type inference, which uf does not implement yet, so the rule \
                     does not run and the examples below are not checked"
                },
                sentence(rule.description)
            );
            page.push_str(&doc.intro);
            page.push_str("\n\n");
            for example in &doc.examples {
                let _ = writeln!(page, "#### {}\n", example.kind.heading());
                if !example.prose.is_empty() {
                    page.push_str(&example.prose);
                    page.push_str("\n\n");
                }
                for file in &example.files {
                    if example.files.len() > 1 || file.named.is_some() {
                        let _ = writeln!(page, "`{}`\n", file.path());
                    }
                    // A Bad example of `flow/syntax` does not parse, and the
                    // docs' snippet check parses every `js` fence. Showing it as
                    // text is honest: it is not a program.
                    let lang = if rule.id == "flow/syntax" && example.kind == Kind::Bad {
                        "text"
                    } else {
                        file.lang.as_str()
                    };
                    let _ = write!(page, "```{lang}\n{}```\n\n", file.code);
                }
                if let Some(expected) = &example.expected {
                    page.push_str("`uf lint` reports:\n\n```text\n");
                    for line in expected {
                        page.push_str(line);
                        page.push('\n');
                    }
                    page.push_str("```\n\n");
                }
            }
        }
    }
    page.truncate(page.trim_end().len());
    page.push('\n');
    BTreeMap::from([("$page.mdx".to_owned(), page)])
}

// --- The harness's own tests ----------------------------------------------------

#[test]
fn parse_reads_sections_files_and_findings() {
    let doc = parse(
        "Why.\n\n## Bad\n\nThis one.\n\n```js\nlet a;\n```\n\n```json path=package.json\n{}\n```\n\n\
         ```diagnostics\napp/example.js:1:1 no\n```\n\n## Good\n\n```js\nlet b;\n```\n",
    )
    .expect("parse");

    assert_eq!(doc.intro, "Why.");
    assert_eq!(doc.examples.len(), 2);
    assert_eq!(doc.examples[0].prose, "This one.");
    assert_eq!(doc.examples[0].files[1].path(), "package.json");
    assert_eq!(
        doc.examples[0].expected.as_deref(),
        Some(&["app/example.js:1:1 no".to_owned()][..])
    );
    assert_eq!(doc.examples[1].kind, Kind::Good);
    assert_eq!(
        render_example_file(&doc),
        render_example_file(&parse(&render_example_file(&doc)).unwrap())
    );
}

#[test]
fn parse_refuses_what_would_otherwise_go_missing() {
    for (text, why) in [
        ("## Ugly\n", "unknown section"),
        ("```js\nlet a;\n```\n", "fence before the first section"),
        ("## Bad\n```js\nlet a;\n", "unclosed fence"),
        (
            "## Bad\n```js\nlet a;\n```\nafterwards\n",
            "prose after code",
        ),
        (
            "## Bad\n```js\na\n```\n```js\nb\n```\n",
            "two unnamed js files",
        ),
        ("## Bad\n```js wat\na\n```\n", "unknown fence option"),
        ("## Good\n", "a section with no code"),
    ] {
        assert!(parse(text).is_err(), "{why}: {text:?}");
    }
}

#[test]
fn anchors_match_the_docs_site() {
    assert_eq!(anchor("flow/unclear-type"), "flowunclear-type");
    assert_eq!(
        anchor("uf build --adapter TARGET"),
        "uf-build---adapter-target"
    );
}
