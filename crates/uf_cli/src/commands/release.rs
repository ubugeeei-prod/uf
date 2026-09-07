//! `uf publish` and `uf release`: the plans written to `.uf`.
//!
//! `uf prepare` was here too, when it was a plan rather than a run of one.
//! It is in [`super::prepare`] now, because it stopped being a file this
//! command writes and became five steps that do things.

use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_term::{KeyValue, Status, Tone};

use crate::cli::ReleaseBump;
use crate::support::{write_json_file, yes_no};
use crate::ui::Ui;

pub(crate) fn publish(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("publish.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "registry": resolved.config.publish.registry.as_str(),
            "dryRun": resolved.config.publish.dry_run,
            "firstPublish": {
                "mode": resolved.config.publish.first_publish.mode,
                "localBootstrap": resolved.config.publish.first_publish.local_bootstrap,
            },
            "trustedPublish": {
                "provider": resolved.config.publish.trusted_publish.provider,
                "tokenless": resolved.config.publish.trusted_publish.tokenless,
                "trigger": resolved.config.publish.trusted_publish.trigger,
            },
        }),
    )?;

    let registry = resolved.config.publish.registry.to_string();
    let first_publish = format!("{:?}", resolved.config.publish.first_publish.mode);
    let provider = format!("{:?}", resolved.config.publish.trusted_publish.provider);
    let trigger = format!("{:?}", resolved.config.publish.trusted_publish.trigger);
    let manifest_path = manifest.to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf publish", Some(&registry));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("registry", &registry, Tone::Accent),
                KeyValue::new("dry run", yes_no(resolved.config.publish.dry_run)),
                KeyValue::new("first publish", &first_publish),
                KeyValue::new(
                    "local bootstrap",
                    yes_no(resolved.config.publish.first_publish.local_bootstrap),
                ),
                KeyValue::new("trusted provider", &provider),
                KeyValue::new(
                    "tokenless",
                    yes_no(resolved.config.publish.trusted_publish.tokenless),
                ),
                KeyValue::new("trigger", &trigger),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, "publish plan written");
    });
    Ok(())
}

pub(crate) fn release(cwd: &Utf8Path, ui: &mut Ui, bump: ReleaseBump, force: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let current_version = env!("CARGO_PKG_VERSION");
    let next_version = bump_semver(current_version, bump)?;
    let tag = format!("{}{}", resolved.config.release.tag_prefix, next_version);
    let published = Published::of(&resolved.root, &tag);
    if let Some(refusal) = published.refusal(&tag, current_version, force) {
        bail!(refusal);
    }
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let changelog = write_changelog(&resolved.root, &tag, &resolved.config.release.tag_prefix)?;
    let manifest = state_dir.join("release.json");
    let unnumbered: Vec<String> = changelog
        .as_ref()
        .map(|written| written.unnumbered.clone())
        .unwrap_or_default();
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "bump": format!("{bump:?}"),
            "currentVersion": current_version,
            "nextVersion": next_version,
            "tag": tag,
            "command": resolved.config.release.command.as_str(),
            "publish": resolved.config.release.publish,
            "trustedTrigger": resolved.config.publish.trusted_publish.trigger,
            "changelog": changelog.as_ref().map(Changelog::path),
            "changes": changelog.as_ref().map_or(0, |written| written.changes),
            "unnumbered": unnumbered,
        }),
    )?;

    let bump_label = format!("{bump:?}");
    let command = resolved.config.release.command.to_string();
    let trigger = format!("{:?}", resolved.config.publish.trusted_publish.trigger);
    let manifest_path = manifest.to_string();
    let changelog_path = changelog.as_ref().map(Changelog::path);
    let changelog_row = changelog_path
        .as_deref()
        .map(|path| KeyValue::toned("changelog", path, Tone::Path));
    let summary = match &changelog {
        Some(written) => format!(
            "release {tag} planned, {} change{} written to the changelog",
            written.changes,
            if written.changes == 1 { "" } else { "s" }
        ),
        None => format!("release {tag} planned"),
    };
    let unnumbered_rows: Vec<&str> = unnumbered.iter().map(String::as_str).collect();
    let unnumbered_summary = format!(
        "{} commit{} in the range carr{} no pull request number; nothing downstream can find {} by one",
        unnumbered.len(),
        if unnumbered.len() == 1 { "" } else { "s" },
        if unnumbered.len() == 1 { "ies" } else { "y" },
        if unnumbered.len() == 1 { "it" } else { "them" },
    );

    ui.render(|renderer, out| {
        renderer.banner(out, "uf release", Some(&tag));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("bump", &bump_label),
                KeyValue::new("current version", current_version),
                KeyValue::toned("next version", &next_version, Tone::Accent),
                KeyValue::toned("tag", &tag, Tone::Accent),
                KeyValue::new("command", &command),
                KeyValue::new("publish", yes_no(resolved.config.release.publish)),
                KeyValue::new("trusted trigger", &trigger),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
            ],
        );
        if let Some(entry) = &changelog_row {
            renderer.key_values(out, 2, std::slice::from_ref(entry));
        }
        if !unnumbered_rows.is_empty() {
            renderer.blank(out);
            renderer.status(out, Status::Warn, &unnumbered_summary);
            renderer.bullet_list(out, 4, &unnumbered_rows);
        }
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// What the tree already says about the version `uf release` is planning.
///
/// `uf release <bump>` takes the version it is planning from the version
/// compiled into the binary running it, which is right — the binary that cuts a
/// release is the binary being released — and had no guard against being an
/// *old* binary. `uf@0.0.0-alpha.7`'s binary, run in a tree already released as
/// `uf@0.0.0-alpha.8`, planned alpha.8 again: it rewrote the published alpha.8
/// section with the commits made *since* that tag, which are alpha.9's, deleted
/// the thirty-eight lines of summary and hand-placed entries in it, and
/// reported success. Nothing else noticed, because nothing else was asked:
/// neither the tag nor the section it was about to overwrite was consulted.
///
/// Both are cheap, and both are consulted here. See #457.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Published {
    /// The repository has a tag of exactly this name.
    tagged: bool,
    /// `CHANGELOG.md` already has a `## <tag>` heading of its own.
    sectioned: bool,
}

