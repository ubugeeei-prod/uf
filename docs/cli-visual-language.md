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

## Help pages

- **uf draws its own help.** clap still parses every command line and decides
  when help was asked for; `crates/uf_cli/src/help.rs` draws the page from the
  same command tree the parser used, so a name, flag, default or sentence on
  the page cannot drift from what the parser accepts.
- **`uf --help` groups the commands** under headings, in the order a project
  meets them (`help::GROUPS`). A test fails when a visible command is in no
  section or in two, so a new command cannot fall off the front page.
- **A page is wrapped to the terminal**, and never wider than 100 columns. A
  pipe has no width, so it gets 100, and what a script reads is the same on
  every machine. A description that wraps continues under itself; on a
  terminal too narrow for two columns it goes under its name instead.
- `-h` shows the first paragraph of each flag's help, `--help` all of it.
- Help follows the same colour and glyph rules as every other screen:
  `--color`, `NO_COLOR` and `TERM=dumb` reach it, and so does the ASCII frame.
  What a reader types is in the accent; placeholders, defaults and values
  recede.
- A mistyped command is a uf error on stderr with exit code `2`, and its
  suggestion names commands only, never an alias.
- Snapshots of whole pages live in `crates/uf_cli/src/help/snapshots/`, at
  100, 60 and 32 columns, in colour and in ASCII.
