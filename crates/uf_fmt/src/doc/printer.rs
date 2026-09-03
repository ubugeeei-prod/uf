//! Turning a [`Doc`] into text: Prettier's `printDocToString`, ported.
//!
//! The algorithm is a single pass over a command stack. Each command is a doc
//! with the indentation to print it at and a *mode*: flat or broken. A group
//! met in broken mode is measured — [`fits`] walks its flat form, and the
//! commands after it up to the next line break, counting columns — and is
//! printed flat if it fits and broken otherwise. Nothing backtracks: once a
//! group's mode is decided its contents are printed in it, which is what
//! makes the printer linear in the size of the doc rather than exponential
//! in the number of groups.
//!
//! Everything here is iterative with explicit stacks. Documents mirror the
//! nesting of the source they were built from, and a formatter that recursed
//! over them would turn deep input into a stack overflow rather than an
//! error.

use unicode_width::UnicodeWidthStr;

use super::{AlignKind, Doc, DocKind, GroupId, HARDLINE_ONLY, LineMode};

/// How wide `text` is on a terminal, in columns.
///
/// East Asian wide characters and emoji count double, combining marks count
/// nothing: Prettier's `getStringWidth`, so a line of Japanese text breaks
/// where Prettier would break it.
pub fn text_width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.width()
    }
}

/// Which way a group is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// The indentation a line starts with.
///
/// Only spaces are ever emitted, so an indentation is a width plus the
/// bookkeeping [`AlignKind::Root`] and [`AlignKind::Dedent`] need: the
/// indentation this one was derived from, and the one marked as root.
#[derive(Debug, Clone, Copy)]
struct Indent<'a> {
    width: usize,
    parent: Option<&'a Indent<'a>>,
    root: Option<&'a Indent<'a>>,
}

impl Indent<'_> {
    const ROOT: Indent<'static> = Indent {
        width: 0,
        parent: None,
        root: None,
    };
}

/// One pending piece of work.
#[derive(Clone, Copy)]
struct Command<'a> {
    indent: &'a Indent<'a>,
    mode: Mode,
    doc: Doc<'a>,
}

/// Options the printer needs from the formatter configuration.
#[derive(Debug, Clone, Copy)]
pub struct PrintOptions {
    /// Columns a line may use.
    pub width: usize,
    /// Spaces per indentation level.
    pub indent_width: usize,
}

/// Print `doc` to a string.
pub fn print(doc: Doc<'_>, options: PrintOptions, group_count: usize) -> String {
    let arena = uf_infra::Bump::new();
    let mut printer = Printer {
        options,
        indents: &arena,
        out: String::new(),
        position: 0,
        group_modes: vec![None; group_count],
        line_suffixes: Vec::new(),
        should_remeasure: false,
    };
    printer.run(doc);
    printer.out
}

struct Printer<'a> {
    options: PrintOptions,
    indents: &'a uf_infra::Bump,
    out: String,
    /// Columns printed on the current line.
    position: usize,
    /// The mode each identified group was printed in, once known.
    group_modes: Vec<Option<Mode>>,
    line_suffixes: Vec<Command<'a>>,
    /// A hard line was met in flat mode, so the next group must be measured
    /// rather than inheriting flatness.
    should_remeasure: bool,
}