impl Published {
    /// What `root` says about `tag`.
    ///
    /// Neither question can fail into a refusal: a directory that is not a
    /// repository has no tags, and a tree with no `CHANGELOG.md` has no
    /// sections. Both are the ordinary state of a first release.
    fn of(root: &Utf8Path, tag: &str) -> Self {
        let tagged = git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/tags/{tag}"),
            ],
        )
        .is_some_and(|line| !line.trim().is_empty());
        let heading = format!("## {tag}");
        let sectioned = fs::read_to_string(root.join("CHANGELOG.md")).is_ok_and(|changelog| {
            changelog
                .lines()
                .any(|line| line.trim_end() == heading.as_str())
        });
        Self { tagged, sectioned }
    }

    /// Why this release must not be written, or [`None`] to go ahead.
    ///
    /// The changelog is append-mostly: a section a release has been cut from is
    /// finished, and the three ways this file has lost content were all a tool
    /// deciding otherwise on its own. So the rule has two tiers.
    ///
    /// A section with no tag is a release being *prepared* — `uf release alpha`
    /// is run again when a commit lands late — and `--force` covers it, because
    /// the person asking is the person who wrote it.
    ///
    /// A tag is a release that went out. `--force` deliberately does not cover
    /// it: regenerating a section a tag points at should be harder than typing a
    /// flag, and there is no flag that makes an old binary's idea of the range
    /// correct. The way out is to stop being an old binary.
    fn refusal(self, tag: &str, current_version: &str, force: bool) -> Option<String> {
        if self.tagged {
            let also = if self.sectioned {
                ", and CHANGELOG.md already has its section"
            } else {
                ""
            };
            return Some(format!(
                "{tag} is already released\n  \
                 this repository has a {tag} tag{also}.\n  \
                 `uf release` plans the version after the one compiled into the binary running\n  \
                 it, and this binary is {current_version} — so it is older than the tree, and\n  \
                 writing this section would replace a released version's notes with a later\n  \
                 release's commits.\n  \
                 Build uf from this tree and run it again. `--force` does not cover a version\n  \
                 that has been tagged: a section a release was cut from is finished."
            ));
        }
        if self.sectioned && !force {
            return Some(format!(
                "CHANGELOG.md already has a section for {tag}\n  \
                 This run would replace it. If that is the release you are preparing and the\n  \
                 section should be rewritten from the commits that exist now, pass `--force`.\n  \
                 If it is not, the binary is older than the tree: `uf release` plans the\n  \
                 version after the {current_version} compiled into it, so an out-of-date binary\n  \
                 plans a version the tree has already written. Build uf from this tree."
            ));
        }
        None
    }
}

