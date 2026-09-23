//! The rules every default style keeps, checked on what the compiler emits.
//!
//! "Default styles" are what a project gets without writing a line of CSS:
//! `packages/stylex/tokens.stylex.js`, `preset.js` and `theme.js`, and every
//! component and example in `registry/ui/`, which `uf ui add` copies into a
//! project and the documentation renders live. Each is compiled here the way
//! a build compiles it, and the rules are read off the stylesheet rather than
//! searched for in the source. That way a comment that names a shadow is not a
//! failure, and a shadow spelled in any way the compiler accepts is one.
//!
//! What they hold, and why, is `packages/stylex/tokens.stylex.js`'s header:
//!
//! * **no shadow and no gradient**: no `box-shadow`, no `text-shadow`, no
//!   `drop-shadow()`, no `*-gradient()`, in a rule or in a token;
//! * **restrained corners**: a `border-radius` is a radius token, `0`, `50%`
//!   for a thing that is a circle, or `inherit`, and the radius tokens other
//!   than the pill are 6px or less;
//! * **quiet motion**: a transition or animation lasts a duration token (both
//!   180ms or less), names its properties rather than `all`, moves no layout
//!   property, does not scale a thing up from nothing, and is `0s` under
//!   `prefers-reduced-motion: reduce`; the easing never overshoots.

use std::fs;
use std::path::{Path, PathBuf};

use crate::compile::{CompiledModule, compile_module};
use crate::condition::StyleCondition;
use crate::sheet::StyleRule;
use crate::variable_name;
use uf_infra::FxHashMap;

/// The namespace the preset's tokens are declared under.
const NAMESPACE: &str = "ufTokens";

/// The radius tokens, and the largest a restrained one may be. The pill is
/// exempt: it is for things that are round by what they are.
const RADII: &[(&str, u32)] = &[("radiusSm", 6), ("radiusMd", 6), ("radiusLg", 6)];

/// This checkout.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the repository")
}

/// Every default style, by path relative to the repository, and its source.
fn defaults() -> Vec<(String, String)> {
    let root = repository();
    let mut files: Vec<String> = [
        "packages/stylex/tokens.stylex.js",
        "packages/stylex/preset.js",
        "packages/stylex/theme.js",
    ]
    .iter()
    .map(|path| (*path).to_owned())
    .collect();
    let registry = root.join("registry/ui");
    let mut components: Vec<String> = fs::read_dir(&registry)
        .unwrap_or_else(|error| panic!("{} cannot be listed: {error}", registry.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|extension| extension == "js")
                && !name.ends_with(".test.js")
        })
        .map(|name| format!("registry/ui/{name}"))
        .collect();
    components.sort();
    assert!(
        components.len() > 50,
        "registry/ui/ listed almost nothing, so this is not checking anything"
    );
    files.extend(components);
    files
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(root.join(&path))
                .unwrap_or_else(|error| panic!("{path} cannot be read: {error}"));
            (path, source)
        })
        .collect()
}

/// `source`, compiled, naming the file if it does not compile.
fn compiled(path: &str, source: &str) -> CompiledModule {
    compile_module(source).unwrap_or_else(|error| panic!("{path} must compile, got: {error}"))
}

/// Whether a CSS value draws a gradient or a shadow.
fn paints_depth(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("gradient(") || value.contains("drop-shadow(")
}

