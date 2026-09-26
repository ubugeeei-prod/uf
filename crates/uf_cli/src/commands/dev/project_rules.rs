//! Project rules in the editor: one host for the session, and a project
//! rule's fix as a code action.
//!
//! `uf lint` holds a [`Session`] for one pass; the language server holds one
//! for as long as the editor keeps it running. It starts with the first
//! document that needs linting rather than at `initialize`: a server started
//! for a workspace with no project rule should never pay for a process, and
//! one with them should not make the editor wait on a host before it has
//! answered its first request.

use std::io::Write;

use anyhow::Result;
use camino::Utf8PathBuf;
use serde_json::{Value, json};
use uf_config::UniflowedConfig;
use uf_lint::SourceFile;

use super::notify;
use crate::commands::lint::plugins::{Answer, ProjectFix, Session};

/// The project rules of one editor session.
pub(super) struct EditorRules {
    root: Utf8PathBuf,
    session: Option<Session>,
    /// Starting has been tried. Tried once, because a project that cannot run
    /// its rules — no host, no `@uniflowed/host` — cannot run them on the next
    /// keystroke either, and saying so on every one would bury the log.
    tried: bool,
    /// Why starting failed, until the editor has been told.
    refused: Option<String>,
    /// How many of the session's problems the editor has been told about.
    told: usize,
}

impl EditorRules {
    pub(super) fn new(root: Utf8PathBuf) -> Self {
        Self {
            root,
            session: None,
            tried: false,
            refused: None,
            told: 0,
        }
    }

    /// What the project's rules say about one document's text.
    pub(super) fn lint(&mut self, file: &SourceFile, config: &UniflowedConfig) -> Answer {
        if self.session.is_none() && !self.tried {
            self.tried = true;
            match Session::start(&self.root, config) {
                Ok(session) => self.session = session,
                Err(error) => {
                    self.refused = Some(uf_infra::into_string(uf_infra::cstr!("{error:#}")))
                }
            }
        }
        self.session
            .as_mut()
            .map_or_else(Answer::default, |session| session.lint(file))
    }

    /// Write every problem the editor has not been told about to its log.
    ///
    /// The log rather than a diagnostic: a plugin that did not import is a
    /// problem with the project and not with the line under the cursor, and a
    /// message box on every keystroke would be worse than either.
    pub(super) fn tell(&mut self, out: &mut impl Write) -> Result<()> {
        if let Some(refused) = self.refused.take() {
            log(out, &refused)?;
        }
        let Some(session) = self.session.as_ref() else {
            return Ok(());
        };
        let problems = session.problems();
        for problem in &problems[self.told.min(problems.len())..] {
            log(out, problem)?;
        }
        self.told = problems.len();
        Ok(())
    }
}

fn log(out: &mut impl Write, message: &str) -> Result<()> {
    notify(
        out,
        "window/logMessage",
        json!({ "type": 1, "message": uf_infra::into_string(uf_infra::cstr!("uf project rules: {message}")) }),
    )
}

/// A project rule's fix as a `TextEdit` over `source`.
///
/// Its range is bytes of the whole document rather than a column on one line,
/// because a project's edit can span lines where uf's own cannot.
pub(super) fn fix_edit(source: &str, fix: &ProjectFix) -> Value {
    json!({
        "range": { "start": position(source, fix.start), "end": position(source, fix.end) },
        "newText": fix.text,
    })
}

/// A byte offset as the protocol counts: a zero-based line, and UTF-16 code
/// units into it.
fn position(source: &str, offset: usize) -> Value {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &source[..offset];
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    json!({
        "line": before.matches('\n').count(),
        "character": before[line_start..].encode_utf16().count(),
    })
}
