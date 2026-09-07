use camino::Utf8Path;

use super::{PlanSource, Prerender, RenderingPlan, check};
use crate::{ConfigError, RenderingMode, UniflowedConfig};

/// A config whose `modes` is exactly `modes`, with everything else default.
fn with_modes(modes: &[RenderingMode]) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.app.rendering.modes = modes.to_vec();
    config
}

#[test]
fn the_default_decides_per_route_and_emits_a_server() {
    let plan = RenderingPlan::resolve(&UniflowedConfig::default());
    assert_eq!(plan.prerender(), Prerender::Possible);
    assert!(plan.emits_a_server());
    assert!(!plan.is_static_only());
    // The behaviour every project had before anything read the list, so
    // nothing is quoted at a reader who narrowed nothing.
    assert_eq!(plan.source(), PlanSource::Default);
    assert_eq!(plan.source().key(), None);
}

#[test]
fn ssg_alone_makes_a_per_request_route_an_error() {
    let plan = RenderingPlan::resolve(&with_modes(&[RenderingMode::Ssg]));
    assert_eq!(plan.prerender(), Prerender::Everything);
    assert!(plan.is_static_only());
    // Still emitted: `modes` says which routes may exist and `staticBuild`
    // says what is left behind. A project that made only the first
    // declaration can still check the build with `uf start`.
    assert!(plan.emits_a_server());
    assert_eq!(plan.source().key(), Some("app.rendering.modes"));
}

#[test]
fn ssr_alone_prerenders_nothing() {
    // The assertion ubugeeei-prod/uf#336 is named for: `["ssr"]` was accepted
    // and silently meant SSG, because SSG was the only thing a build could
    // produce. It now selects the one behaviour it always claimed to.
    let plan = RenderingPlan::resolve(&with_modes(&[RenderingMode::Ssr]));
    assert_eq!(plan.prerender(), Prerender::Nothing);
    assert!(plan.emits_a_server());
    assert!(!plan.is_static_only());
}

#[test]
fn both_together_build_the_way_the_default_does() {
    let plan = RenderingPlan::resolve(&with_modes(&[RenderingMode::Ssg, RenderingMode::Ssr]));
    assert_eq!(plan.prerender(), Prerender::Possible);
    assert!(plan.emits_a_server());
}

#[test]
fn static_build_prerenders_everything_and_emits_no_server() {
    let mut config = UniflowedConfig::default();
    config.build.static_build = true;
    let plan = RenderingPlan::resolve(&config);
    assert_eq!(plan.prerender(), Prerender::Everything);
    assert!(!plan.emits_a_server());
    assert_eq!(plan.source().key(), Some("build.staticBuild"));
    assert!(
        plan.because().contains("emits no server"),
        "{}",
        plan.because()
    );
}

#[test]
fn the_docs_site_configuration_still_builds() {
    // `docs/uf.config.js` sets both, and has done since before either was
    // read. It is genuinely static, so the plan for it has to be the one that
    // succeeds rather than the one that refuses.
    let mut config = with_modes(&[RenderingMode::Ssg]);
    config.build.static_build = true;
    check(Utf8Path::new("uf.config.js"), &config).unwrap();
    let plan = RenderingPlan::resolve(&config);
    assert_eq!(plan.prerender(), Prerender::Everything);
    assert!(!plan.emits_a_server());
}

#[test]
fn a_planned_mode_beside_an_implemented_one_is_allowed() {
    // `modes` is an allowlist: naming `isr` permits something that never gets
    // selected, which changes nothing and is worth no error.
    let config = with_modes(&[RenderingMode::Ssg, RenderingMode::Isr]);
    check(Utf8Path::new("uf.config.js"), &config).unwrap();
    assert_eq!(
        RenderingPlan::resolve(&config).prerender(),
        Prerender::Everything
    );
}

#[test]
fn a_list_of_only_planned_modes_is_refused() {
    let config = with_modes(&[RenderingMode::Ppr, RenderingMode::Isr]);
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(matches!(
        error,
        ConfigError::NoImplementedRenderingMode { .. }
    ));
    let message = error.to_string();
    assert!(message.contains("ppr, isr"), "{message}");
    assert!(message.contains("ssg"), "{message}");
}

#[test]
fn an_empty_list_is_refused_too() {
    let error = check(Utf8Path::new("uf.config.js"), &with_modes(&[])).unwrap_err();
    assert!(matches!(
        error,
        ConfigError::NoImplementedRenderingMode { .. }
    ));
}

#[test]
fn static_build_with_no_ssg_is_the_contradiction_that_is_refused() {
    // The case the plan exists to catch: no server, and routes that need one.
    // Neither setting is wrong on its own, and there is no behaviour that
    // honours both.
    let mut config = with_modes(&[RenderingMode::Ssr]);
    config.build.static_build = true;
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(matches!(error, ConfigError::StaticBuildWithoutSsg { .. }));
    let message = error.to_string();
    assert!(message.contains("staticBuild"), "{message}");
    assert!(message.contains("ssg"), "{message}");
}
