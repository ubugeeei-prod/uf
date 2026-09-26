//! `uf-lint-disable` comment handling.
//!
//! Two forms are supported, both in `//` comments:
//!
//! ```js
//! // uf-lint-disable-next-line flow/unclear-type
//! // uf-lint-disable flow/unclear-type, react-compiler/hooks
//! // uf-lint-enable flow/unclear-type
//! ```
//!
//! A suppression that names a rule this linter does not know is **not** a silent
//! no-op: it raises `uniflowed/unknown-lint-suppression`. A typo'd suppression
//! that quietly suppressed nothing would be indistinguishable from a working one,
//! which is exactly how a rule stops being enforced without anyone noticing.

use uf_infra::SmallVec;

use crate::rules::canonical_rule_id;
use crate::scan::{FileScan, next_non_space};

/// Rule id reported when a suppression comment names an unknown rule.
pub(crate) const UNKNOWN_SUPPRESSION_RULE: &str = "uniflowed/unknown-lint-suppression";

/// Rule id reported when a suppression comment silences nothing.
pub(crate) const UNUSED_SUPPRESSION_RULE: &str = "uniflowed/unused-lint-suppression";

const DISABLE_NEXT_LINE: &str = "uf-lint-disable-next-line";
const DISABLE: &str = "uf-lint-disable";
const ENABLE: &str = "uf-lint-enable";

/// A suppression comment that could not be honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BadSuppression {
    /// 1-based line of the offending comment.
    pub line: usize,
    /// 1-based byte column of the offending token.
    pub column: usize,
    /// Human-readable explanation.
    pub message: String,
}

/// Where a suppression names its rule: the comment's line and the rule id's
/// column, both 1-based. What `uniflowed/unused-lint-suppression` points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Named {
    pub line: usize,
    pub column: usize,
}

