//! Sequencing: read, rescan, invalidate, build a payload, publish.

use super::*;
use crate::hmr::channel::Waited;
use crate::hmr::watch::{PollWatcher, watched_files};

struct Session {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
    session: HmrSession,
}

impl Session {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().expect("canonical"))
            .expect("utf-8 temp dir");
        let session = HmrSession::new(&root, Arc::new(UpdateChannel::new()));
        Self {
            _dir: dir,
            root,
            session,
        }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
        std::fs::write(&path, contents).expect("write");
    }

    fn remove(&self, relative: &str) {
        std::fs::remove_file(self.root.join(relative)).expect("remove");
    }

    fn seed_all(&mut self) {
        let watcher = PollWatcher::with_default_interval(&self.root);
        for file in watched_files(&watcher).expect("lists") {
            self.session.seed(&file).expect("seeds");
        }
    }

    fn change(&mut self, relative: &str, change: ChangeKind) -> HmrUpdate {
        self.session
            .apply(&FileChange {
                path: Utf8PathBuf::from(relative),
                change,
            })
            .expect("applies")
    }
}

const COUNTER: &str = "\"use client\";\n// @flow\nimport { helper } from \"./util.js\";\n\
                       export function Counter() { return null; }\n";
const UTIL: &str = "// @flow\nexport function helper() { return 1; }\n";
const PAGE: &str = "// @flow\nimport { Counter } from \"./Counter.js\";\n\
                    import { helper } from \"./util.js\";\n\
                    export default function () { return null; }\n";

fn app() -> Session {
    let mut session = Session::new();
    session.write("app/util.js", UTIL);
    session.write("app/Counter.js", COUNTER);
    session.write("app/page.js", PAGE);
    session.seed_all();
    session
}

#[test]
fn seeding_scans_every_watched_file() {
    let session = app();

    assert_eq!(session.session.graph().present_count(), 3);
    assert!(session.session.graph().find("app/Counter.js").is_some());
}

#[test]
fn seeding_a_file_that_is_not_there_reports_that_rather_than_failing() {
    let mut session = Session::new();

    assert!(
        !session
            .session
            .seed(Utf8Path::new("app/ghost.js"))
            .expect("seeds")
    );
    assert_eq!(session.session.graph().present_count(), 0);
}

#[test]
fn the_session_reports_the_root_it_watches() {
    let session = Session::new();

    assert_eq!(session.session.root(), session.root);
}

#[test]
fn a_component_edit_publishes_a_hot_update() {
    let mut session = app();
    session.write("app/Counter.js", COUNTER);

    let update = session.change("app/Counter.js", ChangeKind::Modified);

    assert_eq!(update.kind, UpdateKind::Hot);
    assert_eq!(update.change, ChangeKind::Modified);
    assert_eq!(update.module_count(), 1);
    assert_eq!(update.modules[0].path, "app/Counter.js");
    assert_eq!(update.modules[0].role, UpdateRole::Boundary);
    assert!(update.routes.is_empty());
}

#[test]
fn a_shared_edit_names_the_client_modules_and_the_route() {
    let mut session = app();
    session.write("app/util.js", UTIL);

    let update = session.change("app/util.js", ChangeKind::Modified);

    assert_eq!(update.kind, UpdateKind::HotAndRoute);
    assert_eq!(
        update
            .modules
            .iter()
            .map(|module| module.path.as_str())
            .collect::<Vec<_>>(),
        ["app/util.js", "app/Counter.js"]
    );
    assert_eq!(update.modules[0].role, UpdateRole::Dependency);
    assert_eq!(update.modules[1].role, UpdateRole::Boundary);
    assert_eq!(
        update
            .routes
            .iter()
            .map(CompactString::as_str)
            .collect::<Vec<_>>(),
        ["app/page.js", "app/util.js"]
    );
}

