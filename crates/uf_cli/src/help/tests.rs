//! The help pages, read the way a person reads them: at several widths, with
//! and without colour, in both glyph vocabularies.

use clap::CommandFactory;
use uf_term::{ColorLevel, GlyphSet};

use super::*;
use crate::Cli;

fn renderer(color: ColorLevel, glyphs: GlyphSet) -> Renderer {
    Renderer::new(Capabilities::new(color, glyphs, Tty::Piped))
}

fn request(line: &[&str]) -> Request {
    let args: Vec<OsString> = line.iter().map(OsString::from).collect();
    Request::from_args(&args, &Cli::command())
}

fn page(line: &[&str], width: usize, color: ColorLevel) -> String {
    render(
        &mut Cli::command(),
        &request(line),
        &renderer(color, GlyphSet::Unicode),
        width,
    )
}

/// What a reader sees: every escape sequence removed.
fn visible(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// The command rows under one heading of the root page.
fn section_rows<'a>(page: &'a str, heading: &str) -> Vec<&'a str> {
    page.lines()
        .skip_while(|line| *line != heading)
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

#[test]
fn every_visible_command_is_in_exactly_one_group() {
    let root = Cli::command();
    let visible: Vec<&str> = root
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .map(Command::get_name)
        .collect();
    let grouped: Vec<&str> = GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .collect();

    for name in &visible {
        let times = grouped.iter().filter(|grouped| *grouped == name).count();
        assert_eq!(
            times, 1,
            "`uf {name}` is in {times} of help::GROUPS; put it in exactly one section"
        );
    }
    for name in &grouped {
        assert!(
            visible.contains(name),
            "help::GROUPS names `{name}`, which is not a visible command"
        );
    }
}

#[test]
fn the_root_page_lists_every_command_under_its_heading() {
    let page = page(&["uf", "--help"], 100, ColorLevel::Never);
    for (title, names) in GROUPS {
        assert_eq!(
            section_rows(&page, title),
            names.to_vec(),
            "section {title}\n{page}"
        );
    }
    // And the headings come in the order they are declared.
    let positions: Vec<usize> = GROUPS
        .iter()
        .map(|(title, _)| {
            page.lines()
                .position(|line| line == *title)
                .unwrap_or_else(|| panic!("no heading {title}\n{page}"))
        })
        .collect();
    assert!(positions.is_sorted(), "{positions:?}");
}

#[test]
fn the_root_page_opens_with_the_version_and_what_uf_is() {
    let page = page(&["uf", "--help"], 100, ColorLevel::Never);
    let mut lines = page.lines();
    assert_eq!(
        lines.next(),
        Some(format!("uf · {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(
        lines
            .next()
            .is_some_and(|rule| rule.chars().all(|ch| ch == '─'))
    );
    assert_eq!(lines.next(), Some("Unified Toolchain for Flow (React)"));
}

#[test]
fn every_root_description_starts_in_one_column() {
    let page = page(&["uf", "--help"], 100, ColorLevel::Never);
    let mut columns: Vec<usize> = Vec::new();
    for (title, _) in GROUPS {
        for line in page
            .lines()
            .skip_while(|line| line != title)
            .skip(1)
            .take_while(|line| !line.is_empty())
            .filter(|line| !line.starts_with("   "))
        {
            let name_end = 2 + line[2..].find(' ').unwrap_or(0);
            let text_start =
                name_end + line[name_end..].len() - line[name_end..].trim_start().len();
            columns.push(text_start);
        }
    }
    columns.dedup();
    assert_eq!(
        columns.len(),
        1,
        "descriptions start at {columns:?}\n{page}"
    );
}

#[test]
fn no_line_is_wider_than_the_terminal() {
    for line in [
        &["uf", "--help"][..],
        &["uf", "lint", "--help"],
        &["uf", "lint", "-h"],
        &["uf", "test", "--help"],
        &["uf", "run", "--help"],
        &["uf", "env", "--help"],
    ] {
        for width in [100, 80, 60, 40, 32] {
            for color in [ColorLevel::Never, ColorLevel::TrueColor] {
                let page = visible(&page(line, width, color));
                for row in page.lines() {
                    // A single word wider than the page — a URL — is allowed
                    // to overrun rather than be broken; nothing else is.
                    if row.trim().contains(' ') {
                        assert!(
                            display_width(row) <= width,
                            "{line:?} at {width}: {row:?} is {} wide",
                            display_width(row)
                        );
                    }
                }
            }
        }
    }
}

/// clap's page reached the terminal as one line per paragraph; the longest
/// was 358 columns.
#[test]
fn long_help_is_wrapped_under_its_flag() {
    let page = page(&["uf", "lint", "--help"], 80, ColorLevel::Never);
    let at = page
        .lines()
        .position(|line| line.trim_start().starts_with("--fix-unsafe"))
        .expect("--fix-unsafe is listed");
    let lines: Vec<&str> = page.lines().collect();
    let column = lines[at]
        .find("Also apply")
        .expect("its description is beside it");
    // The next line continues the sentence in the same column.
    assert!(
        lines[at + 1][..column].trim().is_empty(),
        "{}",
        lines[at + 1]
    );
    assert!(
        !lines[at + 1][column..].starts_with(' '),
        "{}",
        lines[at + 1]
    );
    // And the paragraph that explains "unsafe" is there in full.
    let flowing = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flowing.contains("can change what the program does"),
        "{page}"
    );
}

#[test]
fn short_help_keeps_only_the_first_paragraph() {
    let short = page(&["uf", "lint", "-h"], 100, ColorLevel::Never);
    let long = page(&["uf", "lint", "--help"], 100, ColorLevel::Never);
    assert!(short.contains("--fix-unsafe"));
    assert!(
        !short.contains("can change what the program does"),
        "{short}"
    );
    assert!(long.len() > short.len());
}

#[test]
fn a_narrow_terminal_stacks_descriptions_under_their_flags() {
    let page = page(&["uf", "lint", "-h"], 32, ColorLevel::Never);
    let lines: Vec<&str> = page.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim_start() == "--json")
        .unwrap_or_else(|| panic!("--json on a line of its own\n{page}"));
    assert!(lines[at + 1].starts_with("      Emit"), "{}", lines[at + 1]);
}

