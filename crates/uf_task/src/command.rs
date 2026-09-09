//! What a task's command string means, before anything is spawned.
//!
//! `uf run` was `sh -c` for every task, which is three problems in one string:
//! there is no `sh` on Windows, so the whole command was unavailable there;
//! uf could not say what a task would start; and a task's meaning was
//! whichever `/bin/sh` the machine happened to have.
//!
//! So this module reads the command string first. Most task commands are a
//! program and some arguments — every one in this repository's own
//! `uf.config.js` is, which
//! [`tests::every_task_in_this_repository_runs_without_a_shell`] asserts
//! rather than assumes — and those uf starts itself, with no shell on any
//! platform. A command that uses shell *syntax* is still a shell's to run, and
//! [`Command::Shell`] says which construct made it one so that the caller can
//! name it rather than shrug.
//!
//! # What this module does not decide
//!
//! Which shell, and whether there is one. That is `uf run`'s question — see
//! `uf_cli`'s `TaskSpawner` — and the answer is different on Windows, where
//! the honest options are "a POSIX shell is installed" and "say so", not
//! "reinterpret POSIX text under `cmd.exe`'s rules".
//!
//! # The subset, and where it departs from `sh`
//!
//! Quoting, escaping and word splitting are POSIX shell's, in full: `'…'` is
//! literal, `"…"` takes `\` before `"`, `\`, `` ` ``, `$` and a newline, `\`
//! escapes one character outside quotes, a trailing `\` is a line
//! continuation, and `#` opens a comment only at the start of a word — so
//! `uf test#library` is a word and not a truncated one.
//!
//! Everything that would make the shell *do* something rather than pass text
//! along sends the command to the shell instead: operators, expansions,
//! globs, tildes, newlines, reserved words, and a first word that is a
//! built-in with no program behind it. [`ShellSyntax`] is the list.
//!
//! Two departures are deliberate and visible.
//!
//! **uf runs programs, not built-ins.** `echo` in a direct command is the
//! `echo` on `PATH`, and a shell's built-in `echo` interprets backslash
//! escapes where `/bin/echo` does not — `echo "a\nb"` prints two lines under
//! macOS's `sh` and one under `/bin/echo`. No task in this repository has a
//! backslash in an `echo` argument, and `printf` means the same thing either
//! way.
//!
//! **The shell this implements is POSIX `sh`, not `bash`.** `{a,b}` is two
//! characters and a comma, because brace expansion is not in POSIX — which is
//! also why `uf run` already meant two different things here: `/bin/sh` is
//! `dash` on the Linux runner, which does not expand it, and `bash` on a Mac,
//! which does. One meaning is the point of reading the string at all.

use std::fmt;

/// What uf makes of a task's command string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// A program and its arguments. uf starts it itself, on every platform.
    Direct(Direct),
    /// Shell syntax uf does not implement, named.
    ///
    /// The *whole* original string is what a shell should be given: this is a
    /// classification and not a partial parse, so nothing here has consumed
    /// any of it.
    Shell(ShellSyntax),
    /// The string names no command: it is blank, or all of it is a comment.
    Nothing,
    /// Not a command in any shell, and why.
    Malformed(&'static str),
}

/// A command uf can start without a shell.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Direct {
    /// `NAME=value` written in front of the program, in order.
    ///
    /// These are the child's environment and nothing else — uf never exports
    /// them into its own process, so two tasks running at once cannot see each
    /// other's.
    pub assignments: Vec<(String, String)>,
    /// The program to start.
    pub program: String,
    /// Its arguments, one `argv` entry each, in order.
    pub args: Vec<String>,
}

