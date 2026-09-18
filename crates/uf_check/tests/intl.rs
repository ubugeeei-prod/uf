//! Intl option dictionaries, as TypeScript declarations name them.
//!
//! Flow's own libdefs type the constructors through `$` aliases such as
//! `Intl$DateTimeFormatOptions`, while translated `.d.ts` files name the
//! namespace members TypeScript ships: `Intl.DateTimeFormatOptions`,
//! `Intl.ResolvedDateTimeFormatOptions` and
//! `Intl.RelativeTimeFormatOptions`. `libdefs/intl.js` is that spelling
//! bridge. See ubugeeei-prod/uf#1089.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const INTL_OPTIONS: &str = r#"// @flow
export function dateLocale(options: Intl.DateTimeFormatOptions): string {
  const formatter = new Intl.DateTimeFormat("en-US", options);
  const resolved: Intl.ResolvedDateTimeFormatOptions = formatter.resolvedOptions();
  return resolved.locale;
}

export const relative: Intl.RelativeTimeFormatOptions = {
  localeMatcher: "best fit",
  numeric: "auto",
  style: "short",
};

export const resolvedRelative: Intl.ResolvedRelativeTimeFormatOptions = {
  locale: "en-US",
  numberingSystem: "latn",
  numeric: "always",
  style: "long",
};
"#;

const INTL_OPTIONS_MISUSE: &str = r#"// @flow
const badYear: Intl.DateTimeFormatOptions = { year: "wide" };
const badRelative: Intl.RelativeTimeFormatOptions = { numeric: "sometimes" };
const badResolvedDate: Intl.ResolvedDateTimeFormatOptions = { locale: 1, calendar: "gregory", numberingSystem: "latn", hour12: true };
const badResolvedRelative: Intl.ResolvedRelativeTimeFormatOptions = { locale: "en", numberingSystem: "latn", numeric: "always", style: "tiny" };
"#;

const INTL_OPTIONS_INHERITANCE: &str = r#"// @flow
interface DistanceOptions extends Intl.RelativeTimeFormatOptions {
  unit?: "day" | "hour";
}
const options: DistanceOptions = { numeric: "auto", unit: "day" };
const bad: DistanceOptions = { numeric: "sometimes" };
"#;

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &[], &limits()).expect("the checker runs")
}

/// The `(line, code)` of every diagnostic, which is what these tests compare.
fn lines_and_codes(diagnostics: &[TypeDiagnostic]) -> Vec<(u32, &str)> {
    let mut found: Vec<(u32, &str)> = diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.primary.start.line,
                diagnostic.code.unwrap_or("<none>"),
            )
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

#[test]
fn typescript_intl_option_names_check_clean() {
    let diagnostics = check("intl_options.js", INTL_OPTIONS);

    assert!(
        diagnostics.is_empty(),
        "expected Intl option names to check clean, got {:?}",
        lines_and_codes(&diagnostics)
    );
}

#[test]
fn intl_option_members_are_typed() {
    let diagnostics = check("intl_options_misuse.js", INTL_OPTIONS_MISUSE);
    let found = lines_and_codes(&diagnostics);

    let required: &[(u32, &str)] = &[
        (2, "`DateTimeFormatOptions.year` is a closed set"),
        (3, "`RelativeTimeFormatOptions.numeric` is a closed set"),
        (4, "`ResolvedDateTimeFormatOptions.locale` is a string"),
        (
            5,
            "`ResolvedRelativeTimeFormatOptions.style` is a closed set",
        ),
    ];

    let missing: Vec<&str> = required
        .iter()
        .filter(|(line, _)| !found.iter().any(|(reported, _)| reported == line))
        .map(|(_, why)| *why)
        .collect();
    assert!(
        missing.is_empty(),
        "these Intl option misuses were accepted: {missing:?}; the checker reported {found:?}"
    );
}

#[test]
fn relative_time_format_options_are_inheritable() {
    let diagnostics = check("intl_options_inheritance.js", INTL_OPTIONS_INHERITANCE);
    let found = lines_and_codes(&diagnostics);

    assert!(
        found.iter().any(|(line, _)| *line == 6),
        "`DistanceOptions.numeric` should keep RelativeTimeFormatOptions' closed set; got {found:?}"
    );
    assert!(
        found.iter().all(|(line, _)| *line == 6),
        "expected only the bad numeric value to fail; got {found:?}"
    );
}
