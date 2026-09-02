//! What `uf dev` prints, and what it must never print silently.

use compact_str::CompactString;
use uf_term::{Capabilities, ColorChoice, TerminalEnv, Tty};

use super::*;
use crate::hmr::invalidate::ReloadReason;
use crate::hmr::update::{UpdateModule, UpdateRole};

fn plain() -> Renderer {
    Renderer::new(Capabilities::detect(
        ColorChoice::Never,
        Tty::Piped,
        &TerminalEnv::default(),
    ))
}

fn colored() -> Renderer {
    Renderer::new(Capabilities::detect(
        ColorChoice::Always,
        Tty::Interactive,
        &TerminalEnv::default(),
    ))
}

fn update(kind: UpdateKind) -> HmrUpdate {
    HmrUpdate {
        id: 3,
        path: CompactString::const_new("app/Counter.js"),
        change: ChangeKind::Modified,
        kind,
        reason: None,
        modules: vec![UpdateModule {
            path: CompactString::const_new("app/Counter.js"),
            url: CompactString::const_new("/app/Counter.js?t=2"),
            role: UpdateRole::Boundary,
        }],
        routes: Vec::new(),
        elapsed_micros: 1_500,
    }
}

fn render(update: &HmrUpdate) -> String {
    let mut out = String::new();
    render_update(&plain(), &mut out, update, 2);
    out
}

#[test]
fn an_update_line_names_the_file_the_verdict_and_the_duration() {
    let line = render(&update(UpdateKind::Hot));

    assert!(line.contains("app/Counter.js"));
    assert!(line.contains("changed"));
    assert!(line.contains("hot update"));
    assert!(line.contains("1.5ms"));
    assert!(line.ends_with('\n'));
}

#[test]
fn an_update_line_is_indented_by_the_requested_columns() {
    let line = render(&update(UpdateKind::Hot));

    assert!(line.starts_with("  "));
}

#[test]
fn a_piped_stream_receives_no_escape_byte() {
    let mut line = render(&update(UpdateKind::Hot));
    let mut reload = update(UpdateKind::FullReload);
    reload.reason = Some(ReloadReason::NoAcceptingBoundary);
    line.push_str(&render(&reload));

    assert!(!line.contains('\u{1b}'));
}

#[test]
fn a_terminal_stream_receives_styling() {
    let mut out = String::new();
    render_update(&colored(), &mut out, &update(UpdateKind::Hot), 2);

    assert!(out.contains('\u{1b}'));
}

#[test]
fn the_full_reload_fallback_is_visible_rather_than_silent() {
    let mut reload = update(UpdateKind::FullReload);
    reload.reason = Some(ReloadReason::NoAcceptingBoundary);
    reload.modules.clear();

    let line = render(&reload);

    assert!(line.contains("full reload"));
    assert!(line.contains(ReloadReason::NoAcceptingBoundary.message()));
}

#[test]
fn every_reload_reason_reaches_the_line() {
    for reason in [
        ReloadReason::NoAcceptingBoundary,
        ReloadReason::ModuleRemoved,
        ReloadReason::DepthExceeded,
        ReloadReason::Unservable,
        ReloadReason::TooManyModules,
    ] {
        let mut reload = update(UpdateKind::FullReload);
        reload.reason = Some(reason);
        assert!(render(&reload).contains(reason.message()));
    }
}

#[test]
fn a_full_reload_is_marked_as_a_warning_not_a_success() {
    let mut reload = update(UpdateKind::FullReload);
    reload.reason = Some(ReloadReason::NoAcceptingBoundary);

    assert_eq!(update_status(&reload), Status::Warn);
    assert_eq!(update_status(&update(UpdateKind::Hot)), Status::Success);
    assert_eq!(update_status(&update(UpdateKind::Route)), Status::Success);
    assert_eq!(update_status(&update(UpdateKind::Inert)), Status::Skip);
}

#[test]
fn module_counts_are_pluralized() {
    let one = render(&update(UpdateKind::Hot));
    assert!(one.contains("1 module"));
    assert!(!one.contains("1 modules"));

    let mut two = update(UpdateKind::Hot);
    two.modules.push(UpdateModule {
        path: CompactString::const_new("app/util.js"),
        url: CompactString::const_new("/app/util.js?t=1"),
        role: UpdateRole::Dependency,
    });
    assert!(render(&two).contains("2 modules"));
}

#[test]
fn route_counts_are_pluralized() {
    let mut one = update(UpdateKind::Route);
    one.modules.clear();
    one.routes.push(CompactString::const_new("app/page.js"));
    assert!(render(&one).contains("1 route"));

    one.routes.push(CompactString::const_new("app/other.js"));
    assert!(render(&one).contains("2 routes"));
}

#[test]
fn an_inert_update_names_no_counts() {
    let mut inert = update(UpdateKind::Inert);
    inert.modules.clear();

    let line = render(&inert);

    assert!(line.contains("no runtime change"));
    assert!(!line.contains("module"));
    assert!(!line.contains("route"));
}

#[test]
fn every_verdict_has_a_label() {
    for kind in [
        UpdateKind::Inert,
        UpdateKind::Hot,
        UpdateKind::Route,
        UpdateKind::HotAndRoute,
        UpdateKind::FullReload,
    ] {
        assert!(!update_label(kind).is_empty());
    }
}

#[test]
fn every_change_has_a_label() {
    assert_eq!(change_label(ChangeKind::Created), "added");
    assert_eq!(change_label(ChangeKind::Modified), "changed");
    assert_eq!(change_label(ChangeKind::Deleted), "deleted");
}

#[test]
fn the_module_detail_block_lists_modules_and_routes() {
    let mut detailed = update(UpdateKind::HotAndRoute);
    detailed.modules.push(UpdateModule {
        path: CompactString::const_new("app/util.js"),
        url: CompactString::const_new("/app/util.js?t=1"),
        role: UpdateRole::Dependency,
    });
    detailed
        .routes
        .push(CompactString::const_new("app/page.js"));

    let mut out = String::new();
    render_update_modules(&plain(), &mut out, &detailed, 4);

    assert!(out.contains("app/Counter.js"));
    assert!(out.contains("boundary"));
    assert!(out.contains("app/util.js"));
    assert!(out.contains("dependency"));
    assert!(out.contains("app/page.js"));
    assert!(out.contains("route"));
    assert_eq!(out.lines().count(), 3);
}

#[test]
fn a_watch_error_is_reported_rather_than_swallowed() {
    let mut out = String::new();
    render_watch_error(&plain(), &mut out, &WatchError::TooManyFiles, 2);

    assert!(out.contains("watched files"));
    assert!(out.ends_with('\n'));
}

#[test]
fn rendering_appends_to_a_reused_buffer() {
    let mut out = String::from("before\n");
    render_update(&plain(), &mut out, &update(UpdateKind::Hot), 2);
    render_update(&plain(), &mut out, &update(UpdateKind::Route), 2);

    assert!(out.starts_with("before\n"));
    assert_eq!(out.lines().count(), 3);
}

#[test]
fn a_sub_millisecond_update_still_reports_a_readable_duration() {
    let mut quick = update(UpdateKind::Hot);
    quick.elapsed_micros = 412;

    assert!(render(&quick).contains("412µs"));
}

#[test]
fn a_non_ascii_module_path_renders_without_breaking_the_line() {
    let mut unicode = update(UpdateKind::Hot);
    unicode.path = CompactString::const_new("app/日本/café.js");

    let line = render(&unicode);

    assert!(line.contains("app/日本/café.js"));
    assert_eq!(line.lines().count(), 1);
}
