//! What the two screens look like.
//!
//! Both are rendered here rather than driven through a package manager,
//! because a test that runs npm asserts on whatever npm installed today. What
//! is pinned below is the layout: the exact plain-text block, which is the
//! thing a change to any of the drawing code moves.

use super::*;
use uf_pm::delta::PackageChange;
use uf_pm::{InstallPhase, ManagerEvent};
use uf_term::{ColorLevel, Tty};

/// A renderer with nothing to draw with: no colour, ASCII glyphs, nobody
/// watching. The same capability `--json` and a redirected stream get.
fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

/// The same, in the vocabulary a terminal gets.
fn rich() -> Renderer {
    Renderer::new(Capabilities::new(
        ColorLevel::Never,
        GlyphSet::Unicode,
        Tty::Interactive,
    ))
}

fn change(name: &str, kind: ChangeKind, before: &str, after: &str) -> PackageChange {
    PackageChange {
        name: name.into(),
        kind,
        before: before.into(),
        after: after.into(),
    }
}

fn report(delta: LockfileDelta) -> InstallReport {
    InstallReport {
        manager: "npm".to_owned(),
        chosen_by: "package-lock.json".to_owned(),
        command: "npm install --ignore-scripts".to_owned(),
        runtime: Some("node · /usr/bin/node".to_owned()),
        lockfile: "package-lock.json · 17 packages · 12.40 kB".to_owned(),
        plan: "/tmp/app/.uf/install.json".to_owned(),
        phases: vec![
            Phase {
                label: "config",
                duration: Duration::from_micros(1_200),
            },
            Phase {
                label: "resolve",
                duration: Duration::from_millis(1_400),
            },
            Phase {
                label: "fetch",
                duration: Duration::from_millis(4_200),
            },
            Phase {
                label: "link",
                duration: Duration::from_millis(1_100),
            },
        ],
        total: Duration::from_millis(6_900),
        delta,
    }
}

fn unchanged() -> LockfileDelta {
    LockfileDelta {
        packages_before: Some(17),
        packages_after: Some(17),
        bytes_before: 12_400,
        bytes_after: 12_400,
        existed_before: true,
        exists_now: true,
        detailed: true,
        ..LockfileDelta::default()
    }
}

fn changed() -> LockfileDelta {
    LockfileDelta {
        changes: vec![
            change("react", ChangeKind::Added, "", "18.3.1"),
            change("vite", ChangeKind::Added, "", "5.4.10"),
            change("left-pad", ChangeKind::Removed, "1.3.0", ""),
            change("lodash", ChangeKind::Updated, "4.17.20", "4.17.21"),
            change("ms", ChangeKind::Moved, "2.1.3", "2.1.3"),
        ],
        packages_before: Some(14),
        packages_after: Some(17),
        bytes_before: 11_000,
        bytes_after: 12_400,
        existed_before: true,
        exists_now: true,
        detailed: true,
    }
}

#[test]
fn an_install_that_changed_nothing_is_a_block_and_a_line() {
    let mut out = String::new();
    render_summary(&plain(), &mut out, &report(unchanged()));

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts",
            "  runtime    node · /usr/bin/node",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "  plan       /tmp/app/.uf/install.json",
            "",
            "+ already up to date in 6.9s",
        ]
    );
    assert!(out.ends_with('\n'));
}

#[test]
fn an_install_that_changed_something_says_what() {
    let mut out = String::new();
    render_summary(&plain(), &mut out, &report(changed()));

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts",
            "  runtime    node · /usr/bin/node",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "  plan       /tmp/app/.uf/install.json",
            "",
            "  config  ........................ 1.2ms",
            "  resolve ........................  1.4s",
            "  fetch   ........................  4.2s",
            "  link    ........................  1.1s",
            "  total   ........................  6.9s",
            "",
            "  dependency tree",
            "    added    2",
            "    removed  1",
            "    updated  1",
            "    moved    1",
            "",
            "       package   version",
            "    +  react     18.3.1",
            "    +  vite      5.4.10",
            "    -  left-pad  1.3.0",
            "    ~  lodash    4.17.20 -> 4.17.21",
            "    >  ms        2.1.3",
            "",
            "  next steps",
            "    1. uf dev",
            "    2. uf check",
            "",
            "+ dependencies installed in 6.9s",
        ]
    );
}