#[test]
fn a_stacked_list_does_not_line_long_flags_up() {
    // Stacked, a description starts four columns in. `--cwd` padded by four
    // to line up under `-h, ` would start in that same column, as if it were
    // the last line of the row above.
    let narrow = page(&["uf", "clean", "--help"], 32, ColorLevel::Never);
    let rows = narrow
        .lines()
        .skip_while(|line| *line != "Global options")
        .skip(1)
        .collect::<Vec<_>>();
    assert!(rows.contains(&"  --cwd <DIR>"), "{narrow}");
    assert!(rows.contains(&"  -h, --help"), "{narrow}");

    // Beside a description column the padding stays: that is what it is for.
    let wide = page(&["uf", "clean", "--help"], 100, ColorLevel::Never);
    assert!(wide.contains("\n      --cwd <DIR>  "), "{wide}");
}

#[test]
fn defaults_and_values_are_noted_after_the_description() {
    let page = page(&["uf", "lint", "-h"], 100, ColorLevel::Never);
    let color = page
        .lines()
        .find(|line| line.trim_start().starts_with("--color"))
        .expect("--color is a global option");
    assert!(color.contains("[values: auto, always, never]"), "{color}");
    assert!(color.contains("[default: auto]"), "{color}");
}

#[test]
fn global_options_are_listed_once_at_the_end() {
    let page = page(&["uf", "lint", "--help"], 100, ColorLevel::Never);
    let global = page
        .lines()
        .position(|line| line == "Global options")
        .expect("a global options section");
    let options = page
        .lines()
        .position(|line| line == "Options")
        .expect("an options section");
    assert!(options < global);
    let cwd = page.matches("--cwd").count();
    assert_eq!(cwd, 1, "{page}");
}

#[test]
fn a_command_with_subcommands_lists_them() {
    let page = page(&["uf", "env", "--help"], 100, ColorLevel::Never);
    let rows = section_rows(&page, "Commands");
    assert!(rows.contains(&"doctor"), "{page}");
    assert!(
        !rows.contains(&"help"),
        "clap's `help` is not one of uf's: {page}"
    );
}

#[test]
fn the_usage_line_names_the_binary_it_was_run_as() {
    let page = page(&["ufr", "run", "--help"], 100, ColorLevel::Never);
    assert!(page.contains("  ufr run "), "{page}");
}

