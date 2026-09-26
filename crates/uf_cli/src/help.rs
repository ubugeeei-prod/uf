//! uf's help pages, drawn by uf rather than by clap.
//!
//! clap still *parses* every command line and still decides when help was
//! asked for; what changes here is what the reader then sees. clap's page is
//! a fine default and a poor front door for a toolchain with forty-odd
//! commands:
//!
//! * it listed every command in declaration order, so `dev` sat between `new`
//!   and `doc` and the dependency commands were scattered through the list;
//! * it did not wrap (uf is built without clap's `wrap_help`), so a flag's
//!   long help reached the terminal as one 350-column line, cut wherever the
//!   window ended and continued at column zero under the flag column;
//! * it drew in its own palette and ignored `--color`, because the flag is
//!   read after clap has already printed the page.
//!
//! So the model is clap's — every name, flag, value, default and sentence
//! comes from the one [`clap::Command`] the parser uses, and cannot drift from
//! it — and the page is drawn with `uf_term`, in the same vocabulary as every
//! other screen uf prints: headings in bold, what a reader types in the
//! accent, placeholders and defaults receding, sentences wrapped to the
//! terminal with a hanging indent.
//!
//! The one thing that is *not* derived is [`GROUPS`], the sections `uf --help`
//! sorts commands into. A test fails when a visible command is in no group or
//! in two, so a new command cannot silently fall off the front page.

use std::ffi::OsString;
use std::fmt::Write as _;

use clap::{Arg, ArgAction, Command};
use uf_term::{
    Capabilities, ColorChoice, Definition, Renderer, TerminalEnv, TerminalSize, Tty, display_width,
};

/// The widest a help page is laid out, however wide the terminal is.
///
/// Prose much wider than this is hard to read — the eye loses the start of the
/// next line — and it is the width clap wrapped to when it wrapped at all.
pub(crate) const MAX_WIDTH: usize = 100;

/// The sections of `uf --help`, in order, and the commands in each.
///
/// Ordered by when in a project's life a reader reaches for them, and within a
/// section by how often. Every visible top-level command appears exactly once;
/// see `every_visible_command_is_in_exactly_one_group`.
pub(crate) const GROUPS: &[(&str, &[&str])] = &[
    ("Start", &["new", "init", "migrate", "codemod"]),
    (
        "Develop",
        &[
            "dev", "build", "preview", "start", "run", "exec", "routes", "ui", "doc", "i18n",
            "sqlc", "clean",
        ],
    ),
    ("Verify", &["check", "lint", "fmt", "test", "prepare"]),
    (
        "Dependencies",
        &[
            "install", "add", "remove", "update", "dedupe", "ls", "why", "audit", "search",
            "patch", "link", "unlink", "catalog", "pm",
        ],
    ),
    ("Release", &["release", "publish"]),
    (
        "Toolchain",
        &[
            "info",
            "inspect",
            "explain",
            "env",
            "use",
            "self-update",
            "self-uninstall",
            "completion",
        ],
    ),
    ("Editors and agents", &["lsp", "editor", "mcp"]),
];

/// Where the documentation lives, printed at the foot of `uf --help`.
const DOCS: &str = "https://docs.uniflowed.dev";

/// Which page was asked for, read off the command line clap refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    /// The subcommand path below the root: `["env", "doctor"]`.
    pub(crate) path: Vec<String>,
    /// `--help` or `help`, rather than `-h`.
    pub(crate) long: bool,
    /// What `--color` said, if it was given.
    pub(crate) color: ColorChoice,
    /// What the program was invoked as: `uf`, or `ufr` for the alias binary.
    pub(crate) bin: String,
}