/// A changelog section written to disk.
struct Changelog {
    /// Where it was written.
    file: Utf8PathBuf,
    /// How many commits it describes.
    changes: usize,
    /// The subjects among them that carry no `(#NNN)`, newest first.
    unnumbered: Vec<String>,
}

impl Changelog {
    fn path(&self) -> String {
        self.file.to_string()
    }
}

/// Write the section for `tag` to `CHANGELOG.md`, from the commits since the
/// last release tag.
///
/// [`None`] when there is no git history to read — an exported source tree, a
/// directory that is not a repository. A release plan is still worth writing
/// there; a missing changelog is not a reason to refuse to cut a release, and
/// saying so is better than an error that stops the command.
fn write_changelog(root: &Utf8Path, tag: &str, tag_prefix: &str) -> Result<Option<Changelog>> {
    let Some(subjects) = commit_subjects(root, tag_prefix)? else {
        return Ok(None);
    };
    if subjects.is_empty() {
        return Ok(None);
    }
    // The date of the commit being released rather than the wall clock: a
    // changelog regenerated next week should say the same thing, and there is
    // no date crate in this binary to read a clock with anyway.
    let date = git(root, &["log", "-1", "--format=%cs"])
        .map(|date| date.trim().to_owned())
        .filter(|date| !date.is_empty())
        .unwrap_or_else(|| String::from("unreleased"));
    let section = crate::changelog::section(tag, &date, &subjects);
    let file = root.join("CHANGELOG.md");
    let existing = fs::read_to_string(&file).ok();
    let contents = crate::changelog::prepend(existing.as_deref(), &section);
    fs::write(&file, contents).with_context(|| format!("failed to write {file}"))?;
    Ok(Some(Changelog {
        file,
        changes: subjects.len(),
        unnumbered: unnumbered(&subjects),
    }))
}

/// The subjects that carry no `(#NNN)` pull request number.
///
/// GitHub puts that number in the subject when it squash-merges — unless the
/// person merging edits the title, or the merge is made another way. Then the
/// commit is real, its change is in the tarball, and every tool that identifies
/// a change by its number is blind to it. `docs: sharpen uf React hero copy`
/// shipped in `uf@0.0.0-alpha.8` that way, while the check that exists to catch
/// omissions reported sixteen pull requests in the range and sixteen named. See
/// #443.
///
/// The section itself is not the problem: a commit with no number gets a line
/// like any other, because the range is what the section is written from. What
/// this adds is *saying so* — the line for such a commit is the one a person has
/// to place by hand, and it is the one
/// `tools/ci/changelog-covers-the-release.sh` will ask to see cited by its
/// summary or by its hash, having no number to look for.
///
/// The release's own commits are left out, and that is a rule rather than a
/// guess: they are made locally, so they carry no number until they are merged,
/// and `uf release` is run more than once while a release is prepared. Without
/// it every re-run would report the two commits the release itself made.
fn unnumbered(subjects: &[String]) -> Vec<String> {
    subjects
        .iter()
        .filter(|subject| !subject.starts_with("chore(release):"))
        .filter(|subject| !names_a_pull_request(subject))
        .cloned()
        .collect()
}