impl<'a> Printer<'a> {
    fn run(&mut self, doc: Doc<'a>) {
        let root: &'a Indent<'a> = self.indents.alloc(Indent::ROOT);
        let mut commands: Vec<Command<'a>> = vec![Command {
            indent: root,
            mode: Mode::Break,
            doc,
        }];

        while let Some(command) = commands.pop() {
            self.step(command, &mut commands);
            if commands.is_empty() && !self.line_suffixes.is_empty() {
                commands.extend(self.line_suffixes.drain(..).rev());
            }
        }
    }

    fn step(&mut self, command: Command<'a>, commands: &mut Vec<Command<'a>>) {
        let Command { indent, mode, doc } = command;
        match doc.kind {
            DocKind::Text(text) => {
                self.out.push_str(text);
                self.position += text_width(text);
            }
            DocKind::Concat(parts) => {
                commands.extend(parts.iter().rev().map(|&doc| Command { indent, mode, doc }));
            }
            DocKind::Indent(contents) => {
                let indent = self.make_indent(indent);
                commands.push(Command {
                    indent,
                    mode,
                    doc: contents,
                });
            }
            DocKind::Align(kind, contents) => {
                let indent = self.make_align(indent, kind);
                commands.push(Command {
                    indent,
                    mode,
                    doc: contents,
                });
            }
            DocKind::Trim => {
                self.position = self.position.saturating_sub(self.trim());
            }
            DocKind::Group {
                contents,
                expanded_states,
                should_break,
                id,
            } => {
                let chosen =
                    self.print_group(command, contents, expanded_states, should_break, commands);
                if let Some(GroupId(id)) = id
                    && let Some(slot) = self.group_modes.get_mut(id as usize)
                {
                    *slot = Some(chosen);
                }
            }
            DocKind::Fill(parts) => self.print_fill(indent, mode, parts, commands),
            DocKind::IfBreak {
                break_contents,
                flat_contents,
                group_id,
            } => {
                let group_mode = match group_id {
                    Some(GroupId(id)) => self.group_modes.get(id as usize).copied().flatten(),
                    None => Some(mode),
                };
                match group_mode {
                    Some(Mode::Break) => commands.push(Command {
                        indent,
                        mode,
                        doc: break_contents,
                    }),
                    Some(Mode::Flat) => commands.push(Command {
                        indent,
                        mode,
                        doc: flat_contents,
                    }),
                    // The group has not been printed yet, so it cannot have
                    // broken: Prettier reads the missing entry as flat.
                    None => commands.push(Command {
                        indent,
                        mode,
                        doc: flat_contents,
                    }),
                }
            }
            DocKind::IndentIfBreak {
                contents,
                group_id: GroupId(id),
                negate,
            } => {
                let broken =
                    self.group_modes.get(id as usize).copied().flatten() == Some(Mode::Break);
                let indented = broken != negate;
                let indent = if indented {
                    self.make_indent(indent)
                } else {
                    indent
                };
                commands.push(Command {
                    indent,
                    mode,
                    doc: contents,
                });
            }
            DocKind::LineSuffix(contents) => self.line_suffixes.push(Command {
                indent,
                mode,
                doc: contents,
            }),
            DocKind::LineSuffixBoundary => {
                if !self.line_suffixes.is_empty() {
                    commands.push(Command {
                        indent,
                        mode,
                        doc: &HARDLINE_ONLY,
                    });
                }
            }
            DocKind::Line(line) => self.print_line(command, line, commands),
            DocKind::Label(_, contents) => commands.push(Command {
                indent,
                mode,
                doc: contents,
            }),
            DocKind::BreakParent => {}
        }
    }

    fn print_group(
        &mut self,
        command: Command<'a>,
        contents: Doc<'a>,
        expanded_states: Option<&'a [Doc<'a>]>,
        should_break: bool,
        commands: &mut Vec<Command<'a>>,
    ) -> Mode {
        let Command { indent, mode, .. } = command;
        if mode == Mode::Flat && !self.should_remeasure {
            let mode = if should_break {
                Mode::Break
            } else {
                Mode::Flat
            };
            commands.push(Command {
                indent,
                mode,
                doc: contents,
            });
            return mode;
        }

        self.should_remeasure = false;
        let flat = Command {
            indent,
            mode: Mode::Flat,
            doc: contents,
        };
        let remaining = self.options.width as isize - self.position as isize;
        let has_line_suffix = !self.line_suffixes.is_empty();

        if !should_break && self.fits(flat, commands, remaining, has_line_suffix, false) {
            commands.push(flat);
            return Mode::Flat;
        }

        let Some(states) = expanded_states else {
            commands.push(Command {
                indent,
                mode: Mode::Break,
                doc: contents,
            });
            return Mode::Break;
        };

        let most_expanded = states.last().copied().unwrap_or(contents);
        if should_break {
            commands.push(Command {
                indent,
                mode: Mode::Break,
                doc: most_expanded,
            });
            return Mode::Break;
        }
        for state in states.iter().skip(1) {
            let candidate = Command {
                indent,
                mode: Mode::Flat,
                doc: state,
            };
            if self.fits(candidate, commands, remaining, has_line_suffix, false) {
                commands.push(candidate);
                return Mode::Flat;
            }
        }
        commands.push(Command {
            indent,
            mode: Mode::Break,
            doc: most_expanded,
        });
        Mode::Break
    }

    fn print_fill(
        &mut self,
        indent: &'a Indent<'a>,
        mode: Mode,
        parts: &'a [Doc<'a>],
        commands: &mut Vec<Command<'a>>,
    ) {
        let remaining = self.options.width as isize - self.position as isize;
        let has_line_suffix = !self.line_suffixes.is_empty();
        let [content, rest @ ..] = parts else {
            return;
        };
        let content_flat = Command {
            indent,
            mode: Mode::Flat,
            doc: content,
        };
        let content_break = Command {
            indent,
            mode: Mode::Break,
            doc: content,
        };
        let content_fits = self.fits(content_flat, &[], remaining, has_line_suffix, true);

        let [whitespace, rest @ ..] = rest else {
            commands.push(if content_fits {
                content_flat
            } else {
                content_break
            });
            return;
        };
        let whitespace_flat = Command {
            indent,
            mode: Mode::Flat,
            doc: whitespace,
        };
        let whitespace_break = Command {
            indent,
            mode: Mode::Break,
            doc: whitespace,
        };

        let [second_content, ..] = rest else {
            if content_fits {
                commands.push(whitespace_flat);
                commands.push(content_flat);
            } else {
                commands.push(whitespace_break);
                commands.push(content_break);
            }
            return;
        };

        let remaining_fill: Doc<'a> = self.indents.alloc(super::DocNode {
            kind: DocKind::Fill(rest),
            breaks: rest.iter().any(|part| part.breaks),
        });
        let remaining_command = Command {
            indent,
            mode,
            doc: remaining_fill,
        };

        let first_and_second: Doc<'a> = self.indents.alloc(super::DocNode {
            kind: DocKind::Concat(self.indents.alloc_slice_copy(&[
                *content,
                *whitespace,
                *second_content,
            ])),
            breaks: false,
        });
        let first_and_second_flat = Command {
            indent,
            mode: Mode::Flat,
            doc: first_and_second,
        };
        let pair_fits = self.fits(first_and_second_flat, &[], remaining, has_line_suffix, true);

        commands.push(remaining_command);
        if pair_fits {
            commands.push(whitespace_flat);
            commands.push(content_flat);
        } else if content_fits {
            commands.push(whitespace_break);
            commands.push(content_flat);
        } else {
            commands.push(whitespace_break);
            commands.push(content_break);
        }
    }

    fn print_line(
        &mut self,
        command: Command<'a>,
        line: LineMode,
        commands: &mut Vec<Command<'a>>,
    ) {
        let Command { indent, mode, .. } = command;
        if mode == Mode::Flat {
            match line {
                LineMode::Soft => return,
                LineMode::Space => {
                    self.out.push(' ');
                    self.position += 1;
                    return;
                }
                LineMode::Hard | LineMode::Literal => self.should_remeasure = true,
            }
        }

        if !self.line_suffixes.is_empty() {
            commands.push(command);
            commands.extend(self.line_suffixes.drain(..).rev());
            return;
        }

        if line == LineMode::Literal {
            self.out.push('\n');
            let width = indent.root.map_or(0, |root| root.width);
            self.push_spaces(width);
            self.position = width;
        } else {
            self.trim();
            self.out.push('\n');
            self.push_spaces(indent.width);
            self.position = indent.width;
        }
    }

    fn push_spaces(&mut self, count: usize) {
        self.out.extend(std::iter::repeat_n(' ', count));
    }

    /// Remove trailing spaces and tabs from the current line, returning how
    /// many were removed.
    fn trim(&mut self) -> usize {
        let trimmed = self.out.trim_end_matches([' ', '\t']).len();
        let removed = self.out.len() - trimmed;
        self.out.truncate(trimmed);
        removed
    }

    fn make_indent(&self, from: &'a Indent<'a>) -> &'a Indent<'a> {
        self.indents.alloc(Indent {
            width: from.width + self.options.indent_width,
            parent: Some(from),
            root: from.root,
        })
    }

    fn make_align(&self, from: &'a Indent<'a>, kind: AlignKind) -> &'a Indent<'a> {
        match kind {
            AlignKind::Spaces(n) => self.indents.alloc(Indent {
                width: from.width + n as usize,
                parent: Some(from),
                root: from.root,
            }),
            AlignKind::DedentToRoot => match from.root {
                Some(root) => root,
                None => self.indents.alloc(Indent::ROOT),
            },
            AlignKind::Root => self.indents.alloc(Indent {
                width: from.width,
                parent: from.parent,
                root: Some(from),
            }),
            AlignKind::Dedent => match from.parent {
                Some(parent) => parent,
                None => from,
            },
        }
    }

    /// Whether `next`, followed by the pending commands up to the first line
    /// break in broken mode, fits in `width` columns.
    ///
    /// `must_be_flat` is the fill printer asking whether content fits on the
    /// current line *without* any of its own groups breaking.
    fn fits(
        &self,
        next: Command<'a>,
        rest: &[Command<'a>],
        mut width: isize,
        mut has_line_suffix: bool,
        must_be_flat: bool,
    ) -> bool {
        let mut rest_index = rest.len();
        let mut commands: Vec<(Mode, Doc<'a>)> = vec![(next.mode, next.doc)];
        let mut out_width_trimmable = 0usize;

        while width >= 0 {
            let Some((mode, doc)) = commands.pop() else {
                if rest_index == 0 {
                    return true;
                }
                rest_index -= 1;
                let command = rest[rest_index];
                commands.push((command.mode, command.doc));
                continue;
            };

            match doc.kind {
                DocKind::Text(text) => {
                    width -= text_width(text) as isize;
                    out_width_trimmable = if text.ends_with([' ', '\t']) {
                        out_width_trimmable + text.len() - text.trim_end_matches([' ', '\t']).len()
                    } else {
                        0
                    };
                }
                DocKind::Concat(parts) | DocKind::Fill(parts) => {
                    commands.extend(parts.iter().rev().map(|&part| (mode, part)));
                }
                DocKind::Indent(contents)
                | DocKind::Align(_, contents)
                | DocKind::IndentIfBreak { contents, .. }
                | DocKind::Label(_, contents) => commands.push((mode, contents)),
                DocKind::Trim => {
                    width += out_width_trimmable as isize;
                    out_width_trimmable = 0;
                }
                DocKind::Group {
                    contents,
                    expanded_states,
                    should_break,
                    ..
                } => {
                    if must_be_flat && should_break {
                        return false;
                    }
                    let group_mode = if should_break { Mode::Break } else { mode };
                    let contents = match expanded_states {
                        Some(states) if group_mode == Mode::Break => {
                            states.last().copied().unwrap_or(contents)
                        }
                        _ => contents,
                    };
                    commands.push((group_mode, contents));
                }
                DocKind::IfBreak {
                    break_contents,
                    flat_contents,
                    group_id,
                } => {
                    let group_mode = match group_id {
                        Some(GroupId(id)) => self
                            .group_modes
                            .get(id as usize)
                            .copied()
                            .flatten()
                            .unwrap_or(Mode::Flat),
                        None => mode,
                    };
                    let contents = if group_mode == Mode::Break {
                        break_contents
                    } else {
                        flat_contents
                    };
                    commands.push((mode, contents));
                }
                DocKind::Line(line) => {
                    if mode == Mode::Break || matches!(line, LineMode::Hard | LineMode::Literal) {
                        return true;
                    }
                    if line == LineMode::Space {
                        width -= 1;
                        out_width_trimmable += 1;
                    }
                }
                DocKind::LineSuffix(_) => has_line_suffix = true,
                DocKind::LineSuffixBoundary => {
                    if has_line_suffix {
                        return true;
                    }
                }
                DocKind::BreakParent => {}
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Docs, HARDLINE, LINE, SOFTLINE};
    use super::*;
    use uf_infra::Bump;

    fn render(width: usize, build: impl for<'a> FnOnce(&Docs<'a>) -> Doc<'a>) -> String {
        let arena = Bump::new();
        let docs = Docs::new(&arena);
        let doc = build(&docs);
        print(
            doc,
            PrintOptions {
                width,
                indent_width: 2,
            },
            docs.group_count(),
        )
    }

    fn call<'a>(docs: &Docs<'a>, name: &str, args: &[&str]) -> Doc<'a> {
        let items = args.iter().map(|arg| docs.text(arg)).collect::<Vec<_>>();
        let separator = docs.concat([docs.text(","), &LINE]);
        docs.group(docs.concat([
            docs.text(name),
            docs.text("("),
            docs.indent(docs.concat([&SOFTLINE, docs.join(separator, items)])),
            docs.if_break(docs.text(","), &super::super::EMPTY, None),
            &SOFTLINE,
            docs.text(")"),
        ]))
    }

    #[test]
    fn a_group_that_fits_stays_flat() {
        assert_eq!(render(80, |d| call(d, "f", &["a", "b"])), "f(a, b)");
    }

    #[test]
    fn a_group_that_does_not_fit_breaks_with_indentation() {
        assert_eq!(
            render(10, |d| call(d, "call", &["alpha", "beta"])),
            "call(\n  alpha,\n  beta,\n)"
        );
    }

    #[test]
    fn hard_lines_break_enclosing_groups() {
        let out = render(80, |d| {
            d.group(d.concat([
                d.text("{"),
                d.indent(d.concat([&HARDLINE, d.text("a")])),
                &HARDLINE,
                d.text("}"),
            ]))
        });
        assert_eq!(out, "{\n  a\n}");
    }

    #[test]
    fn conditional_groups_try_each_state() {
        let out = render(12, |d| {
            let long = d.text("a-very-long-state");
            let short = d.text("short");
            d.conditional_group(&[long, short], false)
        });
        assert_eq!(out, "short");
    }

    #[test]
    fn line_suffixes_flush_before_the_next_newline() {
        let out = render(80, |d| {
            d.concat([
                d.text("a;"),
                d.line_suffix(d.text(" // c")),
                d.text(""),
                &HARDLINE,
                d.text("b;"),
            ])
        });
        assert_eq!(out, "a; // c\nb;");
    }

    #[test]
    fn fill_packs_as_many_items_as_fit() {
        let out = render(11, |d| {
            let mut parts = Vec::new();
            for (index, word) in ["one", "two", "three", "four"].iter().enumerate() {
                if index > 0 {
                    parts.push(&LINE as Doc<'_>);
                }
                parts.push(d.text(word));
            }
            d.fill(parts)
        });
        assert_eq!(out, "one two\nthree four");
    }

    #[test]
    fn if_break_can_name_another_group() {
        let out = render(80, |d| {
            let id = d.group_id();
            d.concat([
                d.group_with(
                    d.concat([d.text("x"), &HARDLINE, d.text("y")]),
                    false,
                    Some(id),
                ),
                d.if_break(d.text(" broke"), d.text(" flat"), Some(id)),
            ])
        });
        assert_eq!(out, "x\ny broke");
    }

    #[test]
    fn trailing_whitespace_is_trimmed_before_newlines() {
        let out = render(80, |d| d.concat([d.text("a  "), &HARDLINE, d.text("b")]));
        assert_eq!(out, "a\nb");
    }

    #[test]
    fn width_counts_wide_characters_double() {
        assert_eq!(text_width("日本語"), 6);
        assert_eq!(text_width("abc"), 3);
    }

    #[test]
    fn literal_lines_reset_to_the_root_indentation() {
        let out = render(80, |d| {
            d.indent(d.concat([
                &HARDLINE,
                d.text("a"),
                &super::super::LITERALLINE,
                d.text("b"),
            ]))
        });
        assert_eq!(out, "\n  a\nb");
    }
}