/// The construct that makes a command a shell's to run.
///
/// Every variant is something a shell would *do* — expand, redirect, branch,
/// or reach for a built-in — rather than something it would pass along. A
/// command uf declines for one of these reasons is not a command uf is
/// refusing; it is one uf hands over, and on a machine with no POSIX shell it
/// is the reason the hand-over failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellSyntax {
    /// `|`, `&&`, `;`, `>`, `<`, `(` and the rest, as written.
    Operator(&'static str),
    /// `$` or `` ` ``: a parameter, a command substitution, an arithmetic
    /// expansion.
    Expansion(char),
    /// `*`, `?` or `[`: a pattern the shell matches against the filesystem.
    Glob(char),
    /// A leading `~`, which is a home directory and not a character.
    Tilde,
    /// A newline, which separates two commands.
    Newline,
    /// A reserved word in the command position: `if`, `for`, `while`, …
    Keyword(&'static str),
    /// A built-in in the command position with no program behind it: `cd`,
    /// `exit`, `export`, …
    Builtin(&'static str),
    /// An inline `PATH=`, which decides where the program itself is found.
    ///
    /// Its own variant because uf must not get this one subtly right: the
    /// program lookup a direct spawn does is not guaranteed to be the one the
    /// assignment asked for, and a command that starts a different program
    /// than the shell would is worse than a command uf hands to the shell.
    PathAssignment,
    /// Assignments and nothing else, which sets variables in a shell that then
    /// exits.
    NoProgram,
}

impl fmt::Display for ShellSyntax {
    /// The construct as a phrase, for the sentence "… uses {this}".
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operator(text) => write!(formatter, "the shell operator `{text}`"),
            Self::Expansion(character) => write!(formatter, "an expansion (`{character}`)"),
            Self::Glob(character) => write!(formatter, "a glob (`{character}`)"),
            Self::Tilde => formatter.write_str("a leading `~`"),
            Self::Newline => formatter.write_str("a newline between two commands"),
            Self::Keyword(word) => write!(formatter, "the shell keyword `{word}`"),
            Self::Builtin(word) => write!(formatter, "the shell built-in `{word}`"),
            Self::PathAssignment => formatter.write_str("an inline `PATH=`"),
            Self::NoProgram => formatter.write_str("assignments with no command after them"),
        }
    }
}

/// Reserved words. A command that begins with one is a compound command.
///
/// `time` is here for the same reason: it is a reserved word in every shell
/// that has it, `/usr/bin/time` is a different program with different output,
/// and picking the wrong one silently is exactly the failure this module
/// exists to avoid.
const KEYWORDS: [&str; 17] = [
    "!", "case", "do", "done", "elif", "else", "esac", "fi", "for", "if", "in", "then", "time",
    "until", "while", "{", "}",
];

/// Built-ins uf will not try to start as programs.
///
/// The test is not "is it a built-in" — `echo`, `printf`, `pwd`, `test`,
/// `true` and `false` are built-ins *and* programs, and the program does the
/// same thing. It is "would starting the program do what the shell does", and
/// for everything below the answer is no: either there is no such program
/// (`exit`, `export`, `source`), or the one that exists cannot have the effect
/// the command wants because it happens in a child process (`cd`, `set`,
/// `umask`, `ulimit`).
const BUILTINS: [&str; 30] = [
    ".", ":", "alias", "bg", "break", "cd", "command", "continue", "eval", "exec", "exit",
    "export", "fg", "getopts", "hash", "jobs", "local", "read", "readonly", "return", "set",
    "shift", "source", "times", "trap", "type", "ulimit", "umask", "unalias", "unset",
];

/// One word of a command, with enough of its provenance to read it correctly.
struct Word {
    /// The text, with quotes and escapes removed.
    text: String,
    /// How much of `text` was written without quotes or escapes, in bytes.
    ///
    /// `NAME=value` is an assignment only when the name and the `=` were
    /// written plainly — `'NAME'=value` is a command called `NAME=value` — and
    /// a leading `~` or `#` is only itself when it was quoted. One length
    /// answers all three questions.
    plain: usize,
}

impl Word {
    /// The word's leading plain run, which is where the shell looks for the
    /// characters that change what a word *is*.
    fn plain(&self) -> &str {
        &self.text[..self.plain]
    }
}