#[test]
fn a_lockfile_uf_cannot_read_gets_no_empty_section() {
    // Bun, pnpm and Yarn write a lockfile uf does not parse. The install
    // plainly changed something — the file is a different size — and uf has
    // nothing to list, so it lists nothing rather than heading a blank space.
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(LockfileDelta {
            bytes_before: 11_000,
            bytes_after: 12_400,
            existed_before: true,
            exists_now: true,
            ..LockfileDelta::default()
        }),
    );

    assert!(!out.contains("dependency tree"), "{out}");
    assert!(
        out.contains("total   "),
        "the timings are still worth having:\n{out}"
    );
    assert!(out.contains("next steps"), "{out}");
    assert!(out.contains("dependencies installed in 6.9s"), "{out}");
}

#[test]
fn nothing_the_summary_draws_needs_an_escape_sequence() {
    for delta in [unchanged(), changed()] {
        let mut out = String::new();
        render_summary(&plain(), &mut out, &report(delta));
        assert!(
            !out.contains('\u{1b}'),
            "a redirected `uf install` must be plain text:\n{out}"
        );
    }
}

#[test]
fn a_long_change_list_is_cut_and_says_how_much_it_cut() {
    let mut delta = changed();
    delta.changes = (0..CHANGES_SHOWN + 4)
        .map(|index| {
            change(
                &format!("package-{index:02}"),
                ChangeKind::Added,
                "",
                "1.0.0",
            )
        })
        .collect();
    let mut out = String::new();
    render_summary(&plain(), &mut out, &report(delta));

    assert!(out.contains("package-14"));
    assert!(!out.contains("package-15"));
    assert!(out.contains("and 4 more"));
}

#[test]
fn a_terminal_gets_the_arrow_a_terminal_can_draw() {
    let mut out = String::new();
    render_summary(&rich(), &mut out, &report(changed()));
    assert!(out.contains("4.17.20 → 4.17.21"));
}

#[test]
fn every_ladder_row_fits_the_region_it_is_drawn_in() {
    let renderer = rich();
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    // The longest thing a registry can put on a row, at the widest phase
    // label, with counts and a duration that do not fit their columns either.
    watch.observe(
        &ManagerEvent::Fetched {
            package: "@a-very-long-scope-indeed/a-package-with-a-long-name@10.20.30".to_owned(),
            cached: false,
        },
        start,
    );
    watch.finish(start + Duration::from_secs(4_000));

    let mut row = String::new();
    for phase in watch.phases() {
        row.clear();
        phase_row(
            &mut row,
            &renderer,
            Ladder::for_width(ROW_WIDTH),
            "⠹",
            phase,
        );
        assert!(
            display_width(&row) <= uf_term::LIVE_WIDTH,
            "a row wider than the region wraps, and a wrapped row breaks every \
             redraw after it: {} columns in {row:?}",
            display_width(&row)
        );
    }
    const { assert!(ROW_WIDTH <= uf_term::LIVE_WIDTH) };
}

#[test]
fn the_timings_run_in_the_order_the_phases_did() {
    // The ladder on screen and the timings in the summary describe the same
    // phases; two vocabularies for them would read as twice as many. The
    // manager's rows also have to land between uf's own, which is not the
    // order they were measured in.
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Fetched {
            package: "react@18.3.1".to_owned(),
            cached: false,
        },
        start + Duration::from_millis(400),
    );
    watch.finish(start + Duration::from_millis(900));
    let prelude = [Phase {
        label: "config",
        duration: Duration::from_millis(1),
    }];
    let labels: Vec<&str> = phases(&prelude, Some(&watch), Duration::from_millis(2))
        .iter()
        .map(|phase| phase.label)
        .collect();

    assert_eq!(
        labels,
        [
            "config",
            InstallPhase::Resolve.label(),
            InstallPhase::Fetch.label(),
            "lockfile",
        ]
    );
}

#[test]
fn a_phase_that_took_no_time_is_not_given_a_row() {
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    // npm posts its advisory request while it is still fetching, so the audit
    // is counted and never holds the ladder. `audit ··· 0ns` would be a
    // measurement uf did not make.
    watch.observe(&ManagerEvent::Audited, start);
    watch.finish(start);
    let labels: Vec<&str> = phases(&[], Some(&watch), Duration::ZERO)
        .iter()
        .map(|phase| phase.label)
        .collect();

    assert_eq!(watch.phases()[3].count, 1);
    assert_eq!(labels, ["lockfile"]);
}

#[test]
fn a_manager_uf_could_not_read_still_gets_its_own_timings() {
    let prelude = [
        Phase {
            label: "config",
            duration: Duration::from_millis(1),
        },
        Phase {
            label: "workspace",
            duration: Duration::from_millis(2),
        },
    ];
    let labels: Vec<&str> = phases(&prelude, None, Duration::from_millis(3))
        .iter()
        .map(|phase| phase.label)
        .collect();

    assert_eq!(labels, ["config", "workspace", "lockfile"]);
}

