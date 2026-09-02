//! The entry point's contract: which `FmtConfig` values are rejected outright,
//! and when the result reports that it changed the source.

use super::*;

#[test]
fn a_zero_indent_width_is_rejected() {
    let config = config_with(|config| {
        config.indent_width = 0;
    });
    assert_eq!(
        format_source("x;\n", &config),
        Err(FormatError::InvalidIndentWidth)
    );
}

#[test]
fn an_oversized_indent_width_is_rejected() {
    let config = config_with(|config| {
        config.indent_width = 200;
    });
    assert_eq!(
        format_source("x;\n", &config),
        Err(FormatError::InvalidIndentWidth)
    );
    let allowed = config_with(|config| {
        config.indent_width = MAX_INDENT_WIDTH;
    });
    assert!(format_source("x;\n", &allowed).is_ok());
}

#[test]
fn changed_reports_whether_the_output_differs() {
    let config = FmtConfig::default();
    assert!(!format_source("const x = 1;\n", &config).unwrap().changed);
    assert!(format_source("const x = 1;  \n", &config).unwrap().changed);
}
