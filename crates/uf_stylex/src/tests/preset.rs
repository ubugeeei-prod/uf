//! The preset uf ships, put through the compiler that has to compile it.
//!
//! `packages/stylex/tokens.stylex.js`, `preset.js` and `theme.js` are ordinary
//! StyleX, so the only way to know they work is to compile them — and a preset
//! that only worked at run time would be a preset the build could not inline,
//! which is the one thing it must never be. The sources are included verbatim,
//! so editing them and breaking a token reference fails here rather than in a
//! browser.
//!
//! What these check is what a project would notice:
//!
//! * every `var(--…)` the base layer emits resolves to a token the token module
//!   actually declares — a mistyped token compiles to CSS that silently does
//!   nothing, and neither the sheet nor a screenshot would say so;
//! * every theme override writes a token that exists, and the two shipped
//!   themes cover the same set;
//! * the colour pairs the preset actually pairs meet WCAG AA, computed from the
//!   compiled stylesheet rather than asserted in a comment.

use crate::compile::CompiledModule;
use crate::condition::StyleCondition;
use crate::sheet::StyleSheet;
use crate::{compile_module, variable_name};

/// The shipped token module.
const TOKENS: &str = include_str!("../../../../packages/stylex/tokens.stylex.js");
/// The shipped base layer.
const PRESET: &str = include_str!("../../../../packages/stylex/preset.js");
/// The shipped themes.
const THEMES: &str = include_str!("../../../../packages/stylex/theme.js");

/// The namespace the preset's tokens are declared under.
const NAMESPACE: &str = "ufTokens";

/// Compile one shipped module, naming it if it does not compile.
fn shipped(name: &str, source: &str) -> CompiledModule {
    compile_module(source)
        .unwrap_or_else(|error| panic!("packages/stylex/{name} must compile, got: {error}"))
}

/// The value the token module gives `key`.
fn token(sheet: &StyleSheet, key: &str) -> String {
    let name = variable_name(NAMESPACE, key);
    sheet
        .variables()
        .find(|variable| variable.name == name)
        .map(|variable| variable.value.to_string())
        .unwrap_or_else(|| panic!("the preset declares no `{key}` token"))
}

/// The value the unconditional dark theme gives `key`.
fn dark(sheet: &StyleSheet, key: &str) -> String {
    let name = variable_name(NAMESPACE, key);
    sheet
        .rules()
        .find(|rule| rule.property == name && rule.condition == StyleCondition::Base)
        .map(|rule| rule.value.to_string())
        .unwrap_or_else(|| panic!("the dark theme overrides no `{key}` token"))
}

/// One channel of a `#rrggbb` colour, linearised the way WCAG defines it.
fn channel(byte: u8) -> f64 {
    let value = f64::from(byte) / 255.0;
    if value <= 0.039_28 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The relative luminance of a `#rrggbb` colour.
fn luminance(colour: &str) -> f64 {
    let digits = colour
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("`{colour}` is not a hex colour"));
    assert_eq!(digits.len(), 6, "`{colour}` is not a six-digit hex colour");
    let byte = |at: usize| {
        u8::from_str_radix(&digits[at..at + 2], 16)
            .unwrap_or_else(|_| panic!("`{colour}` is not hexadecimal"))
    };
    0.2126 * channel(byte(0)) + 0.7152 * channel(byte(2)) + 0.0722 * channel(byte(4))
}

