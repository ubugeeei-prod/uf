//! Literals: strings requoted to the configured style, numbers normalised
//! the way Prettier normalises them, regular expressions with sorted flags,
//! and template literals with their text kept byte for byte.

use uf_config::QuoteStyle;
use uf_flow::Loc;
use uf_flow::ast::{self, expression};

use super::Printer;
use crate::doc::{Doc, LINE_SUFFIX_BOUNDARY, SOFTLINE};
use crate::flow::node::{Expression, NodeRef};

/// The quote character preferred by the configuration.
pub fn preferred_quote(quote: QuoteStyle) -> char {
    match quote {
        QuoteStyle::Double => '"',
        QuoteStyle::Single => '\'',
    }
}

/// The quote to use for `content`: the preferred one unless the content
/// holds more of it than of the other, in which case switching saves
/// escapes. Prettier's `getPreferredQuote`.
pub fn choose_quote(content: &str, preferred: char) -> char {
    let alternate = if preferred == '"' { '\'' } else { '"' };
    let mut preferred_count = 0usize;
    let mut alternate_count = 0usize;
    for ch in content.chars() {
        if ch == preferred {
            preferred_count += 1;
        } else if ch == alternate {
            alternate_count += 1;
        }
    }
    if preferred_count > alternate_count {
        alternate
    } else {
        preferred
    }
}

/// Re-escape `content` for `quote`: escape the enclosing quote, unescape
/// the other one, and drop escapes that do nothing. Prettier's
/// `makeString`.
pub fn make_string(content: &str, quote: char) -> String {
    let other = if quote == '"' { '\'' } else { '"' };
    let mut out = String::with_capacity(content.len() + 2);
    out.push(quote);
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(escaped) if escaped == other => out.push(escaped),
                Some(escaped) if escaped == quote => {
                    out.push('\\');
                    out.push(escaped);
                }
                Some(escaped) => {
                    if is_meaningful_escape(escaped) {
                        out.push('\\');
                    }
                    out.push(escaped);
                }
                None => out.push('\\'),
            }
        } else if ch == quote {
            out.push('\\');
            out.push(ch);
        } else {
            out.push(ch);
        }
    }
    out.push(quote);
    out
}

/// Whether `\` before `ch` changes what the string means. Prettier keeps
/// exactly these and drops the rest.
fn is_meaningful_escape(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '\r' | '"' | '\'' | '0'
            ..='7' | '\\' | 'b' | 'f' | 'n' | 'r' | 't' | 'u' | 'v' | 'x' | '\u{2028}' | '\u{2029}'
    )
}

/// Print a string literal from its raw source spelling.
pub fn print_string(raw: &str, quote: QuoteStyle) -> String {
    if raw.len() < 2 {
        return raw.to_string();
    }
    let content = &raw[1..raw.len() - 1];
    let chosen = choose_quote(content, preferred_quote(quote));
    make_string(content, chosen)
}

/// Normalise a numeric literal: lowercase, no `+` or leading zeros in an
/// exponent, no trailing zeros or dot, a leading `0` before a bare dot.
/// Prettier's `printNumber`.
pub fn print_number(raw: &str) -> String {
    let mut value = raw.to_ascii_lowercase();

    // Hex, octal and binary literals only get the lowercasing.
    if value.starts_with("0x") || value.starts_with("0o") || value.starts_with("0b") {
        return value;
    }

    // Remove unnecessary plus and zeroes from scientific notation.
    if let Some(e) = value.find('e') {
        let (mantissa, exponent) = value.split_at(e);
        let exponent = &exponent[1..];
        let (sign, digits) = match exponent.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("", exponent.strip_prefix('+').unwrap_or(exponent)),
        };
        let digits = digits.trim_start_matches('0');
        value = if digits.is_empty() {
            // Remove unnecessary scientific notation (1x).
            mantissa.to_string()
        } else {
            format!("{mantissa}e{sign}{digits}")
        };
    }

    // Make sure numbers always start with a digit.
    if value.starts_with('.') {
        value.insert(0, '0');
    }

    // Remove extraneous trailing decimal zeroes — `(\.\d+?)0+(?=e|$)`,
    // which keeps one digit after the dot so `1.0` stays `1.0` — then a
    // trailing dot.
    let (mantissa, exponent) = match value.find('e') {
        Some(e) => (value[..e].to_string(), value[e..].to_string()),
        None => (value.clone(), String::new()),
    };
    let mantissa = match mantissa.find('.') {
        Some(dot) => {
            let (integer, fraction) = mantissa.split_at(dot + 1);
            let kept = fraction.trim_end_matches('0');
            let fraction = if kept.is_empty() && !fraction.is_empty() {
                &fraction[..1]
            } else {
                kept
            };
            if fraction.is_empty() {
                integer.strip_suffix('.').unwrap_or(integer).to_string()
            } else {
                format!("{integer}{fraction}")
            }
        }
        None => mantissa,
    };
    format!("{mantissa}{exponent}")
}

