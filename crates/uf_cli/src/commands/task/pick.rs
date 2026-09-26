//! A task's arguments: sorted from the command line, and asked for when
//! something required is missing and somebody is there to answer.
//!
//! Sorting and checking are `uf_task::arguments`'. What is here is the part
//! that is `uf run`'s alone — whether to ask, how, and what to say when it
//! cannot — and it is written against [`Ask`] rather than the terminal, so
//! the tests below drive it with a list of answers instead of a keyboard.
//!
//! # When uf asks
//!
//! Only for an argument that is required, was not given, and has no default:
//! the ones there is no other answer for. An argument with a default is never
//! asked about, so `uf run deploy` with every argument defaulted runs as it is
//! typed, at a terminal or not. And only at a terminal: in a pipeline, in CI
//! or with stdin redirected, the same missing argument is an error that names
//! it and its choices, because a prompt nobody can see is a hang.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use uf_config::TaskArgument;
use uf_task::arguments::{ArgumentError, Given, Resolved, check};
use uf_term::prompt::{self, Choice, Outcome, Question, Request};

/// What asking came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// The value to use.
    Value(String),
    /// The reader left without answering.
    Cancelled,
    /// Nobody was there to ask.
    NotInteractive,
}

/// Somebody who can be asked for an argument's value.
pub(crate) trait Ask {
    /// Ask for `argument` of the task called `task`.
    fn ask(&mut self, task: &str, argument: &TaskArgument) -> Asked;
}

/// The person at the terminal, when there is one.
pub(crate) struct Terminal;

impl Ask for Terminal {
    fn ask(&mut self, task: &str, argument: &TaskArgument) -> Asked {
        let title = title(task, argument);
        if argument.choices.is_empty() {
            return match prompt::input(&Question::new(&title, "type a value")) {
                prompt::Answer::Typed(value) => Asked::Value(value),
                prompt::Answer::Cancelled => Asked::Cancelled,
                prompt::Answer::NotInteractive => Asked::NotInteractive,
            };
        }
        let choices: Vec<Choice<'_>> = argument
            .choices
            .iter()
            .map(|choice| Choice::new(choice, ""))
            .collect();
        match prompt::select(&Request::new(&title, &choices)) {
            Outcome::Chose(choice) => Asked::Value(choice.name.to_owned()),
            Outcome::Cancelled => Asked::Cancelled,
            Outcome::NotInteractive => Asked::NotInteractive,
        }
    }
}

/// The question over the picker: whose argument, which one, and what it is
/// for.
fn title(task: &str, argument: &TaskArgument) -> String {
    match &argument.description {
        Some(description) => {
            uf_infra::cstr!("{task} · {} — {description}", argument.name).into_string()
        }
        None => uf_infra::cstr!("{task} · {}", argument.name).into_string(),
    }
}

/// `words`, sorted into the arguments `declared` names, with anything missing
/// asked for.
///
/// `answers` carries what was asked across calls, so a run that reaches the
/// same task in several workspace members asks once per argument name rather
/// than once per member. The `bool` is whether anybody was asked.
///
/// # Errors
///
/// A declaration `uf run` cannot fill, a word that does not fit, a question
/// the reader walked away from, or — with nobody to ask — every required
/// argument still missing, with its choices and how to pass it.
pub(crate) fn resolve(
    task: &str,
    declared: &[TaskArgument],
    words: &[String],
    ask: &mut dyn Ask,
    answers: &mut BTreeMap<String, String>,
) -> Result<(Resolved, bool)> {
    if let Err(error) = check(declared) {
        bail!(uf_infra::cstr!(
            "task {task:?} declares an argument `uf run` cannot fill: {error}"
        ));
    }
    let mut given =
        Given::parse(declared, words).map_err(|error| explain(task, declared, &error))?;

    let missing: Vec<usize> = given.missing().map(|(at, _)| at).collect();
    let mut asked = false;
    for at in missing {
        let argument = &declared[at];
        let value = match answers.get(argument.name.as_str()) {
            Some(value) => value.clone(),
            None => match ask.ask(task, argument) {
                Asked::Value(value) => value,
                Asked::Cancelled => bail!(uf_infra::cstr!(
                    "task {task:?} was not run: <{}> was not given",
                    argument.name
                )),
                // Nobody to ask about this one means nobody to ask about the
                // rest, and `resolve` names all of them at once.
                Asked::NotInteractive => break,
            },
        };
        asked = true;
        given
            .fill(at, value.clone())
            .map_err(|error| explain(task, declared, &error))?;
        answers.insert(argument.name.to_string(), value);
    }

    let resolved = given
        .resolve()
        .map_err(|error| explain(task, declared, &error))?;
    Ok((resolved, asked))
}

