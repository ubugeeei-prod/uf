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
//! * **crafted motion, present and restrained**: every transition lasts one of
//!   the three duration tokens (100ms to 320ms, in order) on one of the three
//!   easing tokens (none of which overshoots), names its properties rather than
//!   `all`, moves no layout property, and never scales a thing below 0.9 or
//!   above 1 — no pop from nothing, no bounce past the end. Under
//!   `prefers-reduced-motion: reduce` it either stops (`0s`) or only fades and
//!   changes colour. And the other half, which #1414 lost: every overlay has an
//!   enter transition from a `@starting-style` and an exit on
//!   `[data-state=closed]` that is shorter than the entrance and accelerates,
//!   and every control that changes state in a way the eye should follow
//!   transitions it.

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

/// The at-rule an enter transition starts from.
///
/// uf's StyleX has no `@keyframes`, and does not need them for an entrance: a
/// `@starting-style` value is the style an element is taken to have had before
/// it was first rendered, so a transition runs from it to the resting value
/// the moment the element is inserted.
const STARTING_STYLE: &str = "@starting-style";

/// The duration tokens, shortest first.
const DURATIONS: &[&str] = &["durationFast", "durationBase", "durationSlow"];

/// The easing tokens.
const EASINGS: &[&str] = &["easing", "easingEnter", "easingExit"];

/// The shortest a duration token may be: below about 100ms a transition is not
/// seen at all, which is how the defaults came to look motionless.
const SHORTEST_MOTION_MS: u32 = 100;

/// The longest a duration token may be: past about 300ms an interface starts
/// to feel like it is performing rather than answering.
const LONGEST_MOTION_MS: u32 = 320;

/// What may still transition under reduced motion: nothing that travels, grows
/// or is drawn, only what fades or changes colour.
const STILL: &[&str] = &[
    "none",
    "opacity",
    "color",
    "background-color",
    "border-color",
    "outline-color",
    "text-decoration-color",
    "fill",
    "stroke",
];

/// The smallest and largest a default `scale()` may be. Below the first a
/// thing grows out of nothing, which reads as a pop; above the second it has
/// passed its own size, which is a bounce.
const SCALE_RANGE: (f64, f64) = (0.9, 1.0);

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
const LAYOUT_MOTION_ALLOWED: &[(&str, &str)] = &[
    (
        "registry/ui/drawer.js",
        "a drawer is a fixed overlay on its own edge, and moving between snap points resizes \
         it; nothing else on the page moves",
    ),
    (
        "registry/ui/accordion.js",
        "a panel opening is the content below it making room, which is the one thing the \
         reader has to see happen; it moves to a measured height, not to `auto`, and stops \
         under reduced motion",
    ),
    (
        "registry/ui/collapsible.js",
        "the same as an accordion's panel, one section at a time",
    ),
];

