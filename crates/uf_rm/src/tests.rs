use super::*;

#[test]
fn default_plan_is_uf_hermes_and_deploy_anywhere() {
    let plan = RuntimeManagerPlan::default();

    assert_eq!(plan.engine, RuntimeEngine::Uf);
    assert!(plan.includes_uf_runtime());
    assert!(plan.uses_hermes());
    assert!(plan.hosts.contains(&RuntimeHost::Node));
    assert!(plan.hosts.contains(&RuntimeHost::Bun));
    assert!(plan.hosts.contains(&RuntimeHost::Deno));
    assert!(plan.hosts.contains(&RuntimeHost::Edge));
    assert!(plan.steps.contains(&RuntimeManagerStep::AcquireRuntime));
    assert!(plan.steps.contains(&RuntimeManagerStep::ApplyAdapters));
    assert_eq!(plan.installer.endpoint, "https://setup.uniflowed.dev");
    assert!(plan.installer.shells.contains(&InstallerShell::Sh));
    assert!(plan.installer.shells.contains(&InstallerShell::Bash));
    assert!(plan.installer.shells.contains(&InstallerShell::Zsh));
    assert!(plan.installer.shells.contains(&InstallerShell::Ush));
    assert!(
        plan.installer
            .platforms
            .contains(&InstallerPlatform::Windows)
    );
    assert!(plan.installer.platforms.contains(&InstallerPlatform::MacOs));
    assert!(plan.installer.platforms.contains(&InstallerPlatform::Linux));
}

#[test]
fn infers_hosts_from_config_without_duplicates() {
    let config = UniflowedConfig::default();
    let plan = RuntimeManagerPlan::infer_from_config(&config);

    assert_eq!(plan.engine, RuntimeEngine::Uf);
    assert_eq!(plan.hosts[0], RuntimeHost::Uf);
    assert_eq!(
        plan.hosts
            .iter()
            .filter(|host| **host == RuntimeHost::Node)
            .count(),
        1
    );
}

#[test]
fn parses_use_request_and_keeps_xdg_paths() {
    let requested = RuntimeReference::parse("uf@0.1.0").unwrap();
    let plan = RuntimeUsePlan::new(requested, XdgLayout::from_home("/home/uf"), true);

    assert_eq!(plan.requested.name, "uf");
    assert_eq!(plan.requested.version, "0.1.0");
    assert_eq!(plan.layout.config_dir, "/home/uf/.config/uniflowed");
    assert_eq!(plan.layout.data_dir, "/home/uf/.local/share/uniflowed");
    assert_eq!(plan.layout.cache_dir, "/home/uf/.cache/uniflowed");
    assert_eq!(plan.layout.state_dir, "/home/uf/.local/state/uniflowed");
    assert_eq!(plan.layout.shim_path, "/home/uf/.local/bin/uf");
    assert!(plan.auto_switch);
    assert!(plan.steps.contains(&RuntimeUseStep::WriteShim));
    assert!(plan.steps.contains(&RuntimeUseStep::ActivateVersion));
}

#[test]
fn xdg_env_uses_absolute_paths_and_ignores_relative_values() {
    let layout = XdgLayout::from_env(XdgEnv {
        home: "/home/uf",
        config_home: Some("/xdg/config"),
        data_home: Some("/xdg/data"),
        cache_home: Some("relative-cache"),
        state_home: Some("/xdg/state"),
        runtime_dir: Some("/run/user/1000"),
    });

    assert_eq!(layout.config_dir, "/xdg/config/uniflowed");
    assert_eq!(layout.data_dir, "/xdg/data/uniflowed");
    assert_eq!(layout.cache_dir, "/home/uf/.cache/uniflowed");
    assert_eq!(layout.state_dir, "/xdg/state/uniflowed");
    assert_eq!(
        layout.runtime_dir.as_deref(),
        Some("/run/user/1000/uniflowed")
    );
    assert_eq!(layout.versions_dir, "/xdg/data/uniflowed/runtimes");
}