/// Resolved suppressions for one file.
#[derive(Debug, Default)]
pub(crate) struct Suppressions {
    /// `(rule id, 1-based line)` pairs from `uf-lint-disable-next-line`.
    single: Vec<(&'static str, usize, Named)>,
    /// `(rule id, first line, last line)` inclusive ranges from block form.
    ranges: Vec<(&'static str, usize, usize, Named)>,
}

impl Suppressions {
    /// Whether the file carries no suppression comments at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.single.is_empty() && self.ranges.is_empty()
    }

    /// Whether `rule` is suppressed on the given 1-based `line`.
    #[cfg(test)]
    pub fn is_suppressed(&self, rule: &str, line: usize) -> bool {
        self.matching(rule, line).next().is_some()
    }

    /// Every suppression that silences `rule` on `line`, by the place it is
    /// named. One finding can be silenced twice over, and both did work.
    fn matching<'a>(&'a self, rule: &'a str, line: usize) -> impl Iterator<Item = Named> + 'a {
        let single = self
            .single
            .iter()
            .filter(move |&&(suppressed, at, _)| at == line && suppressed == rule)
            .map(|&(_, _, named)| named);
        let ranges = self
            .ranges
            .iter()
            .filter(move |&&(suppressed, start, end, _)| {
                line >= start && line <= end && suppressed == rule
            })
            .map(|&(_, _, _, named)| named);
        single.chain(ranges)
    }

    /// Drop every diagnostic a suppression silences, and return the
    /// suppressions that silenced nothing, with the rule each one names.
    ///
    /// A suppression that silences nothing is either a typo'd line — it sits
    /// above the wrong statement — or a leftover from a finding that has gone
    /// away, most often because the rule stopped reporting a false positive.
    /// Either way it is a comment arguing for an exception nobody is taking,
    /// and the next real finding on that line would be silenced unread.
    pub fn apply(&self, diagnostics: &mut Vec<crate::Diagnostic>) -> Vec<(&'static str, Named)> {
        let mut used: uf_infra::FxHashSet<Named> = uf_infra::FxHashSet::default();
        diagnostics.retain(|diagnostic| {
            let mut silenced = false;
            for named in self.matching(diagnostic.rule, diagnostic.line) {
                silenced = true;
                used.insert(named);
            }
            !silenced
        });
        let mut unused: Vec<(&'static str, Named)> = self
            .single
            .iter()
            .map(|&(rule, _, named)| (rule, named))
            .chain(self.ranges.iter().map(|&(rule, _, _, named)| (rule, named)))
            .filter(|(_, named)| !used.contains(named))
            .collect();
        unused.sort_by_key(|&(_, named)| (named.line, named.column));
        unused.dedup();
        unused
    }
}

/// Parse every suppression comment in `scan`.
///
/// Returns the resolved suppressions plus every comment that named a rule the
/// linter does not know about.
pub(crate) fn collect(scan: &FileScan<'_>) -> (Suppressions, Vec<BadSuppression>) {
    let mut suppressions = Suppressions::default();
    let mut bad = Vec::new();
    // Block-form directives that have not seen a matching `uf-lint-enable`.
    let mut open: SmallVec<[(&'static str, usize, Named); 4]> = SmallVec::new();

    for (position, line) in scan.lines.iter().enumerate() {
        let comment = line.trailing_comment();
        let Some(body) = comment.strip_prefix("//") else {
            continue;
        };
        let base = line.comment_offset() + 2;
        let Some((directive_at, _)) = next_non_space(body, 0) else {
            continue;
        };
        let rest = &body[directive_at..];
        let number = position + 1;

        let (kind, args_at) = if let Some(args) = rest.strip_prefix(DISABLE_NEXT_LINE) {
            (Directive::DisableNextLine, rest.len() - args.len())
        } else if let Some(args) = rest.strip_prefix(DISABLE) {
            (Directive::Disable, rest.len() - args.len())
        } else if let Some(args) = rest.strip_prefix(ENABLE) {
            (Directive::Enable, rest.len() - args.len())
        } else {
            continue;
        };

        // `uf-lint-disabled` must not be read as `uf-lint-disable` plus junk.
        if rest
            .as_bytes()
            .get(args_at)
            .is_some_and(|&byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            continue;
        }

        let mut named_any = false;
        for (offset, name) in split_rule_ids(&rest[args_at..]) {
            named_any = true;
            let column = base + directive_at + args_at + offset + 1;
            let Some(rule) = canonical_rule_id(name) else {
                bad.push(BadSuppression {
                    line: number,
                    column,
                    message: uf_infra::cstr!("unknown lint rule `{name}` in suppression comment")
                        .into_string(),
                });
                continue;
            };
            let named = Named {
                line: number,
                column,
            };
            match kind {
                Directive::DisableNextLine => {
                    suppressions.single.push((rule, number + 1, named));
                }
                Directive::Disable => open.push((rule, number, named)),
                Directive::Enable => {
                    if let Some(index) = open.iter().rposition(|&(open, _, _)| open == rule) {
                        let (rule, start, named) = open.remove(index);
                        suppressions.ranges.push((rule, start, number, named));
                    }
                }
            }
        }

        if !named_any {
            bad.push(BadSuppression {
                line: number,
                column: base + directive_at + 1,
                message: "suppression comment must name at least one lint rule".to_string(),
            });
        }
    }

    for (rule, start, named) in open {
        suppressions.ranges.push((rule, start, usize::MAX, named));
    }

    (suppressions, bad)
}

/// Which suppression directive a comment carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Directive {
    DisableNextLine,
    Disable,
    Enable,
}

/// Split a directive's argument list into `(offset, rule id)` pairs.
///
/// Rule ids are separated by whitespace and/or commas, so both
/// `a/b, c/d` and `a/b c/d` work.
fn split_rule_ids(args: &str) -> impl Iterator<Item = (usize, &str)> {
    args.split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|token| !token.is_empty())
        .map(move |token| {
            let offset = token.as_ptr() as usize - args.as_ptr() as usize;
            (offset, token)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceFile;

    fn parse(source: &str) -> (Suppressions, Vec<BadSuppression>) {
        let file = SourceFile {
            path: "app/index.js".to_string(),
            source: source.to_string(),
        };
        let masked = crate::scan::mask_inline_comments(&file.source);
        let scan = FileScan::new(&file, &masked);
        let (suppressions, bad) = collect(&scan);
        (suppressions, bad)
    }

    #[test]
    fn disable_next_line_targets_only_the_following_line() {
        let (suppressions, bad) =
            parse("// uf-lint-disable-next-line flow/unclear-type\nlet a: any;\nlet b: any;\n");

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("flow/unclear-type", 2));
        assert!(!suppressions.is_suppressed("flow/unclear-type", 3));
        assert!(!suppressions.is_suppressed("flow/unclear-type", 1));
    }

    #[test]
    fn disable_next_line_only_suppresses_the_named_rule() {
        let (suppressions, _) = parse("// uf-lint-disable-next-line flow/unclear-type\nx\n");

        assert!(!suppressions.is_suppressed("security/no-eval", 2));
    }

    #[test]
    fn block_form_covers_the_directive_line_through_the_enable() {
        let (suppressions, bad) = parse(
            "// uf-lint-disable security/no-eval\neval('a');\neval('b');\n// uf-lint-enable security/no-eval\neval('c');\n",
        );

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("security/no-eval", 2));
        assert!(suppressions.is_suppressed("security/no-eval", 3));
        assert!(!suppressions.is_suppressed("security/no-eval", 5));
    }

    #[test]
    fn block_form_without_an_enable_runs_to_end_of_file() {
        let (suppressions, bad) = parse("// uf-lint-disable security/no-eval\neval('a');\n");

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("security/no-eval", 99_999));
    }

    #[test]
    fn several_rule_ids_may_share_one_comment() {
        let (suppressions, bad) =
            parse("// uf-lint-disable-next-line flow/unclear-type, security/no-eval\nx\n");

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("flow/unclear-type", 2));
        assert!(suppressions.is_suppressed("security/no-eval", 2));
    }

