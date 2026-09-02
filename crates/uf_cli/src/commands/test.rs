//! `uf test`: a live progress line while running, then a pass/fail summary.

use anyhow::{Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_project::collect_source_files;
use uf_term::{
    Align, Cell, Column, KeyValue, PhaseTimer, Status, Table, Tone, format_duration, push_padded,
    push_spaces,
};
use uf_test::{NativeTestRunnerPlan, TestKind, discover_tests, merge_plans, run_tests};

use crate::support::{plural, project_label};
use crate::ui::{Ui, widest};

pub(crate) fn test(cwd: &Utf8Path, ui: &mut Ui, list: bool) -> Result<()> {
    let mut timer = PhaseTimer::start();
    let resolved = load_config(cwd)?;
    let runner = NativeTestRunnerPlan::self_hosted();
    let files = collect_source_files(&resolved.root, &resolved.config)?;
    let plan = timer.measure("discovery", || {
        merge_plans(
            files
                .iter()
                .map(|file| discover_tests(&file.relative_path, &file.source)),
        )
    });

    let runtime = format!("{:?}", runner.runtime);
    let target = format!("{:?}", runner.performance_target);

    if list {
        let rows = plan
            .cases
            .iter()
            .map(|case| {
                (
                    format!("{}:{}:{}", case.file, case.line, case.column),
                    case.name.clone(),
                )
            })
            .collect::<Vec<_>>();
        let discovered = plural(plan.runnable_count(), "runnable test");

        ui.render(|renderer, out| {
            renderer.banner(out, "uf test", Some("discovery"));
            renderer.blank(out);
            let mut table = Table::new(vec![Column::left("location"), Column::left("test")]);
            for (location, name) in &rows {
                table.push(vec![
                    Cell::toned(location, Tone::Path),
                    Cell::new(name.as_str()),
                ]);
            }
            renderer.table(out, 2, &table);
            renderer.blank(out);
            renderer.key_values(
                out,
                2,
                &[
                    KeyValue::new("runtime", &runtime),
                    KeyValue::new("target", &target),
                ],
            );
            renderer.blank(out);
            renderer.status(out, Status::Info, &format!("discovered {discovered}"));
        });
        return Ok(());
    }

    let mut progress = ui.progress();
    for case in plan.cases.iter().filter(|case| case.kind == TestKind::Test) {
        progress.tick(&case.name);
    }
    let sources = files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.source.as_str()));
    let report = timer.measure("run", || run_tests(sources));
    progress.finish();
    drop(progress);

    let duration = timer.total();
    let file_width = widest(
        report
            .plan
            .cases
            .iter()
            .filter(|case| case.kind == TestKind::Test)
            .map(|case| case.file.as_str()),
    );
    let passed = report.passed.to_string();
    let failures = report.failed.to_string();
    let unsupported = report.unsupported_assertions.to_string();
    let summary = if report.unsupported_assertions == 0 {
        format!(
            "{} passed, {} failed in {}",
            report.passed,
            report.failed,
            format_duration(duration)
        )
    } else {
        format!(
            "{} passed, {} failed, {} unsupported in {}",
            report.passed,
            report.failed,
            report.unsupported_assertions,
            format_duration(duration)
        )
    };
    let phases = timer.phases().to_vec();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf test", Some(project_label(&resolved.root)));
        renderer.blank(out);
        let mut line = String::new();
        for case in report
            .plan
            .cases
            .iter()
            .filter(|case| case.kind == TestKind::Test)
        {
            let failure = report
                .failures
                .iter()
                .find(|failure| failure.file == case.file && failure.name == case.name);
            line.clear();
            push_padded(&mut line, &case.file, file_width + 2, Align::Left);
            line.push_str(&case.name);
            push_spaces(out, 2);
            renderer.status(
                out,
                if failure.is_some() {
                    Status::Error
                } else {
                    Status::Success
                },
                &line,
            );
            // The reason a test failed belongs directly under it, not in a
            // separate block the reader has to correlate by name.
            if let Some(failure) = failure {
                push_spaces(out, 6);
                renderer.line(out, renderer.theme().error, &failure.message);
            }
        }
        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(duration));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("passed", &passed, Tone::Good),
                KeyValue::toned("failed", &failures, Tone::Bad),
                KeyValue::toned("unsupported assertions", &unsupported, Tone::Warn),
                KeyValue::new("runtime", &runtime),
                KeyValue::new("target", &target),
            ],
        );
        renderer.blank(out);
        renderer.status(
            out,
            if report.is_success() {
                Status::Success
            } else {
                Status::Error
            },
            &summary,
        );
    });

    if !report.is_success() {
        if report.failed > 0 {
            bail!("uf test failed with {}", plural(report.failed, "failure"));
        }
        bail!(
            "uf test found {}",
            plural(report.unsupported_assertions, "unsupported assertion")
        );
    }
    Ok(())
}