impl Request {
    /// Read the request off the arguments, the way clap read them.
    ///
    /// The subcommand path is walked through the real command tree, aliases
    /// included, and stops at the first word that is not a subcommand: in
    /// `uf run build --help`, `build` is a task, and the page is `uf run`'s.
    /// `help` as a word switches to reading the rest as the path, which is
    /// clap's `uf help env doctor`.
    pub(crate) fn from_args(args: &[OsString], root: &Command) -> Self {
        let bin = args
            .first()
            .and_then(|arg| std::path::Path::new(arg).file_stem())
            .and_then(|stem| stem.to_str())
            .unwrap_or("uf")
            .to_owned();
        let mut request = Self {
            path: Vec::new(),
            long: false,
            color: ColorChoice::Auto,
            bin,
        };
        let mut command = root;
        let mut descending = true;
        let mut words = args.iter().skip(1).filter_map(|arg| arg.to_str());
        while let Some(word) = words.next() {
            match word {
                "--" => break,
                "--help" => request.long = true,
                "-h" => {}
                "--color" => {
                    if let Some(value) = words.next() {
                        request.color = ColorChoice::parse(value).unwrap_or(request.color);
                    }
                }
                "--cwd" => {
                    let _ = words.next();
                }
                flag if flag.starts_with("--color=") => {
                    request.color =
                        ColorChoice::parse(&flag["--color=".len()..]).unwrap_or(request.color);
                }
                flag if flag.starts_with('-') => {}
                "help" if descending && command.has_subcommands() => request.long = true,
                name if descending => match command.find_subcommand(name) {
                    Some(sub) => {
                        request.path.push(sub.get_name().to_owned());
                        command = sub;
                    }
                    None => descending = false,
                },
                _ => {}
            }
        }
        request
    }

    /// The renderer for a stream, honouring `--color` as well as the
    /// environment.
    pub(crate) fn renderer(&self, stderr: bool) -> Renderer {
        let tty = if stderr {
            Tty::of(&std::io::stderr())
        } else {
            Tty::of(&std::io::stdout())
        };
        Renderer::new(self.capabilities(tty, &TerminalEnv::from_process()))
    }

    /// What a page may draw with on a stream, from `--color` and `env`: the
    /// same resolution every other screen gets, so `NO_COLOR`, `TERM=dumb`
    /// and the ASCII fallback reach the help too.
    pub(crate) fn capabilities(&self, tty: Tty, env: &TerminalEnv) -> Capabilities {
        Capabilities::detect(self.color, tty, env)
    }
}

/// How wide to lay a page out on a stream with these capabilities.
///
/// A terminal is measured, and a page is never wider than [`MAX_WIDTH`]. A
/// pipe has no width of its own, so it gets [`MAX_WIDTH`] — which also makes
/// what a script or a test reads the same on every machine.
pub(crate) fn width_for(capabilities: Capabilities) -> usize {
    if capabilities.is_interactive() {
        TerminalSize::detect(Tty::Interactive, &TerminalEnv::from_process())
            .columns()
            .min(MAX_WIDTH)
    } else {
        MAX_WIDTH
    }
}

/// Draw the page `request` asks for.
///
/// `root` is the tree the parser refused the command line with. Only the
/// commands on the requested path are built here — each one's build hands
/// the global flags down to its children — because clap's `build` builds
/// every command in the tree, and building forty-odd commands' flags for a
/// page that shows one of them is most of what drawing it would cost.
pub(crate) fn render(
    root: &mut Command,
    request: &Request,
    renderer: &Renderer,
    width: usize,
) -> String {
    root.set_bin_name(request.bin.clone());
    let mut out = String::with_capacity(4 * 1024);
    let mut usage = usage_of(root);
    if request.path.is_empty() {
        root_page(&mut out, root, &usage, renderer, width);
        return out;
    }
    let mut command: &mut Command = root;
    for name in &request.path {
        // The path was walked through this same tree, so every name is here;
        // stopping short would only draw a parent's page.
        if command.find_subcommand(name).is_none() {
            break;
        }
        let bin = uf_infra::into_string(uf_infra::cstr!(
            "{} {name}",
            command.get_bin_name().unwrap_or(command.get_name())
        ));
        command = command
            .find_subcommand_mut(name)
            .expect("the subcommand was found above");
        command.set_bin_name(bin);
        usage = usage_of(command);
    }
    command_page(&mut out, command, &usage, request.long, renderer, width);
    out
}