#[test]
fn why_this_manager_reads_as_a_sentence() {
    assert_eq!(
        chosen_by(&DetectionSource::Default, true),
        "no lockfile or packageManager field"
    );
    // `uf install` writes `uf.lock` a step before it runs the manager, so this
    // is what almost every project sees. Naming the file is the difference
    // between an explanation and a riddle.
    assert_eq!(
        chosen_by(
            &DetectionSource::Lockfile {
                lockfile: uf_pm::Lockfile::UfLock,
                path: "/tmp/app/uf.lock".into(),
            },
            true
        ),
        "uf.lock names uf, whose resolver cannot fetch yet"
    );
    assert_eq!(
        chosen_by(&DetectionSource::ConfigOverride, true),
        "pm.packageManager names uf, whose resolver cannot fetch yet"
    );
    assert_eq!(
        chosen_by(&DetectionSource::ConfigOverride, false),
        "pm.packageManager in uf.config.js"
    );
    assert_eq!(
        chosen_by(
            &DetectionSource::Lockfile {
                lockfile: uf_pm::Lockfile::PnpmLock,
                path: "/tmp/app/pnpm-lock.yaml".into(),
            },
            false
        ),
        "pnpm-lock.yaml"
    );
}

#[test]
fn the_lockfile_row_names_a_file_that_was_never_written() {
    let snapshot = LockfileSnapshot {
        path: "/tmp/app/package-lock.json".into(),
        ..LockfileSnapshot::default()
    };
    assert_eq!(
        lockfile_label(Utf8Path::new("/tmp/app"), &snapshot),
        "package-lock.json · not written"
    );
}

#[test]
fn the_live_frame_is_a_ladder_with_the_running_phase_on_it() {
    // What a person watching an install actually sees, drawn here because a
    // pseudo-terminal is the only thing that would make `Live` draw it for
    // real. The frame is the picture; `uf_term`'s own tests cover putting it
    // on the screen and taking it off again.
    let renderer = rich();
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    let mut rows = Vec::new();
    let compose = |watch: &InstallWatch, rows: &mut Vec<String>, at: u64, spinner: &str| {
        frame(
            &renderer,
            rows,
            &FrameHeader {
                title: "uf install · demo-app",
                manager: "npm",
                elapsed: Duration::from_millis(at),
            },
            ROW_WIDTH,
            spinner,
            watch,
        );
    };

    for (index, name) in ["vite", "react", "react-dom", "esbuild", "@types/node"]
        .into_iter()
        .enumerate()
    {
        watch.observe(
            &ManagerEvent::Resolved {
                package: name.to_owned(),
            },
            start + Duration::from_millis(200 * index as u64),
        );
    }
    watch.idle(start + Duration::from_millis(900));
    compose(&watch, &mut rows, 900, "⠹");
    assert_eq!(
        rows,
        [
            "  uf install · demo-app                                  npm · 900ms",
            "  ⠹ resolve  @types/node                                  5    900ms",
            "  · fetch    —                                            —        —",
            "  · link     —                                            —        —",
            "  · audit    —                                            —        —",
        ]
    );

    for (index, name) in [
        "react@18.3.1",
        "react-dom@18.3.1",
        "@esbuild/darwin-arm64@0.21.5",
    ]
    .into_iter()
    .enumerate()
    {
        watch.observe(
            &ManagerEvent::Fetched {
                package: name.to_owned(),
                cached: index == 0,
            },
            start + Duration::from_millis(1_400 + 300 * index as u64),
        );
    }
    // npm asks about advisories while it is still unpacking, so the audit has
    // a count before it has the ladder.
    watch.observe(&ManagerEvent::Audited, start + Duration::from_millis(2_100));
    watch.idle(start + Duration::from_millis(3_400));
    compose(&watch, &mut rows, 3_400, "⠧");
    assert_eq!(
        rows,
        [
            "  uf install · demo-app                                   npm · 3.4s",
            "  ✓ resolve  @types/node                                  5     1.4s",
            "  ✓ fetch    @esbuild/darwin-arm64@0.21.5                 3       2s",
            "  ⠧ link     node_modules                                 —        —",
            "  · audit    —                                            1        —",
        ]
    );

    for row in &rows {
        assert!(display_width(row) <= uf_term::LIVE_WIDTH);
    }
}

