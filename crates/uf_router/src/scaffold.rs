//! Writing a route, from the same grammar [`discover_routes`] reads one.
//!
//! A route is a directory and one to four reserved files, and until this
//! module a reader wrote them by hand and found out from `uf build` whether
//! the names were right. `uf routes add /articles/[slug]` writes them instead.
//!
//! # Why it is here rather than in the CLI
//!
//! Because the generator and the discoverer have to be one list. Every file
//! name below comes out of [`ReservedFile::file_name`] and every directory
//! name is classified by [`classify_route_segment`] — the same two functions
//! `discover_routes` calls — so a role added to [`ReservedRole`] is a role
//! `uf routes add` can write, and a directory spelling the router refuses is a
//! spelling this refuses with the *same sentence*. A generator with its own
//! table of names would be the third copy of the grammar, and the first two
//! disagreeing is what ubugeeei-prod/uf#224, #291, #386 and #437 all are.
//!
//! It is also why this refuses more than a scaffold usually would. A path with
//! `(.)photo` in it, and a `[...rest]` with a segment after it, are rejected
//! here rather than written and then reported by the next `uf build`: the point
//! of one grammar is that the answer does not depend on which tool you ask.
//!
//! `@team` is refused for a different reason, and the difference is worth
//! keeping: a slot is a route uf serves, but it is not a *URL*, and this
//! command's argument is a URL. See [`ScaffoldError::SlotSegment`].
//!
//! [`discover_routes`]: crate::discover_routes

use camino::{Utf8Path, Utf8PathBuf};

use crate::classify_route_segment;
use crate::reserved::{ReservedFile, ReservedRole, ReservedVariant, RouteSegment};

/// What `uf routes add` is asked to write beside the page.
///
/// The page is not optional and so is not here: a route without one is a
/// directory. Everything else is a role a segment *may* have, and each is one
/// flag on the command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RouteParts {
    /// Also write `_uf.layout.js` — a wrapper for this path and everything
    /// under it.
    pub layout: bool,
    /// Also write `_uf.middleware.js` — what runs before this path answers.
    pub middleware: bool,
    /// Give the page a `loader`.
    ///
    /// Not a file: a loader is an export of the page module, which is the one
    /// place this differs from the flags either side of it. `--loader` is
    /// still the right spelling for it, because from where a reader stands it
    /// is the same question — "does this route load data" — and which of the
    /// four answers is a separate file is uf's business rather than theirs.
    pub loader: bool,
}

/// A file `uf routes add` will write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldFile {
    /// Where it goes: the `app_root` [`scaffold_route`] was given, joined with
    /// the route's directory and the file's own name.
    pub path: Utf8PathBuf,
    /// What it does, so a caller can report the set by role rather than by
    /// re-reading the file names it was just handed.
    pub role: ReservedRole,
    /// What goes in it.
    pub contents: String,
}

/// Why a path is not one `uf routes add` can write.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScaffoldError {
    /// A directory spelled the way an intercepting route is. The sentence is
    /// [`RouteSegment::unsupported_reason`]'s, so this and `uf build` and
    /// `uf lint` all say the same thing about the same spelling.
    #[error("{segment}: {reason}")]
    UnsupportedSegment {
        /// The segment, as it was written.
        segment: String,
        /// What is wrong with it and what to do instead.
        reason: String,
    },
    /// A `@slot` in the path this command was given.
    ///
    /// Not a refusal of the feature — uf serves parallel routes — but of the
    /// argument. `uf routes add` takes a URL and writes the page that answers
    /// it, and a slot has no URL: it renders into the layout of the segment
    /// that declares it, at that segment's own paths. So there is nothing here
    /// for this command to do that a reader would recognise as "adding a
    /// route", and writing the directory alone would leave a slot with no
    /// layout to render into, which the next `uf build` refuses.
    #[error(
        "`{segment}` is a parallel-route slot, and `uf routes add` takes a URL — a slot has none. \
         It renders into the layout of the segment that declares it, at that segment's own paths. \
         Create the directory beside that layout and put the slot's pages in it; \
         `uf routes add` writes the pages the URL names."
    )]
    SlotSegment {
        /// The segment, as it was written.
        segment: String,
    },
    /// A `[...param]` with another routing segment after it, which is a page
    /// no URL can reach — see [`crate::RouterError::NonTerminalCatchAll`].
    #[error(
        "`{catch_all}` is a catch-all and `{following}` comes after it, so no URL could reach \
         this page — a catch-all takes every segment of the path that is left. Put `{catch_all}` \
         last, or write it as `[{parameter}]`."
    )]
    NonTerminalCatchAll {
        /// The catch-all segment, as it was written.
        catch_all: String,
        /// The first routing segment after it.
        following: String,
        /// The catch-all's parameter name, for the suggested spelling.
        parameter: String,
    },
    /// A segment that is empty or that names a parameter with no name:
    /// `//`, `[]`, `[...]`.
    #[error(
        "`{segment}` is not a path segment uf can turn into a directory. A segment is a name, \
         `[name]`, `[...name]` or `(group)`."
    )]
    EmptySegment {
        /// The segment, as it was written.
        segment: String,
    },
    /// The route already has a page, and writing one would replace what is
    /// there.
    #[error("{path} already exists; `uf routes add` never overwrites a file")]
    Exists {
        /// The file that is already on disk.
        path: Utf8PathBuf,
    },
}

