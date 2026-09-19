//! One install-time script policy for package management and lint.

/// Hooks that package managers may run during install, including git packing.
/// Named commands such as Expo's `ios` and `android` are not install hooks.
pub const INSTALL_LIFECYCLE_SCRIPTS: &[&str] = &[
    "preinstall",
    "install",
    "postinstall",
    "prepublish",
    "preprepare",
    "prepare",
    "postprepare",
    "prepack",
    "postpack",
    "dependencies",
    "pnpm:devPreinstall",
];
