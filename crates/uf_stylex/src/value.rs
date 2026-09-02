//! What a declaration's right-hand side may be, and what it becomes in CSS.
//!
//! Only three shapes compile: a string literal, a numeric literal, and a
//! reference to a variable declared by `stylex.defineVars`. Everything else —
//! a call, a concatenation, an identifier, a template with a substitution — is
//! refused, because a value uf cannot resolve now is a value that would have to
//! be resolved by shipping a runtime, which is the cost this pass exists to
//! remove.

use compact_str::CompactString;
use serde::Serialize;

use crate::error::{MAX_VALUE_BYTES, SourcePosition, StyleXError};
use crate::property::is_unitless;

/// A resolved declaration value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "text")]
pub enum StyleValue {
    /// A string literal, without its quotes.
    Text(CompactString),
    /// A numeric literal, exactly as written.
    Number(CompactString),
    /// A reference to a `stylex.defineVars` entry, as the variable's name.
    Variable(CompactString),
}

impl StyleValue {
    /// The CSS text this value becomes for `property`.
    ///
    /// A bare number becomes pixels unless the property is one of the unitless
    /// set, which is the rule that makes `padding: 8` mean what an author
    /// expects. Zero stays unitless whatever the property is.
    pub fn to_css(&self, property: &str) -> CompactString {
        match self {
            Self::Number(number) if !is_unitless(property) && !is_zero(number) => {
                let mut out = CompactString::const_new("");
                out.push_str(number);
                out.push_str("px");
                out
            }
            _ => self.to_css_raw(),
        }
    }

    /// The CSS text this value becomes where no property gives it a unit.
    ///
    /// `stylex.defineVars` declares a custom property, and a custom property is
    /// substituted into whatever property uses it later — so uf cannot know
    /// whether `8` meant pixels, and refuses to guess. An author who wants a
    /// length writes `"8px"`.
    pub fn to_css_raw(&self) -> CompactString {
        match self {
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.clone(),
            Self::Variable(name) => {
                let mut out = CompactString::const_new("var(");
                out.push_str(name);
                out.push(')');
                out
            }
        }
    }
}

/// Whether a numeric literal is zero, in any of the ways it can be written.
fn is_zero(number: &str) -> bool {
    number
        .bytes()
        .all(|byte| matches!(byte, b'0' | b'.' | b'-' | b'+'))
        && number.bytes().any(|byte| byte == b'0')
}

/// Check that a string literal is safe to emit into a stylesheet.
///
/// Three things are refused, each for a concrete failure:
///
/// * `{`, `}` and `;` would let a declaration close its own rule and open
///   another one, so a dependency's `.stylex.js` could write rules for
///   selectors it does not own.
/// * `<` and `>` would let a value close the `<style>` element a sheet is
///   inlined into, which is stored XSS (the CSS-injection half of CWE-79).
/// * `\` and control bytes would let a value smuggle any of the above past
///   this check through a CSS escape sequence or a newline.
pub fn check_value_text(text: &str, at: SourcePosition) -> Result<(), StyleXError> {
    if text.len() > MAX_VALUE_BYTES {
        return Err(StyleXError::ValueTooLong {
            at,
            bytes: text.len(),
            limit: MAX_VALUE_BYTES,
        });
    }
    for byte in text.bytes() {
        let fragment = match byte {
            b'{' => "{",
            b'}' => "}",
            b';' => ";",
            b'<' => "<",
            b'>' => ">",
            b'\\' => "\\",
            0x00..=0x1f | 0x7f => "a control character",
            _ => continue,
        };
        return Err(StyleXError::UnsafeValue {
            at,
            fragment: CompactString::const_new(fragment),
        });
    }
    if text.contains("/*") || text.contains("*/") {
        return Err(StyleXError::UnsafeValue {
            at,
            fragment: CompactString::const_new("a CSS comment"),
        });
    }
    Ok(())
}

