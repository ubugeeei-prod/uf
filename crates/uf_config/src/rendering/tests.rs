use camino::Utf8Path;

use super::{PlanSource, Prerender, RenderingPlan, check};
use crate::{ConfigError, Navigation, RenderingMode, UniflowedConfig};

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

#[test]
fn csr_alone_writes_one_shell_and_no_server() {
    let config = with_modes(&[RenderingMode::Csr]);
    check(Utf8Path::new("uf.config.js"), &config).unwrap();
    let plan = RenderingPlan::resolve(&config);
    assert_eq!(plan.prerender(), Prerender::Shell);
    assert!(!plan.emits_a_server());
    // Not `is_static_only`: nothing has to be prerenderable, because nothing is
    // prerendered. The refusal a `csr` build makes is a different one.
    assert!(!plan.is_static_only());
    assert_eq!(plan.source().key(), Some("app.rendering.modes"));
    assert!(plan.because().contains("shell"), "{}", plan.because());
}

#[test]
fn csr_is_not_in_the_default_list() {
    // The whole of why: a default that could select it would make a build
    // decide to be a single-page application because nothing forbade it.
    let plan = RenderingPlan::resolve(&UniflowedConfig::default());
    assert_eq!(plan.prerender(), Prerender::Possible);
    assert!(
        !UniflowedConfig::default()
            .app
            .rendering
            .modes
            .contains(&RenderingMode::Csr)
    );
}

#[test]
fn csr_beside_another_mode_is_refused() {
    for other in [
        RenderingMode::Ssg,
        RenderingMode::Ssr,
        RenderingMode::Ppr,
        RenderingMode::Isr,
    ] {
        let config = with_modes(&[RenderingMode::Csr, other]);
        let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
        assert!(
            matches!(error, ConfigError::CsrIsNotOneOfSeveral { .. }),
            "{other:?}: {error}"
        );
        let message = error.to_string();
        assert!(message.contains("csr"), "{message}");
        assert!(message.contains(other.as_str()), "{message}");
    }
}

#[test]
fn csr_with_static_build_is_refused_in_its_own_words() {
    // Both refusals are available for this file and only one of them is any
    // use: the `ssg` rule would tell a single-page project to add `"ssg"` to
    // the list, which is advice to build a different application.
    let mut config = with_modes(&[RenderingMode::Csr]);
    config.build.static_build = true;
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(
        matches!(error, ConfigError::CsrWithStaticBuild { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("drop `staticBuild`"), "{error}");
}

#[test]
fn csr_composes_with_navigation_the_way_everything_else_does() {
    // A single-page application navigates in the browser by construction, and
    // the config can still say `document`. Not refused, because it is not a
    // contradiction: every link fetches the shell again and the browser renders
    // the route from it, which works and is a slow way to have an SPA. A
    // refusal here would be uf deciding that a working deployment is a mistake.
    let mut config = with_modes(&[RenderingMode::Csr]);
    config.app.rendering.navigation = Navigation::Document;
    check(Utf8Path::new("uf.config.js"), &config).unwrap();
    let plan = RenderingPlan::resolve(&config);
    assert_eq!(plan.prerender(), Prerender::Shell);
    assert!(!plan.ships_a_client_router());
}

#[test]
fn navigation_is_the_client_router_unless_a_project_says_otherwise() {
    let plan = RenderingPlan::resolve(&UniflowedConfig::default());
    assert_eq!(plan.navigation(), Navigation::Client);
    assert!(plan.ships_a_client_router());
}

#[test]
fn document_navigation_changes_nothing_about_the_prerender() {
    // The whole argument for `navigation` being a second key rather than a
    // fifth `modes` value, as an assertion: it composes with each of the three
    // prerender answers and moves none of them.
    for modes in [
        vec![RenderingMode::Ssg, RenderingMode::Ssr],
        vec![RenderingMode::Ssg],
        vec![RenderingMode::Ssr],
    ] {
        let mut config = with_modes(&modes);
        let before = RenderingPlan::resolve(&config);
        config.app.rendering.navigation = Navigation::Document;
        let after = RenderingPlan::resolve(&config);
        assert_eq!(before.prerender(), after.prerender(), "{modes:?}");
        assert_eq!(before.emits_a_server(), after.emits_a_server(), "{modes:?}");
        assert_eq!(before.source(), after.source(), "{modes:?}");
        assert!(!after.ships_a_client_router(), "{modes:?}");
    }
}

#[test]
fn a_static_build_carries_its_navigation_too() {
    // `staticBuild` returns early, before the match, so it is the one path
    // that could have dropped the setting on the floor.
    let mut config = UniflowedConfig::default();
    config.build.static_build = true;
    config.app.rendering.navigation = Navigation::Document;
    let plan = RenderingPlan::resolve(&config);
    assert_eq!(plan.prerender(), Prerender::Everything);
    assert!(!plan.emits_a_server());
    assert_eq!(plan.navigation(), Navigation::Document);
}

#[test]
fn document_navigation_is_never_a_refusal_on_its_own() {
    // There is no `modes` it contradicts: every cell of the table in
    // `Navigation`'s documentation is a deployment somebody wants.
    for modes in [
        vec![RenderingMode::Ssg, RenderingMode::Ssr],
        vec![RenderingMode::Ssg],
        vec![RenderingMode::Ssr],
        vec![RenderingMode::Ssg, RenderingMode::Isr],
    ] {
        let mut config = with_modes(&modes);
        config.app.rendering.navigation = Navigation::Document;
        check(Utf8Path::new("uf.config.js"), &config).unwrap();
    }
}