/// The directory `path` names, relative to the router root, checked.
///
/// Separate from [`scaffold_route`] because `uf routes add` reports the
/// directory it is about to write into before it writes anything, and the
/// checks belong to the path rather than to the writing.
///
/// A leading `/` is optional and a trailing one is ignored, so `/articles/`,
/// `/articles` and `articles` are one route. `/` on its own is the router
/// root, whose directory is the router root itself.
fn route_directory(path: &str) -> Result<Utf8PathBuf, ScaffoldError> {
    let mut directory = Utf8PathBuf::new();
    let mut catch_all: Option<&str> = None;

    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let classified = classify_route_segment(segment);
        if let Some(reason) = classified.unsupported_reason(segment) {
            return Err(ScaffoldError::UnsupportedSegment {
                segment: segment.to_owned(),
                reason,
            });
        }
        if classified.slot().is_some() {
            return Err(ScaffoldError::SlotSegment {
                segment: segment.to_owned(),
            });
        }
        if matches!(
            classified,
            RouteSegment::Param("") | RouteSegment::CatchAll("") | RouteSegment::Literal("")
        ) {
            return Err(ScaffoldError::EmptySegment {
                segment: segment.to_owned(),
            });
        }
        // A `(group)` contributes no URL segment, so it is not what a
        // catch-all leaves nothing for — the same exception
        // `non_terminal_catch_all` makes over a discovered tree.
        if classified != RouteSegment::Group {
            if let Some(found) = catch_all {
                return Err(ScaffoldError::NonTerminalCatchAll {
                    parameter: found
                        .trim_start_matches("[...")
                        .trim_end_matches(']')
                        .to_owned(),
                    catch_all: found.to_owned(),
                    following: segment.to_owned(),
                });
            }
            if matches!(classified, RouteSegment::CatchAll(_)) {
                catch_all = Some(segment);
            }
        }
        directory.push(segment);
    }
    Ok(directory)
}

/// The files `uf routes add <path>` will write, in the order it writes them.
///
/// `app_root` is the router root as a path from the project root — the
/// directory `uf.config.js` calls `app.router.root` — and every returned
/// [`ScaffoldFile::path`] is under it.
///
/// Nothing is written and nothing is read except to refuse: a file that
/// already exists is [`ScaffoldError::Exists`] rather than a silent overwrite,
/// because the alternative is a command that can lose a page somebody wrote.
/// The check is here rather than in the caller so that it happens *before* the
/// first file is written and a run either writes its whole set or none of it.
///
/// # Errors
///
/// When `path` is not a route path uf serves, or when one of the files is
/// already on disk.
pub fn scaffold_route(
    app_root: &Utf8Path,
    path: &str,
    parts: RouteParts,
) -> Result<Vec<ScaffoldFile>, ScaffoldError> {
    let relative = route_directory(path)?;
    let directory = app_root.join(&relative);
    let mut files = Vec::new();

    files.push(ScaffoldFile {
        path: directory.join(file_name(ReservedRole::Page)),
        role: ReservedRole::Page,
        contents: page_source(&relative, parts.loader),
    });
    if parts.layout {
        files.push(ScaffoldFile {
            path: directory.join(file_name(ReservedRole::Layout)),
            role: ReservedRole::Layout,
            contents: LAYOUT_SOURCE.to_owned(),
        });
    }
    if parts.middleware {
        files.push(ScaffoldFile {
            path: directory.join(file_name(ReservedRole::Middleware)),
            role: ReservedRole::Middleware,
            contents: MIDDLEWARE_SOURCE.to_owned(),
        });
    }

    for file in &files {
        if file.path.exists() {
            return Err(ScaffoldError::Exists {
                path: file.path.clone(),
            });
        }
    }
    Ok(files)
}