/// Check that a numeric literal is a plain decimal number.
///
/// `0x10`, `1_000`, `10n` and `1e400` all lex as numbers and none of them mean
/// anything useful once `px` is appended, so the accepted shape is exactly
/// `-?digits(.digits)?`.
pub fn check_number(text: &str, at: SourcePosition) -> Result<(), StyleXError> {
    let body = text.strip_prefix('-').unwrap_or(text);
    let mut parts = body.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    let malformed = parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction
            .is_some_and(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()));

    if malformed {
        Err(StyleXError::UnsupportedValue {
            at,
            value: CompactString::new(&text[..text.len().min(MAX_VALUE_BYTES)]),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position() -> SourcePosition {
        SourcePosition {
            line: 1,
            column: 1,
            offset: 0,
        }
    }

    #[test]
    fn a_bare_number_becomes_pixels_for_a_length() {
        let value = StyleValue::Number(CompactString::const_new("8"));
        assert_eq!(value.to_css("padding-top"), "8px");
    }

    #[test]
    fn a_bare_number_stays_unitless_for_a_unitless_property() {
        let value = StyleValue::Number(CompactString::const_new("2"));
        assert_eq!(value.to_css("line-height"), "2");
        assert_eq!(value.to_css("z-index"), "2");
    }

    #[test]
    fn zero_stays_unitless_whatever_the_property() {
        let value = StyleValue::Number(CompactString::const_new("0"));
        assert_eq!(value.to_css("padding-top"), "0");
    }

    #[test]
    fn a_variable_becomes_a_var_call() {
        let value = StyleValue::Variable(CompactString::const_new("--xabc"));
        assert_eq!(value.to_css("color"), "var(--xabc)");
    }

    #[test]
    fn a_value_that_closes_its_rule_is_refused() {
        let outcome = check_value_text("red}.evil{color:blue", position());
        assert!(matches!(outcome, Err(StyleXError::UnsafeValue { .. })));
    }

    #[test]
    fn a_value_that_closes_a_style_element_is_refused() {
        let outcome = check_value_text("</style><script>alert(1)", position());
        assert!(matches!(outcome, Err(StyleXError::UnsafeValue { .. })));
    }

    #[test]
    fn a_value_carrying_a_css_escape_is_refused() {
        let outcome = check_value_text("re\\64 ", position());
        assert!(matches!(outcome, Err(StyleXError::UnsafeValue { .. })));
    }

    #[test]
    fn a_value_carrying_a_comment_is_refused() {
        let outcome = check_value_text("red/*", position());
        assert!(matches!(outcome, Err(StyleXError::UnsafeValue { .. })));
    }

    #[test]
    fn a_value_carrying_a_newline_is_refused() {
        let outcome = check_value_text("red\n", position());
        assert!(matches!(outcome, Err(StyleXError::UnsafeValue { .. })));
    }

    #[test]
    fn an_over_long_value_is_refused() {
        let text = "a".repeat(MAX_VALUE_BYTES + 1);
        let outcome = check_value_text(&text, position());
        assert!(matches!(outcome, Err(StyleXError::ValueTooLong { .. })));
    }

    #[test]
    fn a_url_value_is_accepted() {
        assert!(check_value_text("url(/logo.svg)", position()).is_ok());
    }

    #[test]
    fn a_non_ascii_value_is_accepted() {
        assert!(check_value_text("\"日本語\"", position()).is_ok());
    }

    #[test]
    fn a_plain_decimal_is_accepted() {
        assert!(check_number("0", position()).is_ok());
        assert!(check_number("42", position()).is_ok());
        assert!(check_number("1.5", position()).is_ok());
        assert!(check_number("-1.5", position()).is_ok());
    }

    #[test]
    fn an_exotic_numeric_literal_is_refused() {
        assert!(check_number("0x10", position()).is_err());
        assert!(check_number("1_000", position()).is_err());
        assert!(check_number("1e400", position()).is_err());
        assert!(check_number("10n", position()).is_err());
        assert!(check_number("1.", position()).is_err());
        assert!(check_number(".", position()).is_err());
    }
}