    #[test]
    fn trailing_suppression_comments_are_honoured() {
        let (suppressions, bad) =
            parse("let a = 1; // uf-lint-disable-next-line security/no-eval\n");

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("security/no-eval", 2));
    }

    #[test]
    fn deprecated_rule_ids_resolve_to_their_replacement() {
        let (suppressions, bad) =
            parse("// uf-lint-disable-next-line flow/type-aware/no-explicit-any\nx\n");

        assert!(bad.is_empty());
        assert!(suppressions.is_suppressed("flow/unclear-type", 2));
    }

    #[test]
    fn unknown_rule_ids_are_reported_not_ignored() {
        let (suppressions, bad) = parse("// uf-lint-disable-next-line flow/typo-here\nx\n");

        assert!(suppressions.is_empty());
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].line, 1);
        assert_eq!(bad[0].column, 30);
        assert!(bad[0].message.contains("flow/typo-here"));
    }

    #[test]
    fn a_suppression_with_no_rule_id_is_reported() {
        let (_, bad) = parse("// uf-lint-disable-next-line\nx\n");

        assert_eq!(bad.len(), 1);
        assert!(bad[0].message.contains("at least one lint rule"));
    }

    #[test]
    fn known_and_unknown_ids_in_one_comment_are_handled_independently() {
        let (suppressions, bad) =
            parse("// uf-lint-disable-next-line security/no-eval nope/nope\nx\n");

        assert_eq!(bad.len(), 1);
        assert!(suppressions.is_suppressed("security/no-eval", 2));
    }

    #[test]
    fn similar_looking_words_are_not_directives() {
        let (suppressions, bad) = parse("// uf-lint-disabled security/no-eval\nx\n");

        assert!(suppressions.is_empty());
        assert!(bad.is_empty());
    }

    #[test]
    fn files_without_suppressions_take_the_empty_fast_path() {
        let (suppressions, bad) = parse("let a = 1;\n// an ordinary comment\n");

        assert!(suppressions.is_empty());
        assert!(bad.is_empty());
        assert!(!suppressions.is_suppressed("security/no-eval", 1));
    }

    #[test]
    fn nested_block_directives_close_in_reverse_order() {
        let (suppressions, _) = parse(
            "// uf-lint-disable security/no-eval\n// uf-lint-disable flow/unclear-type\nx\n// uf-lint-enable flow/unclear-type\ny\n// uf-lint-enable security/no-eval\nz\n",
        );

        assert!(suppressions.is_suppressed("flow/unclear-type", 3));
        assert!(!suppressions.is_suppressed("flow/unclear-type", 5));
        assert!(suppressions.is_suppressed("security/no-eval", 5));
        assert!(!suppressions.is_suppressed("security/no-eval", 7));
    }

    #[test]
    fn an_enable_without_a_disable_is_harmless() {
        let (suppressions, bad) = parse("// uf-lint-enable security/no-eval\nx\n");

        assert!(suppressions.is_empty());
        assert!(bad.is_empty());
    }

    #[test]
    fn suppression_text_inside_a_string_is_not_a_directive() {
        let (suppressions, bad) = parse("const s = \"// uf-lint-disable security/no-eval\";\n");

        assert!(suppressions.is_empty());
        assert!(bad.is_empty());
    }
}
