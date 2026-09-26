//! Tool specs in `uf.config.js`: `runtime: "node@26"`.
//!
//! # Found by type
//!
//! A value is a tool spec when its key's type is written through one of the
//! four aliases `@uniflowed/config` declares for them — `RuntimeSpec`,
//! `PackageManagerSpec`, `TestRunnerSpec` and `BuilderSpec`. The schema says it
//! keeps them aliases so that an editor can find these keys by type, and
//! `every_tool_key_is_found_by_its_type` holds the embedded schema to that: a
//! renamed alias fails there instead of quietly completing nothing.
//!
//! # Names, then versions
//!
//! Before the `@`, the names the role takes, from `uf_config::tools` — the
//! vocabulary the loader checks the same key against, so a name offered here
//! is a name `uf` accepts there. After the `@`, the versions of that tool,
//! newest first: each major, which resolves to its newest release and is
//! locked, then every release. They come from [`super::releases`], which never
//! waits for a network; a version cannot be offered until a list is cached.
//!
//! Prereleases are not offered unless what has been typed has a `-` in it. A
//! major is never offered for a line that has only prereleases, because a
//! prefix never resolves to one.

use uf_config::schema::Shape;
use uf_config::{
    BuilderSpec, CapabilityJsHost, PackageManagerName, QuoteStyle, TestRunnerSpec, ToolName,
    ToolVersion,
};
use uf_env::Tool;
use uf_env::index::{Index, Release};

use super::outline::{Span, Word};
use super::releases::{Lookup, Releases};
use super::{Completion, Described, Item, Kind, offer, quote_byte};

/// Which of the four grammars a value is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// `runtime`, `build.runtime`, `test.runtime`: `RuntimeSpec`.
    Runtime,
    /// `packageManager`: `PackageManagerSpec`.
    PackageManager,
    /// `test.runner`: `TestRunnerSpec`.
    TestRunner,
    /// `build.builder`: `BuilderSpec`.
    Builder,
}

impl Role {
    /// The role of a value that may be any of `shapes`, by the alias one of
    /// them is written through.
    pub(super) fn of(shapes: &[&Shape]) -> Option<Self> {
        shapes
            .iter()
            .flat_map(|shape| shape.aliases())
            .find_map(Self::from_alias)
    }

    fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "RuntimeSpec" => Some(Self::Runtime),
            "PackageManagerSpec" => Some(Self::PackageManager),
            "TestRunnerSpec" => Some(Self::TestRunner),
            "BuilderSpec" => Some(Self::Builder),
            _ => None,
        }
    }

    /// Every name the role takes, in the order `uf_config` lists them, each
    /// with the tool whose releases a version after it is one of.
    fn names(self) -> Vec<(String, Option<Tool>)> {
        match self {
            Self::Runtime => versioned(<CapabilityJsHost as ToolName>::ALL),
            Self::PackageManager => versioned(<PackageManagerName as ToolName>::ALL),
            // `uf` is the binary that is running and takes no version. `bun
            // test` runs on the Bun it names, so its versions are Bun's.
            Self::TestRunner => [TestRunnerSpec::Uf, TestRunnerSpec::Bun(ToolVersion::OnPath)]
                .into_iter()
                .map(|runner| {
                    let tool = runner
                        .implied_runtime()
                        .and_then(|runtime| Tool::parse(ToolName::name(runtime.name)));
                    (runner.to_string(), tool)
                })
                .collect(),
            // Past its one word a builder is a module specifier, and a module
            // has no release list uf reads.
            Self::Builder => vec![(BuilderSpec::Vite.to_string(), None)],
        }
    }
}

fn versioned<N: ToolName>(names: &[N]) -> Vec<(String, Option<Tool>)> {
    names
        .iter()
        .map(|name| {
            let name = ToolName::name(*name);
            (name.to_owned(), Tool::parse(name))
        })
        .collect()
}

