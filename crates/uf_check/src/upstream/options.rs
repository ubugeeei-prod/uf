//! How `uf` configures Flow.
//!
//! `uf` has no `.flowconfig`: the toolchain decides, and every project gets the
//! same modern dialect. That is the whole point of a unified toolchain, so
//! these are constants rather than configuration. They match what
//! `flow_dot_js_wasm` runs with, which is the configuration Flow's own
//! try-it-online uses, plus `uf`'s limits.

use std::path::PathBuf;
use std::sync::Arc;

use dupe::Dupe;
use flow_common::options::{Options, ReactRule, ReactRuntime};
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::severity::Severity;
use flow_parser_utils::file_sig::FileSigOptions;
use flow_type_sig::type_sig_options::TypeSigOptions;

use crate::CheckLimits;

/// The React rules Flow enforces during inference.
///
/// These are the rules React itself requires to hold for the compiler to be
/// sound, not style preferences, so they are on for every `uf` project.
const REACT_RULES: [ReactRule; 4] = [
    ReactRule::ValidateRefAccessDuringRender,
    ReactRule::DeepReadOnlyProps,
    ReactRule::DeepReadOnlyHookReturns,
    ReactRule::RulesOfHooks,
];

/// How many tokens of a file's header are scanned for a docblock.
const MAX_HEADER_TOKENS: i32 = 10;

/// Build the checker options for one run.
///
/// Flow lints are left entirely off: `uf_lint` owns lint rules and reports them
/// with its own ids, so turning Flow's on here would double-report and give the
/// same finding two different names.
pub(super) fn options(limits: &CheckLimits) -> Options {
    Options {
        all: true,
        component_syntax: true,
        enable_pattern_matching: true,
        enable_records: true,
        enums: true,
        hook_compatibility: true,
        lint_severities: LintSettings::<Severity>::empty_severities(),
        max_header_tokens: MAX_HEADER_TOKENS,
        react_rules: Arc::from(REACT_RULES),
        react_runtime: ReactRuntime::Automatic,
        recursion_limit: recursion_limit(limits.recursion_limit),
        root: Arc::new(PathBuf::new()),
        strip_root: true,
        ts_syntax: true,
        tslib_syntax: true,
        ts_utility_syntax: true,
        type_expansion_recursion_limit: recursion_limit(limits.type_expansion_recursion_limit),
        ..Default::default()
    }
}

/// Flow's recursion limits are `i32`; [`CheckLimits`] keeps them unsigned
/// because a negative limit is not a thing. Saturate rather than wrap, so a
/// caller who asks for a limit past `i32::MAX` gets the largest limit Flow can
/// express instead of a negative one that disables the guard entirely.
fn recursion_limit(limit: u32) -> i32 {
    i32::try_from(limit).unwrap_or(i32::MAX)
}

/// Signature options for the builtin library definitions.
///
/// `for_builtins` and `is_lib_file` are what separate this from a source file:
/// library definitions may declare globals and are never munged.
pub(super) fn builtin_sig_options() -> TypeSigOptions {
    TypeSigOptions {
        munge: false,
        facebook_key_mirror: false,
        facebook_fbt: None,
        enable_custom_error: false,
        enable_enums: true,
        enable_component_syntax: true,
        component_syntax_enabled_in_config: true,
        enable_ts_syntax: true,
        enable_ts_utility_syntax: true,
        hook_compatibility: true,
        enable_records: true,
        enable_relay_integration: false,
        relay_integration_module_prefix: None,
        for_builtins: true,
        locs_to_dirtify: Vec::new(),
        is_ts_file: false,
        is_dts_file: false,
        tslib_syntax: true,
        is_lib_file: true,
    }
}

/// File signature options for one parsed file.
pub(super) fn file_sig_options(options: &Options, is_lib_file: bool) -> FileSigOptions {
    FileSigOptions {
        enable_enums: options.enums,
        enable_jest_integration: options.enable_jest_integration,
        enable_relay_integration: options.enable_relay_integration,
        explicit_available_platforms: None,
        file_options: options.file_options.dupe(),
        haste_module_ref_prefix: options.haste_module_ref_prefix.dupe(),
        project_options: options.projects_options.dupe(),
        relay_integration_module_prefix: options.relay_integration_module_prefix.dupe(),
        is_lib_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_flow_syntax_is_on() {
        let options = options(&CheckLimits::default());

        assert!(options.component_syntax);
        assert!(options.enums);
        assert!(options.enable_pattern_matching);
        assert!(options.hook_compatibility);
    }

    #[test]
    fn flow_lints_are_left_to_uf_lint() {
        use flow_lint_settings::lints::LintKind;

        let options = options(&CheckLimits::default());

        assert_eq!(*options.lint_severities.get_default(), Severity::Off);
        assert!(!options.lint_severities.is_enabled(LintKind::UnclearType));
        assert!(!options.lint_severities.is_enabled(LintKind::UntypedImport));
    }

    #[test]
    fn the_recursion_limit_follows_the_configured_limits() {
        let limits = CheckLimits {
            recursion_limit: 42,
            ..CheckLimits::default()
        };

        assert_eq!(options(&limits).recursion_limit, 42);
    }

    #[test]
    fn an_out_of_range_recursion_limit_saturates_instead_of_going_negative() {
        assert_eq!(recursion_limit(u32::MAX), i32::MAX);
        assert_eq!(recursion_limit(0), 0);
    }

    #[test]
    fn builtin_signature_options_declare_a_library_file() {
        let sig_options = builtin_sig_options();

        assert!(sig_options.for_builtins);
        assert!(sig_options.is_lib_file);
        assert!(!sig_options.munge);
    }

    #[test]
    fn source_file_signature_options_are_not_library_options() {
        let options = options(&CheckLimits::default());

        assert!(!file_sig_options(&options, false).is_lib_file);
        assert!(file_sig_options(&options, true).is_lib_file);
    }
}
