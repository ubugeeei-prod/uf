//! The preset uf ships, put through the compiler that has to compile it.
//!
//! `npm/stylex/tokens.stylex.js`, `preset.js` and `theme.js` are ordinary
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
//! * every theme override writes a token that exists, and the three shipped
//!   themes cover the same set;
//! * the colour pairs the preset actually pairs meet WCAG AA, computed from the
//!   compiled stylesheet rather than asserted in a comment.

use crate::compile::CompiledModule;
use crate::condition::StyleCondition;
use crate::sheet::StyleSheet;
use crate::{compile_module, variable_name};

/// The shipped token module.
const TOKENS: &str = include_str!("../../../../npm/stylex/tokens.stylex.js");
/// The shipped base layer.
const PRESET: &str = include_str!("../../../../npm/stylex/preset.js");
/// The shipped themes.
const THEMES: &str = include_str!("../../../../npm/stylex/theme.js");

/// The namespace the preset's tokens are declared under.
const NAMESPACE: &str = "ufTokens";

/// Compile one shipped module, naming it if it does not compile.
fn shipped(name: &str, source: &str) -> CompiledModule {
    compile_module(source)
        .unwrap_or_else(|error| panic!("npm/stylex/{name} must compile, got: {error}"))
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

/// The value theme `index` gives `key`, read off that theme's own class.
///
/// `theme.js` declares them in source order: automatic, dark, then contrast.
/// The class is hashed over the value and the condition, so two unconditional
/// themes that disagree are two rules, and the first `Base` rule on the sheet
/// is not "the dark theme".
fn theme_value(module: &CompiledModule, index: usize, key: &str) -> String {
    let name = variable_name(NAMESPACE, key);
    let theme = module
        .themes
        .get(index)
        .unwrap_or_else(|| panic!("theme.js has no theme at {index}"));
    let class = theme
        .property(&name)
        .and_then(|property| property.classes.first())
        .map(|class| class.class.clone())
        .unwrap_or_else(|| panic!("theme {index} overrides no `{key}` token"));
    module
        .sheet
        .rules()
        .find(|rule| rule.class == class)
        .map(|rule| rule.value.to_string())
        .unwrap_or_else(|| panic!("theme {index} has no rule for `{key}`"))
}

/// Theme index of `ufDarkTheme` in `theme.js`.
const DARK: usize = 1;
/// Theme index of `ufContrastTheme` in `theme.js`.
const CONTRAST: usize = 2;

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
    ("muted", "surfaceHover"),
    ("accentInk", "accent"),
    ("accentInk", "accentHover"),
    ("accent", "accentSoft"),
    // The accent is also text: a link, the current tab, a selected option's
    // check, a hovered day.
    ("accent", "surface"),
    ("accent", "canvas"),
    ("accent", "surfaceHover"),
    ("ink", "accentSoft"),
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
        assert!(compiled.changed, "npm/stylex/{name} declares styles");
        // Compiling the output again finds nothing, which is the same thing as
        // saying no call site is left — and unlike a text search it is not
        // fooled by the calls these modules' own documentation quotes.
        let again = shipped(name, &compiled.code);
        assert!(
            !again.changed,
            "npm/stylex/{name} still calls StyleX after the rewrite"
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

/// The custom properties one shipped theme overrides, in a stable order.
fn theme_keys(module: &CompiledModule, index: usize) -> Vec<String> {
    let theme = module
        .themes
        .get(index)
        .unwrap_or_else(|| panic!("theme.js has no theme at {index}"));
    let mut keys: Vec<String> = theme
        .properties
        .iter()
        .map(|property| property.key.to_string())
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn the_shipped_themes_cover_the_same_tokens() {
    // One follows the operating system, one is dark by hand, and one is high
    // contrast by hand. A token covered by only one of them is a colour that
    // changes in one mode and not the other, which is the failure a reader
    // sees as a white flash.
    let themes = shipped("theme.js", THEMES);
    assert_eq!(themes.themes.len(), 3, "automatic, dark, and contrast");
    let automatic = theme_keys(&themes, 0);
    assert!(!automatic.is_empty());
    assert_eq!(automatic, theme_keys(&themes, DARK));
    assert_eq!(automatic, theme_keys(&themes, CONTRAST));

    // The automatic theme is the media query and nothing else. The other two
    // are unconditional, which is what makes the index above meaningful: a
    // fourth `createTheme` inserted above contrast would move CONTRAST.
    for property in &themes.themes[0].properties {
        assert!(
            property
                .classes
                .iter()
                .all(|class| class.condition.at_rule().is_some()),
            "the automatic theme wrote an unconditional class"
        );
    }
    for index in [DARK, CONTRAST] {
        for property in &themes.themes[index].properties {
            assert!(
                property
                    .classes
                    .iter()
                    .all(|class| class.condition == StyleCondition::Base),
                "theme {index} wrote a conditional class"
            );
        }
    }
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
            &theme_value(&themes, DARK, foreground),
            &theme_value(&themes, DARK, background),
        );
        assert!(
            ratio >= 4.5,
            "in dark, `{foreground}` on `{background}` is {ratio:.2}:1, under the 4.5:1 AA floor"
        );
    }
}

#[test]
fn the_contrast_theme_meets_wcag_aaa() {
    // The other two themes are held to AA, 4.5:1. This one is the theme a
    // reader asks for when that is not enough, so its text pairs are held to
    // 7:1, which is AAA, and its focus ring is held to the same 7:1 rather
    // than the 3:1 non-text floor.
    let themes = shipped("theme.js", THEMES);
    for (foreground, background) in PAIRS {
        let ratio = contrast(
            &theme_value(&themes, CONTRAST, foreground),
            &theme_value(&themes, CONTRAST, background),
        );
        assert!(
            ratio >= 7.0,
            "in contrast, `{foreground}` on `{background}` is {ratio:.2}:1, under the 7:1 floor"
        );
    }
    for background in ["surface", "canvas"] {
        let ratio = contrast(
            &theme_value(&themes, CONTRAST, "focus"),
            &theme_value(&themes, CONTRAST, background),
        );
        assert!(
            ratio >= 7.0,
            "in contrast the focus ring is {ratio:.2}:1 on `{background}`"
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
            &theme_value(&themes, DARK, "focus"),
            &theme_value(&themes, DARK, background),
        );
        assert!(
            night >= 3.0,
            "in dark the focus ring is {night:.2}:1 on `{background}`"
        );
        let contrast_ring = contrast(
            &theme_value(&themes, CONTRAST, "focus"),
            &theme_value(&themes, CONTRAST, background),
        );
        assert!(
            contrast_ring >= 3.0,
            "in contrast the focus ring is {contrast_ring:.2}:1 on `{background}`"
        );
    }
}

/// The red, green and blue bytes of a `#rrggbb` colour.
fn bytes(colour: &str) -> [u8; 3] {
    let digits = colour
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("`{colour}` is not a hex colour"));
    let byte = |at: usize| {
        u8::from_str_radix(&digits[at..at + 2], 16)
            .unwrap_or_else(|_| panic!("`{colour}` is not hexadecimal"))
    };
    [byte(0), byte(2), byte(4)]
}