/// What deserves motion and must have it: `(file, style, property)` where the
/// style transitions `property`, and — for an overlay, marked `true` — enters
/// and leaves. It fades in from a `@starting-style` opacity, starts `property`
/// from a `@starting-style` too, and arrives on `easingEnter`; and it has a
/// value for both under [`CLOSED`], which `@uniflowed/ui` writes on a closing
/// part while it keeps it on the page, reached in a shorter duration token than
/// the entrance's on `easingExit`.
///
/// A style is found by its name in the file, so a name listed here has to be
/// unique there; the preset's recipes name their namespaces for that reason.
///
/// This is the half of the rules that keeps the defaults from going quiet
/// again. #1414 made motion so restrained it was nearly absent, and a rule
/// that only limits motion cannot notice that.
const MUST_MOVE: &[(&str, &str, &str, bool)] = &[
    ("registry/ui/dialog.js", "overlay", "opacity", true),
    ("registry/ui/dialog.js", "panel", "transform", true),
    ("registry/ui/alert-dialog.js", "overlay", "opacity", true),
    ("registry/ui/alert-dialog.js", "panel", "transform", true),
    ("registry/ui/sheet.js", "overlay", "opacity", true),
    ("registry/ui/sheet.js", "panel", "transform", true),
    ("registry/ui/drawer.js", "overlay", "opacity", true),
    ("registry/ui/drawer.js", "panel", "transform", true),
    ("registry/ui/popover.js", "content", "transform", true),
    ("registry/ui/menu.js", "content", "transform", true),
    ("registry/ui/select.js", "list", "transform", true),
    ("registry/ui/combobox.js", "list", "transform", true),
    ("registry/ui/hover-card.js", "content", "transform", true),
    ("registry/ui/tooltip.js", "content", "transform", true),
    (
        "registry/ui/navigation-menu.js",
        "content",
        "transform",
        true,
    ),
    ("registry/ui/date-picker.js", "content", "transform", true),
    (
        "registry/ui/date-range-picker.js",
        "panel",
        "transform",
        true,
    ),
    ("registry/ui/toast.js", "toast", "transform", true),
    ("registry/ui/button.js", "base", "transform", false),
    ("registry/ui/button.js", "base", "outline-width", false),
    ("registry/ui/switch.js", "thumb", "transform", false),
    (
        "registry/ui/checkbox.js",
        "tick",
        "stroke-dashoffset",
        false,
    ),
    ("registry/ui/radio-group.js", "dot", "opacity", false),
    ("registry/ui/tabs.js", "tab", "border-color", false),
    ("registry/ui/tabs.js", "indicator", "transform", false),
    ("registry/ui/accordion.js", "content", "height", false),
    ("registry/ui/collapsible.js", "content", "height", false),
    ("registry/ui/progress.js", "fill", "transform", false),
    ("registry/ui/accordion.js", "chevron", "transform", false),
    ("registry/ui/collapsible.js", "chevron", "transform", false),
    ("registry/ui/select.js", "chevron", "transform", false),
    (
        "registry/ui/navigation-menu.js",
        "arrow",
        "transform",
        false,
    ),
    // `@uniflowed/stylex/preset`, which a project reaches without the
    // registry: the same overlays enter and the same controls move.
    ("packages/stylex/preset.js", "backdrop", "opacity", true),
    ("packages/stylex/preset.js", "dialog", "transform", true),
    ("packages/stylex/preset.js", "menu", "transform", true),
    (
        "packages/stylex/preset.js",
        "item",
        "background-color",
        false,
    ),
    ("packages/stylex/preset.js", "tab", "border-color", false),
    ("packages/stylex/preset.js", "tab", "outline-width", false),
];

/// The attribute `@uniflowed/ui` writes on a part that is closing and still on
/// the page, which is what an exit transitions from.
const CLOSED: &str = "[data-state=closed]";

/// The value a token declares.
fn token(module: &CompiledModule, key: &str) -> String {
    let name = variable_name(NAMESPACE, key);
    module
        .sheet
        .variables()
        .find(|variable| variable.name == name)
        .map(|variable| variable.value.to_string())
        .unwrap_or_else(|| panic!("the preset declares no `{key}`"))
}

/// The milliseconds a duration token declares.
fn token_milliseconds(module: &CompiledModule, key: &str) -> u32 {
    let value = token(module, key);
    value
        .strip_suffix("ms")
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("`{key}` is `{value}`, not milliseconds"))
}

/// The four control values of a `cubic-bezier()` token.
fn bezier(module: &CompiledModule, key: &str) -> [f64; 4] {
    let easing = token(module, key);
    let points: Vec<f64> = easing
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("`{key}` is `{easing}`, not a cubic-bezier()"))
        .split(',')
        .map(|number| number.trim().parse().expect("a number"))
        .collect();
    points
        .try_into()
        .unwrap_or_else(|_| panic!("`{key}` is `{easing}`, which needs four control values"))
}

/// The token module, compiled.
fn tokens() -> CompiledModule {
    let source = fs::read_to_string(repository().join("packages/stylex/tokens.stylex.js"))
        .expect("the token module");
    compiled("packages/stylex/tokens.stylex.js", &source)
}

#[test]
fn the_motion_tokens_are_ordered_visible_and_never_overshoot() {
    let module = tokens();
    let durations: Vec<u32> = DURATIONS
        .iter()
        .map(|key| token_milliseconds(&module, key))
        .collect();
    assert!(
        durations.windows(2).all(|pair| pair[0] < pair[1]),
        "{DURATIONS:?} are {durations:?}ms; each step is longer than the one before"
    );
    assert!(
        durations[0] >= SHORTEST_MOTION_MS && durations[2] <= LONGEST_MOTION_MS,
        "durations are {durations:?}ms; a default moves in {SHORTEST_MOTION_MS}ms to \
         {LONGEST_MOTION_MS}ms, visible and never slow"
    );

    for key in EASINGS {
        let [_, y1, _, y2] = bezier(&module, key);
        // Outside 0..1 the curve passes its end or pulls back before its start,
        // which is a bounce.
        assert!(
            (0.0..=1.0).contains(&y1) && (0.0..=1.0).contains(&y2),
            "`{key}` overshoots, and the defaults do not bounce"
        );
    }
    // An enter decelerates: both control points sit on or above the diagonal,
    // so the curve is concave — fast away, then settling. An exit accelerates:
    // both sit on or below it. A linear curve is neither.
    let [x1, y1, x2, y2] = bezier(&module, "easingEnter");
    assert!(
        y1 >= x1 && y2 >= x2 && (y1 > x1 || y2 > x2),
        "`easingEnter` must decelerate: it starts fast and settles"
    );
    let [x1, y1, x2, y2] = bezier(&module, "easingExit");
    assert!(
        y1 <= x1 && y2 <= x2 && (y1 < x1 || y2 < x2),
        "`easingExit` must accelerate: it starts slowly and gets out of the way"
    );
}