/// The usage line clap derives, without its `Usage:` title.
///
/// Derived rather than written here because it is the one line that has to
/// agree with the parser about which arguments are required and in what order.
/// clap builds the command it is asked about first, which is why this takes
/// it by `&mut`.
fn usage_of(command: &mut Command) -> Vec<String> {
    let rendered = command.render_usage().to_string();
    rendered
        .lines()
        .map(|line| {
            line.trim_start()
                .strip_prefix("Usage:")
                .unwrap_or(line)
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// `uf --help`: the commands in their sections, and the global options.
fn root_page(
    out: &mut String,
    root: &Command,
    usage: &[String],
    renderer: &Renderer,
    width: usize,
) {
    let bin = root.get_bin_name().unwrap_or("uf");
    renderer.banner(out, bin, root.get_version());
    if let Some(about) = root.get_about() {
        renderer.prose(out, &about.to_string(), renderer.theme().value, 0, 0, width);
        out.push('\n');
    }

    out.push('\n');
    section(out, renderer, "Usage");
    for line in usage {
        usage_line(out, renderer, line, width);
    }

    let mut terms: Vec<String> = Vec::new();
    for (_, names) in GROUPS {
        for name in *names {
            terms.push(literal(renderer, name));
        }
    }
    let term_column = terms
        .iter()
        .map(|term| display_width(term))
        .max()
        .unwrap_or(0);
    let mut index = 0;
    for (title, names) in GROUPS {
        out.push('\n');
        section(out, renderer, title);
        let abouts: Vec<String> = names
            .iter()
            .map(|name| {
                root.find_subcommand(name)
                    .and_then(Command::get_about)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            })
            .collect();
        let rows: Vec<Definition<'_>> = abouts
            .iter()
            .enumerate()
            .map(|(offset, about)| Definition::new(&terms[index + offset], trim_period(about)))
            .collect();
        // One term column for every section, so the descriptions form one
        // column down the whole page rather than a new one per heading.
        renderer.definitions_at(out, 2, width, &rows, false, term_column);
        index += names.len();
    }

    out.push('\n');
    section(out, renderer, "Options");
    let options = options_of(root, Scope::Global, false);
    let rows = option_rows(renderer, &options, false, width);
    let definitions: Vec<Definition<'_>> = rows.iter().map(OptionRow::definition).collect();
    renderer.definitions(out, 2, width, &definitions, false);

    out.push('\n');
    renderer.hint_within(
        out,
        0,
        width,
        &uf_infra::into_string(uf_infra::cstr!(
            "`{bin} <command> --help` for a command's options, or `{bin}` alone for a menu"
        )),
    );
    renderer.hint_within(
        out,
        0,
        width,
        uf_infra::cstr!("documentation at {DOCS}").as_str(),
    );
}

/// `uf <command> --help`.
fn command_page(
    out: &mut String,
    command: &Command,
    usage: &[String],
    long: bool,
    renderer: &Renderer,
    width: usize,
) {
    let name = command.get_bin_name().unwrap_or(command.get_name());
    renderer.banner(out, name, None);

    let about = command.get_about().map(ToString::to_string);
    let long_about = command.get_long_about().map(ToString::to_string);
    let text = if long {
        long_about.or(about)
    } else {
        about.or(long_about)
    };
    if let Some(text) = text {
        renderer.prose(out, &text, renderer.theme().value, 0, 0, width);
        out.push('\n');
    }

    out.push('\n');
    section(out, renderer, "Usage");
    for line in usage {
        usage_line(out, renderer, line, width);
    }

    let subcommands: Vec<&Command> = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
        .collect();
    if !subcommands.is_empty() {
        let terms: Vec<String> = subcommands
            .iter()
            .map(|sub| literal(renderer, sub.get_name()))
            .collect();
        let abouts: Vec<String> = subcommands
            .iter()
            .map(|sub| sub.get_about().map(ToString::to_string).unwrap_or_default())
            .collect();
        let rows: Vec<Definition<'_>> = terms
            .iter()
            .zip(&abouts)
            .map(|(term, about)| Definition::new(term, trim_period(about)))
            .collect();
        out.push('\n');
        section(out, renderer, "Commands");
        renderer.definitions(out, 2, width, &rows, false);
    }

    let positionals: Vec<&Arg> = command
        .get_positionals()
        .filter(|arg| shown(arg, long))
        .collect();
    if !positionals.is_empty() {
        let rows = option_rows(renderer, &positionals, long, width);
        let definitions: Vec<Definition<'_>> = rows.iter().map(OptionRow::definition).collect();
        out.push('\n');
        section(out, renderer, "Arguments");
        renderer.definitions(out, 2, width, &definitions, long);
        let column = definitions
            .iter()
            .map(|definition| display_width(definition.term))
            .max()
            .unwrap_or(0);
        values_below(
            out,
            renderer,
            width,
            &rows,
            long,
            text_column(column, width),
        );
    }

    // Options under their own headings where the command gave them one, then
    // the rest, then the global ones every command takes.
    let own = options_of(command, Scope::Own, long);
    let mut headings: Vec<Option<&str>> = Vec::new();
    for arg in &own {
        if !headings.contains(&arg.get_help_heading()) {
            headings.push(arg.get_help_heading());
        }
    }
    headings.sort_by_key(Option::is_some);
    for heading in headings {
        let args: Vec<&Arg> = own
            .iter()
            .copied()
            .filter(|arg| arg.get_help_heading() == heading)
            .collect();
        out.push('\n');
        section(out, renderer, heading.unwrap_or("Options"));
        option_section(out, renderer, width, &args, long);
    }

    let global = options_of(command, Scope::Global, long);
    if !global.is_empty() {
        out.push('\n');
        section(out, renderer, "Global options");
        // One line each, whatever `--help` asked for: they are the same on
        // every page, and `uf --help` is where they are explained.
        option_section(out, renderer, width, &global, false);
    }

    let after = if long {
        command.get_after_long_help().or(command.get_after_help())
    } else {
        command.get_after_help()
    };
    if let Some(after) = after {
        out.push('\n');
        renderer.prose(out, &after.to_string(), renderer.theme().muted, 0, 0, width);
        out.push('\n');
    }
}

/// A section of options: the list, then any possible values that have
/// descriptions of their own.
fn option_section(out: &mut String, renderer: &Renderer, width: usize, args: &[&Arg], long: bool) {
    let rows = option_rows(renderer, args, long, width);
    let definitions: Vec<Definition<'_>> = rows.iter().map(OptionRow::definition).collect();
    if long && rows.iter().any(|row| !row.values.is_empty()) {
        // A row with a list of values under it is drawn on its own, so the
        // list lands under its flag rather than after the whole section —
        // in the column the whole section shares.
        let column = definitions
            .iter()
            .map(|definition| display_width(definition.term))
            .max()
            .unwrap_or(0);
        for (index, (row, definition)) in rows.iter().zip(&definitions).enumerate() {
            if index > 0 {
                out.push('\n');
            }
            renderer.definitions_at(
                out,
                2,
                width,
                std::slice::from_ref(definition),
                false,
                column,
            );
            values_below(
                out,
                renderer,
                width,
                std::slice::from_ref(row),
                true,
                text_column(column, width),
            );
        }
    } else {
        renderer.definitions(out, 2, width, &definitions, long);
    }
}

/// The possible values of the rows that have them, each with its sentence.
fn values_below(
    out: &mut String,
    renderer: &Renderer,
    width: usize,
    rows: &[OptionRow],
    long: bool,
    indent: usize,
) {
    if !long {
        return;
    }
    for row in rows {
        if row.values.is_empty() {
            continue;
        }
        let terms: Vec<String> = row
            .values
            .iter()
            .map(|(name, _)| literal(renderer, name))
            .collect();
        let definitions: Vec<Definition<'_>> = terms
            .iter()
            .zip(&row.values)
            .map(|(term, (_, help))| Definition::new(term, help))
            .collect();
        renderer.definitions(out, indent, width, &definitions, false);
    }
}

/// Where a two-column list indented by two starts its descriptions, for a
/// list drawn under one of them: the description column, or a fixed indent
/// when the list is narrow enough to stack.
fn text_column(term_column: usize, width: usize) -> usize {
    if Definition::stacks(2, width, term_column) {
        6
    } else {
        2 + term_column.min(width.saturating_sub(2) / 3) + 2
    }
}

/// Which arguments a section is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The command's own flags.
    Own,
    /// The flags every command takes: `--cwd`, `--color`, `--help`.
    Global,
}

/// Whether `arg` is drawn at all on this page.
fn shown(arg: &Arg, long: bool) -> bool {
    !(arg.is_hide_set()
        || (long && arg.is_hide_long_help_set())
        || (!long && arg.is_hide_short_help_set()))
}

/// The flags of `command` in `scope`, in the order clap would print them.
fn options_of(command: &Command, scope: Scope, long: bool) -> Vec<&Arg> {
    let mut args: Vec<&Arg> = command
        .get_arguments()
        .filter(|arg| !arg.is_positional() && shown(arg, long))
        .filter(|arg| {
            let global = arg.is_global_set() || is_builtin(arg);
            match scope {
                Scope::Own => !global,
                Scope::Global => global,
            }
        })
        .collect();
    // `--help` and `--version` last, where clap puts them.
    args.sort_by_key(|arg| (is_builtin(arg), arg.get_display_order()));
    args
}

/// clap's own `--help` and `--version`.
fn is_builtin(arg: &Arg) -> bool {
    matches!(
        arg.get_action(),
        ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
    )
}

/// One argument, ready to draw.
struct OptionRow {
    /// `-j, --concurrency <N>`, styled.
    term: String,
    /// What it does.
    text: String,
    /// `[default: auto]` and its kin.
    meta: String,
    /// Possible values with a sentence each, drawn under the row in the long
    /// form.
    values: Vec<(String, String)>,
}

impl OptionRow {
    fn definition(&self) -> Definition<'_> {
        Definition::new(&self.term, &self.text).with_meta(&self.meta)
    }
}

/// Build the rows for a list of arguments.
///
/// Long flags line up when any flag in the list has a short form, the way
/// every Unix manual lays them out: `-j, --concurrency` above
/// `    --force`.
///
/// The alignment is four spaces a flag without a short form does not need,
/// and on a terminal narrow enough that those four push the flag past the
/// edge it is dropped for that flag: a column that lines up is worth less than
/// a flag that is not cut in two.
///
/// It is dropped for the whole list when the list stacks. A stacked
/// description starts four columns in, under its flag, and a long flag
/// padded by the same four would start where its own description does, so
/// that it read as the last line of the row above rather than the head of its
/// own.
fn option_rows(renderer: &Renderer, args: &[&Arg], long: bool, width: usize) -> Vec<OptionRow> {
    let aligned_widest = args
        .iter()
        .map(|arg| 4 + unaligned_width(arg))
        .max()
        .unwrap_or(0);
    let align = args.iter().any(|arg| arg.get_short().is_some())
        && !Definition::stacks(2, width, aligned_widest);
    args.iter()
        .map(|arg| option_row(renderer, arg, long, align, width))
        .collect()
}

fn option_row(renderer: &Renderer, arg: &Arg, long: bool, align: bool, width: usize) -> OptionRow {
    let theme = renderer.theme();
    let level = renderer.color();
    let mut term = String::new();

    if arg.is_positional() {
        let name = placeholder(arg);
        let multiple = arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1);
        let text = match (arg.is_required_set(), multiple) {
            (true, false) => uf_infra::into_string(uf_infra::cstr!("<{name}>")),
            (true, true) => uf_infra::into_string(uf_infra::cstr!("<{name}>...")),
            (false, false) => uf_infra::into_string(uf_infra::cstr!("[{name}]")),
            (false, true) => uf_infra::into_string(uf_infra::cstr!("[{name}]...")),
        };
        theme.accent.paint(level, &text, &mut term);
    } else {
        match arg.get_short() {
            Some(short) => {
                theme
                    .accent
                    .paint(level, uf_infra::cstr!("-{short}").as_str(), &mut term);
                if arg.get_long().is_some() {
                    term.push_str(", ");
                }
            }
            None if align && 2 + 4 + unaligned_width(arg) <= width => term.push_str("    "),
            None => {}
        }
        if let Some(name) = arg.get_long() {
            theme
                .accent
                .paint(level, uf_infra::cstr!("--{name}").as_str(), &mut term);
        }
        if arg.get_action().takes_values() {
            term.push(' ');
            theme.muted.paint(
                level,
                uf_infra::cstr!("<{}>", placeholder(arg)).as_str(),
                &mut term,
            );
        }
    }

    let text = match arg.get_action() {
        // clap's own sentences mention `-h` and `--help` in its own quoting;
        // these say the same thing in uf's.
        ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong => {
            "Print help: `-h` for a summary, `--help` for everything".to_owned()
        }
        ArgAction::Version => "Print the version".to_owned(),
        _ => {
            let help = if long {
                arg.get_long_help().or(arg.get_help())
            } else {
                arg.get_help().or(arg.get_long_help())
            };
            let help = help.map(ToString::to_string).unwrap_or_default();
            if long {
                help
            } else {
                trim_period(first_paragraph(&help)).to_owned()
            }
        }
    };

    let possible: Vec<_> = if arg.is_hide_possible_values_set() || !arg.get_action().takes_values()
    {
        Vec::new()
    } else {
        arg.get_possible_values()
            .into_iter()
            .filter(|value| !value.is_hide_set())
            .collect()
    };
    let described = possible.iter().any(|value| value.get_help().is_some());

    let mut meta = String::new();
    let mut note = |label: &str, value: &str| {
        if !meta.is_empty() {
            meta.push(' ');
        }
        let _ = write!(meta, "[{label}: {value}]");
    };
    if !possible.is_empty() && !(long && described) {
        let names: Vec<&str> = possible.iter().map(|value| value.get_name()).collect();
        note("values", &names.join(", "));
    }
    if arg.get_action().takes_values() && !arg.is_hide_default_value_set() {
        let defaults: Vec<String> = arg
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        if !defaults.is_empty() {
            note("default", &defaults.join(", "));
        }
    }
    if let Some(env) = arg.get_env()
        && !arg.is_hide_env_set()
    {
        note("env", &env.to_string_lossy());
    }

    let values = if long && described {
        possible
            .iter()
            .map(|value| {
                (
                    value.get_name().to_owned(),
                    value
                        .get_help()
                        .map(|help| trim_period(&help.to_string()).to_owned())
                        .unwrap_or_default(),
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    OptionRow {
        term,
        text,
        meta,
        values,
    }
}

/// How wide a flag's term is without the alignment: `--long <VALUE>`.
fn unaligned_width(arg: &Arg) -> usize {
    let long = arg.get_long().map_or(0, |name| 2 + display_width(name));
    let value = if arg.get_action().takes_values() {
        3 + display_width(&placeholder(arg))
    } else {
        0
    };
    long + value
}

/// The name standing in for an argument's value: its `value_name`, or its id
/// in capitals.
fn placeholder(arg: &Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string)
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase())
}

/// A section heading.
fn section(out: &mut String, renderer: &Renderer, title: &str) {
    renderer.line(out, renderer.theme().title, title);
}

/// A word a reader types, in the accent.
fn literal(renderer: &Renderer, text: &str) -> String {
    let mut out = String::new();
    renderer
        .theme()
        .accent
        .paint(renderer.color(), text, &mut out);
    out
}

/// A usage line: what is typed literally in the accent, placeholders muted.
///
/// On a terminal too narrow for the whole line it wraps between arguments,
/// and the continuation lines start under the first argument, so the command
/// itself stands alone at the left:
///
/// ```text
///   uf run [OPTIONS]
///          [SCRIPT] [ARGS]...
/// ```
fn usage_line(out: &mut String, renderer: &Renderer, line: &str, width: usize) {
    const INDENT: usize = 2;
    let theme = renderer.theme();
    let is_placeholder = |word: &str| word.starts_with('<') || word.starts_with('[');
    let command_width: usize = line
        .split(' ')
        .take_while(|word| !is_placeholder(word))
        .map(|word| display_width(word) + 1)
        .sum();
    let margin = if width.saturating_sub(INDENT + command_width) >= 16 {
        INDENT + command_width
    } else {
        INDENT + 2
    };
    uf_term::push_spaces(out, INDENT);
    let mut column = INDENT;
    for (index, word) in line.split(' ').enumerate() {
        let word_width = display_width(word);
        if index > 0 {
            if column + 1 + word_width > width && column > margin {
                out.push('\n');
                uf_term::push_spaces(out, margin);
                column = margin;
            } else {
                out.push(' ');
                column += 1;
            }
        }
        let style = if is_placeholder(word) {
            theme.muted
        } else {
            theme.accent
        };
        style.paint(renderer.color(), word, out);
        column += word_width;
    }
    out.push('\n');
}

/// The first paragraph of a help text: what `-h` shows of it.
fn first_paragraph(text: &str) -> &str {
    text.split("\n\n").next().unwrap_or(text).trim()
}

/// A one-line description without its closing full stop, the way a list of
/// commands reads: `Build the project for production`, not a list of
/// sentences.
fn trim_period(text: &str) -> &str {
    let text = text.trim();
    match text.strip_suffix('.') {
        // An ellipsis or an abbreviation is not a full stop.
        Some(rest) if !rest.ends_with('.') => rest,
        _ => text,
    }
}

#[cfg(test)]
mod tests;