/// Read a task's command string.
///
/// Never fails in the sense of a `Result`: every string is one of the four
/// answers, and three of them are things the caller has to say out loud.
#[must_use]
pub fn parse(command: &str) -> Command {
    let mut words: Vec<Word> = Vec::new();
    let mut chars = command.chars().peekable();
    let mut current: Option<Word> = None;

    // `word` is the word being built; `push` closes it. A word exists as soon
    // as a quote is opened, so `''` is an empty argument and not nothing.
    macro_rules! word {
        () => {
            current.get_or_insert_with(|| Word {
                text: String::new(),
                plain: 0,
            })
        };
    }

    // Whether a newline has gone past with a command already behind it. A
    // leading blank line and a trailing one are nothing; a newline between two
    // commands is the shell's job, and this is what tells them apart.
    let mut ended_a_line = false;

    while let Some(character) = chars.next() {
        if ended_a_line && !matches!(character, ' ' | '\t' | '\n' | '#') {
            return Command::Shell(ShellSyntax::Newline);
        }
        match character {
            ' ' | '\t' => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
            }
            '\n' => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
                ended_a_line |= !words.is_empty();
            }
            '\'' => {
                let word = word!();
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.text.push(inner),
                        None => return Command::Malformed("a `'` is never closed"),
                    }
                }
            }
            '"' => {
                let word = word!();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            // The five characters `\` escapes inside double
                            // quotes, and a line continuation. Before anything
                            // else the backslash is an ordinary character —
                            // `"a\qb"` is `a\qb`.
                            Some(escaped @ ('"' | '\\' | '`' | '$')) => word.text.push(escaped),
                            Some('\n') => {}
                            Some(other) => {
                                word.text.push('\\');
                                word.text.push(other);
                            }
                            None => return Command::Malformed("a `\"` is never closed"),
                        },
                        Some(expansion @ ('$' | '`')) => {
                            return Command::Shell(ShellSyntax::Expansion(expansion));
                        }
                        Some(inner) => word.text.push(inner),
                        None => return Command::Malformed("a `\"` is never closed"),
                    }
                }
            }
            '\\' => match chars.next() {
                // A trailing `\` is a continuation onto a line that never
                // came, which `sh` drops rather than complains about.
                None => {
                    if let Some(word) = current.take() {
                        words.push(word);
                    }
                    break;
                }
                Some('\n') => {}
                Some(escaped) => word!().text.push(escaped),
            },
            // A comment runs to the end of its line rather than to the end of
            // the string: `# what this is\ncargo build` is one command with a
            // note above it, and reading it as nothing would refuse a task
            // `sh` ran.
            '#' if current.is_none() => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        ended_a_line |= !words.is_empty();
                        break;
                    }
                }
            }
            // A home directory at the start of a word, and — because `bash`
            // expands there too — after a plain `=`, which is what
            // `--prefix=~/opt` is.
            '~' if current
                .as_ref()
                .is_none_or(|word| word.plain == word.text.len() && word.text.ends_with('=')) =>
            {
                return Command::Shell(ShellSyntax::Tilde);
            }
            '$' | '`' => return Command::Shell(ShellSyntax::Expansion(character)),
            '*' | '?' | '[' => return Command::Shell(ShellSyntax::Glob(character)),
            '|' | '&' | ';' | '<' | '>' | '(' | ')' => {
                return Command::Shell(ShellSyntax::Operator(operator(character, chars.peek())));
            }
            plain => {
                let word = word!();
                word.text.push(plain);
                // Only a *leading* run counts: `a"b"c` is plain, quoted, plain,
                // and the `c` must not make the `=` in `a"b"c=d` an assignment.
                if word.plain == word.text.len() - plain.len_utf8() {
                    word.plain = word.text.len();
                }
            }
        }
    }
    if let Some(word) = current {
        words.push(word);
    }

    classify(&words)
}

