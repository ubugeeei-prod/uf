//! `uf lint --rules`: every rule, the level it runs at in this project, and
//! what fixes its findings.
//!
//! Answered from the three places a lint run reads for itself — the catalogue
//! in `uf_lint`, the project's resolved config, and the fix catalogue in
//! [`crate::fix`] — so the list cannot name a rule the run does not have, a
//! level the run would not use, or a fix `--fix` would not make. There is no
//! list of rules in this file, and that is the design rather than an omission:
//! a table written out here would be one more copy for documentation to quote
//! while it drifted, which is exactly how a rule count written into a guide
//! goes stale.
//!
//! It lints nothing and reads no source file, so it answers the same in a
//! project whose files do not parse, and it exits `0` whatever the levels are:
//! the list is a description, not a verdict.

#[cfg(test)]
mod tests;

use anyhow::Result;
use camino::Utf8Path;
use serde_json::json;
use uf_config::{RuleLevel, UniflowedConfig, load_config};
use uf_lint::RuleDescriptor;
use uf_term::{Cell, Column, Status, Table, Tone};

use crate::fix::{RuleFix, rule_fix};
use crate::support::plural;
use crate::ui::Ui;

/// Print every rule, as a table or, with `json`, as the machine payload.
pub(crate) fn rules_command(cwd: &Utf8Path, ui: &mut Ui, json: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let listed = listed_rules(&resolved.config);
    if json {
        ui.json(&rules_payload(&listed))?;
    } else {
        render_rules(ui, &listed);
    }
    Ok(())
}

/// One row of `uf lint --rules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListedRule {
    /// The catalogue's entry: id, category, default level, description.
    pub(crate) descriptor: &'static RuleDescriptor,
    /// The level a lint run in this project uses; see [`uf_lint::rule_level`].
    pub(crate) level: RuleLevel,
    /// Whether uf can fix the rule's findings, and with what.
    pub(crate) fix: Option<RuleFix>,
}

impl ListedRule {
    /// Enabled, and waiting on type inference uf does not have — so a lint run
    /// names it under `unavailableRules` rather than running it.
    fn skipped(&self) -> bool {
        self.level.is_enabled() && !self.descriptor.requirement.is_available()
    }
}

/// Every rule, in catalogue order, at the level `config` runs it.
pub(crate) fn listed_rules(config: &UniflowedConfig) -> Vec<ListedRule> {
    uf_lint::rules()
        .iter()
        .map(|descriptor| ListedRule {
            descriptor,
            level: uf_lint::rule_level(config, descriptor.id),
            fix: rule_fix(descriptor.id),
        })
        .collect()
}

/// The machine-readable list.
///
/// Each entry is the catalogue's own shape — the one `uf inspect --json` prints
/// under `lintRules` — with the two answers only a project has added to it:
/// `level` and `fix`. `fix` is `null` for a rule uf has no fix for rather than
/// absent, so a script can tell "no fix" from "a uf that did not say".
pub(crate) fn rules_payload(listed: &[ListedRule]) -> serde_json::Value {
    json!({
        "command": "uf lint --rules",
        "rules": listed.iter().map(|rule| json!({
            "id": rule.descriptor.id,
            "category": rule.descriptor.category,
            "level": rule.level,
            "defaultLevel": rule.descriptor.default_level,
            "requirement": rule.descriptor.requirement,
            "fix": rule.fix.map(RuleFix::as_str),
            "description": rule.descriptor.description,
        })).collect::<Vec<_>>(),
    })
}

/// The table: rule, level, the command that fixes it, and what it checks.
///
/// The fix column names the flag or command rather than a tier, because the
/// question a reader brings to it is "what do I run", and `safe` answers that
/// only for someone who has already read what `--fix` means.
fn render_rules(ui: &mut Ui, listed: &[ListedRule]) {
    let enabled = listed.iter().filter(|rule| rule.level.is_enabled()).count();
    let fixable = listed.iter().filter(|rule| rule.fix.is_some()).count();
    let any_skipped = listed.iter().any(ListedRule::skipped);
    let summary = format!(
        "{}, {enabled} enabled in this project, {fixable} with a fix",
        plural(listed.len(), "rule")
    );
    // A marker on the level, not a column: it is true of a handful of rules,
    // and a column that is empty on every other row is width nobody reads.
    let levels: Vec<String> = listed
        .iter()
        .map(|rule| {
            let word = level_word(rule.level);
            if rule.skipped() {
                format!("{word} *")
            } else {
                word.to_owned()
            }
        })
        .collect();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf lint --rules", None);
        let mut table = Table::new(vec![
            Column::left("rule"),
            Column::left("level"),
            Column::left("fix"),
            Column::left("description"),
        ]);
        for (rule, level) in listed.iter().zip(&levels) {
            table.push(vec![
                Cell::toned(rule.descriptor.id, Tone::Accent),
                Cell::toned(level, level_tone(rule.level)),
                Cell::toned(rule.fix.map_or("", RuleFix::command), Tone::Good),
                Cell::toned(rule.descriptor.description, Tone::Muted),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        if any_skipped {
            renderer.status(
                out,
                Status::Info,
                "* enabled, and needs Flow type inference, which uf does not implement yet: a lint run names it instead of running it",
            );
        }
        renderer.status(out, Status::Info, &summary);
    });
}

/// How a level is spelled in `uf.config.js`, which is how it is shown.
fn level_word(level: RuleLevel) -> &'static str {
    match level {
        RuleLevel::Off => "off",
        RuleLevel::Warn => "warn",
        RuleLevel::Error => "error",
    }
}

fn level_tone(level: RuleLevel) -> Tone {
    match level {
        RuleLevel::Off => Tone::Muted,
        RuleLevel::Warn => Tone::Warn,
        RuleLevel::Error => Tone::Bad,
    }
}