/// The words `uf prepare` appends — staged files — which fill no declared
/// argument: each argument takes its default, and one that has none is an
/// error, because a commit hook is no place to ask.
pub(crate) fn undeclared(
    task: &str,
    declared: &[TaskArgument],
    words: &[String],
) -> Result<Resolved> {
    if let Err(error) = check(declared) {
        bail!(uf_infra::cstr!(
            "task {task:?} declares an argument `uf run` cannot fill: {error}"
        ));
    }
    let mut resolved = Given::parse(declared, &[])
        .and_then(Given::resolve)
        .map_err(|error| explain(task, declared, &error))?;
    resolved.extra = words.to_vec();
    Ok(resolved)
}

/// An argument error as `uf run` reports it: whose, and how to pass one.
fn explain(task: &str, declared: &[TaskArgument], error: &ArgumentError) -> anyhow::Error {
    let mut message = uf_infra::cstr!("task {task:?}: {error}").into_string();
    if let ArgumentError::Missing(missing) = error
        && let Some(first) = missing.first()
    {
        uf_infra::append!(
            message,
            "\n\n  pass them after the task's name, in this order — `uf run {task} {}` — \
             or by name, as in `uf run {task} --{} <value>`. At a terminal, uf asks for them.",
            signature(declared),
            first.name,
        );
    }
    anyhow::anyhow!(message)
}

/// The long options `uf run` reads itself, wherever they are written before a
/// `--` — so a task argument with one of these names is reached past one.
const RUN_OPTIONS: &[&str] = &[
    "mode",
    "concurrency",
    "force",
    "why",
    "recursive",
    "filter",
    "list",
    "cwd",
    "color",
    "help",
];

/// How a task's arguments read in a list: `<target> [region=eu] [note]`.
///
/// Angle brackets for what has to be given, square ones for what may be left
/// out, and a default written after its name — the usage-line convention a
/// reader has already seen in every `--help`.
pub(crate) fn signature(declared: &[TaskArgument]) -> String {
    declared
        .iter()
        .map(
            |argument| match (&argument.default, argument.is_required()) {
                (Some(default), _) => {
                    uf_infra::cstr!("[{}={default}]", argument.name).into_string()
                }
                (None, true) => uf_infra::cstr!("<{}>", argument.name).into_string(),
                (None, false) => uf_infra::cstr!("[{}]", argument.name).into_string(),
            },
        )
        .collect::<Vec<_>>()
        .join(" ")
}

/// The command line that runs the same thing without asking, for saying after
/// a run that asked: every declared value by name, then what was not declared.
///
/// Past a `--` when anything in it would otherwise be read as one of `uf run`'s
/// own options: a declared `--mode` is the task's only after one.
pub(crate) fn replay(task: &str, resolved: &Resolved) -> String {
    let mut line = uf_infra::cstr!("uf run {task}").into_string();
    let shadowed = resolved
        .values
        .iter()
        .any(|(name, _)| RUN_OPTIONS.contains(&name.as_str()))
        || resolved.extra.iter().any(|word| word.starts_with('-'));
    if shadowed {
        line.push_str(" --");
    }
    for (name, value) in &resolved.values {
        uf_infra::append!(line, " --{name} {}", uf_task::arguments::quote(value));
    }
    for word in &resolved.extra {
        line.push(' ');
        line.push_str(word);
    }
    line
}

#[cfg(test)]
mod tests {
    use compact_str::CompactString;

    use super::*;

    /// Answers from a list, in order, recording what was asked.
    struct Scripted {
        answers: Vec<Asked>,
        asked: Vec<String>,
    }

    impl Scripted {
        fn new(answers: Vec<Asked>) -> Self {
            Self {
                answers,
                asked: Vec::new(),
            }
        }
    }

    impl Ask for Scripted {
        fn ask(&mut self, task: &str, argument: &TaskArgument) -> Asked {
            self.asked.push(title(task, argument));
            if self.answers.is_empty() {
                Asked::NotInteractive
            } else {
                self.answers.remove(0)
            }
        }
    }

