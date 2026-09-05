//! The changelog section a release writes.
//!
//! `uf release` had no answer to "what is in this one". `release.yml` asked
//! GitHub for `--generate-notes`, which is a list of pull request titles in
//! merge order — the same information, sorted by when somebody happened to
//! press the button, and readable only on the release page.
//!
//! This turns the same commits into a section of `CHANGELOG.md`: grouped by
//! what the change *is*, in the repository, where the next person looks.
//!
//! The grouping is Conventional Commits, which this repository already
//! writes — every commit between `uf@0.0.0-alpha.2` and today parses. A
//! subject that does not is kept rather than dropped: a changelog that
//! silently omits a change is worse than one with an untidy line in it.

/// One commit, as the changelog cares about it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Entry<'a> {
    /// `feat`, `fix`, … or [`None`] when the subject is not conventional.
    kind: Option<&'a str>,
    /// The `(scope)`, when there is one.
    scope: Option<&'a str>,
    /// Whether the subject was marked `!` as a breaking change.
    breaking: bool,
    /// Everything after the `: `.
    summary: &'a str,
}

/// The headings a section can have, in the order they are printed.
///
/// Ordered by what a reader is looking for rather than alphabetically:
/// what broke, what is new, what was fixed, what got faster, then the work
/// that does not change the product. A heading with nothing under it is not
/// printed at all.
const GROUPS: &[(&str, &[&str])] = &[
    ("Added", &["feat"]),
    ("Fixed", &["fix"]),
    ("Performance", &["perf"]),
    ("Documentation", &["docs"]),
    (
        "Internal",
        &["build", "chore", "ci", "refactor", "style", "test"],
    ),
];

/// Read `type(scope)!: summary`.
///
/// Anything else comes back as a summary with no type, which puts it under
/// "Other" rather than dropping it.
pub(crate) fn parse(subject: &str) -> Entry<'_> {
    let subject = subject.trim();
    let Some((head, summary)) = subject.split_once(": ") else {
        return Entry {
            kind: None,
            scope: None,
            breaking: false,
            summary: subject,
        };
    };
    // A `: ` inside prose is not a type separator. A type is one word, with
    // an optional parenthesised scope, and nothing else.
    let (head, breaking) = head
        .strip_suffix('!')
        .map_or((head, false), |head| (head, true));
    let (kind, scope) = match head.split_once('(') {
        Some((kind, rest)) => match rest.strip_suffix(')') {
            Some(scope) => (kind, Some(scope)),
            None => {
                return Entry {
                    kind: None,
                    scope: None,
                    breaking: false,
                    summary: subject,
                };
            }
        },
        None => (head, None),
    };
    // Only the types this file knows how to file. Anything else is prose
    // that happens to have a colon in it — `note: a thing: and another` — and
    // reading it as a type would eat the word and put the rest under a
    // heading nobody chose.
    if !GROUPS.iter().any(|(_, kinds)| kinds.contains(&kind)) {
        return Entry {
            kind: None,
            scope: None,
            breaking: false,
            summary: subject,
        };
    }
    Entry {
        kind: Some(kind),
        scope,
        breaking,
        summary,
    }
}

/// The `## uf@<version>` section for `subjects`, newest commit first.
///
/// `date` is passed in rather than read from the clock so the output is a
/// function of its inputs and the tests can say what they expect.
pub(crate) fn section(tag: &str, date: &str, subjects: &[String]) -> String {
    let entries: Vec<Entry<'_>> = subjects.iter().map(|subject| parse(subject)).collect();

    let mut out = format!("## {tag}\n\n_{date}_\n");

    let breaking: Vec<&Entry<'_>> = entries.iter().filter(|entry| entry.breaking).collect();
    if !breaking.is_empty() {
        out.push_str("\n### Breaking\n\n");
        for entry in breaking {
            out.push_str(&line(entry));
        }
    }

    for (heading, kinds) in GROUPS {
        let group: Vec<&Entry<'_>> = entries
            .iter()
            .filter(|entry| !entry.breaking)
            .filter(|entry| entry.kind.is_some_and(|kind| kinds.contains(&kind)))
            .collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {heading}\n\n"));
        for entry in group {
            out.push_str(&line(entry));
        }
    }

    let other: Vec<&Entry<'_>> = entries
        .iter()
        .filter(|entry| !entry.breaking)
        .filter(|entry| entry.kind.is_none())
        .collect();
    if !other.is_empty() {
        out.push_str("\n### Other\n\n");
        for entry in other {
            out.push_str(&line(entry));
        }
    }

    out
}

/// One bullet: the scope in bold when there is one, then the summary.
fn line(entry: &Entry<'_>) -> String {
    match entry.scope {
        Some(scope) => format!("- **{scope}**: {}\n", entry.summary),
        None => format!("- {}\n", entry.summary),
    }
}

/// `section` written into `existing`, under the file's title.
///
/// The newest release goes directly under the title, so the file reads
/// newest-first and nobody has to scroll past a year of history to see what
/// shipped today.
///
/// A section for the same tag is *replaced*, not stacked. `uf release alpha`
/// is run more than once while a release is being prepared — a commit lands,
/// the changelog is regenerated — and a file that grew a second
/// `## uf@0.0.0-alpha.3` every time would be a file nobody trusted.
pub(crate) fn prepend(existing: Option<&str>, section: &str) -> String {
    const TITLE: &str = "# Changelog\n";
    let heading = section.lines().next().unwrap_or_default();
    let Some(existing) = existing else {
        return format!("{TITLE}\n{section}");
    };
    // A file that does not start with the title is somebody else's, and the
    // section goes on top of it whole rather than into the middle of it.
    let rest = existing.strip_prefix(TITLE).unwrap_or(existing);
    let rest = without_section(rest, heading);
    let rest = rest.trim_start_matches('\n');
    if rest.is_empty() {
        return format!("{TITLE}\n{section}");
    }
    format!("{TITLE}\n{section}\n{rest}")
}

