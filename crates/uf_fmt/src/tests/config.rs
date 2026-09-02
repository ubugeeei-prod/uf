//! The entry point's contract: which `FmtConfig` values are rejected outright,
//! and when the result reports that it changed the source.

use super::*;

/// Flow's printer is fixed at two-space indentation and 80 columns, so a
/// configuration asking for anything else is refused by name rather than
/// accepted and quietly ignored.
#[test]
fn a_configuration_flows_printer_cannot_honour_is_refused_by_name() {
    for (mutate, field, fixed_at) in [
        (
            Box::new(|config: &mut FmtConfig| config.indent_width = 4)
                as Box<dyn Fn(&mut FmtConfig)>,
            "indentWidth",
            u16::from(crate::layout::FLOW_INDENT_WIDTH),
        ),
        (
            Box::new(|config: &mut FmtConfig| config.line_width = 100),
            "lineWidth",
            crate::layout::FLOW_LINE_WIDTH,
        ),
    ] {
        let config = config_with(|config| mutate(config));
        assert_eq!(
            format_source("x;\n", &config),
            Err(FormatError::Unsupported { field, fixed_at }),
        );
    }
}

/// A file that does not parse is refused, not reformatted.
///
/// The token-driven formatter this replaced would happily rewrite whitespace in
/// source it could not understand, which is how it moved a `;` into a tuple
/// return type. An AST printer has no AST to print, and says so.
#[test]
fn source_that_does_not_parse_is_refused() {
    let config = FmtConfig::default();
    let result = format_source("// @flow\nconst = ;\n", &config);

    assert!(
        matches!(result, Err(FormatError::Parse { line: 2, .. })),
        "{result:?}"
    );
}

#[test]
fn changed_reports_whether_the_output_differs() {
    let config = FmtConfig::default();
    assert!(!format_source("const x = 1;\n", &config).unwrap().changed);
    assert!(format_source("const x = 1;  \n", &config).unwrap().changed);
}
