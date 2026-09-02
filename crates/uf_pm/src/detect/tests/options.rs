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