/// The name uf reserves for `role`'s default variant.
///
/// One call rather than a literal per role, which is the whole reason this
/// module is in this crate: `_uf.layout.js` written out here would be a second
/// spelling of [`ReservedRole::as_str`]'s answer.
fn file_name(role: ReservedRole) -> String {
    ReservedFile {
        role,
        variant: ReservedVariant::Default,
    }
    .file_name()
}

/// The parameters `relative` captures, in order, as `(name, is_catch_all)`.
fn parameters(relative: &Utf8Path) -> Vec<(&str, bool)> {
    relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| match classify_route_segment(segment) {
            RouteSegment::Param(name) => Some((name, false)),
            RouteSegment::CatchAll(name) => Some((name, true)),
            _ => None,
        })
        .collect()
}

/// The placeholder heading for the page at `relative`.
///
/// The last segment that names something, in `PascalCase`. A heading and not
/// the component's name: the component is exported as `Page`, because
/// `module.default ?? module.Page` is how the runtime resolves one and a third
/// spelling would be a page uf silently does not render.
///
/// Only reached for a route that captures nothing — a route with parameters
/// puts the last of them on screen instead — so the fallback covers the router
/// root, a path that is groups all the way down, and a segment that is not an
/// identifier.
fn heading(relative: &Utf8Path) -> String {
    let last = relative
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| match classify_route_segment(segment) {
            RouteSegment::Group => None,
            RouteSegment::Param(name) | RouteSegment::CatchAll(name) => Some(name),
            RouteSegment::Literal(name) => Some(name),
            RouteSegment::Slot(_) | RouteSegment::Interception { .. } => None,
        })
        .next_back();
    let Some(last) = last else {
        return "Page".to_owned();
    };

    let mut name = String::new();
    let mut capitalize = true;
    for character in last.chars() {
        if character == '-' || character == '_' || character == '.' {
            capitalize = true;
            continue;
        }
        if capitalize {
            name.extend(character.to_uppercase());
            capitalize = false;
        } else {
            name.push(character);
        }
    }
    // A segment that is punctuation, or that starts with a digit: neither is a
    // JavaScript identifier, and a scaffold that does not parse is worse than
    // one whose component is called `Page`.
    if name.is_empty() || name.starts_with(|character: char| character.is_ascii_digit()) {
        return "Page".to_owned();
    }
    name
}

