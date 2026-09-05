//! `uf prepare`, `uf publish`, and `uf release`: the plans written to `.uf`.

use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_prepare::default_plan;
use uf_router::write_router_manifest;
use uf_term::{KeyValue, Status, Tone};

use crate::cli::ReleaseBump;
use crate::support::{enabled, project_label, write_json_file, yes_no};
use crate::ui::Ui;

pub(crate) fn prepare(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let plan = default_plan();
    let router_manifest = write_router_manifest(&resolved.root, &resolved.config)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("prepare.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "routerManifest": router_manifest,
            "lintStagedCompatible": plan.lint_staged_compatible,
            "codeGenerator": plan.code_generator,
            "writeGeneratedFiles": plan.write_generated_files,
            "cache": format!("{:?}", plan.cache),
            "steps": plan.steps.iter().map(|step| format!("{step:?}")).collect::<Vec<_>>(),
        }),
    )?;

    let root = resolved.root.as_str().to_string();
    let manifest_path = manifest.to_string();
    let cache = format!("{:?}", plan.cache);
    let steps = plan
        .steps
        .iter()
        .map(|step| format!("{step:?}"))
        .collect::<Vec<_>>();
    let step_labels = steps.iter().map(String::as_str).collect::<Vec<_>>();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf prepare", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("root", &root, Tone::Path),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
                KeyValue::new(
                    "lint-staged compatible",
                    yes_no(plan.lint_staged_compatible),
                ),
                KeyValue::new("code generator", enabled(plan.code_generator)),
                KeyValue::new("cache", &cache),
            ],
        );
        renderer.blank(out);
        renderer.heading(out, 2, "steps");
        renderer.bullet_list(out, 4, &step_labels);
        renderer.blank(out);
        renderer.status(out, Status::Success, "prepare plan written");
    });
    Ok(())
}

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

pub(crate) fn release(cwd: &Utf8Path, ui: &mut Ui, bump: ReleaseBump) -> Result<()> {
    let resolved = load_config(cwd)?;
    let current_version = env!("CARGO_PKG_VERSION");
    let next_version = bump_semver(current_version, bump)?;
    let tag = format!("{}{}", resolved.config.release.tag_prefix, next_version);
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let changelog = write_changelog(&resolved.root, &tag, &resolved.config.release.tag_prefix)?;
    let manifest = state_dir.join("release.json");
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
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// A changelog section written to disk.
struct Changelog {
    /// Where it was written.
    file: Utf8PathBuf,
    /// How many commits it describes.
    changes: usize,
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
    }))
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

    #[test]
    fn a_malformed_version_is_rejected() {
        assert!(bump_semver("1.2", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.3.4", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.x", ReleaseBump::Patch).is_err());
        assert!(bump_semver("1.2.3-alpha.x", ReleaseBump::Alpha).is_err());
        assert!(bump_semver("", ReleaseBump::Patch).is_err());
    }
}