/// A bigint literal: lowercased.
pub fn print_bigint(raw: &str) -> String {
    raw.to_ascii_lowercase()
}

/// A regular expression with its flags sorted.
pub fn print_regex(pattern: &str, flags: &str) -> String {
    let mut flags: Vec<char> = flags.chars().collect();
    flags.sort_unstable();
    let flags: String = flags.into_iter().collect();
    format!("/{pattern}/{flags}")
}

/// Whether `name` is a plain identifier, so a quoted key can lose its
/// quotes.
pub fn is_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first == '$' || first == '_' || first.is_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '$' || ch == '_' || ch.is_alphanumeric())
}

impl<'a> Printer<'a> {
    /// A string literal in the configured quotes.
    pub fn print_string_literal(&self, literal: &'a ast::StringLiteral<Loc>) -> Doc<'a> {
        let printed = print_string(&literal.raw, self.options.quote);
        let doc = self.text(&printed);
        if printed.contains('\n') {
            let owned: &'a str = match doc.kind {
                crate::doc::DocKind::Text(text) => text,
                _ => "",
            };
            return self.replace_end_of_line(owned);
        }
        doc
    }

    /// A string literal node standing on its own, with its comments.
    pub fn print_string_node(
        &mut self,
        loc: &'a Loc,
        literal: &'a ast::StringLiteral<Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::StringLiteral(loc, literal), |p| {
            p.print_string_literal(literal)
        })
    }

    /// A number literal, normalised.
    pub fn print_number_literal(&self, literal: &'a ast::NumberLiteral<Loc>) -> Doc<'a> {
        self.text(&print_number(&literal.raw))
    }

    /// A bigint literal, lowercased.
    pub fn print_bigint_literal(&self, literal: &'a ast::BigIntLiteral<Loc>) -> Doc<'a> {
        self.text(&print_bigint(&literal.raw))
    }

    /// A boolean literal.
    pub fn print_boolean_literal(&self, literal: &'a ast::BooleanLiteral<Loc>) -> Doc<'a> {
        if literal.value {
            self.s("true")
        } else {
            self.s("false")
        }
    }

    /// A template literal: text verbatim, each `${}` printed flat unless the
    /// source already broke inside it.
    pub fn print_template_literal(
        &mut self,
        template: &'a expression::TemplateLiteral<Loc, Loc>,
    ) -> Doc<'a> {
        let mut parts = vec![&LINE_SUFFIX_BOUNDARY as Doc<'a>, self.s("`")];
        // The alignment of a `${}` is that of the last line of text before
        // it, carried forward across text that holds no newline.
        let mut indent_size = 0;
        for (index, quasi) in template.quasis.iter().enumerate() {
            parts.push(self.replace_end_of_line(&quasi.value.raw));
            if let Some(expression) = template.expressions.get(index) {
                if quasi.value.raw.contains('\n') {
                    indent_size = indent_size_of(&quasi.value.raw, self.options.indent_width);
                }
                let next_quasi = template.quasis.get(index + 1);
                parts.push(self.print_template_expression(
                    expression,
                    quasi,
                    next_quasi,
                    indent_size,
                ));
            }
        }
        parts.push(self.s("`"));
        self.docs.concat_vec(parts)
    }

    fn print_template_expression(
        &mut self,
        expression: &'a Expression,
        previous_quasi: &'a expression::template_literal::Element<Loc>,
        next_quasi: Option<&'a expression::template_literal::Element<Loc>>,
        indent_size: usize,
    ) -> Doc<'a> {
        use expression::ExpressionInner as E;
        let mut printed = self.print_expression(expression);
        let start = self.text.span(&previous_quasi.loc).end;
        let end = next_quasi.map_or(start, |quasi| self.text.span(&quasi.loc).start);
        let mut has_newline = self.text.has_newline_in_range(start, end);
        if !has_newline {
            let flat = crate::doc::printer::print(
                printed,
                crate::doc::printer::PrintOptions {
                    width: usize::MAX / 4,
                    indent_width: self.options.indent_width,
                },
                self.docs.group_count(),
            );
            if flat.contains('\n') {
                has_newline = true;
            } else {
                printed = self.text(&flat);
            }
        }
        let key = NodeRef::Expression(expression).key();
        let wraps = self.has_comment(key)
            || matches!(
                &**expression,
                E::Identifier { .. }
                    | E::Member { .. }
                    | E::OptionalMember { .. }
                    | E::Conditional { .. }
                    | E::Sequence { .. }
                    | E::AsExpression { .. }
                    | E::AsConstExpression { .. }
                    | E::TSSatisfies { .. }
                    | E::Binary { .. }
                    | E::Logical { .. }
            );
        if has_newline && wraps {
            printed = self.concat([self.indent(self.concat([&SOFTLINE, printed])), &SOFTLINE]);
        }
        let aligned = if indent_size == 0 && previous_quasi.value.raw.ends_with('\n') {
            self.docs.dedent_to_root(printed)
        } else {
            self.add_alignment(printed, indent_size)
        };
        self.group(self.concat([self.s("${"), aligned, &LINE_SUFFIX_BOUNDARY, self.s("}")]))
    }

    /// Indent `doc` by `size` columns: whole levels as indents, the rest as
    /// an align, then anchored as root. Prettier's `addAlignmentToDoc`.
    fn add_alignment(&self, doc: Doc<'a>, size: usize) -> Doc<'a> {
        if size == 0 {
            return doc;
        }
        let mut aligned = doc;
        for _ in 0..(size / self.options.indent_width) {
            aligned = self.indent(aligned);
        }
        aligned = self
            .docs
            .align((size % self.options.indent_width) as u16, aligned);
        self.docs.dedent_to_root(aligned)
    }

    /// A tagged template: the tag, its type arguments, and the literal.
    pub fn print_tagged_template(
        &mut self,
        tagged: &'a expression::TaggedTemplate<Loc, Loc>,
    ) -> Doc<'a> {
        let tag = self.print_expression(&tagged.tag);
        let targs = tagged
            .targs
            .as_ref()
            .map_or(self.s(""), |targs| self.print_call_type_args(targs));
        let quasi = self.print_template_literal(&tagged.quasi.1);
        self.concat([tag, targs, &LINE_SUFFIX_BOUNDARY, quasi])
    }
}

/// Columns of indentation on the last line of `text`, tabs counted as
/// `tab_width`. Prettier's `getIndentSize`.
fn indent_size_of(text: &str, tab_width: usize) -> usize {
    let Some(last_newline) = text.rfind('\n') else {
        return 0;
    };
    text[last_newline + 1..]
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(|ch| if ch == '\t' { tab_width } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_normalised_like_prettier() {
        assert_eq!(print_number("1.50e10"), "1.5e10");
        assert_eq!(print_number("1E+05"), "1e5");
        assert_eq!(print_number("1e-05"), "1e-5");
        assert_eq!(print_number("1e0"), "1");
        assert_eq!(print_number(".5"), "0.5");
        assert_eq!(print_number("5."), "5");
        assert_eq!(print_number("5.0"), "5.0");
        assert_eq!(print_number("5.00"), "5.0");
        assert_eq!(print_number("0XABCDEF"), "0xabcdef");
        assert_eq!(print_number("1_000.000_0"), "1_000.000_");
        assert_eq!(print_number("0.0"), "0.0");
        assert_eq!(print_number("10"), "10");
    }

    #[test]
    fn strings_switch_quotes_only_to_save_escapes() {
        assert_eq!(print_string("'a'", QuoteStyle::Double), "\"a\"");
        assert_eq!(
            print_string("'say \"hi\"'", QuoteStyle::Double),
            "'say \"hi\"'"
        );
        assert_eq!(print_string("\"it's\"", QuoteStyle::Single), "\"it's\"");
        assert_eq!(print_string("'it\\'s'", QuoteStyle::Double), "\"it's\"");
        assert_eq!(print_string("'\\d'", QuoteStyle::Double), "\"d\"");
        assert_eq!(
            print_string("'\\n\\u0041'", QuoteStyle::Double),
            "\"\\n\\u0041\""
        );
        assert_eq!(print_string("\"\\\"\"", QuoteStyle::Double), "'\"'");
    }

    #[test]
    fn regex_flags_are_sorted() {
        assert_eq!(print_regex("a", "gim"), "/a/gim");
        assert_eq!(print_regex("a", "mgi"), "/a/gim");
    }

    #[test]
    fn identifier_names_decide_whether_a_key_keeps_its_quotes() {
        assert!(is_identifier_name("a_b$1"));
        assert!(!is_identifier_name("1a"));
        assert!(!is_identifier_name("a-b"));
        assert!(!is_identifier_name(""));
    }
}
