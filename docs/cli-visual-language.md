# The CLI's visual language

Every `uf` command draws with the same primitives from `crates/uf_term`, and
writes through the same surface, `crates/uf_cli/src/ui.rs`. This document
records the rules those two places enforce. It also records the reasoning, so
a new command follows the rules because it uses the primitives, not because
someone remembered them.

Only rules that the code enforces today are listed here. A rule that exists
only as intent does not belong in this document.

## Streams

- Human output goes to stdout. Errors and progress go to stderr.
- `--json` means stdout carries JSON and nothing else: no banner, no colour, no
  progress.
- Nothing animates unless the stream is a terminal. A spinner or live region
  in a pipe or a CI log writes nothing, so the transcript contains only final
  lines.

## Colour and glyphs

- Colour is resolved once, at start-up, from `--color`, `NO_COLOR`,
  `FORCE_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `TERM` and whether the stream is
  a terminal.
- The accent is declared as 24-bit and downgraded to the 256-colour cube and
  then to the 16 base colours. Without colour there is no escape byte anywhere.
- Every glyph has an ASCII stand-in of the same width, for terminals and
  locales that cannot draw box characters: `─` → `-`, `✓` → `+`, `✗` → `x`,
  `›` → `>`, `·` → `.`. Nothing is an emoji.

## Layout

- **A command opens with a banner:** the command, a `·`, what it acts on, and
  a rule under them as wide as that line.
- **A rule is never followed by a blank line.** The rule already separates the
  banner from the report. A blank line under it would put two separators in a
  row. `Ui` enforces this for every command: each rendered block passes through
  `uf_term::RuleSpacing`, which drops any blank lines directly under a rule.
  That includes a blank line at the start of a later write, which is how a
  command that renders its banner and its report separately used to get one.
  A test covers it (`ui::tests::a_rule_is_never_followed_by_a_blank_line`),
  and `uf_term::blank_after_rule` lets any transcript test assert the same
  thing.
- Blank lines separate sections of a report from each other. They never
  separate a heading from its own content.
- **A command ends on one summary line:** a status mark, the verdict in the
  status colour, then the facts that qualify it, receding and separated by
  `·`, for example `✗ 1 error · 4 files checked · 8ms`.
- **What to do next is a hint:** `›` followed by muted text, with commands in
  backticks drawn in the accent.