#[test]
fn no_default_style_casts_a_shadow_or_paints_a_gradient() {
    let mut found = Vec::new();
    for (path, source) in defaults() {
        let module = compiled(&path, &source);
        for rule in module.sheet.rules() {
            let property = rule.property.as_str();
            if property == "box-shadow" || property == "text-shadow" || paints_depth(&rule.value) {
                found.push(format!("{path}: `{property}: {}`", rule.value));
            }
        }
        for variable in module.sheet.variables() {
            if paints_depth(&variable.value) {
                found.push(format!("{path}: a token holds `{}`", variable.value));
            }
        }
    }
    assert!(
        found.is_empty(),
        "default styles separate surfaces with a border and a background, never a shadow or a \
         gradient:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn no_token_is_a_shadow() {
    let tokens = repository().join("packages/stylex/tokens.stylex.js");
    let source = fs::read_to_string(&tokens).expect("the token module");
    let module = compiled("packages/stylex/tokens.stylex.js", &source);
    for key in ["shadowCard", "shadowPanel"] {
        let name = variable_name(NAMESPACE, key);
        assert!(
            module
                .sheet
                .variables()
                .all(|variable| variable.name != name),
            "`{key}` is back; a surface is set apart by `border`, not by elevation"
        );
    }
}

/// The pixel size of a `NNpx` value.
fn pixels(value: &str) -> Option<u32> {
    value.strip_suffix("px")?.parse().ok()
}

#[test]
fn the_radius_tokens_are_restrained() {
    let source = fs::read_to_string(repository().join("packages/stylex/tokens.stylex.js"))
        .expect("the token module");
    let module = compiled("packages/stylex/tokens.stylex.js", &source);
    for (key, largest) in RADII {
        let name = variable_name(NAMESPACE, key);
        let value = module
            .sheet
            .variables()
            .find(|variable| variable.name == name)
            .map(|variable| variable.value.to_string())
            .unwrap_or_else(|| panic!("the preset declares no `{key}`"));
        let size = pixels(&value).unwrap_or_else(|| panic!("`{key}` is `{value}`, not pixels"));
        assert!(
            size <= *largest,
            "`{key}` is {size}px; the default corners stop at {largest}px"
        );
    }
    let gone = variable_name(NAMESPACE, "radiusXl");
    assert!(
        module
            .sheet
            .variables()
            .all(|variable| variable.name != gone),
        "`radiusXl` is back; the largest surface uses `radiusLg`"
    );
}

#[test]
fn every_default_corner_is_a_radius_token() {
    let allowed: Vec<String> = RADII
        .iter()
        .map(|(key, _)| *key)
        .chain(["radiusPill"])
        .map(|key| format!("var({})", variable_name(NAMESPACE, key)))
        .chain(["0", "0px", "50%", "inherit"].map(ToOwned::to_owned))
        .collect();
    let mut found = Vec::new();
    for (path, source) in defaults() {
        let module = compiled(&path, &source);
        for rule in module.sheet.rules() {
            if rule.property.contains("radius") && !allowed.iter().any(|ok| *ok == rule.value) {
                found.push(format!("{path}: `{}: {}`", rule.property, rule.value));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a default corner is `radiusSm`, `radiusMd`, `radiusLg`, `radiusPill` for a thing that \
         is round, or `50%` for a circle:\n  {}",
        found.join("\n  ")
    );
}

/// The at-rule a reduced-motion override is written under.
const REDUCED_MOTION: &str = "@media (prefers-reduced-motion: reduce)";

/// The longest a default transition may take, in milliseconds.
const LONGEST_MOTION_MS: u32 = 180;

/// Properties whose animation lays the page out again on every frame.
const LAYOUT: &[&str] = &[
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "top",
    "right",
    "bottom",
    "left",
    "inset",
    "margin",
    "padding",
    "flex-basis",
    "grid-template-columns",
    "grid-template-rows",
];

/// Where a default style animates a layout property on purpose, and why.
const LAYOUT_MOTION_ALLOWED: &[(&str, &str)] = &[(
    "registry/ui/drawer.js",
    "a drawer is a fixed overlay on its own edge, and moving between snap points resizes \
     it; nothing else on the page moves",
)];

/// The milliseconds a duration token declares.
fn token_milliseconds(module: &CompiledModule, key: &str) -> u32 {
    let name = variable_name(NAMESPACE, key);
    let value = module
        .sheet
        .variables()
        .find(|variable| variable.name == name)
        .map(|variable| variable.value.to_string())
        .unwrap_or_else(|| panic!("the preset declares no `{key}`"));
    value
        .strip_suffix("ms")
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("`{key}` is `{value}`, not milliseconds"))
}

#[test]
fn the_motion_tokens_are_short_and_never_overshoot() {
    let source = fs::read_to_string(repository().join("packages/stylex/tokens.stylex.js"))
        .expect("the token module");
    let module = compiled("packages/stylex/tokens.stylex.js", &source);
    let fast = token_milliseconds(&module, "durationFast");
    let base = token_milliseconds(&module, "durationBase");
    assert!(
        (80..=base).contains(&fast) && base <= LONGEST_MOTION_MS,
        "durations are {fast}ms and {base}ms; the defaults move in {LONGEST_MOTION_MS}ms or less"
    );

    let name = variable_name(NAMESPACE, "easing");
    let easing = module
        .sheet
        .variables()
        .find(|variable| variable.name == name)
        .map(|variable| variable.value.to_string())
        .expect("the preset declares an easing");
    let points: Vec<f64> = easing
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("the easing is `{easing}`, not a cubic-bezier()"))
        .split(',')
        .map(|number| number.trim().parse().expect("a number"))
        .collect();
    assert_eq!(points.len(), 4, "`{easing}` has four control values");
    // The y values are the second and fourth; outside 0..1 the curve
    // overshoots its end or pulls back before its start, which is a bounce.
    assert!(
        [points[1], points[3]]
            .iter()
            .all(|y| (0.0..=1.0).contains(y)),
        "the easing `{easing}` overshoots, and the defaults do not bounce"
    );
}

#[test]
fn every_default_transition_is_quiet_and_honours_reduced_motion() {
    let tokens = [
        format!("var({})", variable_name(NAMESPACE, "durationFast")),
        format!("var({})", variable_name(NAMESPACE, "durationBase")),
    ];
    let mut found = Vec::new();
    let mut checked = 0usize;
    for (path, source) in defaults() {
        let module = compiled(&path, &source);
        let value_of: FxHashMap<&str, &StyleRule> = module
            .sheet
            .rules()
            .map(|rule| (rule.class.as_str(), rule))
            .collect();
        for style in &module.styles {
            let values = |key: &str| -> Vec<(&StyleCondition, String)> {
                style
                    .property(key)
                    .map(|property| {
                        property
                            .classes
                            .iter()
                            .filter_map(|class| {
                                value_of
                                    .get(class.class.as_str())
                                    .map(|rule| (&class.condition, rule.value.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let at = format!("{path} `{}`", style.name);
            for duration in ["transitionDuration", "animationDuration"] {
                let entries = values(duration);
                if entries.is_empty() {
                    continue;
                }
                checked += 1;
                for (_, value) in &entries {
                    let still = value == "0s" || value == "0ms";
                    if !still && !tokens.contains(value) {
                        found.push(format!(
                            "{at}: {duration} is `{value}`, not a duration token"
                        ));
                    }
                }
                let reduced = entries.iter().any(|(condition, value)| {
                    condition.at_rule() == Some(REDUCED_MOTION) && (value == "0s" || value == "0ms")
                });
                if !reduced {
                    found.push(format!(
                        "{at}: {duration} is not `0s` under {REDUCED_MOTION}"
                    ));
                }
            }
            if !values("animationName").is_empty() || !values("animation").is_empty() {
                found.push(format!("{at}: a default style runs a keyframe animation"));
            }
            for (_, value) in values("transitionProperty") {
                let moved: Vec<&str> = value.split(',').map(str::trim).collect();
                if moved.contains(&"all") {
                    found.push(format!("{at}: transitions `all`"));
                }
                let allowed = LAYOUT_MOTION_ALLOWED.iter().any(|(file, _)| *file == path);
                for property in &moved {
                    if !allowed && LAYOUT.iter().any(|layout| property.starts_with(layout)) {
                        found.push(format!("{at}: animates `{property}`, a layout property"));
                    }
                }
                if moved.contains(&"transform") {
                    for (_, transform) in values("transform") {
                        if transform.contains("scale(") {
                            found.push(format!(
                                "{at}: animates `transform: {transform}`, which grows a thing \
                                 from nothing"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        checked >= 10,
        "only {checked} transitions were found, so this is not checking the registry"
    );
    assert!(
        found.is_empty(),
        "default motion is a duration token, specific properties, no bounce or pop, and nothing \
         under reduced motion:\n  {}",
        found.join("\n  ")
    );
}
