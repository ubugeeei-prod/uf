//! Detection knobs and their documented defaults.

use super::*;

#[test]
fn detection_options_default_to_the_documented_bounds() {
    let options = DetectionOptions::new();

    assert_eq!(options.max_ancestors, MAX_ANCESTOR_DEPTH);
    assert_eq!(options.max_manifest_bytes, MAX_MANIFEST_BYTES);
    assert_eq!(options.boundary, None);
    assert_eq!(options.config_override, None);
}

#[test]
fn config_options_pick_up_the_package_manager_override() {
    let mut config = UniflowedConfig::default();
    assert_eq!(DetectionOptions::from_config(&config).config_override, None);

    config.pm.package_manager = PackageManagerPreference::Pnpm;
    assert_eq!(
        DetectionOptions::from_config(&config).config_override,
        Some(PackageManager::Pnpm)
    );
}

/// The top-level `packageManager` is the override, and a Yarn spec's major
/// decides the edition. ubugeeei-prod/uf#940.
#[test]
fn config_options_prefer_the_top_level_package_manager() {
    let mut config = UniflowedConfig::default();
    config.package_manager = Some(uf_config::Written::new("pnpm@12.1.0"));
    assert_eq!(
        DetectionOptions::from_config(&config).config_override,
        Some(PackageManager::Pnpm)
    );

    for (spec, edition) in [
        ("yarn@1", YarnEdition::Classic),
        ("yarn@1.22.22", YarnEdition::Classic),
        ("yarn@4", YarnEdition::Berry),
        ("yarn", YarnEdition::Berry),
    ] {
        config.package_manager = Some(uf_config::Written::new(spec));
        assert_eq!(
            DetectionOptions::from_config(&config).config_override,
            Some(PackageManager::Yarn(edition)),
            "{spec}"
        );
    }

    // A spec that did not parse overrides nothing: the loader has already
    // refused the config it came from, and this is not a second loader.
    config.package_manager = Some(uf_config::Written::new("pnpm@^9"));
    assert_eq!(DetectionOptions::from_config(&config).config_override, None);
}