/// The hue of a `#rrggbb` colour in degrees, `None` for a grey.
fn hue(colour: &str) -> Option<f64> {
    let [r, g, b] = bytes(colour).map(|byte| f64::from(byte) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    if chroma == 0.0 {
        return None;
    }
    let sector = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    Some(sector * 60.0)
}

/// The tokens that are the interface's greys.
const NEUTRALS: &[&str] = &[
    "canvas",
    "sunken",
    "surface",
    "surfaceHover",
    "border",
    "ink",
    "muted",
];

#[test]
fn the_neutrals_are_grey_rather_than_tinted() {
    // A slate or navy grey is what makes an interface read as a generated one:
    // everything carries the same faint blue. The defaults' greys stay within a
    // few steps of neutral in every channel, in light, in dark, and in the
    // high-contrast theme.
    let tokens = shipped("tokens.stylex.js", TOKENS);
    let themes = shipped("theme.js", THEMES);
    for key in NEUTRALS {
        for (mode, colour) in [
            ("light", token(&tokens.sheet, key)),
            ("dark", theme_value(&themes, DARK, key)),
            ("contrast", theme_value(&themes, CONTRAST, key)),
        ] {
            let [r, g, b] = bytes(&colour);
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(
                spread <= 8,
                "in {mode}, `{key}` is {colour}, a tinted grey ({spread} steps between channels)"
            );
        }
    }
}

#[test]
fn the_accent_is_not_violet() {
    // Indigo through magenta (roughly 235° to 330°) is the default of every
    // generated interface; the preset's one accent stays out of that band in
    // every shipped theme, and so does the focus ring.
    let tokens = shipped("tokens.stylex.js", TOKENS);
    let themes = shipped("theme.js", THEMES);
    for key in ["accent", "accentHover", "accentSoft", "focus"] {
        for (mode, colour) in [
            ("light", token(&tokens.sheet, key)),
            ("dark", theme_value(&themes, DARK, key)),
            ("contrast", theme_value(&themes, CONTRAST, key)),
        ] {
            if let Some(degrees) = hue(&colour) {
                assert!(
                    !(235.0..=330.0).contains(&degrees),
                    "in {mode}, `{key}` is {colour}, a violet at {degrees:.0}°"
                );
            }
        }
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