/// Everything a tool spec's completion needs, beside the list it adds to.
pub(super) struct Request<'s, 'r> {
    pub(super) role: Role,
    pub(super) described: &'s Described,
    pub(super) source: &'s str,
    pub(super) offset: usize,
    pub(super) word: Option<Word>,
    pub(super) quotes: QuoteStyle,
    pub(super) releases: &'r mut dyn Releases,
}

/// Add what may be written at the cursor in a tool spec to `completion`.
pub(super) fn complete(request: Request<'_, '_>, completion: &mut Completion) {
    let Request {
        role,
        described,
        source,
        offset,
        word,
        quotes,
        releases,
    } = request;

    let Some(quoted) = word.filter(|word| word.quote.is_some()) else {
        // Not in a string yet: the names, quoted the way the project quotes.
        // A version goes inside the quotes, after the name.
        let quote = char::from(quote_byte(quotes));
        for (name, _) in role.names() {
            let written = uf_infra::into_string(uf_infra::cstr!("{quote}{name}{quote}"));
            offer(
                &mut completion.items,
                described,
                written.clone(),
                written,
                Some(name),
                Kind::Literal,
            );
        }
        return;
    };

    let contents = quoted.contents();
    let text = source.get(contents.start..contents.end).unwrap_or_default();
    let cursor = offset.clamp(contents.start, contents.end) - contents.start;
    let open_quote = quoted.quote.filter(|_| !quoted.terminated).map(char::from);

    match text.find('@') {
        Some(at) if cursor > at => {
            let Some(tool) = tool_named(role, &text[..at]) else {
                return;
            };
            let versions = Versions {
                typed: text.get(at + 1..cursor).unwrap_or_default(),
                replace: Span {
                    start: contents.start + at + 1,
                    end: contents.end,
                },
                open_quote,
            };
            match releases.lookup(tool) {
                Lookup::Ready(index) => versions.offer(&text[..at], index, completion),
                // Asked again as the user types, which is when it may be there.
                Lookup::Pending => completion.incomplete = true,
                Lookup::Unavailable => {}
            }
        }
        at => {
            let end = at.map_or(contents.end, |at| contents.start + at);
            let names = Described {
                detail: described.detail.clone(),
                documentation: described.documentation.clone(),
                replace: Span {
                    start: contents.start,
                    end,
                },
            };
            for (name, tool) in role.names() {
                let mut written = name.clone();
                // A name with releases is left open for its `@`; one without
                // is closed like any other value.
                if tool.is_none() && at.is_none() {
                    written.extend(open_quote);
                }
                offer(
                    &mut completion.items,
                    &names,
                    name,
                    written,
                    None,
                    Kind::Literal,
                );
            }
            // A name written out in full is one whose versions are the next
            // question: start fetching them now, not when the `@` is typed.
            let typed = text.get(..cursor).unwrap_or_default();
            if let Some(tool) = tool_named(role, typed) {
                let _ = releases.lookup(tool);
            }
        }
    }
}

/// The tool a name written for `role` has releases of.
fn tool_named(role: Role, name: &str) -> Option<Tool> {
    role.names()
        .into_iter()
        .find(|(written, _)| written == name)
        .and_then(|(_, tool)| tool)
}

/// How many releases one answer offers before what has been typed narrows them.
///
/// Node has published some eight hundred. Every one of them in every answer was
/// 158 KB and 31 ms per request on a debug build, measured, for a list an editor
/// then filters down to a handful. Past this many the answer is marked
/// incomplete — the protocol's way to have the editor ask again as the user
/// types — so `node@24.` still gets every 24.x, and `node@` gets every major
/// and the newest releases.
const RELEASES_AT_ONCE: usize = 100;

/// The version half of a spec being written.
struct Versions<'s> {
    /// What has been typed after the `@`, up to the cursor.
    typed: &'s str,
    /// Everything after the `@`, to the end of the string.
    replace: Span,
    /// The quote to close the string with, when it is not closed yet.
    open_quote: Option<char>,
}