    fn declared() -> Vec<TaskArgument> {
        let mut target = TaskArgument::named("target");
        target.description = Some("Where to deploy".into());
        target.choices = vec![
            CompactString::new("staging"),
            CompactString::new("production"),
        ];
        let mut region = TaskArgument::named("region");
        region.default = Some("eu".into());
        vec![target, TaskArgument::named("tag"), region]
    }

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn nothing_is_asked_when_the_command_line_has_it_all() {
        let mut ask = Scripted::new(vec![]);
        let (resolved, asked) = resolve(
            "deploy",
            &declared(),
            &words(&["staging", "--tag", "v1"]),
            &mut ask,
            &mut BTreeMap::new(),
        )
        .unwrap();
        assert!(!asked);
        assert!(ask.asked.is_empty());
        assert_eq!(resolved.command_text(), "staging v1 eu");
    }

    #[test]
    fn only_what_is_missing_and_has_no_default_is_asked_for_in_order() {
        let mut ask = Scripted::new(vec![
            Asked::Value("production".into()),
            Asked::Value("v 2".into()),
        ]);
        let (resolved, asked) =
            resolve("deploy", &declared(), &[], &mut ask, &mut BTreeMap::new()).unwrap();
        assert!(asked);
        assert_eq!(
            ask.asked,
            ["deploy · target — Where to deploy", "deploy · tag"]
        );
        assert_eq!(resolved.command_text(), "production 'v 2' eu");
        assert_eq!(
            replay("deploy", &resolved),
            "uf run deploy --target production --tag 'v 2' --region eu"
        );
    }

    #[test]
    fn with_nobody_to_ask_every_missing_argument_is_named_with_its_choices() {
        let mut ask = Scripted::new(vec![Asked::NotInteractive]);
        let error = resolve("deploy", &declared(), &[], &mut ask, &mut BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("task \"deploy\": missing <target>, <tag>"),
            "{error}"
        );
        assert!(error.contains("one of: staging, production"), "{error}");
        assert!(
            error.contains("`uf run deploy <target> <tag> [region=eu]`"),
            "{error}"
        );
        assert!(
            error.contains("`uf run deploy --target <value>`"),
            "{error}"
        );
        // Asked once, then given up on rather than asked again per argument.
        assert_eq!(ask.asked.len(), 1);
    }

    #[test]
    fn a_cancelled_question_runs_nothing() {
        let mut ask = Scripted::new(vec![Asked::Cancelled]);
        let error = resolve("deploy", &declared(), &[], &mut ask, &mut BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert_eq!(error, "task \"deploy\" was not run: <target> was not given");
    }

    #[test]
    fn an_answer_is_reused_for_the_same_name_in_the_same_run() {
        let mut answers = BTreeMap::new();
        let mut ask = Scripted::new(vec![
            Asked::Value("staging".into()),
            Asked::Value("v1".into()),
        ]);
        resolve("ui#deploy", &declared(), &[], &mut ask, &mut answers).unwrap();
        let (resolved, _) =
            resolve("app#deploy", &declared(), &[], &mut ask, &mut answers).unwrap();
        assert_eq!(ask.asked.len(), 2, "{:?}", ask.asked);
        assert_eq!(resolved.command_text(), "staging v1 eu");
    }

    #[test]
    fn a_value_outside_the_choices_is_refused_before_anything_is_asked() {
        let mut ask = Scripted::new(vec![]);
        let error = resolve(
            "deploy",
            &declared(),
            &words(&["--target", "prod"]),
            &mut ask,
            &mut BTreeMap::new(),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            "task \"deploy\": <target> cannot be \"prod\"; it is one of: staging, production"
        );
        assert!(ask.asked.is_empty());
    }

    #[test]
    fn staged_files_fill_no_argument() {
        let mut region = TaskArgument::named("region");
        region.default = Some("eu".into());
        let resolved = undeclared("fmt", &[region], &words(&["a.js"])).unwrap();
        assert_eq!(resolved.command_text(), "eu a.js");
        assert!(undeclared("fmt", &[TaskArgument::named("x")], &[]).is_err());
    }

    #[test]
    fn a_signature_marks_what_is_required_and_what_defaults() {
        let mut declared = declared();
        let mut note = TaskArgument::named("note");
        note.required = Some(false);
        declared.push(note);
        assert_eq!(signature(&declared), "<target> <tag> [region=eu] [note]");
    }
}
