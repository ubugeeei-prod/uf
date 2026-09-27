//! Complete missing command information from clap's metadata.
use anyhow::Result;
use clap::{
    Command,
    error::{ContextKind, ContextValue, ErrorKind},
};
use std::ffi::OsString;
use uf_term::prompt::{self, Answer, Choice, Outcome, Question, Request};

#[derive(Debug)]
pub(crate) struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

pub(crate) fn complete(
    args: &mut Vec<OsString>,
    root: &Command,
    error: &clap::Error,
) -> Result<bool> {
    if !prompt::is_interactive() {
        return Ok(false);
    }
    let path = crate::help::Request::from_args(args, root).path;
    let mut command = root;
    for name in &path {
        let Some(sub) = command.find_subcommand(name) else {
            return Ok(false);
        };
        command = sub;
    }
    if matches!(
        error.kind(),
        ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        let choices: Vec<_> = command
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(|sub| {
                (
                    sub.get_name().to_owned(),
                    sub.get_about().map(ToString::to_string).unwrap_or_default(),
                )
            })
            .collect();
        if let Some(value) = choose("Choose a command", &choices)? {
            args.push(value.into());
            return Ok(true);
        }
        return Ok(false);
    }
    let written = match (error.kind(), error.get(ContextKind::InvalidArg)) {
        (ErrorKind::MissingRequiredArgument, Some(ContextValue::Strings(values))) => {
            values.first().map(String::as_str)
        }
        (ErrorKind::InvalidValue, Some(ContextValue::String(value))) if matches!(error.get(ContextKind::InvalidValue), Some(ContextValue::String(value)) if value.is_empty()) => {
            Some(value.as_str())
        }
        _ => None,
    };
    let Some(written) = written else {
        return Ok(false);
    };
    let Some(arg) = command.get_arguments().find(|arg| {
        arg.to_string() == written
            || arg
                .get_long()
                .is_some_and(|long| written.starts_with(uf_infra::cstr!("--{long}").as_str()))
    }) else {
        return Ok(false);
    };
    let cwd = cwd(args)?;
    let mut values: Vec<(String, String)> = arg
        .get_value_parser()
        .possible_values()
        .into_iter()
        .flatten()
        .filter(|value| !value.is_hide_set())
        .map(|value| {
            (
                value.get_name().to_owned(),
                value
                    .get_help()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            )
        })
        .collect();
    if path.iter().map(String::as_str).eq(["env", "use"]) {
        values.extend(
            ["node", "bun", "deno", "cfw", "uf"]
                .map(|name| (name.to_owned(), "Select runtime".to_owned())),
        );
        if let Ok(files) = std::fs::read_dir(uf_config::discover_root(&cwd)) {
            for file in files.flatten() {
                let name = file.file_name();
                if let Some(mode) = name.to_str().and_then(|name| name.strip_prefix(".env."))
                    && mode != "local"
                    && !mode.ends_with(".local")
                {
                    values.push((mode.to_owned(), "Select dotenv profile".to_owned()));
                }
            }
        }
    } else if matches!(path.last().map(String::as_str), Some("remove" | "why"))
        && let Ok(bytes) = std::fs::read(cwd.join("package.json"))
        && let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        for field in uf_pm::DEPENDENCY_FIELDS {
            if let Some(deps) = manifest.get(field).and_then(serde_json::Value::as_object) {
                values.extend(deps.keys().map(|name| (name.clone(), field.to_owned())));
            }
        }
    }
    values.sort();
    values.dedup_by(|a, b| a.0 == b.0);
    let title = uf_infra::into_string(uf_infra::cstr!("Choose {}", arg.get_id()));
    let value = if values.is_empty() {
        enter(&title, written)?
    } else {
        values.push((
            "Enter a value…".to_owned(),
            "Specify another value".to_owned(),
        ));
        match choose(&title, &values)? {
            Some(value) if value == "Enter a value…" => enter(&title, written)?,
            other => other,
        }
    };
    let Some(value) = value else { return Ok(false) };
    // A required option has not been supplied; an empty supplied option needs only its value.
    if error.kind() == ErrorKind::MissingRequiredArgument
        && !arg.is_positional()
        && let Some(long) = arg.get_long()
    {
        args.push(uf_infra::into_string(uf_infra::cstr!("--{long}")).into());
    }
    args.push(value.into());
    Ok(true)
}

pub(crate) fn cwd(args: &[OsString]) -> Result<camino::Utf8PathBuf> {
    let written = args
        .windows(2)
        .find(|pair| pair[0] == "--cwd")
        .map(|pair| pair[1].clone())
        .or_else(|| {
            args.iter()
                .filter_map(|arg| arg.to_str())
                .find_map(|arg| arg.strip_prefix("--cwd=").map(OsString::from))
        });
    crate::resolve_cwd(written.map(camino::Utf8PathBuf::try_from).transpose()?)
}

pub(crate) fn choose(title: &str, values: &[(String, String)]) -> Result<Option<String>> {
    if values.is_empty() {
        return Ok(None);
    }
    let choices: Vec<_> = values
        .iter()
        .map(|(name, about)| Choice::new(name, about))
        .collect();
    match prompt::select(&Request::new(title, &choices)) {
        Outcome::Chose(choice) => Ok(Some(choice.name.to_owned())),
        Outcome::Cancelled => Err(Cancelled.into()),
        Outcome::NotInteractive => Ok(None),
    }
}
fn enter(title: &str, placeholder: &str) -> Result<Option<String>> {
    match prompt::input(&Question::new(title, placeholder)) {
        Answer::Typed(value) => Ok(Some(value)),
        Answer::Cancelled => Err(Cancelled.into()),
        Answer::NotInteractive => Ok(None),
    }
}
