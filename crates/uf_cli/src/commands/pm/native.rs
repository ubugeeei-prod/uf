//! The native manager's measured work and security findings in uf's terminal style.
use crate::ui::Ui;
use anyhow::{Result, ensure};
use uf_config::ResolvedConfig;
use uf_term::{Cell, Column, KeyValue, Status, Table, Tone};

pub(super) fn install(
    resolved: &ResolvedConfig,
    ui: &mut Ui,
    frozen: bool,
    prod: bool,
) -> Result<()> {
    install_options(
        resolved,
        ui,
        uf_pm::installer::Options {
            frozen,
            prod,
            ..Default::default()
        },
    )
}

pub(super) fn install_options(
    resolved: &ResolvedConfig,
    ui: &mut Ui,
    mut options: uf_pm::installer::Options,
) -> Result<()> {
    ui.render(|renderer, out| {
        renderer.banner(
            out,
            "uf install",
            Some(crate::support::project_label(&resolved.root)),
        )
    });
    options.path = if resolved.config.pm.allow_lifecycle_scripts {
        crate::commands::runtimes::manager_path(
            resolved,
            uf_pm::PackageManager::Uf,
            options.frozen,
            &mut |_| {},
        )?
    } else {
        Vec::new()
    };
    let report = uf_pm::installer::install(&resolved.root, &resolved.config, options)?;
    let packages = report.packages.to_string();
    let fetched = report.downloaded.to_string();
    let linked = report.hardlinked.to_string();
    let copied = report.copied.to_string();
    let elapsed = uf_infra::into_string(uf_infra::cstr!(
        "resolve {} ms · fetch {} ms · link {} ms",
        report.resolve_ms,
        report.fetch_ms,
        report.link_ms
    ));
    ui.render(|renderer, out| {
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("manager", "uf native", Tone::Accent),
                KeyValue::toned("store", report.store.as_str(), Tone::Path),
                KeyValue::new("packages", &packages),
                KeyValue::new("downloaded", &fetched),
                KeyValue::new("hardlinked", &linked),
                KeyValue::new("copied", &copied),
            ],
        );
        renderer.status(out, Status::Success, &elapsed);
    });
    Ok(())
}

pub(super) fn query(
    resolved: &ResolvedConfig,
    ui: &mut Ui,
    operation: uf_pm::Operation<'_>,
    operands: &[String],
) -> Result<()> {
    if operation == uf_pm::Operation::Audit {
        return audit(resolved, ui, operands, false, false);
    }
    let value = uf_pm::installer::execute(&resolved.root, operation, operands)?;
    ui.json(&value)?;
    Ok(())
}

pub(super) fn audit(
    resolved: &ResolvedConfig,
    ui: &mut Ui,
    packages: &[String],
    json: bool,
    prod: bool,
) -> Result<()> {
    let report =
        uf_pm::installer::audit_packages(&resolved.root, &resolved.config, prod, packages)?;
    if json {
        ui.json(&serde_json::to_value(&report)?)?;
    } else {
        let versions: Vec<_> = report
            .findings
            .iter()
            .map(|finding| {
                finding
                    .versions
                    .iter()
                    .map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        ui.render(|renderer, out| {
            renderer.banner(
                out,
                "uf audit",
                Some(crate::support::project_label(&resolved.root)),
            );
            let mut table = Table::new(vec![
                Column::left("severity"),
                Column::left("package"),
                Column::left("installed"),
                Column::left("fix version"),
                Column::left("advisory"),
            ]);
            for (finding, versions) in report.findings.iter().zip(&versions) {
                let tone = match finding.severity.as_str() {
                    "critical" | "high" => Tone::Bad,
                    "moderate" => Tone::Warn,
                    _ => Tone::Muted,
                };
                table.push(vec![
                    Cell::toned(&finding.severity, tone),
                    Cell::toned(&finding.package, Tone::Path),
                    Cell::new(versions),
                    Cell::toned(
                        finding.fix_version.as_deref().unwrap_or("unknown"),
                        if finding.fix_version.is_some() {
                            Tone::Good
                        } else {
                            Tone::Muted
                        },
                    ),
                    Cell::new(&finding.title),
                ]);
            }
            renderer.table(out, 2, &table);
            for finding in &report.findings {
                renderer.status(out, Status::Info, &finding.url);
            }
            let message = uf_infra::into_string(uf_infra::cstr!(
                "{} package versions checked · {} advisories",
                report.checked,
                report.findings.len()
            ));
            renderer.status(
                out,
                if report.findings.is_empty() {
                    Status::Success
                } else {
                    Status::Warn
                },
                &message,
            );
        });
    }
    ensure!(
        report.findings.is_empty(),
        "security advisories affect locked dependencies; review uf update"
    );
    Ok(())
}