/// The operator as written, so a message can quote what the reader typed.
///
/// Two characters where the shell reads two — `&&` is not an `&` — and the
/// rest as themselves. `2>&1` is reported as `>`, which is the character that
/// made it a redirection.
fn operator(first: char, next: Option<&char>) -> &'static str {
    match (first, next) {
        ('&', Some('&')) => "&&",
        ('|', Some('|')) => "||",
        ('>', Some('>')) => ">>",
        ('<', Some('<')) => "<<",
        (';', Some(';')) => ";;",
        ('|', _) => "|",
        ('&', _) => "&",
        (';', _) => ";",
        ('<', _) => "<",
        ('>', _) => ">",
        ('(', _) => "(",
        _ => ")",
    }
}

/// Turn words into a command, or into the reason there is no command here.
fn classify(words: &[Word]) -> Command {
    let mut assignments = Vec::new();
    let mut rest = words.iter();
    let program = loop {
        let Some(word) = rest.next() else {
            return if assignments.is_empty() {
                Command::Nothing
            } else {
                Command::Shell(ShellSyntax::NoProgram)
            };
        };
        match assignment(word) {
            Some(("PATH", _)) => return Command::Shell(ShellSyntax::PathAssignment),
            Some((name, value)) => assignments.push((name.to_owned(), value.to_owned())),
            None => break word,
        }
    };

    // Keywords and built-ins are only themselves in the command position, and
    // only when written plainly: `"exit" 3` is a program called `exit`, which
    // is what a shell would look for too.
    if program.plain() == program.text {
        if let Some(keyword) = KEYWORDS
            .into_iter()
            .find(|word| *word == program.text.as_str())
        {
            return Command::Shell(ShellSyntax::Keyword(keyword));
        }
        if let Some(builtin) = BUILTINS
            .into_iter()
            .find(|word| *word == program.text.as_str())
        {
            return Command::Shell(ShellSyntax::Builtin(builtin));
        }
    }

    Command::Direct(Direct {
        assignments,
        program: program.text.clone(),
        args: rest.map(|word| word.text.clone()).collect(),
    })
}