impl Versions<'_> {
    /// Each major, then each release, newest first, as far as `typed` allows
    /// and no more than [`RELEASES_AT_ONCE`] releases.
    ///
    /// Pushed rather than [`offer`]ed: a major has no dot and a release has two,
    /// and the list has one entry per version, so there is nothing to
    /// deduplicate — and Node's list is long enough that looking would show.
    fn offer(&self, name: &str, index: &Index, completion: &mut Completion) {
        let items = &mut completion.items;
        let fetched = fetched(index);

        // The list is newest first, so the first release of a major that a
        // prefix can resolve to is the one it does resolve to: the answer
        // `Index::resolve` gives, from one pass rather than one per major.
        let mut majors: Vec<(&str, &Release)> = Vec::new();
        for release in index
            .releases
            .iter()
            .filter(|release| !release.is_prerelease() && is_semver(&release.version))
        {
            let major = major(release);
            if !majors.iter().any(|(seen, _)| *seen == major) {
                majors.push((major, release));
            }
        }
        for (major, newest) in majors
            .into_iter()
            .filter(|(major, _)| major.starts_with(self.typed))
        {
            let detail = match &newest.lts {
                Some(line) => {
                    uf_infra::into_string(uf_infra::cstr!("{} · LTS {line}", newest.version))
                }
                None => newest.version.clone(),
            };
            let documentation = uf_infra::into_string(uf_infra::cstr!(
                "The newest release of `{name}` that starts with `{major}`: **{version}** in the \
                 list fetched {fetched}. Resolved once and locked in `uf.lock`.",
                version = newest.version,
            ));
            self.push(items, major, Some(detail), Some(documentation));
        }

        let prereleases = self.typed.contains('-');
        let mut matching = index
            .releases
            .iter()
            .filter(|release| prereleases || !release.is_prerelease())
            .filter(|release| release.version.starts_with(self.typed));
        for release in matching.by_ref().take(RELEASES_AT_ONCE) {
            let detail = [
                release.date.clone(),
                release
                    .lts
                    .as_ref()
                    .map(|line| uf_infra::into_string(uf_infra::cstr!("LTS {line}"))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            self.push(
                items,
                &release.version,
                (!detail.is_empty()).then_some(detail),
                None,
            );
        }
        if matching.next().is_some() {
            completion.incomplete = true;
        }
    }

    fn push(
        &self,
        items: &mut Vec<Item>,
        version: &str,
        detail: Option<String>,
        documentation: Option<String>,
    ) {
        let mut written = version.to_owned();
        written.extend(self.open_quote);
        items.push(Item {
            label: version.to_owned(),
            kind: Kind::Version,
            detail,
            documentation,
            replace: self.replace,
            new_text: written,
            filter_text: None,
            sort_text: Some(uf_infra::into_string(uf_infra::cstr!("{:05}", items.len()))),
        });
    }
}

/// A release's major version, as written.
fn major(release: &Release) -> &str {
    release
        .version
        .split(['.', '-', '+'])
        .next()
        .unwrap_or_default()
}

/// Whether a version is `major.minor.patch` and nothing looser, which is the
/// only kind `uf_env::index::resolve` resolves a prefix to.
fn is_semver(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts = core.split('.');
    parts.clone().count() == 3
        && parts
            .into_iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// When a list was fetched, the way a sentence says it.
fn fetched(index: &Index) -> String {
    match index.age().map(|age| age.as_secs() / (24 * 60 * 60)) {
        Some(0) => String::from("today"),
        Some(1) => String::from("yesterday"),
        Some(days) => uf_infra::into_string(uf_infra::cstr!("{days} days ago")),
        None => String::from("by uf"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use uf_config::schema::{Schema, Step};
    use uf_env::index::FORMAT;
    use uf_infra::FxHashMap;

    use super::super::{Outline, complete as complete_config};
    use super::*;

    /// Release lists a test wrote, and whether a refresh is "running" for the
    /// tools it has none of.
    struct Fixed {
        lists: FxHashMap<Tool, Index>,
        pending: bool,
        asked: Vec<Tool>,
    }

    impl Releases for Fixed {
        fn lookup(&mut self, tool: Tool) -> Lookup<'_> {
            self.asked.push(tool);
            match self.lists.get(&tool) {
                Some(index) => Lookup::Ready(index),
                None if self.pending => Lookup::Pending,
                None => Lookup::Unavailable,
            }
        }
    }

    fn release(version: &str, date: Option<&str>, lts: Option<&str>) -> Release {
        Release {
            version: version.to_owned(),
            date: date.map(str::to_owned),
            lts: lts.map(str::to_owned),
        }
    }

    /// Node and Bun, newest first, the way `uf_env` caches them.
    fn fixed() -> Fixed {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let index = |tool, releases| Index {
            format: FORMAT,
            tool,
            fetched_at: now,
            sources: Vec::new(),
            releases,
        };
        let mut lists = FxHashMap::default();
        lists.insert(
            Tool::Node,
            index(
                Tool::Node,
                vec![
                    release("27.0.0-rc.1", Some("2026-10-02"), None),
                    release("26.10.0", Some("2026-10-01"), None),
                    release("26.8.2", Some("2026-09-09"), None),
                    release("24.14.0", Some("2026-08-20"), Some("Krypton")),
                    release("24.13.1", Some("2026-07-01"), Some("Krypton")),
                    release("22.20.0", Some("2026-06-01"), Some("Jod")),
                ],
            ),
        );
        lists.insert(
            Tool::Bun,
            index(
                Tool::Bun,
                vec![
                    release("1.4.2", None, None),
                    release("1.4.2-canary.1", None, None),
                    release("1.3.9", None, None),
                ],
            ),
        );
        Fixed {
            lists,
            pending: false,
            asked: Vec::new(),
        }
    }

    fn complete_at(marked: &str, releases: &mut Fixed) -> (String, Completion) {
        let offset = marked.find('‸').expect("a cursor");
        let source = marked.replacen('‸', "", 1);
        let outline = Outline::read(&source);
        let completion = complete_config(
            Schema::embedded(),
            &source,
            &outline,
            offset,
            QuoteStyle::Double,
            releases,
        );
        (source, completion)
    }

    fn labels(completion: &Completion) -> Vec<&str> {
        completion
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect()
    }

    fn accept(source: &str, item: &Item) -> String {
        uf_infra::into_string(uf_infra::cstr!(
            "{}{}{}",
            &source[..item.replace.start],
            item.new_text,
            &source[item.replace.end..]
        ))
    }

    /// The contract with `@uniflowed/config`: every key it types with a spec
    /// alias is found by that alias.
    #[test]
    fn every_tool_key_is_found_by_its_type() {
        let schema = Schema::embedded();
        for (path, role) in [
            (vec![Step::Key("runtime")], Role::Runtime),
            (
                vec![Step::Key("build"), Step::Key("runtime")],
                Role::Runtime,
            ),
            (vec![Step::Key("test"), Step::Key("runtime")], Role::Runtime),
            (vec![Step::Key("packageManager")], Role::PackageManager),
            (
                vec![Step::Key("test"), Step::Key("runner")],
                Role::TestRunner,
            ),
            (
                vec![Step::Key("build"), Step::Key("builder")],
                Role::Builder,
            ),
        ] {
            assert_eq!(Role::of(&schema.resolve(&path)), Some(role), "{path:?}");
        }
        // And a key that only looks like one is not.
        assert_eq!(
            Role::of(&schema.resolve(&[Step::Key("pm"), Step::Key("packageManager")])),
            None
        );
    }

    /// Every name offered is one the role's own parser accepts.
    #[test]
    fn every_name_offered_parses_as_its_role() {
        use uf_config::{PackageManagerSpec, RuntimeSpec};

        for (name, _) in Role::Runtime.names() {
            assert!(RuntimeSpec::parse(&name).is_ok(), "{name}");
        }
        for (name, _) in Role::PackageManager.names() {
            assert!(PackageManagerSpec::parse(&name).is_ok(), "{name}");
        }
        for (name, _) in Role::TestRunner.names() {
            assert!(TestRunnerSpec::parse(&name).is_ok(), "{name}");
        }
        assert_eq!(
            BuilderSpec::parse(&Role::Builder.names()[0].0),
            Ok(BuilderSpec::Vite)
        );
    }

    #[test]
    fn each_role_offers_the_names_it_takes() {
        let mut releases = fixed();
        for (marked, names) in [
            (
                "export default defineConfig({ runtime: \"‸\" })",
                &["node", "bun", "deno"][..],
            ),
            (
                "export default defineConfig({ build: { runtime: \"‸\" } })",
                &["node", "bun", "deno"],
            ),
            (
                "export default defineConfig({ packageManager: \"‸\" })",
                &["npm", "pnpm", "yarn", "bun"],
            ),
            (
                "export default defineConfig({ test: { runner: \"‸\" } })",
                &["uf", "bun"],
            ),
            (
                "export default defineConfig({ build: { builder: \"‸\" } })",
                &["vite"],
            ),
        ] {
            let (_, completion) = complete_at(marked, &mut releases);
            assert_eq!(labels(&completion), names, "{marked}");
            assert!(!completion.incomplete);
        }
    }

    #[test]
    fn a_name_is_replaced_up_to_its_at_and_left_open_for_one() {
        let mut releases = fixed();
        let (source, completion) = complete_at(
            "export default defineConfig({ runtime: \"b‸un@1.4\" })",
            &mut releases,
        );
        let deno = completion
            .items
            .iter()
            .find(|item| item.label == "deno")
            .unwrap();
        assert_eq!(
            accept(&source, deno),
            "export default defineConfig({ runtime: \"deno@1.4\" })"
        );

        // An unclosed string: a name with releases stays open for its `@`,
        // and one without is closed.
        let (source, completion) = complete_at(
            "export default defineConfig({ test: { runner: \"‸",
            &mut releases,
        );
        let [uf, bun] = &completion.items[..] else {
            panic!("{:?}", labels(&completion));
        };
        assert_eq!(
            accept(&source, uf),
            "export default defineConfig({ test: { runner: \"uf\""
        );
        assert_eq!(
            accept(&source, bun),
            "export default defineConfig({ test: { runner: \"bun"
        );

        // Unquoted: quoted names, filtered on the name.
        let (_, completion) =
            complete_at("export default defineConfig({ runtime: ‸ })", &mut releases);
        assert_eq!(labels(&completion), ["\"node\"", "\"bun\"", "\"deno\""]);
        assert_eq!(completion.items[0].filter_text.as_deref(), Some("node"));
    }

    #[test]
    fn after_the_at_come_majors_then_releases_newest_first() {
        let mut releases = fixed();
        let (source, completion) = complete_at(
            "export default defineConfig({ runtime: \"node@‸\" })",
            &mut releases,
        );

        assert_eq!(
            labels(&completion),
            [
                "26", "24", "22", "26.10.0", "26.8.2", "24.14.0", "24.13.1", "22.20.0"
            ]
        );
        let items = &completion.items;
        assert!(
            items
                .windows(2)
                .all(|pair| pair[0].sort_text < pair[1].sort_text)
        );
        assert!(items.iter().all(|item| item.kind == Kind::Version));

        // A major says what it resolves to, and that it is locked.
        assert_eq!(items[1].detail.as_deref(), Some("24.14.0 · LTS Krypton"));
        let said = items[1].documentation.as_deref().unwrap();
        assert!(
            said.contains("**24.14.0**") && said.contains("uf.lock"),
            "{said}"
        );
        assert_eq!(items[4].detail.as_deref(), Some("2026-09-09"));

        assert_eq!(
            accept(&source, &items[1]),
            "export default defineConfig({ runtime: \"node@24\" })"
        );
    }

    #[test]
    fn what_is_typed_after_the_at_narrows_the_versions() {
        let mut releases = fixed();
        let (_, completion) = complete_at(
            "export default defineConfig({ runtime: \"node@24.‸\" })",
            &mut releases,
        );
        assert_eq!(labels(&completion), ["24.14.0", "24.13.1"]);

        // A `-` asks for prereleases; a prerelease line gets no major.
        let (_, completion) = complete_at(
            "export default defineConfig({ runtime: \"bun@1.4.2-‸\" })",
            &mut releases,
        );
        assert_eq!(labels(&completion), ["1.4.2-canary.1"]);

        // The whole version is replaced, and an open string is closed.
        let (source, completion) = complete_at(
            "export default defineConfig({ test: { runner: \"bun@1‸",
            &mut releases,
        );
        assert_eq!(labels(&completion), ["1", "1.4.2", "1.3.9"]);
        assert_eq!(
            accept(&source, &completion.items[1]),
            "export default defineConfig({ test: { runner: \"bun@1.4.2\""
        );
    }

    #[test]
    fn a_name_without_releases_or_outside_the_role_gets_no_versions() {
        let mut releases = fixed();
        for marked in [
            "export default defineConfig({ runtime: \"pnpm@‸\" })",
            "export default defineConfig({ test: { runner: \"uf@‸\" } })",
            "export default defineConfig({ build: { builder: \"vite@‸\" } })",
            "export default defineConfig({ packageManager: \"npm@‸\" })",
        ] {
            let (_, completion) = complete_at(marked, &mut releases);
            assert!(
                completion.items.is_empty(),
                "{marked}: {:?}",
                labels(&completion)
            );
            // `npm` has releases, and this fixture has no list of them.
            assert!(!completion.incomplete, "{marked}");
        }
    }

    #[test]
    fn a_long_list_is_offered_a_hundred_releases_at_a_time() {
        let many: Vec<Release> = (0..30u32)
            .rev()
            .flat_map(|major| {
                (0..10u32).rev().map(move |minor| {
                    release(uf_infra::cstr!("{major}.{minor}.0").as_str(), None, None)
                })
            })
            .collect();
        let mut lists = FxHashMap::default();
        lists.insert(
            Tool::Node,
            Index {
                format: FORMAT,
                tool: Tool::Node,
                fetched_at: 0,
                sources: Vec::new(),
                releases: many,
            },
        );
        let mut releases = Fixed {
            lists,
            pending: false,
            asked: Vec::new(),
        };

        // Every major, then the newest hundred of three hundred releases, and
        // a list the editor is told to ask for again.
        let (_, completion) = complete_at(
            "export default defineConfig({ runtime: \"node@‸\" })",
            &mut releases,
        );
        assert_eq!(completion.items.len(), 30 + RELEASES_AT_ONCE);
        assert_eq!(completion.items[0].label, "29");
        assert_eq!(completion.items[0].detail.as_deref(), Some("29.9.0"));
        assert_eq!(completion.items[30].label, "29.9.0");
        assert!(completion.incomplete);

        // Narrowed to one major: all of it, and nothing left to ask for.
        let (_, completion) = complete_at(
            "export default defineConfig({ runtime: \"node@12.‸\" })",
            &mut releases,
        );
        assert_eq!(completion.items.len(), 10);
        assert!(!completion.incomplete);
    }

    #[test]
    fn a_list_being_fetched_is_an_incomplete_answer() {
        let mut releases = fixed();
        releases.pending = true;
        let (_, completion) = complete_at(
            "export default defineConfig({ packageManager: \"pnpm@‸\" })",
            &mut releases,
        );

        assert!(completion.items.is_empty());
        assert!(completion.incomplete);
    }

    #[test]
    fn a_name_written_in_full_starts_fetching_its_versions() {
        let mut releases = fixed();
        let _ = complete_at(
            "export default defineConfig({ runtime: \"deno‸\" })",
            &mut releases,
        );
        assert_eq!(releases.asked, [Tool::Deno]);

        releases.asked.clear();
        let _ = complete_at(
            "export default defineConfig({ runtime: \"de‸\" })",
            &mut releases,
        );
        assert!(releases.asked.is_empty());
    }
}