/// The `_uf.page.js` for a route at `relative`.
///
/// # Two properties the generated source has to have
///
/// It is formatted the way `uf fmt` writes it, because a project scaffolds a
/// route and then runs `uf fmt --check` in CI: a generated file the formatter
/// disagrees with fails on code nobody wrote.
///
/// And it is exported by name rather than as a `default`. The runtime resolves
/// a page as `module.default ?? module.Page`, so either spelling runs — but
/// `react/no-default-export-component` is on by default, so the `default` one
/// is a warning on every file this command writes, and a scaffold whose first
/// act is to make `uf lint` speak has taught the reader that uf disagrees with
/// itself. `uf new` already writes `export component Page`; this is the same
/// answer for the routes after the first.
///
/// `the_files_a_scaffolded_route_is` in `crates/uf_cli/tests/cli.rs` runs the
/// formatter and the linter over what this writes, which is what keeps both
/// claims true rather than remembered.
fn page_source(relative: &Utf8Path, loader: bool) -> String {
    let params = parameters(relative);
    let mut source = String::from("// @flow\nimport * as React from \"@uniflowed/react\";\n");
    if loader {
        source.push_str("import type { LoaderArgs } from \"@uniflowed/router\";\n");
    }
    source.push('\n');

    if loader {
        source
            .push_str("export async function loader({ params }: LoaderArgs): Promise<{ ... }> {\n");
        if params.is_empty() {
            source.push_str("  void params;\n  return {};\n}\n\n");
        } else {
            source.push_str("  return { params };\n}\n\n");
        }
    }

    let mut props = Vec::new();
    if !params.is_empty() {
        let fields = params
            .iter()
            .map(|(name, catch_all)| {
                let ty = if *catch_all {
                    "$ReadOnlyArray<string>"
                } else {
                    "string"
                };
                format!("+{name}: {ty}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        props.push(format!("params: {{| {fields} |}}"));
    }
    if loader {
        props.push("data: { ... }".to_owned());
    }

    source.push_str(&format!("export component Page({}) {{\n", props.join(", ")));
    if loader {
        source.push_str("  void data;\n");
    }
    // The last parameter as the heading where the route has one, so the
    // placeholder reads the props it was just given rather than declaring them
    // and ignoring them — which is the line the reader was about to write.
    let title = match params.last() {
        Some((name, true)) => format!("{{params.{name}.join(\"/\")}}"),
        Some((name, false)) => format!("{{params.{name}}}"),
        None => heading(relative),
    };
    source.push_str(&format!(
        "  return (\n    <section>\n      <h1>{title}</h1>\n    </section>\n  );\n}}\n"
    ));
    source
}

/// The `_uf.layout.js` a `--layout` writes.
///
/// Content and not a document: a layout that renders `<html>` is the *root*
/// one, and a second one below it would put a document inside a document. The
/// root layout is `uf new`'s to write, once, and this command is for the routes
/// after it.
///
/// `Layout` by name, for the reason [`page_source`] gives about `Page`.
const LAYOUT_SOURCE: &str = r#"// @flow
import * as React from "@uniflowed/react";

export component Layout(children: React.Node) {
  return <div>{children}</div>;
}
"#;

/// The `_uf.middleware.js` a `--middleware` writes.
///
/// It returns nothing, which is the shape that means "carry on": a middleware
/// answers by returning a `Response` and passes by returning `undefined`, so
/// the scaffold is the passing one and the reader adds the check.
///
/// `middleware` by name, which is the second of the two spellings the runner
/// takes and the one `react/no-default-export-component` does not report.
const MIDDLEWARE_SOURCE: &str = r#"// @flow

export function middleware(request: Request): Response | void {
  void request;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(path: &str) -> Result<String, ScaffoldError> {
        route_directory(path).map(|found| found.to_string())
    }

    #[test]
    fn a_leading_and_a_trailing_slash_name_the_same_route() {
        assert_eq!(directory("/articles").unwrap(), "articles");
        assert_eq!(directory("articles").unwrap(), "articles");
        assert_eq!(directory("/articles/").unwrap(), "articles");
    }

    #[test]
    fn the_root_route_is_the_router_root_itself() {
        assert_eq!(directory("/").unwrap(), "");
    }

    #[test]
    fn a_parameter_is_a_directory_spelled_the_way_the_router_reads_one() {
        assert_eq!(directory("/articles/[slug]").unwrap(), "articles/[slug]");
        assert_eq!(directory("/docs/[...path]").unwrap(), "docs/[...path]");
        assert_eq!(
            directory("/(marketing)/about").unwrap(),
            "(marketing)/about"
        );
    }

    #[test]
    fn a_spelling_the_router_refuses_is_refused_here_with_the_same_sentence() {
        // The point of one grammar: `uf routes add` must not write a route
        // that the next `uf build` reports. The sentence is the one
        // `RouteSegment::unsupported_reason` gives, so a reader gets the same
        // answer whichever tool told them.
        let error = directory("/feed/(.)photo").unwrap_err();
        let ScaffoldError::UnsupportedSegment { reason, .. } = &error else {
            panic!("expected an unsupported segment, got {error:?}");
        };
        assert_eq!(
            reason.as_str(),
            classify_route_segment("(.)photo")
                .unsupported_reason("(.)photo")
                .unwrap()
                .as_str()
        );
    }

    /// A slot is refused by this command and not by the router, and the message
    /// has to be about the command: uf serves parallel routes, and what it does
    /// not have is a URL for one to be added at.
    #[test]
    fn a_slot_is_refused_because_this_command_takes_a_url() {
        let error = directory("/dashboard/@team/members").unwrap_err();
        let ScaffoldError::SlotSegment { segment } = &error else {
            panic!("expected a slot segment, got {error:?}");
        };
        assert_eq!(segment, "@team");
        let message = error.to_string();
        assert!(message.contains("takes a URL"), "{message}");
        // And not the sentence the router uses for what it refuses: there is
        // no such sentence for a slot any more.
        assert_eq!(
            classify_route_segment("@team").unsupported_reason("@team"),
            None
        );
    }

    #[test]
    fn a_catch_all_with_a_segment_after_it_is_refused_rather_than_written() {
        let error = directory("/docs/[...path]/edit").unwrap_err();
        assert!(matches!(error, ScaffoldError::NonTerminalCatchAll { .. }));
        assert!(error.to_string().contains("[...path]"));
        assert!(error.to_string().contains("[path]"));
    }

    #[test]
    fn a_group_below_a_catch_all_is_not_a_segment_after_it() {
        assert_eq!(
            directory("/files/[...path]/(internal)").unwrap(),
            "files/[...path]/(internal)"
        );
    }

    #[test]
    fn a_parameter_with_no_name_is_not_a_segment() {
        assert!(matches!(
            directory("/articles/[]"),
            Err(ScaffoldError::EmptySegment { .. })
        ));
        assert!(matches!(
            directory("/articles/[...]"),
            Err(ScaffoldError::EmptySegment { .. })
        ));
    }

    #[test]
    fn the_names_come_from_the_reserved_grammar_rather_than_from_literals() {
        let files = scaffold_route(
            Utf8Path::new("/nowhere/app"),
            "/articles/[slug]",
            RouteParts {
                layout: true,
                middleware: true,
                loader: false,
            },
        )
        .unwrap();

        let names: Vec<&str> = files
            .iter()
            .map(|file| file.path.file_name().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                ReservedFile {
                    role: ReservedRole::Page,
                    variant: ReservedVariant::Default
                }
                .file_name(),
                ReservedFile {
                    role: ReservedRole::Layout,
                    variant: ReservedVariant::Default
                }
                .file_name(),
                ReservedFile {
                    role: ReservedRole::Middleware,
                    variant: ReservedVariant::Default
                }
                .file_name(),
            ]
        );
        assert!(
            files[0]
                .path
                .as_str()
                .ends_with("app/articles/[slug]/_uf.page.js")
        );
    }

    #[test]
    fn a_page_types_the_parameters_its_own_path_captures() {
        let source = page_source(Utf8Path::new("docs/[...path]"), false);
        assert!(source.contains("+path: $ReadOnlyArray<string>"));

        let single = page_source(Utf8Path::new("articles/[slug]"), false);
        assert!(single.contains("+slug: string"));
        assert!(!single.contains("$ReadOnlyArray"));
    }

    #[test]
    fn a_page_with_no_parameters_takes_no_props() {
        let source = page_source(Utf8Path::new("about"), false);
        assert!(source.contains("export component Page() {"));
        assert!(source.contains("<h1>About</h1>"));
    }

    #[test]
    fn the_loader_is_an_export_of_the_page_rather_than_a_file_of_its_own() {
        let files = scaffold_route(
            Utf8Path::new("/nowhere/app"),
            "/articles",
            RouteParts {
                loader: true,
                ..RouteParts::default()
            },
        )
        .unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].contents.contains("export async function loader("));
    }

    #[test]
    fn the_page_is_exported_under_the_name_the_runtime_resolves() {
        // `module.default ?? module.Page`, and not the `default` half:
        // `react/no-default-export-component` is on by default, so a scaffold
        // that used it would make `uf lint` speak on every file this writes.
        for source in [
            page_source(Utf8Path::new("articles/[slug]"), true),
            page_source(Utf8Path::new("about"), false),
        ] {
            assert!(source.contains("export component Page("), "{source}");
            assert!(!source.contains("export default"), "{source}");
        }
        assert!(!LAYOUT_SOURCE.contains("export default"));
        assert!(LAYOUT_SOURCE.contains("export component Layout("));
        assert!(!MIDDLEWARE_SOURCE.contains("export default"));
        assert!(MIDDLEWARE_SOURCE.contains("export function middleware("));
    }

    #[test]
    fn the_placeholder_reads_the_parameters_the_page_just_declared() {
        // A page that types `+slug: string` and then renders a constant is a
        // scaffold whose first edit is to delete a line of itself.
        assert!(
            page_source(Utf8Path::new("articles/[slug]"), false).contains("<h1>{params.slug}</h1>")
        );
        assert!(
            page_source(Utf8Path::new("docs/[...path]"), false)
                .contains(r#"<h1>{params.path.join("/")}</h1>"#)
        );
    }

    #[test]
    fn a_route_that_captures_nothing_is_headed_by_its_own_segment() {
        assert_eq!(heading(Utf8Path::new("release-notes")), "ReleaseNotes");
        assert_eq!(heading(Utf8Path::new("")), "Page");
        // A group names nothing, so the segment above it is the heading.
        assert_eq!(heading(Utf8Path::new("about/(marketing)")), "About");
        // Not an identifier, so not a name: the scaffold has to parse.
        assert_eq!(heading(Utf8Path::new("2024")), "Page");
    }
}