#[test]
fn dependencies_are_listed_before_the_boundary_that_imports_them() {
    let mut session = app();
    session.write("app/util.js", UTIL);

    let update = session.change("app/util.js", ChangeKind::Modified);

    let roles: Vec<u8> = update
        .modules
        .iter()
        .map(|module| module.role.apply_order())
        .collect();
    assert!(roles.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn a_type_only_edit_publishes_an_inert_update() {
    let mut session = Session::new();
    session.write(
        "app/types.js",
        "// @flow\nexport type User = { id: string };\n",
    );
    session.seed_all();
    session.write(
        "app/types.js",
        "// @flow\nexport type User = { id: string, name: string };\n",
    );

    let update = session.change("app/types.js", ChangeKind::Modified);

    assert!(update.is_inert());
    assert_eq!(update.kind, UpdateKind::Inert);
    assert!(update.modules.is_empty());
    assert!(update.routes.is_empty());
}

#[test]
fn a_file_deleted_before_the_read_is_recorded_as_a_delete() {
    let mut session = app();
    session.remove("app/Counter.js");

    // The watcher still believes it was only modified; the read decides.
    let update = session.change("app/Counter.js", ChangeKind::Modified);

    assert_eq!(update.change, ChangeKind::Deleted);
    assert_eq!(update.kind, UpdateKind::FullReload);
    assert_eq!(update.reason, Some(ReloadReason::ModuleRemoved));
}

#[test]
fn a_file_that_came_back_before_the_read_is_recorded_as_a_write() {
    let mut session = app();

    // The watcher believes it was deleted; the file is there.
    let update = session.change("app/Counter.js", ChangeKind::Deleted);

    assert_eq!(update.change, ChangeKind::Modified);
    assert_eq!(update.kind, UpdateKind::Hot);
}

#[test]
fn a_change_for_a_file_that_never_existed_is_an_inert_update() {
    let mut session = app();

    let update = session.change("app/ghost.js", ChangeKind::Modified);

    assert_eq!(update.change, ChangeKind::Deleted);
    assert!(update.is_inert());
}

#[test]
fn a_new_file_is_scanned_and_reported_as_created() {
    let mut session = app();
    session.write(
        "app/Widget.js",
        "\"use client\";\n// @flow\nexport function Widget() { return null; }\n",
    );

    let update = session.change("app/Widget.js", ChangeKind::Created);

    assert_eq!(update.change, ChangeKind::Created);
    assert_eq!(update.kind, UpdateKind::Hot);
    assert_eq!(session.session.graph().present_count(), 4);
}

#[test]
fn a_traversing_change_path_is_refused_before_anything_is_read() {
    let mut session = app();
    // A file that would be read if the join happened before the check.
    std::fs::write(session.root.join("../escape.js"), "export const x = 1;\n").expect("write");

    for raw in ["../escape.js", "/etc/passwd", "app/../../escape.js"] {
        let error = session
            .session
            .apply(&FileChange {
                path: Utf8PathBuf::from(raw),
                change: ChangeKind::Modified,
            })
            .unwrap_err();
        assert!(
            matches!(error, GraphError::NotProjectRelative { .. }),
            "{raw} must be refused as a module path"
        );
    }

    assert_eq!(session.session.graph().present_count(), 3);
    assert_eq!(session.session.channel().last_event_id(), 0);
    std::fs::remove_file(session.root.join("../escape.js")).expect("clean up");
}

#[test]
fn a_change_path_deeper_than_the_graph_bound_is_a_typed_error() {
    let mut session = app();
    let deep = (0..=crate::hmr::MAX_MODULE_DEPTH)
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>()
        .join("/");

    let error = session
        .session
        .apply(&FileChange {
            path: Utf8PathBuf::from(deep),
            change: ChangeKind::Modified,
        })
        .unwrap_err();

    assert!(matches!(error, GraphError::TooDeep { .. }));
}

#[test]
fn every_module_an_update_names_carries_a_fetchable_target() {
    let mut session = app();
    session.write("app/util.js", UTIL);
    let policy = crate::policy::FsPolicy::with_defaults(&session.root).expect("policy");

    let update = session.change("app/util.js", ChangeKind::Modified);

    assert!(!update.modules.is_empty());
    for module in &update.modules {
        let file = crate::hmr::update::fetch_update(&policy, &module.url)
            .unwrap_or_else(|error| panic!("{} must be servable: {error}", module.url));
        assert!(!file.is_empty());
    }
}

#[test]
fn the_update_target_carries_the_modules_revision() {
    let mut session = app();
    session.write("app/Counter.js", COUNTER);
    let first = session.change("app/Counter.js", ChangeKind::Modified);
    session.write("app/Counter.js", COUNTER);
    let second = session.change("app/Counter.js", ChangeKind::Modified);

    assert_ne!(first.modules[0].url, second.modules[0].url);
    assert!(first.modules[0].url.starts_with("/app/Counter.js?t="));
}

#[test]
fn applying_a_change_publishes_it_on_the_channel() {
    let mut session = app();
    let mut subscriber = session.session.channel().subscribe().expect("subscribes");
    session.write("app/Counter.js", COUNTER);

    let update = session.change("app/Counter.js", ChangeKind::Modified);

    let Waited::Frame(bytes) = subscriber.wait(std::time::Duration::from_millis(1)) else {
        panic!("the update must reach a subscriber");
    };
    let text = String::from_utf8(bytes.to_vec()).expect("utf-8");
    assert!(text.contains("app/Counter.js"));
    assert_eq!(update.id, 1);
}

#[test]
fn published_updates_carry_increasing_identifiers() {
    let mut session = app();
    session.write("app/Counter.js", COUNTER);
    let first = session.change("app/Counter.js", ChangeKind::Modified);
    session.write("app/util.js", UTIL);
    let second = session.change("app/util.js", ChangeKind::Modified);

    assert_eq!(first.id, 1);
    assert_eq!(second.id, 2);
}

#[test]
fn an_update_records_how_long_it_took() {
    let mut session = app();
    session.write("app/Counter.js", COUNTER);

    let update = session.change("app/Counter.js", ChangeKind::Modified);

    assert!(update.elapsed_micros < 10_000_000);
}

#[test]
fn a_full_reload_names_no_modules_to_fetch() {
    let mut session = Session::new();
    session.write(
        "app/tokens.js",
        "\"use client\";\n// @flow\nexport const SPACING = 4;\n",
    );
    session.seed_all();
    session.write(
        "app/tokens.js",
        "\"use client\";\n// @flow\nexport const SPACING = 8;\n",
    );

    let update = session.change("app/tokens.js", ChangeKind::Modified);

    assert_eq!(update.kind, UpdateKind::FullReload);
    assert_eq!(update.reason, Some(ReloadReason::NoAcceptingBoundary));
    assert!(update.modules.is_empty());
}

#[test]
fn rescanning_one_file_leaves_the_rest_of_the_graph_at_its_revision() {
    let mut session = app();
    let util = session.session.graph().find("app/util.js").expect("known");
    let before = session
        .session
        .graph()
        .module(util)
        .expect("slot")
        .revision();

    session.write("app/Counter.js", COUNTER);
    session.change("app/Counter.js", ChangeKind::Modified);

    let after = session
        .session
        .graph()
        .module(util)
        .expect("slot")
        .revision();
    assert_eq!(before, after);
}

#[test]
fn a_directory_where_a_module_was_is_treated_as_a_delete() {
    let mut session = app();
    session.remove("app/Counter.js");
    std::fs::create_dir(session.root.join("app/Counter.js")).expect("directory");

    let update = session.change("app/Counter.js", ChangeKind::Modified);

    assert_eq!(update.change, ChangeKind::Deleted);
}