/// A ladder built for a window it does not fit in, at every width down to one.
///
/// This is the failure the width was added for. `Live` redraws by walking the
/// cursor back up over the rows it drew, and a row wider than the window wraps
/// onto two physical lines — so the region walks down the screen instead of
/// staying where it is. One row too wide is enough to destroy every frame that
/// follows it, which is why this asserts the bound at every width rather than
/// at the two or three anybody would think to try.
#[test]
fn no_row_is_ever_wider_than_the_window_it_was_built_for() {
    let renderer = rich();
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Resolved {
            package: "@typescript-eslint/eslint-plugin".to_owned(),
        },
        start,
    );
    watch.observe(
        &ManagerEvent::Fetched {
            package: "@esbuild/darwin-arm64@0.21.5".to_owned(),
            cached: false,
        },
        start + Duration::from_millis(400),
    );
    let mut rows = Vec::new();

    for width in 1..=100 {
        frame(
            &renderer,
            &mut rows,
            &FrameHeader {
                title: "uf install · a-project-with-a-long-name",
                manager: "npm",
                elapsed: Duration::from_millis(1_234),
            },
            width,
            "⠹",
            &watch,
        );
        for row in &rows {
            assert!(
                display_width(row) <= width,
                "at {width} columns this row is {} wide: {row:?}",
                display_width(row)
            );
        }
    }
}

/// What a narrow terminal actually shows.
///
/// Forty columns is a split pane, and it is the width at which the old
/// fixed-72 ladder wrapped every row. The package name shrinks; nothing else
/// moves, because a reader who has seen the wide ladder should recognise this
/// one.
#[test]
fn a_narrow_window_keeps_the_ladder_and_shortens_the_package_name() {
    let renderer = rich();
    let start = Instant::now();
    let mut watch = InstallWatch::start(start);
    watch.observe(
        &ManagerEvent::Resolved {
            package: "@typescript-eslint/eslint-plugin".to_owned(),
        },
        start,
    );
    watch.idle(start + Duration::from_millis(900));
    let mut rows = Vec::new();
    frame(
        &renderer,
        &mut rows,
        &FrameHeader {
            title: "uf install · demo-app",
            manager: "npm",
            elapsed: Duration::from_millis(900),
        },
        40,
        "⠹",
        &watch,
    );

    assert_eq!(
        rows,
        [
            "  uf install · demo-app      npm · 900ms",
            "  ⠹ resolve  @typescrip       1    900ms",
            "  · fetch    —                —        —",
            "  · link     —                —        —",
            "  · audit    —                —        —",
        ]
    );
    for row in &rows {
        assert_eq!(display_width(row), 40, "{row:?}");
    }
}

/// The order the columns leave in, stated once as a table.
#[test]
fn the_columns_leave_in_the_order_a_reader_misses_them_least() {
    // Wide enough for everything: the natural widths, unchanged.
    assert_eq!(
        Ladder::for_width(200),
        Ladder {
            indent: INDENT,
            mark: true,
            label: LABEL_WIDTH,
            detail: DETAIL_WIDTH,
            count: COUNT_WIDTH,
            time: TIME_WIDTH,
        }
    );
    // The package name is the only elastic column, so it gives first.
    assert_eq!(Ladder::for_width(50).detail, 50 - LEFT_WIDTH - 1 - 17);
    // Below the point where a name still says which package, it goes.
    let cramped = Ladder::for_width(LEFT_WIDTH + 1 + MIN_DETAIL_WIDTH + 16);
    assert_eq!(cramped.detail, 0);
    assert_eq!((cramped.count, cramped.time), (COUNT_WIDTH, TIME_WIDTH));
    // Then the count. The clock outlives it: "is this stuck" is the question a
    // narrow window is most often being asked.
    let counted_out = Ladder::for_width(LEFT_WIDTH + 2 + TIME_WIDTH);
    assert_eq!((counted_out.count, counted_out.time), (0, TIME_WIDTH));
    // Then the clock, leaving the mark and the phase.
    let bare = Ladder::for_width(LEFT_WIDTH);
    assert_eq!((bare.count, bare.time), (0, 0));
    assert_eq!(bare.label, LABEL_WIDTH);
    // And past that the label itself is cut. Below the width at which a cut
    // label still reads, the indent and the mark go too: four columns of
    // "reso" say more than two spaces and a dot do.
    assert_eq!(Ladder::for_width(9).label, 5);
    assert!(Ladder::for_width(9).mark);
    assert_eq!(Ladder::for_width(7).indent, 0);
    assert!(!Ladder::for_width(7).mark);
    assert_eq!(Ladder::for_width(7).label, 7);
    assert_eq!(Ladder::for_width(1).label, 1);
}
