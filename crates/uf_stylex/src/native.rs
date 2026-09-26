//! A checked, static subset of StyleX for React Native's `style` prop.

use crate::compile::push_string;
use crate::{Declaration, SourcePosition, StyleCondition, StyleValue, StyleXError, parse_module};
use thiserror::Error;
use uf_infra::LineIndex;

/// A named refusal instead of silently dropping a CSS-only declaration.
#[derive(Debug, Error)]
pub enum NativeStyleError {
    /// A StyleX expression could not be statically extracted.
    #[error(transparent)]
    Source(#[from] StyleXError),
    /// The native renderer cannot express this property, value or condition.
    #[error("StyleX native: unsupported {feature} at {line}:{column}")]
    Unsupported {
        /// Authored feature, including its property or selector name.
        feature: String,
        /// One-based line in the transformed module.
        line: u32,
        /// One-based column in the transformed module.
        column: u32,
    },
}

fn unsupported(feature: impl Into<String>, at: SourcePosition) -> NativeStyleError {
    NativeStyleError::Unsupported {
        feature: feature.into(),
        line: at.line,
        column: at.column,
    }
}

/// Rewrite `create` calls to native objects. The shared runtime's `props`
/// function recognizes the marker and returns a `style`, never a class name.
pub fn compile_native_module(source: &str) -> Result<String, NativeStyleError> {
    let parsed = parse_module(source)?;
    if let Some(call) = parsed.defines.first() {
        return Err(unsupported(
            "defineVars (CSS variables)",
            SourcePosition::new(&LineIndex::new(source), call.start),
        ));
    }
    if let Some(call) = parsed.themes.first() {
        return Err(unsupported(
            "createTheme (CSS variables)",
            SourcePosition::new(&LineIndex::new(source), call.start),
        ));
    }
    let mut code = String::with_capacity(source.len());
    let mut cursor = 0;
    for call in &parsed.creates {
        code.push_str(&source[cursor..call.start]);
        code.push('{');
        for (index, namespace) in call.namespaces.iter().enumerate() {
            if index != 0 {
                code.push(',');
            }
            push_string(&mut code, &namespace.name);
            code.push_str(":{\"$$native\":true");
            for declaration in &namespace.declarations {
                validate(declaration)?;
                code.push(',');
                push_string(&mut code, &declaration.key);
                code.push(':');
                match &declaration.value {
                    StyleValue::Number(number) => code.push_str(number),
                    StyleValue::Text(text) => push_string(&mut code, text),
                    StyleValue::Variable(_) => unreachable!("validate refuses CSS variables"),
                }
            }
            code.push('}');
        }
        code.push('}');
        cursor = call.end;
    }
    code.push_str(&source[cursor..]);
    Ok(code)
}

fn validate(declaration: &Declaration) -> Result<(), NativeStyleError> {
    let Declaration {
        key,
        value,
        condition,
        at,
        ..
    } = declaration;
    if *condition != StyleCondition::Base {
        return Err(unsupported(
            uf_infra::into_string(uf_infra::cstr!("selector {} for {key}", condition.as_str())),
            *at,
        ));
    }
    let choices: &[&str] = match key.as_str() {
        "display" => &["flex", "none"],
        "position" => &["absolute", "relative", "static"],
        "flexDirection" => &["row", "column", "row-reverse", "column-reverse"],
        "flexWrap" => &["wrap", "nowrap", "wrap-reverse"],
        "alignItems" => &["flex-start", "flex-end", "center", "stretch", "baseline"],
        "alignSelf" => &[
            "auto",
            "flex-start",
            "flex-end",
            "center",
            "stretch",
            "baseline",
        ],
        "alignContent" => &[
            "flex-start",
            "flex-end",
            "center",
            "stretch",
            "space-between",
            "space-around",
            "space-evenly",
        ],
        "justifyContent" => &[
            "flex-start",
            "flex-end",
            "center",
            "space-between",
            "space-around",
            "space-evenly",
        ],
        "overflow" => &["visible", "hidden", "scroll"],
        "borderStyle" => &["solid", "dotted", "dashed"],
        "fontStyle" => &["normal", "italic"],
        "fontWeight" => &[
            "normal", "bold", "100", "200", "300", "400", "500", "600", "700", "800", "900",
        ],
        "textAlign" => &["auto", "left", "right", "center", "justify"],
        "textTransform" => &["none", "uppercase", "lowercase", "capitalize"],
        "color" | "backgroundColor" | "borderColor" | "borderTopColor" | "borderRightColor"
        | "borderBottomColor" | "borderLeftColor" | "fontFamily" => &[],
        _ if numeric(key) || dimension(key) => &[],
        _ => {
            return Err(unsupported(
                uf_infra::into_string(uf_infra::cstr!("property {key}")),
                *at,
            ));
        }
    };
    let valid = match value {
        StyleValue::Number(_) => numeric(key) || dimension(key),
        StyleValue::Variable(_) => false,
        StyleValue::Text(text) if text.contains("var(") || text.contains("calc(") => false,
        StyleValue::Text(text) if !choices.is_empty() => choices.contains(&text.as_str()),
        StyleValue::Text(text) if dimension(key) => {
            (text == "auto"
                && matches!(
                    key.as_str(),
                    "width"
                        | "height"
                        | "flexBasis"
                        | "margin"
                        | "marginTop"
                        | "marginRight"
                        | "marginBottom"
                        | "marginLeft"
                ))
                || text
                    .strip_suffix('%')
                    .is_some_and(|n| n.parse::<f64>().is_ok_and(f64::is_finite))
        }
        StyleValue::Text(_) => !numeric(key),
    };
    if !valid {
        return Err(unsupported(
            uf_infra::into_string(uf_infra::cstr!("value {} for {key}", value.to_css_raw())),
            *at,
        ));
    }
    Ok(())
}

fn numeric(key: &str) -> bool {
    matches!(
        key,
        "opacity"
            | "zIndex"
            | "flex"
            | "flexGrow"
            | "flexShrink"
            | "aspectRatio"
            | "fontSize"
            | "lineHeight"
            | "letterSpacing"
            | "borderWidth"
            | "borderTopWidth"
            | "borderRightWidth"
            | "borderBottomWidth"
            | "borderLeftWidth"
            | "borderRadius"
            | "borderTopLeftRadius"
            | "borderTopRightRadius"
            | "borderBottomLeftRadius"
            | "borderBottomRightRadius"
    )
}

fn dimension(key: &str) -> bool {
    matches!(
        key,
        "width"
            | "height"
            | "minWidth"
            | "minHeight"
            | "maxWidth"
            | "maxHeight"
            | "flexBasis"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "padding"
            | "paddingTop"
            | "paddingRight"
            | "paddingBottom"
            | "paddingLeft"
            | "margin"
            | "marginTop"
            | "marginRight"
            | "marginBottom"
            | "marginLeft"
            | "gap"
            | "rowGap"
            | "columnGap"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_the_same_authored_styles_for_both_renderers() {
        let source = "import {stylex as s} from '@uniflowed/stylex'; const styles = s.create({root: {padding: 12, backgroundColor: '#abc', width: '50%'}});";
        let native = compile_native_module(source).unwrap();
        assert!(native.contains(
            "\"$$native\":true,\"padding\":12,\"backgroundColor\":\"#abc\",\"width\":\"50%\""
        ));
        assert_eq!(compile_native_module(&native).unwrap(), native);
        assert!(
            !crate::compile_module(source)
                .unwrap()
                .sheet
                .to_css()
                .is_empty()
        );
    }

    #[test]
    fn compiles_the_typed_native_entry() {
        for import in [
            "import {stylex as s} from '@uniflowed/stylex/native';",
            "import * as s from '@uniflowed/stylex/native';",
        ] {
            let source = uf_infra::into_string(uf_infra::cstr!(
                "{import} const styles = s.create({{root: {{padding: 12}}}});"
            ));
            let native = compile_native_module(&source).unwrap();
            assert!(native.contains("\"$$native\":true,\"padding\":12"));
            assert_eq!(compile_native_module(&native).unwrap(), native);
        }
    }

    #[test]
    fn refuses_each_unsupported_feature_by_name() {
        for (property, name) in [
            ("color: {':hover': 'red'}", ":hover"),
            ("width: {'@media (min-width: 10px)': 5}", "@media"),
            ("display: 'grid'", "grid"),
            ("gridTemplateColumns: '1fr'", "gridTemplateColumns"),
            ("width: '3rem'", "3rem"),
            ("color: 'var(--brand)'", "var(--brand)"),
        ] {
            let error = compile_native_module(&uf_infra::into_string(uf_infra::cstr!(
                "import {{create}} from '@uniflowed/stylex'; create({{root: {{{property}}}}});"
            )))
            .unwrap_err()
            .to_string();
            assert!(error.contains(name), "{error}");
        }
        let error = compile_native_module("import {defineVars} from '@uniflowed/stylex'; const theme = defineVars({brand: 'red'});").unwrap_err().to_string();
        assert!(error.contains("defineVars"));
    }
}