/// `body` with the `## …` section headed by `heading` removed.
///
/// A section runs from its heading to the next `## ` at the start of a line,
/// or to the end of the file.
fn without_section<'a>(body: &'a str, heading: &str) -> std::borrow::Cow<'a, str> {
    if heading.is_empty() {
        return std::borrow::Cow::Borrowed(body);
    }
    let mut out = String::new();
    let mut skipping = false;
    for line in body.split_inclusive('\n') {
        if line.trim_end() == heading {
            skipping = true;
            continue;
        }
        if skipping {
            if line.starts_with("## ") {
                skipping = false;
            } else {
                continue;
            }
        }
        out.push_str(line);
    }
    if out.len() == body.len() {
        return std::borrow::Cow::Borrowed(body);
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conventional_subject_is_read_apart() {
        let entry = parse("fix(fmt): an object's spread keeps its parentheses (#160)");
        assert_eq!(entry.kind, Some("fix"));
        assert_eq!(entry.scope, Some("fmt"));
        assert!(!entry.breaking);
        assert_eq!(
            entry.summary,
            "an object's spread keeps its parentheses (#160)"
        );

        let entry = parse("docs: say that formatter fixtures are data (#158)");
        assert_eq!(entry.kind, Some("docs"));
        assert_eq!(entry.scope, None);

        let entry = parse("feat(cli)!: `uf run` takes the task name first");
        assert_eq!(entry.kind, Some("feat"));
        assert!(entry.breaking);
    }

    /// A subject that is not conventional is kept, not dropped.
    #[test]
    fn an_unconventional_subject_keeps_its_whole_text() {
        for subject in [
            "rename (#131)",
            "Merge branch 'main'",
            "note: this is prose: with a colon in it",
            "FIX(fmt): shouting is not a type",
            "(fmt): a scope with no type",
        ] {
            let entry = parse(subject);
            assert_eq!(entry.kind, None, "{subject}");
            assert_eq!(entry.summary, subject, "{subject}");
        }
    }

    #[test]
    fn a_section_groups_by_what_the_change_is() {
        let subjects: Vec<String> = [
            "feat(explain): say who does the work (#167)",
            "fix(fmt): an object's spread keeps its parentheses (#160)",
            "fix(project): a file that cannot be read does not stop it (#165)",
            "perf(fmt): print each call argument once (#145)",
            "docs: say that formatter fixtures are data (#158)",
            "test(fmt): eleven more Flow codebases in the corpus (#138)",
            "style: reformat the packages (#182)",
            "rename (#131)",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        let section = section("uf@0.0.0-alpha.3", "2026-09-06", &subjects);

        similar_asserts::assert_eq!(
            section,
            "\
## uf@0.0.0-alpha.3

_2026-09-06_

### Added

- **explain**: say who does the work (#167)

### Fixed

- **fmt**: an object's spread keeps its parentheses (#160)
- **project**: a file that cannot be read does not stop it (#165)

### Performance

- **fmt**: print each call argument once (#145)

### Documentation

- say that formatter fixtures are data (#158)

### Internal

- **fmt**: eleven more Flow codebases in the corpus (#138)
- reformat the packages (#182)

### Other

- rename (#131)
"
        );
    }

    /// A heading with nothing under it is not printed, and a breaking change
    /// goes first whatever its type.
    #[test]
    fn empty_headings_are_absent_and_breaking_changes_lead() {
        let subjects: Vec<String> = ["fix(pm)!: the lockfile format changed".to_owned()].to_vec();

        let section = section("uf@1.0.0", "2026-09-06", &subjects);

        similar_asserts::assert_eq!(
            section,
            "## uf@1.0.0\n\n_2026-09-06_\n\n### Breaking\n\n- **pm**: the lockfile format changed\n"
        );
        assert!(!section.contains("### Fixed"));
    }

    /// Cutting the same release twice replaces its section rather than
    /// stacking a second one.
    #[test]
    fn regenerating_a_section_replaces_it() {
        let first = prepend(
            None,
            "## uf@0.0.0-alpha.3\n\n_2026-09-06_\n\n### Fixed\n\n- one\n",
        );
        let older = prepend(
            Some(&first),
            "## uf@0.0.0-alpha.4\n\n_2026-09-07_\n\n- new\n",
        );
        let again = prepend(
            Some(&older),
            "## uf@0.0.0-alpha.3\n\n_2026-09-06_\n\n### Fixed\n\n- one\n- two\n",
        );

        assert_eq!(again.matches("## uf@0.0.0-alpha.3").count(), 1, "{again}");
        assert!(again.contains("- two"), "{again}");
        assert!(again.contains("## uf@0.0.0-alpha.4"), "{again}");
    }

    #[test]
    fn a_new_section_goes_under_the_title() {
        let first = prepend(None, "## uf@0.0.0-alpha.3\n\n_2026-09-06_\n");
        assert_eq!(
            first,
            "# Changelog\n\n## uf@0.0.0-alpha.3\n\n_2026-09-06_\n"
        );

        let second = prepend(Some(&first), "## uf@0.0.0-alpha.4\n\n_2026-09-07_\n");
        similar_asserts::assert_eq!(
            second,
            "\
# Changelog

## uf@0.0.0-alpha.4

_2026-09-07_

## uf@0.0.0-alpha.3

_2026-09-06_
"
        );
    }
}
