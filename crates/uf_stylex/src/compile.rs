//! Turning one module's StyleX calls into rules, class maps, and new source.
//!
//! The rewrite is the point of the pass: after it, `stylex.create` is a plain
//! object literal of class names, so nothing about the styles is computed in
//! the browser. What survives to runtime is [`props`](crate::props), which is a
//! merge over the objects written here and nothing more.

use compact_str::CompactString;
use serde::Serialize;

use crate::condition::StyleCondition;
use crate::error::StyleXError;
use crate::parse::{Declaration, Namespace, ParsedModule, ThemeCall, parse_module};
use crate::sheet::{RulePriority, StyleRule, StyleSheet};

/// Key the compiled object carries so a runtime can tell it apart from a plain
/// object handed to `stylex.props` by mistake.
pub const COMPILED_MARKER: &str = "$$css";

/// One class name, and the state it applies in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalClass {
    /// The state the class applies in.
    pub condition: StyleCondition,
    /// The generated class name.
    pub class: CompactString,
    /// Where the class's rule sits in the sheet.
    pub priority: RulePriority,
}

/// One authored property of one namespace, with every state it was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledProperty {
    /// The key exactly as authored, which is what `props` merges on.
    pub key: CompactString,
    /// The classes, ordered by where their rules sit in the sheet.
    pub classes: Vec<ConditionalClass>,
}

/// One namespace of a `stylex.create` call, compiled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledStyle {
    /// The namespace's name, as authored.
    pub name: CompactString,
    /// Its properties, in the order they were first written.
    pub properties: Vec<CompiledProperty>,
}

impl CompiledStyle {
    /// The compiled entry for `key`, if the namespace sets it.
    pub fn property(&self, key: &str) -> Option<&CompiledProperty> {
        self.properties.iter().find(|entry| entry.key == key)
    }
}

/// Everything one module produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledModule {
    /// The module's source after the rewrite.
    pub code: String,
    /// Whether the rewrite changed anything.
    pub changed: bool,
    /// The rules and variables the module contributed.
    pub sheet: StyleSheet,
    /// The namespaces, for `props` and for tests.
    pub styles: Vec<CompiledStyle>,
    /// The `stylex.createTheme` results, one per call.
    ///
    /// Separate from [`CompiledModule::styles`] because merging a theme with a
    /// style is meaningless: a theme's properties are custom properties, and it
    /// is applied to an ancestor so that the rules *below* it resolve
    /// differently. Merging the two lists would type-check and mean nothing.
    pub themes: Vec<CompiledStyle>,
}

/// Compile one module: extract, name, order, and rewrite.
///
/// Running this over its own output is a no-op — the rewritten module has no
/// `stylex.create` call left to find — which is what makes the pass safe to run
/// again on a warm cache or a hot reload.
pub fn compile_module(source: &str) -> Result<CompiledModule, StyleXError> {
    let parsed = parse_module(source)?;
    Ok(assemble(source, &parsed))
}

/// Build the output for an already-parsed module.
fn assemble(source: &str, parsed: &ParsedModule) -> CompiledModule {
    let mut sheet = StyleSheet::new();
    let mut styles = Vec::new();
    let mut themes = Vec::new();
    // `(start, end, replacement)`, collected in source order.
    let mut splices: Vec<(usize, usize, String)> = Vec::new();

    for call in &parsed.defines {
        let mut literal = String::from("{");
        for (index, variable) in call.variables.iter().enumerate() {
            if index > 0 {
                literal.push(',');
            }
            let value = variable.value.to_css_raw();
            sheet.insert_variable(variable.name.clone(), value);
            push_string(&mut literal, &variable.key);
            literal.push(':');
            let mut reference = CompactString::const_new("var(");
            reference.push_str(&variable.name);
            reference.push(')');
            push_string(&mut literal, &reference);
        }
        literal.push('}');
        splices.push((call.start, call.end, literal));
    }

    for call in &parsed.creates {
        let compiled: Vec<CompiledStyle> = call
            .namespaces
            .iter()
            .map(|namespace| compile_namespace(namespace, &mut sheet))
            .collect();
        splices.push((call.start, call.end, literal_for(&compiled)));
        styles.extend(compiled);
    }

    for call in &parsed.themes {
        let compiled = compile_theme(call, &mut sheet);
        let mut literal = String::new();
        push_namespace_object(&mut literal, &compiled);
        splices.push((call.start, call.end, literal));
        themes.push(compiled);
    }

    splices.sort_by_key(|(start, _, _)| *start);
    let changed = !splices.is_empty();
    let mut code = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for (start, end, replacement) in splices {
        if start < cursor {
            continue;
        }
        code.push_str(&source[cursor..start]);
        code.push_str(&replacement);
        cursor = end;
    }
    code.push_str(&source[cursor.min(source.len())..]);

    CompiledModule {
        code,
        changed,
        sheet,
        styles,
        themes,
    }
}

/// Compile one namespace, adding its rules to the sheet.
fn compile_namespace(namespace: &Namespace, sheet: &mut StyleSheet) -> CompiledStyle {
    CompiledStyle {
        name: namespace.name.clone(),
        properties: compile_declarations(&namespace.name, &namespace.declarations, sheet),
    }
}

