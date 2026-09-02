//! `react-native/platform-split`, which wants a platform branch expressed as
//! `.ios.js` / `.android.js` files rather than as a `Platform.OS` test.

use uf_config::UniflowedConfig;

use crate::scan::FileScan;
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_react_native_platform_split(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react-native/platform-split") else {
        return;
    };

    if !scan.facts.mentions_react_native
        || !(scan.file.source.contains("Platform.OS")
            || scan.file.source.contains("Platform.select"))
        || scan.file.path.contains(".ios.")
        || scan.file.path.contains(".android.")
        || scan.file.path.contains(".native.")
    {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        if let Some(at) = code.find("Platform.") {
            push_in_code(
                diagnostics,
                scan,
                "react-native/platform-split",
                severity,
                position,
                at,
                "prefer platform-specific files for React Native platform branches",
            );
        }
    }
}