#[test]
fn without_colour_there_is_no_escape_anywhere() {
    for line in [&["uf", "--help"][..], &["uf", "lint", "--help"]] {
        assert!(!page(line, 100, ColorLevel::Never).contains('\x1b'));
    }
}

#[test]
fn with_colour_commands_and_flags_are_accented() {
    let renderer = renderer(ColorLevel::Ansi16, GlyphSet::Unicode);
    let mut accent = String::new();
    renderer
        .theme()
        .accent
        .open(ColorLevel::Ansi16, &mut accent);
    let page = page(&["uf", "--help"], 100, ColorLevel::Ansi16);
    assert!(page.contains(&format!("{accent}build")), "{page:?}");
    assert!(page.contains(&format!("{accent}--color")), "{page:?}");
}

#[test]
fn an_ascii_terminal_gets_an_ascii_frame() {
    let page = render(
        &mut Cli::command(),
        &request(&["uf", "--help"]),
        &renderer(ColorLevel::Never, GlyphSet::Ascii),
        100,
    );
    let mut lines = page.lines();
    assert!(lines.next().is_some_and(str::is_ascii));
    assert!(
        lines
            .next()
            .is_some_and(|rule| rule.chars().all(|ch| ch == '-'))
    );
    // The hint mark too.
    assert!(page.lines().any(|line| line.starts_with("> ")), "{page}");
}

#[test]
fn a_request_is_read_off_the_command_line() {
    let lint = request(&["uf", "lint", "--help"]);
    assert_eq!(lint.path, ["lint"]);
    assert!(lint.long);
    assert_eq!(lint.bin, "uf");

    assert!(!request(&["uf", "lint", "-h"]).long);
    assert!(request(&["uf", "--help"]).path.is_empty());

    // clap's `help` subcommand, at any depth.
    let doctor = request(&["uf", "help", "env", "doctor"]);
    assert_eq!(doctor.path, ["env", "doctor"]);
    assert!(doctor.long);

    // An alias is the command it stands for.
    assert_eq!(request(&["uf", "i", "--help"]).path, ["install"]);

    // A task name is not a subcommand, and ends the walk.
    assert_eq!(request(&["uf", "run", "build", "--help"]).path, ["run"]);

    // Global flags and their values are stepped over.
    let global = request(&["uf", "--cwd", "lint", "--color", "never", "fmt", "-h"]);
    assert_eq!(global.path, ["fmt"]);
    assert_eq!(global.color, ColorChoice::Never);
    assert_eq!(
        request(&["uf", "--color=always", "-h"]).color,
        ColorChoice::Always
    );

    assert_eq!(request(&["/usr/local/bin/ufr", "run", "-h"]).bin, "ufr");
}

#[test]
fn a_description_loses_its_full_stop_but_not_its_ellipsis() {
    assert_eq!(trim_period("Build the project."), "Build the project");
    assert_eq!(trim_period("Wait..."), "Wait...");
    assert_eq!(trim_period("  no stop  "), "no stop");
}

#[test]
fn a_usage_line_wraps_under_its_first_argument() {
    let page = page(&["uf", "run", "--help"], 32, ColorLevel::Never);
    let usage: Vec<&str> = page
        .lines()
        .skip_while(|line| *line != "Usage")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .collect();
    assert_eq!(
        usage,
        ["  uf run [OPTIONS] [SCRIPT]", "         [ARGS]..."],
        "{page}"
    );
}

/// A page compared whole against `help/snapshots/<name>.txt`, which a reviewer
/// reads as the page itself. `UF_UPDATE_SNAPSHOTS=1` writes the files instead
/// of comparing, for a change that means to move them.
///
/// Every snapshot is also held to the one layout rule a transcript can break
/// on its own: no blank line directly under a rule.
fn assert_snapshot(name: &str, page: &str) {
    assert_eq!(
        uf_term::blank_after_rule(page),
        None,
        "{name}: a blank line under the rule\n{page}"
    );
    // Escapes spelled out, so a coloured page is a file a diff can show.
    let actual = page.replace('\x1b', "\\e");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/help/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("UF_UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("snapshot directory");
        std::fs::write(&path, &actual).expect("snapshot written");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no snapshot at {}; run with UF_UPDATE_SNAPSHOTS=1 to write it",
            path.display()
        )
    });
    if actual != expected {
        let first = actual
            .lines()
            .zip(expected.lines())
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
        panic!(
            "{name} differs from {} at line {}; rerun with UF_UPDATE_SNAPSHOTS=1 if the \
             change is meant\n--- expected\n{expected}\n--- actual\n{actual}",
            path.display(),
            first + 1
        );
    }
}