/// The WCAG contrast ratio between two `#rrggbb` colours.
fn contrast(foreground: &str, background: &str) -> f64 {
    let (first, second) = (luminance(foreground), luminance(background));
    let (lighter, darker) = if first > second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// The pairs the preset actually puts on top of each other.
const PAIRS: &[(&str, &str)] = &[
    ("ink", "canvas"),
    ("ink", "surface"),
    ("ink", "sunken"),
    ("ink", "surfaceHover"),
    ("muted", "canvas"),
    ("muted", "surface"),
    ("muted", "sunken"),
    ("accentInk", "accent"),
    ("accentInk", "accentHover"),
    ("accent", "accentSoft"),
    ("dangerInk", "danger"),
    ("dangerInk", "dangerHover"),
    ("danger", "dangerSoft"),
    ("danger", "surface"),
];

#[test]
fn the_token_module_compiles_to_custom_properties() {
    let compiled = shipped("tokens.stylex.js", TOKENS);
    assert!(compiled.changed);
    assert!(
        compiled.sheet.variables().count() >= 40,
        "the preset is a whole token set, not a colour or two"
    );
    assert_eq!(compiled.sheet.len(), 0, "a token module declares no rules");
    assert!(compiled.sheet.to_css().starts_with(":root{"));
}

#[test]
fn no_stylex_call_survives_in_the_shipped_preset() {
    // Every function in `index.js` throws when it is reached, so a call left in
    // the output would take an application down on import rather than render it
    // unstyled. That is the design, and this is the check that the preset never
    // relies on it.
    for (name, source) in [
        ("tokens.stylex.js", TOKENS),
        ("preset.js", PRESET),
        ("theme.js", THEMES),
    ] {
        let compiled = shipped(name, source);
        assert!(compiled.changed, "packages/stylex/{name} declares styles");
        // Compiling the output again finds nothing, which is the same thing as
        // saying no call site is left — and unlike a text search it is not
        // fooled by the calls these modules' own documentation quotes.
        let again = shipped(name, &compiled.code);
        assert!(
            !again.changed,
            "packages/stylex/{name} still calls StyleX after the rewrite"
        );
    }
}

#[test]
fn the_base_layer_compiles_to_rules() {
    let compiled = shipped("preset.js", PRESET);
    assert!(
        compiled.sheet.len() >= 100,
        "the base layer covers surfaces, text, buttons, fields, dialogs, menus, tabs and controls"
    );
    assert!(
        compiled.styles.len() >= 8,
        "one namespace group per subject"
    );
}

#[test]
fn every_token_the_base_layer_reads_is_a_token_the_preset_declares() {
    // A mistyped token — `ufTokens.acccent` — compiles to a `var(--…)` that
    // resolves to nothing, and neither the sheet nor a screenshot of the happy
    // path says so. This is the check that says so.
    let tokens = shipped("tokens.stylex.js", TOKENS);
    let preset = shipped("preset.js", PRESET);
    let declared: Vec<String> = tokens
        .sheet
        .variables()
        .map(|variable| variable.name.to_string())
        .collect();

    for rule in preset.sheet.rules() {
        let Some(name) = rule
            .value
            .strip_prefix("var(")
            .and_then(|rest| rest.strip_suffix(')'))
        else {
            continue;
        };
        assert!(
            declared.iter().any(|token| token == name),
            "`{}: {}` reads a token the preset does not declare",
            rule.property,
            rule.value
        );
    }
}

#[test]
fn every_token_a_shipped_theme_overrides_is_a_token_the_preset_declares() {
    let tokens = shipped("tokens.stylex.js", TOKENS);
    let themes = shipped("theme.js", THEMES);
    let declared: Vec<String> = tokens
        .sheet
        .variables()
        .map(|variable| variable.name.to_string())
        .collect();

    for rule in themes.sheet.rules() {
        assert!(
            declared.iter().any(|token| *token == rule.property),
            "a theme overrides `{}`, which the preset does not declare",
            rule.property
        );
    }
}

#[test]
fn the_two_shipped_themes_cover_the_same_tokens() {
    // One follows the operating system and one is applied by hand; a token
    // covered by only one of them is a colour that changes in one mode and not
    // the other, which is the failure a reader sees as a white flash.
    let themes = shipped("theme.js", THEMES);
    let mut automatic: Vec<&str> = Vec::new();
    let mut explicit: Vec<&str> = Vec::new();
    for rule in themes.sheet.rules() {
        match &rule.condition {
            StyleCondition::Base => explicit.push(rule.property.as_str()),
            StyleCondition::AtRule(at) => {
                assert_eq!(at, "@media (prefers-color-scheme: dark)");
                automatic.push(rule.property.as_str());
            }
            other => panic!("a shipped theme used an unexpected condition: {other:?}"),
        }
    }
    automatic.sort_unstable();
    explicit.sort_unstable();
    assert!(!automatic.is_empty());
    assert_eq!(automatic, explicit);
}

#[test]
fn the_automatic_theme_costs_nothing_in_light_mode() {
    let themes = shipped("theme.js", THEMES);
    let conditional = themes
        .sheet
        .rules()
        .filter(|rule| rule.condition.at_rule().is_some())
        .count();
    assert!(conditional > 0);
    let css = themes.sheet.to_css();
    assert!(css.contains("@media (prefers-color-scheme: dark){"));
}

#[test]
fn the_light_defaults_meet_wcag_aa() {
    let tokens = shipped("tokens.stylex.js", TOKENS);
    for (foreground, background) in PAIRS {
        let ratio = contrast(
            &token(&tokens.sheet, foreground),
            &token(&tokens.sheet, background),
        );
        assert!(
            ratio >= 4.5,
            "`{foreground}` on `{background}` is {ratio:.2}:1, under the 4.5:1 AA floor"
        );
    }
}

#[test]
fn the_dark_theme_meets_wcag_aa() {
    let themes = shipped("theme.js", THEMES);
    for (foreground, background) in PAIRS {
        let ratio = contrast(
            &dark(&themes.sheet, foreground),
            &dark(&themes.sheet, background),
        );
        assert!(
            ratio >= 4.5,
            "in dark, `{foreground}` on `{background}` is {ratio:.2}:1, under the 4.5:1 AA floor"
        );
    }
}

#[test]
fn the_focus_ring_is_visible_against_the_surfaces_it_is_drawn_on() {
    // A focus indicator is a non-text contrast requirement: 3:1, not 4.5:1.
    let tokens = shipped("tokens.stylex.js", TOKENS);
    let themes = shipped("theme.js", THEMES);
    for background in ["surface", "canvas"] {
        let light = contrast(
            &token(&tokens.sheet, "focus"),
            &token(&tokens.sheet, background),
        );
        assert!(
            light >= 3.0,
            "the focus ring is {light:.2}:1 on `{background}`"
        );
        let night = contrast(
            &dark(&themes.sheet, "focus"),
            &dark(&themes.sheet, background),
        );
        assert!(
            night >= 3.0,
            "in dark the focus ring is {night:.2}:1 on `{background}`"
        );
    }
}

#[test]
fn a_shorthand_in_the_base_layer_is_emitted_before_the_longhand_that_narrows_it() {
    // A tab writes `borderWidth: 0` and then `borderBottomWidth: "2px"`. Both
    // are one class selector, so the selected tab's underline exists only
    // because the sheet puts the shorthand first.
    let preset = shipped("preset.js", PRESET);
    let rules: Vec<_> = preset.sheet.rules().collect();
    let at = |property: &str| {
        rules
            .iter()
            .position(|rule| rule.property == property)
            .unwrap_or_else(|| panic!("the tab's {property}"))
    };
    let (shorthand, longhand) = (at("border-width"), at("border-bottom-width"));

    // The priority is the reason, and the position is the consequence. Asserting
    // only the position would let a sheet that ranked both rules the same pass
    // whenever the class-name tie-break happened to fall the right way.
    assert!(rules[shorthand].priority < rules[longhand].priority);
    assert!(shorthand < longhand);
}

#[test]
fn a_hover_state_in_the_base_layer_compiles_to_a_state_map() {
    // The shape `@uniflowed/stylex`'s `props` has to read: a property with
    // states is an object, and a runtime that only understood strings would
    // drop every hover in the preset.
    let preset = shipped("preset.js", PRESET);
    assert!(
        preset.code.contains("\":hover\":\""),
        "the base layer's hover states compile to a state map"
    );
    assert!(
        preset.code.contains("\"default\":\""),
        "and each carries its default beside it"
    );
}

#[test]
fn the_base_layer_and_the_themes_use_one_token_namespace() {
    // `variable_name` hashes the binding the tokens were exported under, so a
    // rename in one module and not the other would produce two disjoint sets of
    // custom properties that both look right.
    let preset = shipped("preset.js", PRESET);
    let canvas = variable_name(NAMESPACE, "canvas");
    assert!(
        preset
            .sheet
            .rules()
            .any(|rule| rule.value == format!("var({canvas})")),
        "the base layer reads the `{NAMESPACE}` namespace"
    );
}