/// Every `scale()` argument in a transform value.
fn scales(value: &str) -> Vec<f64> {
    value
        .match_indices("scale(")
        .filter_map(|(at, call)| {
            let rest = &value[at + call.len()..];
            rest.split(')').next()
        })
        .flat_map(|arguments| arguments.split(',').map(str::trim))
        .filter_map(|number| number.parse().ok())
        .collect()
}

#[test]
fn every_default_transition_is_crafted_and_honours_reduced_motion() {
    let durations: Vec<String> = DURATIONS
        .iter()
        .map(|key| format!("var({})", variable_name(NAMESPACE, key)))
        .collect();
    let easings: Vec<String> = EASINGS
        .iter()
        .map(|key| format!("var({})", variable_name(NAMESPACE, key)))
        .collect();
    let still = |value: &str| value == "0s" || value == "0ms";
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
            let under_reduced = |entries: &[(&StyleCondition, String)]| -> Option<String> {
                entries
                    .iter()
                    .find(|(condition, _)| condition.at_rule() == Some(REDUCED_MOTION))
                    .or_else(|| {
                        entries
                            .iter()
                            .find(|(condition, _)| **condition == StyleCondition::Base)
                    })
                    .map(|(_, value)| value.clone())
            };
            let at = format!("{path} `{}`", style.name);

            if !values("animationName").is_empty() || !values("animation").is_empty() {
                found.push(format!(
                    "{at}: runs a keyframe animation; enter from a `@starting-style` instead"
                ));
            }
            for (_, transform) in values("transform") {
                for scale in scales(&transform) {
                    if !(SCALE_RANGE.0..=SCALE_RANGE.1).contains(&scale) {
                        found.push(format!(
                            "{at}: `transform: {transform}` scales to {scale}, outside \
                             {SCALE_RANGE:?}: a pop from nothing or a bounce past the end"
                        ));
                    }
                }
            }

            let duration = values("transitionDuration");
            let property = values("transitionProperty");
            let timing = values("transitionTimingFunction");
            if duration.is_empty() {
                if !property.is_empty() {
                    found.push(format!("{at}: names a transition but gives it no duration"));
                }
                continue;
            }
            checked += 1;
            if property.is_empty() {
                found.push(format!(
                    "{at}: has a transition duration and no property list, which transitions `all`"
                ));
            }
            for (_, value) in &duration {
                if !still(value) && !durations.contains(value) {
                    found.push(format!(
                        "{at}: transitionDuration is `{value}`, not a duration token"
                    ));
                }
            }
            if timing.is_empty() {
                found.push(format!(
                    "{at}: has no easing token, so it moves on the browser's `ease`"
                ));
            }
            for (_, value) in &timing {
                if !easings.contains(value) {
                    found.push(format!(
                        "{at}: transitionTimingFunction is `{value}`, not an easing token"
                    ));
                }
            }
            let allowed = LAYOUT_MOTION_ALLOWED.iter().any(|(file, _)| *file == path);
            for (_, value) in &property {
                let moved: Vec<&str> = value.split(',').map(str::trim).collect();
                if moved.contains(&"all") {
                    found.push(format!("{at}: transitions `all`"));
                }
                for moved in &moved {
                    if !allowed && LAYOUT.iter().any(|layout| moved.starts_with(layout)) {
                        found.push(format!("{at}: animates `{moved}`, a layout property"));
                    }
                }
            }
            let stops = under_reduced(&duration).is_some_and(|value| still(&value));
            let only_fades = under_reduced(&property).is_some_and(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .all(|moved| STILL.contains(&moved))
            });
            if !stops && !only_fades {
                found.push(format!(
                    "{at}: under {REDUCED_MOTION} it still moves `{}`; stop it (`0s`) or \
                     transition only opacity and colour",
                    under_reduced(&property).unwrap_or_default()
                ));
            }
        }
    }
    assert!(
        checked >= 30,
        "only {checked} transitions were found, so this is not checking the registry"
    );
    assert!(
        found.is_empty(),
        "default motion is a duration and an easing token, named properties, no pop or bounce, \
         and only a fade under reduced motion:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn what_deserves_motion_has_it() {
    let mut found = Vec::new();
    let sources: FxHashMap<String, String> = defaults().into_iter().collect();
    for (path, name, moved, enters) in MUST_MOVE {
        let source = sources
            .get(*path)
            .unwrap_or_else(|| panic!("{path} is not a default style"));
        let module = compiled(path, source);
        let value_of: FxHashMap<&str, &StyleRule> = module
            .sheet
            .rules()
            .map(|rule| (rule.class.as_str(), rule))
            .collect();
        let named: Vec<_> = module
            .styles
            .iter()
            .filter(|style| style.name == *name)
            .collect();
        let style = match named.as_slice() {
            [style] => *style,
            [] => {
                found.push(format!("{path} has no `{name}` style"));
                continue;
            }
            _ => {
                found.push(format!(
                    "{path} has {} styles named `{name}`, so this cannot tell which must move",
                    named.len()
                ));
                continue;
            }
        };
        let rules = |key: &str| -> Vec<(&StyleCondition, &str)> {
            style
                .property(key)
                .map(|property| {
                    property
                        .classes
                        .iter()
                        .filter_map(|class| {
                            value_of
                                .get(class.class.as_str())
                                .map(|rule| (&class.condition, rule.value.as_str()))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let transitions = rules("transitionProperty")
            .iter()
            .any(|(condition, value)| {
                **condition == StyleCondition::Base
                    && value.split(',').map(str::trim).any(|each| each == *moved)
            });
        let lasts = rules("transitionDuration")
            .iter()
            .any(|(condition, value)| **condition == StyleCondition::Base && *value != "0s");
        if !transitions || !lasts {
            found.push(format!("{path} `{name}` does not transition `{moved}`"));
        }
        if *enters {
            let mut keys = vec!["opacity"];
            if *moved != "opacity" {
                keys.push("transform");
            }
            for key in keys {
                let starts = rules(key)
                    .iter()
                    .any(|(condition, _)| condition.at_rule() == Some(STARTING_STYLE));
                if !starts {
                    found.push(format!(
                        "{path} `{name}` has no `{STARTING_STYLE}` for `{key}`, so it appears \
                         rather than entering"
                    ));
                }
            }
            let enter = format!("var({})", variable_name(NAMESPACE, "easingEnter"));
            let decelerates = rules("transitionTimingFunction")
                .iter()
                .any(|(condition, value)| **condition == StyleCondition::Base && *value == enter);
            if !decelerates {
                found.push(format!(
                    "{path} `{name}` does not arrive on `easingEnter`, the curve an entrance \
                     settles on"
                ));
            }

            // And it leaves: the same properties have somewhere to go while
            // `@uniflowed/ui` holds the closing part on the page.
            let closing = |condition: &StyleCondition| matches!(condition, StyleCondition::PseudoClass(text) if text.contains(CLOSED));
            for key in ["opacity", "transform"] {
                if key == "transform" && *moved == "opacity" {
                    continue;
                }
                if !rules(key).iter().any(|(condition, _)| closing(condition)) {
                    found.push(format!(
                        "{path} `{name}` has no `{CLOSED}` value for `{key}`, so it vanishes \
                         rather than leaving"
                    ));
                }
            }
            let exit = format!("var({})", variable_name(NAMESPACE, "easingExit"));
            let accelerates = rules("transitionTimingFunction")
                .iter()
                .any(|(condition, value)| closing(condition) && *value == exit);
            if !accelerates {
                found.push(format!(
                    "{path} `{name}` does not leave on `easingExit`, the curve that gets out of \
                     the way"
                ));
            }
            // Shorter than it came: the step down the duration tokens.
            let step = |value: &str| {
                DURATIONS
                    .iter()
                    .position(|key| value == format!("var({})", variable_name(NAMESPACE, key)))
            };
            let durations = rules("transitionDuration");
            let entering = durations
                .iter()
                .find(|(condition, _)| **condition == StyleCondition::Base)
                .and_then(|(_, value)| step(value));
            let leaving = durations
                .iter()
                .find(|(condition, _)| closing(condition))
                .and_then(|(_, value)| step(value));
            if !matches!((entering, leaving), (Some(enters), Some(leaves)) if leaves < enters) {
                found.push(format!(
                    "{path} `{name}` does not leave in a shorter duration token than it enters \
                     in; an exit is quicker than an entrance"
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "every overlay enters and leaves, and every control that changes state moves; quiet \
         is not absent:\n  {}",
        found.join("\n  ")
    );
}
