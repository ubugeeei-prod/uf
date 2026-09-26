//! `uf sqlc`, and `uf` as sqlc's Flow plugin.
//!
//! sqlc owns the SQL: its parser, its catalog, its config file. uf owns the
//! Flow it becomes. The two meet at sqlc's process-plugin protocol, so a
//! project's `sqlc.yaml` names `uf` as the plugin —
//!
//! ```yaml
//! plugins:
//!   - name: flow
//!     process:
//!       cmd: uf
//! ```
//!
//! — and sqlc runs `uf /plugin.CodegenService/Generate` with a
//! `GenerateRequest` on stdin. [`plugin`] answers that; `crates/uf_sqlc` does
//! the work, and this file adds `uf fmt`'s printer on the way out.
//!
//! `uf sqlc generate` and `uf sqlc diff` are `sqlc generate` and `sqlc diff`
//! with one difference: the directory of the running `uf` is put first on
//! `PATH`, so the `uf` sqlc finds is this one rather than whichever an older
//! install left earlier on the path. sqlc itself is the project's, found as
//! `$SQLC` or `sqlc` on `PATH`; uf does not download it. `docs/sqlc.md` is the
//! design record.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_term::Status;

use crate::cli::SqlcCommand;
use crate::ui::Ui;

/// Answer one sqlc plugin call: a request on stdin, a response on stdout.
///
/// Failures go to stderr, which sqlc prints under the query file's name, and
/// exit non-zero, which is how sqlc knows to stop.
pub(crate) fn plugin() -> ExitCode {
    let mut input = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut input) {
        eprintln!("uf: could not read sqlc's request: {error}");
        return ExitCode::FAILURE;
    }
    match uf_sqlc::run_plugin(&input, &format) {
        Ok(output) => match std::io::stdout().write_all(&output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("uf: could not write the response to sqlc: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("uf: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `uf fmt`'s printer with its defaults.
///
/// Not the project's `fmt` settings: the plugin runs wherever sqlc runs, with
/// the environment sqlc clears, and the output must be the same for everyone
/// who runs the same sqlc over the same SQL. The files are signed, and
/// `uf fmt` leaves a signed file alone, so a project that formats differently
/// never sees them change.
fn format(source: &str) -> std::result::Result<String, String> {
    uf_fmt::format_source(source, &uf_config::FmtConfig::default())
        .map(|result| result.output)
        .map_err(|error| error.to_string())
}

pub(crate) fn sqlc(cwd: &Utf8Path, ui: &mut Ui, command: SqlcCommand) -> Result<()> {
    let (verb, file) = match command {
        SqlcCommand::Generate { file } => ("generate", file),
        SqlcCommand::Diff { file } => ("diff", file),
    };
    let sqlc = std::env::var_os("SQLC").unwrap_or_else(|| OsString::from("sqlc"));
    let mut run = Command::new(&sqlc);
    run.arg(verb)
        .current_dir(cwd)
        .env("PATH", path_with_this_uf()?);
    if let Some(file) = &file {
        run.arg("-f").arg(absolute(cwd, file));
    }
    let status = run.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(uf_infra::cstr!(
                "sqlc is not installed, or not on PATH. Install it \
                 (https://docs.sqlc.dev/en/latest/overview/install.html) or point `SQLC` at it"
            ))
        } else {
            anyhow::Error::new(error).context(uf_infra::into_string(uf_infra::cstr!(
                "could not run {}",
                sqlc.to_string_lossy()
            )))
        }
    })?;
    if !status.success() {
        if verb == "diff" {
            bail!(uf_infra::cstr!(
                "the generated files are out of date; run `uf sqlc generate`"
            ));
        }
        bail!(uf_infra::cstr!("sqlc {verb} failed"));
    }
    let message = if verb == "generate" {
        "sqlc generated the Flow modules"
    } else {
        "the generated Flow modules are up to date"
    };
    ui.render(|renderer, out| renderer.status(out, Status::Success, message));
    Ok(())
}

fn absolute(cwd: &Utf8Path, file: &Utf8Path) -> Utf8PathBuf {
    if file.is_absolute() {
        file.to_owned()
    } else {
        cwd.join(file)
    }
}

/// `PATH`, with the directory of the running `uf` first.
fn path_with_this_uf() -> Result<OsString> {
    let exe = std::env::current_exe().context("could not find the running uf")?;
    let dir = exe
        .parent()
        .context("the running uf has no directory")?
        .to_path_buf();
    let rest = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(dir).chain(std::env::split_paths(&rest));
    std::env::join_paths(paths).context("could not build PATH for sqlc")
}