/// `uf clean --help`: a description of two paragraphs, flags without short
/// forms, and the global flags with their values and defaults.
const CLEAN: &[&str] = &["uf", "clean", "--help"];

/// `uf env -h`: a command whose page is a list of subcommands.
const ENV: &[&str] = &["uf", "env", "-h"];

/// What a pipe, a file or a CI log gets: no colour, laid out at the cap.
#[test]
fn snapshot_piped() {
    assert_snapshot("clean-100", &page(CLEAN, MAX_WIDTH, ColorLevel::Never));
    assert_snapshot("env-100", &page(ENV, MAX_WIDTH, ColorLevel::Never));
}

/// A terminal narrower than the cap: descriptions wrap in their column.
#[test]
fn snapshot_at_sixty_columns() {
    assert_snapshot("clean-60", &page(CLEAN, 60, ColorLevel::Never));
    assert_snapshot("env-60", &page(ENV, 60, ColorLevel::Never));
}

/// A narrow terminal: descriptions go under their names.
#[test]
fn snapshot_at_thirty_two_columns() {
    assert_snapshot("clean-32", &page(CLEAN, 32, ColorLevel::Never));
    assert_snapshot("env-32", &page(ENV, 32, ColorLevel::Never));
}

/// A colour terminal: headings bold, what a reader types in the accent,
/// placeholders and notes receding, and a code span drawn in the accent in
/// place of its backticks.
#[test]
fn snapshot_on_a_colour_terminal() {
    let coloured = render(
        &mut Cli::command(),
        &request(CLEAN),
        &Renderer::new(Capabilities::new(
            ColorLevel::Ansi16,
            GlyphSet::Unicode,
            Tty::Interactive,
        )),
        60,
    );
    assert_snapshot("clean-60-color", &coloured);
    assert!(!visible(&coloured).contains('`'), "{coloured}");
}

/// A terminal that cannot draw box characters.
#[test]
fn snapshot_with_ascii_glyphs() {
    let ascii = render(
        &mut Cli::command(),
        &request(CLEAN),
        &renderer(ColorLevel::Never, GlyphSet::Ascii),
        60,
    );
    assert_snapshot("clean-60-ascii", &ascii);
}

/// The page on a terminal, drawn the way `uf` resolves it: from `--color` and
/// the environment.
fn page_on_a_terminal(line: &[&str], env: &TerminalEnv) -> String {
    let request = request(line);
    let renderer = Renderer::new(request.capabilities(Tty::Interactive, env));
    render(&mut Cli::command(), &request, &renderer, 60)
}

/// A colour terminal with a UTF-8 locale, before anything turns colour off.
fn colour_terminal() -> TerminalEnv {
    TerminalEnv::default()
        .with_term("xterm-256color")
        .with_locale("en_US.UTF-8")
}

#[test]
fn no_color_and_a_dumb_terminal_get_the_plain_page() {
    let plain = page(CLEAN, 60, ColorLevel::Never);
    assert!(page_on_a_terminal(CLEAN, &colour_terminal()).contains('\x1b'));

    let no_color = page_on_a_terminal(CLEAN, &colour_terminal().with_no_color("1"));
    assert_eq!(no_color, plain, "NO_COLOR");

    let dumb = page_on_a_terminal(CLEAN, &colour_terminal().with_term("dumb"));
    assert_eq!(visible(&dumb), dumb, "TERM=dumb");
}

#[test]
fn the_color_flag_reaches_the_help() {
    // `--color never` at a colour terminal is plain.
    let never = page_on_a_terminal(
        &["uf", "--color", "never", "clean", "--help"],
        &colour_terminal(),
    );
    assert_eq!(never, page(CLEAN, 60, ColorLevel::Never));

    // `--color always` is coloured even where the stream is a pipe, so
    // `uf --color always --help | less -R` is.
    let request = request(&["uf", "--color=always", "clean", "--help"]);
    let renderer = Renderer::new(request.capabilities(Tty::Piped, &colour_terminal()));
    let always = render(&mut Cli::command(), &request, &renderer, 60);
    assert!(always.contains('\x1b'), "{always}");
}