/// `NAME=value` split, when the word is one.
///
/// The name has to be a plain shell name and the `=` has to be unquoted; the
/// value may be anything, quoted or not, including empty.
fn assignment(word: &Word) -> Option<(&str, &str)> {
    let at = word.plain().find('=')?;
    let (name, _) = word.text.split_at(at);
    let mut characters = name.chars();
    let first = characters.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }
    if !characters.all(|character| character.is_ascii_alphanumeric() || character == '_') {
        return None;
    }
    Some((name, &word.text[at + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The direct command, or a panic naming what came back instead.
    fn direct(command: &str) -> Direct {
        match parse(command) {
            Command::Direct(direct) => direct,
            other => panic!("{command:?} is not a direct command: {other:?}"),
        }
    }

    /// `program` and `args` as one list, which is what most assertions want.
    fn words(command: &str) -> Vec<String> {
        let direct = direct(command);
        assert!(
            direct.assignments.is_empty(),
            "{command:?} has assignments: {:?}",
            direct.assignments
        );
        std::iter::once(direct.program).chain(direct.args).collect()
    }

    /// The syntax that sent `command` to a shell, or a panic.
    fn shell(command: &str) -> ShellSyntax {
        match parse(command) {
            Command::Shell(syntax) => syntax,
            other => panic!("{command:?} does not need a shell: {other:?}"),
        }
    }

    #[test]
    fn a_program_and_its_arguments() {
        assert_eq!(
            words("cargo build --release --bin uf"),
            ["cargo", "build", "--release", "--bin", "uf"]
        );
    }

    #[test]
    fn runs_of_whitespace_separate_words_and_do_not_make_empty_ones() {
        assert_eq!(words("  cargo \t test  "), ["cargo", "test"]);
    }

    /// `--` is an argument like any other. `cargo fmt --all -- --check` is one
    /// of this repository's tasks and it means nothing to the shell.
    #[test]
    fn a_double_dash_is_an_argument() {
        assert_eq!(
            words("cargo fmt --all -- --check"),
            ["cargo", "fmt", "--all", "--", "--check"]
        );
    }

    /// `#` opens a comment at the start of a word and nowhere else, so
    /// `uf test#library` — a real task here — is one word and not a truncated
    /// one. Getting this wrong is the first thing a naive splitter does.
    #[test]
    fn a_hash_inside_a_word_is_part_of_it() {
        assert_eq!(
            words("./target/release/uf test#library"),
            ["./target/release/uf", "test#library"]
        );
    }

    #[test]
    fn a_hash_that_starts_a_word_opens_a_comment() {
        assert_eq!(words("echo hi # and the rest is a note"), ["echo", "hi"]);
        assert_eq!(parse("# nothing but a comment"), Command::Nothing);
        assert_eq!(parse("  # nor this  "), Command::Nothing);
    }

    #[test]
    fn a_quoted_hash_is_a_character() {
        assert_eq!(words("git log '#42'"), ["git", "log", "#42"]);
        assert_eq!(words(r"git log \#42"), ["git", "log", "#42"]);
    }

    /// Single quotes are literal, including the `$` and the backticks that
    /// would otherwise be expansions — which is what makes this repository's
    /// `setup` task a direct command rather than a shell's.
    #[test]
    fn single_quotes_are_literal() {
        assert_eq!(
            words("echo 'ready: run `uf run ci` to check everything'"),
            ["echo", "ready: run `uf run ci` to check everything"]
        );
        assert_eq!(words("echo '$HOME *'"), ["echo", "$HOME *"]);
    }

    /// The `manifests` task: double quotes around a script full of single
    /// quotes, parentheses and a `*` that is a glob to `node` and not to the
    /// shell.
    #[test]
    fn double_quotes_hold_quotes_parentheses_and_globs() {
        let script = "for (const f of require('node:fs').globSync('packages/*/package.json')) f";
        assert_eq!(
            words(&format!("node -e \"{script}\"")),
            ["node", "-e", script]
        );
    }

    #[test]
    fn a_backslash_inside_double_quotes_escapes_only_five_characters() {
        assert_eq!(words(r#"echo "a\"b\\c\$d\`e""#), ["echo", r#"a"b\c$d`e"#]);
        // Anything else and the backslash is a character, as in `sh`.
        assert_eq!(words(r#"echo "a\qb""#), ["echo", r"a\qb"]);
    }

    #[test]
    fn a_backslash_outside_quotes_escapes_one_character() {
        assert_eq!(words(r"echo a\ b"), ["echo", "a b"]);
        assert_eq!(words(r"echo a\$b"), ["echo", "a$b"]);
    }

    /// A backslash before a newline joins the two lines, which is how a long
    /// command is written down without a shell being involved.
    #[test]
    fn a_backslash_before_a_newline_joins_the_lines() {
        assert_eq!(
            words("cargo clippy --workspace \\\n  -- -D warnings"),
            ["cargo", "clippy", "--workspace", "--", "-D", "warnings"]
        );
    }

    /// `sh` reads a trailing backslash as a continuation onto a line that
    /// never arrives, and drops it rather than failing.
    #[test]
    fn a_trailing_backslash_is_dropped() {
        assert_eq!(words("echo a\\"), ["echo", "a"]);
    }

    #[test]
    fn empty_quotes_are_an_empty_argument() {
        assert_eq!(words("echo ''"), ["echo", ""]);
    }

    /// `UF_BIN=./target/release/uf tools/docs/build.sh`, which is `docs:build`.
    #[test]
    fn leading_assignments_belong_to_the_child() {
        let direct = direct("UF_PROJECT_ROOT=. UF_BINARY=./uf node bench.js");
        assert_eq!(
            direct.assignments,
            [
                ("UF_PROJECT_ROOT".to_owned(), ".".to_owned()),
                ("UF_BINARY".to_owned(), "./uf".to_owned()),
            ]
        );
        assert_eq!(direct.program, "node");
        assert_eq!(direct.args, ["bench.js"]);
    }

    #[test]
    fn an_assignment_after_the_program_is_an_argument() {
        assert_eq!(words("make CC=clang all"), ["make", "CC=clang", "all"]);
    }

    /// A name has to be written plainly to be a name, which is why
    /// `'NAME'=value` is a program with an unusual name in every shell.
    #[test]
    fn a_quoted_name_is_not_an_assignment() {
        assert_eq!(words("'A'=1"), ["A=1"]);
        assert_eq!(words("1A=x prog"), ["1A=x", "prog"]);
    }

    #[test]
    fn an_assignment_value_may_be_quoted_or_empty() {
        let direct = direct("MSG='two words' EMPTY= prog");
        assert_eq!(
            direct.assignments,
            [
                ("MSG".to_owned(), "two words".to_owned()),
                ("EMPTY".to_owned(), String::new()),
            ]
        );
        assert_eq!(direct.program, "prog");
    }

    /// An inline `PATH=` decides where the program itself is found, and a
    /// direct spawn's lookup is not guaranteed to be the one it asked for.
    #[test]
    fn an_inline_path_assignment_goes_to_the_shell() {
        assert_eq!(
            shell("PATH=/opt/bin:$PATH prog"),
            ShellSyntax::Expansion('$')
        );
        assert_eq!(shell("PATH=/opt/bin prog"), ShellSyntax::PathAssignment);
    }

    #[test]
    fn assignments_with_no_command_go_to_the_shell() {
        assert_eq!(shell("A=1 B=2"), ShellSyntax::NoProgram);
    }

    #[test]
    fn every_operator_is_named_as_written() {
        for (command, operator) in [
            ("a | b", "|"),
            ("a || b", "||"),
            ("a && b", "&&"),
            ("a & b", "&"),
            ("a; b", ";"),
            ("a > out", ">"),
            ("a >> out", ">>"),
            ("a < in", "<"),
            ("a 2>&1", ">"),
            ("(a)", "("),
        ] {
            assert_eq!(
                shell(command),
                ShellSyntax::Operator(operator),
                "for {command:?}"
            );
        }
    }

    #[test]
    fn expansions_go_to_the_shell_inside_double_quotes_and_out() {
        assert_eq!(shell("echo $HOME"), ShellSyntax::Expansion('$'));
        assert_eq!(shell(r#"echo "$HOME""#), ShellSyntax::Expansion('$'));
        assert_eq!(shell("echo `date`"), ShellSyntax::Expansion('`'));
        assert_eq!(shell(r#"echo "`date`""#), ShellSyntax::Expansion('`'));
    }

    #[test]
    fn globs_go_to_the_shell() {
        assert_eq!(shell("rm src/*.js"), ShellSyntax::Glob('*'));
        assert_eq!(shell("rm a?.js"), ShellSyntax::Glob('?'));
        assert_eq!(shell("rm a[0-9].js"), ShellSyntax::Glob('['));
    }

    /// A tilde is a home directory at the start of a word and after a plain
    /// `=`, which is where `bash` expands it.
    #[test]
    fn a_tilde_goes_to_the_shell() {
        assert_eq!(shell("ls ~/src"), ShellSyntax::Tilde);
        assert_eq!(shell("prog --prefix=~/opt"), ShellSyntax::Tilde);
        // Quoted, or in the middle of a word, it is a character.
        assert_eq!(words("ls '~/src'"), ["ls", "~/src"]);
        assert_eq!(words("ls main.c~"), ["ls", "main.c~"]);
    }

    /// A newline *between two commands* is the shell's. One with nothing on
    /// the other side of it is not a second command.
    #[test]
    fn a_newline_goes_to_the_shell_only_when_it_separates_two_commands() {
        assert_eq!(shell("a\nb"), ShellSyntax::Newline);
        assert_eq!(shell("cargo build\n\ncargo test"), ShellSyntax::Newline);

        assert_eq!(words("cargo build\n"), ["cargo", "build"]);
        assert_eq!(words("\ncargo build"), ["cargo", "build"]);
        assert_eq!(
            words("cargo build\n# a note under it\n"),
            ["cargo", "build"]
        );
        // A comment runs to the end of *its line*, so the command below it is
        // still the command.
        assert_eq!(words("# what this is\ncargo build"), ["cargo", "build"]);
    }

    #[test]
    fn keywords_and_builtins_go_to_the_shell_in_the_command_position() {
        assert_eq!(shell("exit 3"), ShellSyntax::Builtin("exit"));
        assert_eq!(shell("cd packages/core"), ShellSyntax::Builtin("cd"));
        assert_eq!(shell(". ./script.sh"), ShellSyntax::Builtin("."));
        assert_eq!(shell("if true"), ShellSyntax::Keyword("if"));
        assert_eq!(shell("time cargo build"), ShellSyntax::Keyword("time"));
    }

    /// Only in the command position, and only written plainly — the same two
    /// rules a shell applies.
    #[test]
    fn a_builtin_elsewhere_or_in_quotes_is_a_word() {
        assert_eq!(
            words("git commit --no-edit exit"),
            ["git", "commit", "--no-edit", "exit"]
        );
        assert_eq!(words("'exit' 3"), ["exit", "3"]);
    }

    /// `echo`, `printf`, `test` and `true` are built-ins *and* programs, and
    /// the program is the one uf starts. See this module's header for the one
    /// place the two disagree.
    #[test]
    fn a_builtin_with_a_program_behind_it_stays_direct() {
        assert_eq!(words("echo hello"), ["echo", "hello"]);
        assert_eq!(words("printf '%s\\n' hi"), ["printf", "%s\\n", "hi"]);
        assert_eq!(words("true"), ["true"]);
    }

    /// The shell this implements is POSIX `sh`. A brace is a brace, which is
    /// what `dash` makes of it and what `bash` does not.
    #[test]
    fn a_brace_is_a_character() {
        assert_eq!(words("echo {a,b}"), ["echo", "{a,b}"]);
    }

    #[test]
    fn an_unclosed_quote_is_malformed() {
        assert_eq!(
            parse("echo 'hi"),
            Command::Malformed("a `'` is never closed")
        );
        assert_eq!(
            parse("echo \"hi"),
            Command::Malformed("a `\"` is never closed")
        );
    }

    #[test]
    fn a_blank_command_names_nothing() {
        assert_eq!(parse(""), Command::Nothing);
        assert_eq!(parse("   \t "), Command::Nothing);
    }

    #[test]
    fn each_reason_says_what_it_is() {
        assert_eq!(
            ShellSyntax::Operator("&&").to_string(),
            "the shell operator `&&`"
        );
        assert_eq!(
            ShellSyntax::Builtin("exit").to_string(),
            "the shell built-in `exit`"
        );
        assert_eq!(ShellSyntax::Glob('*').to_string(), "a glob (`*`)");
    }

    /// The corpus this subset was drawn from, asserted rather than described.
    ///
    /// Every task in uf's own `uf.config.js` — `uf run ci` and its dependencies
    /// among them — has to be a command uf starts itself, or the subset is not
    /// wide enough to be worth having. If a task is added that needs a shell
    /// this fails, and the answer is either to rewrite the task or to accept
    /// that this repository is not a project `uf run` works on under Windows.
    #[test]
    fn every_task_in_this_repository_runs_without_a_shell() {
        let root = camino::Utf8PathBuf::from_path_buf(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        )
        .expect("a utf-8 repository path");
        let resolved = uf_config::load_config(&root).expect("uf's own uf.config.js");
        assert!(
            resolved.config.tasks.len() > 50,
            "the config did not load: {} tasks",
            resolved.config.tasks.len()
        );
        let mut shells = Vec::new();
        for (name, task) in &resolved.config.tasks {
            match parse(task.command()) {
                Command::Direct(_) => {}
                other => shells.push(format!("{name}: {other:?}")),
            }
        }
        assert!(shells.is_empty(), "tasks that need a shell: {shells:#?}");
    }
}