/// Whether `subject` ends with `(#NNN)`, which is where a squash merge puts it.
fn names_a_pull_request(subject: &str) -> bool {
    let Some(rest) = subject.trim_end().strip_suffix(')') else {
        return false;
    };
    let Some((_, digits)) = rest.rsplit_once("(#") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The subjects of every commit since the last `<prefix>*` tag, newest first.
///
/// Merges are left out: a merge commit's subject says which branch was
/// merged, which is a fact about this repository's history rather than about
/// what changed.
fn commit_subjects(root: &Utf8Path, tag_prefix: &str) -> Result<Option<Vec<String>>> {
    let range = match previous_tag(root, tag_prefix) {
        // `A..HEAD` is "reachable from HEAD and not from A", which is the
        // right set whether or not A is an ancestor.
        Some(previous) => format!("{previous}..HEAD"),
        // No release yet: everything that has ever been committed.
        None => String::from("HEAD"),
    };
    let Some(log) = git(root, &["log", "--no-merges", "--format=%s", &range]) else {
        return Ok(None);
    };
    Ok(Some(
        log.lines()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .map(str::to_owned)
            .collect(),
    ))
}

/// The most recently created `<prefix>*` tag, whether or not HEAD can reach it.
///
/// Not `git describe`, which finds the nearest tag *reachable from HEAD*. A
/// release cut on a branch that was then squash-merged leaves a tag no commit
/// on `main` can reach, so `describe` walks past it to the release before —
/// and the changelog for the new version repeats every entry of the last one.
/// `uf@0.0.0-alpha.3` was cut that way and alpha.4's section came out with
/// fifty-four changes, most of them already released.
///
/// By creation date rather than by version, because git's version sort puts a
/// prerelease after the release it precedes unless `versionsort.suffix` has
/// been configured, and a repository is not required to have configured it.
fn previous_tag(root: &Utf8Path, tag_prefix: &str) -> Option<String> {
    let pattern = format!("refs/tags/{tag_prefix}*");
    let tags = git(
        root,
        &[
            "for-each-ref",
            "--sort=-creatordate",
            "--format=%(refname:short)",
            &pattern,
        ],
    )?;
    tags.lines()
        .map(str::trim)
        .find(|tag| !tag.is_empty())
        .map(str::to_owned)
}

/// Run `git` in `root`, or [`None`] when it is not there or says no.
fn git(root: &Utf8Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root.as_str())
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn bump_semver(version: &str, bump: ReleaseBump) -> Result<String> {
    let (core, prerelease) = version
        .split_once('-')
        .map_or((version, None), |(core, suffix)| (core, Some(suffix)));
    let mut parts = core.split('.');
    let major = parse_semver_part(parts.next(), "major")?;
    let minor = parse_semver_part(parts.next(), "minor")?;
    let patch = parse_semver_part(parts.next(), "patch")?;
    if parts.next().is_some() {
        bail!("version {version:?} is not a three-part semver");
    }

    match bump {
        ReleaseBump::Alpha => next_alpha(core, prerelease),
        ReleaseBump::Patch => Ok(format!("{major}.{minor}.{}", patch + 1)),
        ReleaseBump::Minor => Ok(format!("{major}.{}.0", minor + 1)),
        ReleaseBump::Major => Ok(format!("{}.0.0", major + 1)),
    }
}

fn next_alpha(core: &str, prerelease: Option<&str>) -> Result<String> {
    let Some(prerelease) = prerelease else {
        return Ok(format!("{core}-alpha.0"));
    };

    let Some(alpha) = prerelease.strip_prefix("alpha.") else {
        return Ok(format!("{core}-alpha.0"));
    };
    let current = alpha
        .parse::<u64>()
        .with_context(|| format!("alpha version part {alpha:?} is not numeric"))?;
    Ok(format!("{core}-alpha.{}", current + 1))
}

fn parse_semver_part(part: Option<&str>, name: &str) -> Result<u64> {
    let part = part.ok_or_else(|| anyhow!("version is missing {name}"))?;
    part.parse()
        .with_context(|| format!("version {name} part {part:?} is not numeric"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_bump_moves_the_right_component() {
        assert_eq!(
            bump_semver("0.0.0-alpha.0", ReleaseBump::Alpha).unwrap(),
            "0.0.0-alpha.1"
        );
        assert_eq!(
            bump_semver("0.0.0", ReleaseBump::Alpha).unwrap(),
            "0.0.0-alpha.0"
        );
        assert_eq!(bump_semver("1.2.3", ReleaseBump::Patch).unwrap(), "1.2.4");
        assert_eq!(bump_semver("1.2.3", ReleaseBump::Minor).unwrap(), "1.3.0");
        assert_eq!(bump_semver("1.2.3", ReleaseBump::Major).unwrap(), "2.0.0");
        assert_eq!(
            bump_semver("1.2.3-alpha.4", ReleaseBump::Patch).unwrap(),
            "1.2.4"
        );
    }

    /// #443: a commit GitHub did not stamp is reported, not dropped.
    #[test]
    fn a_subject_with_no_pull_request_number_is_named() {
        let subjects: Vec<String> = [
            "feat(rsc): keep a route out of the client bundle (#438)",
            "docs: sharpen uf React hero copy",
            "fix(fmt): a spread keeps its parentheses (#160)",
            "rename",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        assert_eq!(
            unnumbered(&subjects),
            vec![
                "docs: sharpen uf React hero copy".to_owned(),
                "rename".to_owned()
            ]
        );
    }

    /// And the release's own commits, which have no number until they merge,
    /// are not reported as commits somebody has to place.
    #[test]
    fn the_releases_own_commits_are_not_reported() {
        let subjects: Vec<String> = [
            "chore(release): uf@0.0.0-alpha.9",
            "chore(release): uf@0.0.0-alpha.9 (#458)",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        assert!(unnumbered(&subjects).is_empty());
    }

    /// `(#NNN)` is the trailing form a squash merge writes, and nothing else.
    #[test]
    fn only_a_trailing_pull_request_number_counts() {
        assert!(names_a_pull_request("fix: a thing (#1)"));
        assert!(names_a_pull_request("fix: a thing (#12345)"));
        // A number somewhere else is a number in prose.
        assert!(!names_a_pull_request(
            "fix: (#12) is not the merge's number"
        ));
        assert!(!names_a_pull_request("fix: a thing (#)"));
        assert!(!names_a_pull_request("fix: a thing (#12a)"));
        assert!(!names_a_pull_request("fix: a thing (12)"));
        assert!(!names_a_pull_request("fix: a thing"));
        assert!(!names_a_pull_request(""));
        // The last one wins, which is where the merge writes it.
        assert!(names_a_pull_request("fix: reverts (#11) (#12)"));
    }

    /// #457: an old binary plans a version that is already out, and rewriting
    /// its section replaces a release's notes with a later release's commits.
    #[test]
    fn a_released_version_is_refused_and_force_does_not_cover_it() {
        let published = Published {
            tagged: true,
            sectioned: true,
        };

        for force in [false, true] {
            let refusal = published
                .refusal("uf@0.0.0-alpha.8", "0.0.0-alpha.7", force)
                .unwrap_or_else(|| panic!("a tagged version was allowed with force={force}"));
            assert!(
                refusal.contains("uf@0.0.0-alpha.8 is already released"),
                "{refusal}"
            );
            assert!(refusal.contains("0.0.0-alpha.7"), "{refusal}");
            assert!(refusal.contains("older than the tree"), "{refusal}");
        }

        // The tag alone is enough; a tag with no section is a release whose
        // notes this run would invent from the wrong range.
        assert!(
            Published {
                tagged: true,
                sectioned: false,
            }
            .refusal("uf@0.0.0-alpha.8", "0.0.0-alpha.7", true)
            .is_some()
        );
    }

    /// A section with no tag is a release being prepared, and `--force` is the
    /// person who wrote it saying so.
    #[test]
    fn an_untagged_section_is_refused_until_force() {
        let published = Published {
            tagged: false,
            sectioned: true,
        };

        let refusal = published
            .refusal("uf@0.0.0-alpha.13", "0.0.0-alpha.12", false)
            .expect("a section already there is refused");
        assert!(refusal.contains("uf@0.0.0-alpha.13"), "{refusal}");
        assert!(refusal.contains("--force"), "{refusal}");

        assert!(
            published
                .refusal("uf@0.0.0-alpha.13", "0.0.0-alpha.12", true)
                .is_none()
        );
    }

    /// The ordinary case: a version nothing has heard of yet.
    #[test]
    fn a_version_the_tree_does_not_know_is_planned() {
        assert!(
            Published {
                tagged: false,
                sectioned: false,
            }
            .refusal("uf@0.0.0-alpha.13", "0.0.0-alpha.12", false)
            .is_none()
        );
    }

    #[test]
    fn a_malformed_version_is_rejected() {
        assert!(bump_semver("1.2", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.3.4", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.x", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.3-alpha.x", ReleaseBump::Alpha).is_err());
        assert!(bump_semver("", ReleaseBump::Patch).is_err());
    }
}
