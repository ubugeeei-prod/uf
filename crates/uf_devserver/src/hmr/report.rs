//! What `uf dev` prints when a file changes.
//!
//! One line per update, in the shape `uf build` and `uf lint` already use: a
//! status mark, the path, what happened, and how long it took. Everything is
//! appended to a caller-owned `String` through [`uf_term`], so a session that
//! runs for hours reuses one buffer.
//!
//! The line always says which verdict was reached, and a fallback to a full
//! reload always says *why*. A page that reloads for reasons the developer
//! cannot see is the failure mode that makes people distrust hot reloading, so
//! [`ReloadReason::message`](super::invalidate::ReloadReason::message) is
//! printed rather than swallowed.

use uf_term::{Renderer, Status, push_duration, push_usize};

use super::invalidate::{ChangeKind, UpdateKind};
use super::update::HmrUpdate;
use super::watch::WatchError;

/// The status mark an update is drawn with.
///
/// A full reload is a warning: it is the feature not doing its job, and it
/// should not look like a success.
pub fn update_status(update: &HmrUpdate) -> Status {
    match update.kind {
        UpdateKind::Inert => Status::Skip,
        UpdateKind::Hot | UpdateKind::Route | UpdateKind::HotAndRoute => Status::Success,
        UpdateKind::FullReload => Status::Warn,
    }
}

/// The human phrase for a verdict.
pub fn update_label(kind: UpdateKind) -> &'static str {
    match kind {
        UpdateKind::Inert => "no runtime change",
        UpdateKind::Hot => "hot update",
        UpdateKind::Route => "route refresh",
        UpdateKind::HotAndRoute => "hot update + route refresh",
        UpdateKind::FullReload => "full reload",
    }
}

/// The human phrase for what happened to the file.
pub fn change_label(change: ChangeKind) -> &'static str {
    match change {
        ChangeKind::Created => "added",
        ChangeKind::Modified => "changed",
        ChangeKind::Deleted => "deleted",
    }
}

/// Append one update line.
///
/// ```text
///   ✔ app/Counter.js  changed  hot update  2 modules  412µs
///   ! app/util.js  changed  full reload  no client module accepts the update  1.1ms
/// ```
pub fn render_update(renderer: &Renderer, out: &mut String, update: &HmrUpdate, indent: usize) {
    let theme = renderer.theme();
    let color = renderer.color();
    let status = update_status(update);

    uf_term::push_spaces(out, indent);
    renderer
        .status_style(status)
        .paint(color, status.glyph(renderer.glyph_set()), out);
    out.push(' ');
    theme.path.paint(color, &update.path, out);
    out.push_str("  ");
    theme.muted.paint(color, change_label(update.change), out);
    out.push_str("  ");
    theme.value.paint(color, update_label(update.kind), out);

    if let Some(reason) = update.reason {
        out.push_str("  ");
        theme.warning.paint(color, reason.message(), out);
    }

    if !update.modules.is_empty() {
        out.push_str("  ");
        theme.number.open(color, out);
        push_usize(out, update.modules.len());
        out.push_str(if update.modules.len() == 1 {
            " module"
        } else {
            " modules"
        });
        theme.number.close(color, out);
    }
    if !update.routes.is_empty() {
        out.push_str("  ");
        theme.number.open(color, out);
        push_usize(out, update.routes.len());
        out.push_str(if update.routes.len() == 1 {
            " route"
        } else {
            " routes"
        });
        theme.number.close(color, out);
    }

    out.push_str("  ");
    theme.number.open(color, out);
    push_duration(out, std::time::Duration::from_micros(update.elapsed_micros));
    theme.number.close(color, out);
    out.push('\n');
}

/// Append the modules an update named, one per line, under the update line.
///
/// Printed when a developer asks for detail; the default is the single line
/// above, because a dev server that prints a paragraph per keystroke is a dev
/// server people scroll past.
pub fn render_update_modules(
    renderer: &Renderer,
    out: &mut String,
    update: &HmrUpdate,
    indent: usize,
) {
    let theme = renderer.theme();
    let color = renderer.color();
    for module in &update.modules {
        uf_term::push_spaces(out, indent);
        theme.rule.open(color, out);
        out.push_str("- ");
        theme.rule.close(color, out);
        theme.path.paint(color, &module.path, out);
        out.push_str("  ");
        theme.muted.paint(color, module.role.as_str(), out);
        out.push('\n');
    }
    for route in &update.routes {
        uf_term::push_spaces(out, indent);
        theme.rule.open(color, out);
        out.push_str("- ");
        theme.rule.close(color, out);
        theme.path.paint(color, route, out);
        out.push_str("  ");
        theme.muted.paint(color, "route", out);
        out.push('\n');
    }
}

/// Append a line describing a watcher failure.
///
/// A watcher that has stopped seeing the project is a broken dev server, and
/// saying nothing about it is how a developer spends twenty minutes wondering
/// why their edits do nothing.
pub fn render_watch_error(
    renderer: &Renderer,
    out: &mut String,
    error: &WatchError,
    indent: usize,
) {
    uf_term::push_spaces(out, indent);
    let message = error.to_string();
    renderer.status(out, Status::Error, &message);
}

#[cfg(test)]
mod tests;
