//! Manager and lockfile identifiers, and the config preferences they map from.

use super::*;

#[test]
fn package_manager_identifiers_round_trip() {
    for manager in PackageManager::ALL {
        assert_eq!(PackageManager::parse(manager.as_str()), Some(manager));
        assert_eq!(manager.to_string(), manager.as_str());
    }
    assert_eq!(
        PackageManager::parse("yarn"),
        Some(PackageManager::Yarn(YarnEdition::Berry))
    );
    assert_eq!(PackageManager::parse("deno"), None);
}

#[test]
fn package_manager_serializes_as_a_stable_string() {
    let json = serde_json::to_string(&PackageManager::Yarn(YarnEdition::Classic)).unwrap();

    assert_eq!(json, r#""yarn-classic""#);
    assert_eq!(
        serde_json::from_str::<PackageManager>(&json).unwrap(),
        PackageManager::Yarn(YarnEdition::Classic)
    );
    assert!(serde_json::from_str::<PackageManager>(r#""deno""#).is_err());
}

#[test]
fn every_manager_declares_the_lockfile_it_writes() {
    assert_eq!(PackageManager::Uf.lockfile(), Lockfile::UfLock);
    assert_eq!(PackageManager::Npm.lockfile(), Lockfile::PackageLock);
    assert_eq!(PackageManager::Pnpm.lockfile(), Lockfile::PnpmLock);
    assert_eq!(PackageManager::Bun.lockfile(), Lockfile::BunLock);
    assert_eq!(
        PackageManager::Yarn(YarnEdition::Berry).lockfile(),
        Lockfile::YarnLock
    );
}

#[test]
fn every_lockfile_maps_to_a_manager_and_a_file_name() {
    for lockfile in Lockfile::ALL {
        assert!(!lockfile.file_name().is_empty());
        assert_eq!(lockfile.to_string(), lockfile.file_name());
    }
    assert_eq!(Lockfile::UfLock.manager(), PackageManager::Uf);
    assert_eq!(Lockfile::BunLockb.manager(), PackageManager::Bun);
    assert_eq!(Lockfile::NpmShrinkwrap.manager(), PackageManager::Npm);
}

#[test]
fn config_preferences_map_onto_managers() {
    assert_eq!(
        PackageManager::from_preference(PackageManagerPreference::Auto),
        None
    );
    assert_eq!(
        PackageManager::from_preference(PackageManagerPreference::Yarn),
        Some(PackageManager::Yarn(YarnEdition::Berry))
    );
    assert_eq!(
        PackageManager::from_preference(PackageManagerPreference::YarnClassic),
        Some(PackageManager::Yarn(YarnEdition::Classic))
    );
    assert_eq!(
        PackageManager::from_preference(PackageManagerPreference::Bun),
        Some(PackageManager::Bun)
    );
}