/// Compile one `stylex.createTheme` call, adding its rules to the sheet.
///
/// The hash namespace is the *variables* namespace being themed rather than the
/// binding the theme was assigned to. Two modules that override the same token
/// with the same value therefore produce the same class and one rule — which is
/// the same atomic-CSS promise every other rule makes, applied to the one place
/// a project is most likely to repeat itself.
fn compile_theme(call: &ThemeCall, sheet: &mut StyleSheet) -> CompiledStyle {
    CompiledStyle {
        name: call.namespace.clone(),
        properties: compile_declarations(&call.namespace, &call.overrides, sheet),
    }
}

/// Name, order and emit one list of declarations.
///
/// A key written twice keeps the last value, which is what the JavaScript
/// object literal it was read from would have done. The rule for the dead
/// declaration is never emitted, so a shadowed value costs no bytes.
fn compile_declarations(
    hashed_as: &str,
    declarations: &[Declaration],
    sheet: &mut StyleSheet,
) -> Vec<CompiledProperty> {
    let mut building: Vec<Building> = Vec::new();
    for declaration in declarations {
        let value = declaration.value.to_css(&declaration.property);
        let class = crate::class::class_name(
            hashed_as,
            &declaration.property,
            &declaration.condition,
            &value,
        );
        let rule = StyleRule::new(
            class.clone(),
            declaration.property.clone(),
            value,
            declaration.condition.clone(),
        );
        let entry = ConditionalClass {
            priority: rule.priority,
            condition: declaration.condition.clone(),
            class,
        };

        match building
            .iter_mut()
            .find(|held| held.property.key == declaration.key)
        {
            Some(held) => replace(held, entry, rule),
            None => building.push(Building {
                property: CompiledProperty {
                    key: declaration.key.clone(),
                    classes: vec![entry],
                },
                rules: vec![rule],
            }),
        }
    }

    let mut properties = Vec::with_capacity(building.len());
    for mut held in building {
        // Only the declarations that survived shadowing reach the sheet, so a
        // value written twice costs the bytes of one rule.
        for rule in held.rules {
            sheet.insert(rule);
        }
        held.property.classes.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(left.class.cmp(&right.class))
        });
        properties.push(held.property);
    }

    properties
}

/// A property under construction, with the rule for each of its states held
/// parallel to the class so shadowing drops both together.
struct Building {
    property: CompiledProperty,
    rules: Vec<StyleRule>,
}

/// Put `entry` into `held`, replacing any class and rule for the same state.
fn replace(held: &mut Building, entry: ConditionalClass, rule: StyleRule) {
    match held
        .property
        .classes
        .iter()
        .position(|existing| existing.condition == entry.condition)
    {
        Some(at) => {
            held.property.classes[at] = entry;
            held.rules[at] = rule;
        }
        None => {
            held.property.classes.push(entry);
            held.rules.push(rule);
        }
    }
}

/// The object literal a `stylex.create` call is replaced with.
fn literal_for(styles: &[CompiledStyle]) -> String {
    let mut out = String::from("{");
    for (index, style) in styles.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_string(&mut out, &style.name);
        out.push(':');
        push_namespace_object(&mut out, style);
    }
    out.push('}');
    out
}

/// One compiled namespace, as the marked object the runtime merges.
///
/// A `stylex.create` call becomes a map of these, one per namespace; a
/// `stylex.createTheme` call becomes exactly one, because a theme has no
/// namespaces to name.
fn push_namespace_object(out: &mut String, style: &CompiledStyle) {
    out.push('{');
    push_string(out, COMPILED_MARKER);
    out.push_str(":true");
    for property in &style.properties {
        out.push(',');
        push_string(out, &property.key);
        out.push(':');
        push_classes(out, property);
    }
    out.push('}');
}

/// One property's value: a bare class when it has one state, a map otherwise.
///
/// The map's entries are written in sheet order, and that order is part of the
/// contract with `@uniflowed/stylex`: the runtime keeps *every* class of the
/// property that wins the merge, in the order it reads them, so a `:hover`
/// value survives beside its base value and lands in the class attribute after
/// it. A runtime that kept only one of them would silently drop every
/// conditional style in an application.
fn push_classes(out: &mut String, property: &CompiledProperty) {
    match property.classes.as_slice() {
        [only] if only.condition == StyleCondition::Base => push_string(out, &only.class),
        classes => {
            out.push('{');
            for (index, entry) in classes.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let key = match &entry.condition {
                    StyleCondition::Base => "default",
                    other => other.as_str(),
                };
                push_string(out, key);
                out.push(':');
                push_string(out, &entry.class);
            }
            out.push('}');
        }
    }
}

/// Append a JavaScript string literal.
///
/// Keys and values are already restricted to characters that need no escaping,
/// so this escapes defensively rather than because anything is expected to get
/// here: generated code that can be broken by its own input is how a build step
/// turns into an injection point.
fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other => out.push(other),
        }
    }
    out.push('"');
}
