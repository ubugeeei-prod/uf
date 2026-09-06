//! The GraphQL printer: Prettier's `printer-graphql`, arm for arm, over
//! uf's document IR.
//!
//! Prettier's GraphQL printer is one `switch` over graphql-js `kind`
//! strings, about two hundred lines long, and it is the specification for
//! this file — the fixtures are its output byte for byte. So the shape here
//! follows it rather than what a Rust printer would naturally do: the same
//! groups in the same places, the same `ifBreak` separators, the same
//! unconditional space after `{` in an object value that the printer's
//! trailing-whitespace trim then removes when the group breaks.
//!
//! Two rules are easy to get subtly wrong and are called out where they
//! happen:
//!
//! * a selection set always breaks, because its separator is a hard line;
//! * a blank line the author left between two elements of a *sequence* —
//!   definitions, selections, arguments, fields, enum values — is kept, and
//!   exactly one of them, which is why the printer needs the document text
//!   and the spans that go with it.

use super::ast::{
    Argument, Definition, DefinitionKind, Directive, DirectiveDefinition, Document,
    EnumValueDefinition, Field, FieldDefinition, FragmentDefinition, FragmentSpread,
    InlineFragment, InputValueDefinition, OperationDefinition, OperationTypeDefinition,
    SchemaDefinition, Selection, SelectionKind, SelectionSet, Span, StringValue, Type,
    TypeDefinition, TypeKind, Value, VariableDefinition,
};
use crate::doc::{Doc, Docs, EMPTY, HARDLINE, LINE, SOFTLINE, SPACE};

/// Prettier's `bracketSpacing`, which decides `{ a: 1 }` against `{a: 1}`.
///
/// uf has no option for it, so this is Prettier's default and the value
/// every fixture is generated with. It is a named constant rather than a
/// bare `true` so the place to change it is obvious if uf ever grows the
/// option.
const BRACKET_SPACING: bool = true;

/// Print `document` as a doc, ready to be placed inside the template
/// literal it came from.
///
/// `source` is the text `document` was parsed from; the printer reads it to
/// answer one question — whether the author left a blank line after a
/// sequence element — and nothing else.
pub fn print<'a>(docs: &Docs<'a>, source: &str, document: &Document<'_>) -> Doc<'a> {
    let printer = Printer { docs, source };
    printer.document(document)
}

struct Printer<'d, 'a, 'src> {
    docs: &'d Docs<'a>,
    source: &'src str,
}

impl<'a> Printer<'_, 'a, '_> {
    /// Text that came from the document, escaped for the template literal
    /// it is about to be spliced into.
    ///
    /// See [`escape_template_characters`]. Every string that can hold a
    /// backslash, a backtick or a `${` — a string value, a block string
    /// line — goes through this; the fixed punctuation goes through
    /// [`Printer::s`] and holds none of them.
    fn text(&self, text: &str) -> Doc<'a> {
        self.docs.text(&super::escape_template_characters(text))
    }

    /// Punctuation and keywords, which never need escaping.
    fn s(&self, text: &'static str) -> Doc<'a> {
        self.docs.borrowed(text)
    }

    fn concat(&self, parts: impl IntoIterator<Item = Doc<'a>>) -> Doc<'a> {
        self.docs.concat(parts)
    }

    /// The separator between two items of a comma-separated list: nothing
    /// when the group broke, `", "` when it did not, then a soft line.
    fn comma_separator(&self) -> Doc<'a> {
        self.concat([self.docs.if_break(&EMPTY, self.s(", "), None), &SOFTLINE])
    }

    // ---- sequences ----

    /// Prettier's `printSequence`: each item, with an extra hard line after
    /// it when the author left a blank line there.
    ///
    /// The caller joins the result with a hard line, so an item followed by
    /// a blank line contributes two.
    fn sequence<T>(
        &self,
        items: &[T],
        span: impl Fn(&T) -> Span,
        print: impl Fn(&Self, &T) -> Doc<'a>,
    ) -> Vec<Doc<'a>> {
        let last = items.len().saturating_sub(1);
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let doc = print(self, item);
                if index != last && is_next_line_empty(self.source, span(item).end) {
                    self.concat([doc, &HARDLINE])
                } else {
                    doc
                }
            })
            .collect()
    }

    // ---- documents and definitions ----

    fn document(&self, document: &Document<'_>) -> Doc<'a> {
        let definitions = self.sequence(
            &document.definitions,
            |definition| definition.span,
            Self::definition,
        );
        self.docs.join(&HARDLINE, definitions)
    }

    fn definition(&self, definition: &Definition<'_>) -> Doc<'a> {
        match &definition.kind {
            DefinitionKind::Operation(operation) => self.operation(operation),
            DefinitionKind::Fragment(fragment) => self.fragment(fragment),
            DefinitionKind::Schema(schema) => self.schema(schema),
            DefinitionKind::Type(ty) => self.type_definition(ty),
            DefinitionKind::Directive(directive) => self.directive_definition(directive),
        }
    }

    fn operation(&self, operation: &OperationDefinition<'_>) -> Doc<'a> {
        let has_operation = operation.operation.is_some();
        let has_name = operation.name.is_some();
        let mut parts = Vec::new();
        if let Some(keyword) = operation.operation {
            parts.push(self.s(keyword.keyword()));
        }
        if let (true, Some(name)) = (has_operation, operation.name) {
            parts.push(self.concat([&SPACE, self.text(name)]));
        }
        // `query ($x: Int)` keeps the space the source needs between the
        // keyword and an anonymous operation's variable list.
        if has_operation && !has_name && !operation.variable_definitions.is_empty() {
            parts.push(&SPACE);
        }
        parts.push(self.variable_definitions(&operation.variable_definitions));
        parts.push(self.directives(&operation.directives, true));
        if has_operation || has_name {
            parts.push(&SPACE);
        }
        parts.push(self.selection_set(&operation.selection_set));
        self.concat(parts)
    }

    fn fragment(&self, fragment: &FragmentDefinition<'_>) -> Doc<'a> {
        self.concat([
            self.s("fragment "),
            self.text(fragment.name),
            self.variable_definitions(&fragment.variable_definitions),
            self.s(" on "),
            self.text(fragment.type_condition),
            self.directives(&fragment.directives, true),
            &SPACE,
            self.selection_set(&fragment.selection_set),
        ])
    }

    // ---- selections ----

    /// A selection set. Its separator is a hard line, so it is always
    /// broken; nothing about it is decided by the line width.
    fn selection_set(&self, set: &SelectionSet<'_>) -> Doc<'a> {
        let selections = self.sequence(&set.selections, |item| item.span, Self::selection);
        let body = self.docs.join(&HARDLINE, selections);
        self.concat([
            self.s("{"),
            self.docs.indent(self.concat([&HARDLINE, body])),
            &HARDLINE,
            self.s("}"),
        ])
    }

    fn selection(&self, selection: &Selection<'_>) -> Doc<'a> {
        match &selection.kind {
            SelectionKind::Field(field) => self.field(field),
            SelectionKind::FragmentSpread(spread) => self.fragment_spread(spread),
            SelectionKind::InlineFragment(fragment) => self.inline_fragment(fragment),
        }
    }

    fn field(&self, field: &Field<'_>) -> Doc<'a> {
        let mut parts = Vec::new();
        if let Some(alias) = field.alias {
            parts.push(self.concat([self.text(alias), self.s(": ")]));
        }
        parts.push(self.text(field.name));
        parts.push(self.arguments(&field.arguments));
        parts.push(self.directives(&field.directives, false));
        if let Some(set) = &field.selection_set {
            parts.push(&SPACE);
            parts.push(self.selection_set(set));
        }
        self.docs.group(self.concat(parts))
    }

    fn fragment_spread(&self, spread: &FragmentSpread<'_>) -> Doc<'a> {
        self.concat([
            self.s("..."),
            self.text(spread.name),
            self.arguments(&spread.arguments),
            self.directives(&spread.directives, false),
        ])
    }

    fn inline_fragment(&self, fragment: &InlineFragment<'_>) -> Doc<'a> {
        let condition = match fragment.type_condition {
            Some(name) => self.concat([self.s(" on "), self.text(name)]),
            None => &EMPTY,
        };
        self.concat([
            self.s("..."),
            condition,
            self.directives(&fragment.directives, false),
            &SPACE,
            self.selection_set(&fragment.selection_set),
        ])
    }

    // ---- arguments, directives, variables ----

    fn arguments(&self, arguments: &[Argument<'_>]) -> Doc<'a> {
        if arguments.is_empty() {
            return &EMPTY;
        }
        let printed = self.sequence(arguments, |argument| argument.span, Self::argument);
        self.bracketed_list("(", ")", printed)
    }

    fn argument(&self, argument: &Argument<'_>) -> Doc<'a> {
        self.concat([
            self.text(argument.name),
            self.s(": "),
            self.value(&argument.value),
        ])
    }

    /// `(a, b)` — broken one per line when it does not fit.
    fn bracketed_list(
        &self,
        open: &'static str,
        close: &'static str,
        items: Vec<Doc<'a>>,
    ) -> Doc<'a> {
        let body = self.docs.join(self.comma_separator(), items);
        self.docs.group(self.concat([
            self.s(open),
            self.docs.indent(self.concat([&SOFTLINE, body])),
            &SOFTLINE,
            self.s(close),
        ]))
    }

    /// The directives on a node.
    ///
    /// An operation or fragment definition puts them on their own line when
    /// the header does not fit; everywhere else they hang off the node with
    /// a space in front, and the space is what the printer's
    /// trailing-whitespace trim removes when the group breaks. That
    /// asymmetry is Prettier's, and it is what produces
    /// `...Fragment\n  @module(...)` — the divergence this feature was
    /// opened for.
    fn directives(&self, directives: &[Directive<'_>], own_line: bool) -> Doc<'a> {
        if directives.is_empty() {
            return &EMPTY;
        }
        let printed: Vec<Doc<'a>> = directives
            .iter()
            .map(|directive| self.directive(directive))
            .collect();
        let joined = self.docs.join(&LINE, printed);
        if own_line {
            self.docs.group(self.concat([&LINE, joined]))
        } else {
            self.concat([
                &SPACE,
                self.docs
                    .group(self.docs.indent(self.concat([&SOFTLINE, joined]))),
            ])
        }
    }

    fn directive(&self, directive: &Directive<'_>) -> Doc<'a> {
        self.concat([
            self.s("@"),
            self.text(directive.name),
            self.arguments(&directive.arguments),
        ])
    }

    fn variable_definitions(&self, definitions: &[VariableDefinition<'_>]) -> Doc<'a> {
        if definitions.is_empty() {
            return &EMPTY;
        }
        // Unlike an argument list this is not a `printSequence`: Prettier
        // does not keep blank lines between variable definitions.
        let printed: Vec<Doc<'a>> = definitions
            .iter()
            .map(|definition| self.variable_definition(definition))
            .collect();
        self.bracketed_list("(", ")", printed)
    }

    fn variable_definition(&self, definition: &VariableDefinition<'_>) -> Doc<'a> {
        let mut parts = vec![
            self.s("$"),
            self.text(definition.variable),
            self.s(": "),
            self.ty(&definition.ty),
        ];
        if let Some(default) = &definition.default_value {
            parts.push(self.concat([self.s(" = "), self.value(default)]));
        }
        parts.push(self.directives(&definition.directives, false));
        self.concat(parts)
    }

    // ---- values and types ----

    fn value(&self, value: &Value<'_>) -> Doc<'a> {
        match value {
            Value::Variable(name) => self.concat([self.s("$"), self.text(name)]),
            Value::Int(raw) | Value::Float(raw) | Value::Enum(raw) => self.text(raw),
            Value::String(string) => self.string(string),
            Value::Boolean(true) => self.s("true"),
            Value::Boolean(false) => self.s("false"),
            Value::Null => self.s("null"),
            Value::List(values) => {
                if values.is_empty() {
                    return self
                        .docs
                        .group(self.concat([self.s("["), &SOFTLINE, self.s("]")]));
                }
                let printed: Vec<Doc<'a>> = values.iter().map(|value| self.value(value)).collect();
                self.bracketed_list("[", "]", printed)
            }
            Value::Object(fields) => self.object(fields),
        }
    }

    fn object(&self, fields: &[Argument<'_>]) -> Doc<'a> {
        let spacing = if BRACKET_SPACING && !fields.is_empty() {
            self.s(" ")
        } else {
            &EMPTY
        };
        let body = if fields.is_empty() {
            &EMPTY
        } else {
            let printed: Vec<Doc<'a>> = fields.iter().map(|field| self.argument(field)).collect();
            let joined = self.docs.join(self.comma_separator(), printed);
            self.docs.indent(self.concat([&SOFTLINE, joined]))
        };
        // The space after `{` is unconditional and the one before `}` is
        // not; when the group breaks the first becomes trailing whitespace
        // and the printer trims it. Making both conditional was the obvious
        // first attempt and it prints `{a: 1}` flat, which is wrong.
        self.docs.group(self.concat([
            self.s("{"),
            spacing,
            body,
            &SOFTLINE,
            self.docs.if_break(&EMPTY, spacing, None),
            self.s("}"),
        ]))
    }

    fn string(&self, string: &StringValue) -> Doc<'a> {
        if string.block {
            let escaped = string.value.replace("\"\"\"", "\\\"\"\"");
            let mut lines: Vec<&str> = escaped.split('\n').collect();
            if lines.len() == 1 {
                lines[0] = lines[0].trim();
            }
            if lines.iter().all(|line| line.is_empty()) {
                lines.clear();
            }
            let mut parts = vec![self.s("\"\"\"")];
            parts.extend(lines.iter().map(|line| self.text(line)));
            parts.push(self.s("\"\"\""));
            return self.docs.join(&HARDLINE, parts);
        }
        // Prettier prints the *value*, re-escaped, not the source spelling:
        // `"A"` comes back out as `"A"`.
        let mut escaped = String::with_capacity(string.value.len() + 2);
        escaped.push('"');
        for ch in string.value.chars() {
            match ch {
                '"' | '\\' => {
                    escaped.push('\\');
                    escaped.push(ch);
                }
                '\n' => escaped.push_str("\\n"),
                _ => escaped.push(ch),
            }
        }
        escaped.push('"');
        self.text(&escaped)
    }

    fn ty(&self, ty: &Type<'_>) -> Doc<'a> {
        match ty {
            Type::Named(name) => self.text(name),
            Type::List(inner) => self.concat([self.s("["), self.ty(inner), self.s("]")]),
            Type::NonNull(inner) => self.concat([self.ty(inner), self.s("!")]),
        }
    }

    // ---- the type system ----

    fn description(&self, description: Option<&StringValue>, input_value: bool) -> Doc<'a> {
        let Some(description) = description else {
            return &EMPTY;
        };
        // An input value's one-line description sits on the same line as
        // the value when it fits; everything else gets its own line.
        let separator: Doc<'a> = if input_value && !description.block {
            &LINE
        } else {
            &HARDLINE
        };
        self.concat([self.string(description), separator])
    }

    fn schema(&self, schema: &SchemaDefinition<'_>) -> Doc<'a> {
        let operation_types = self.sequence(
            &schema.operation_types,
            |item| item.span,
            Self::operation_type,
        );
        if schema.extend {
            let mut parts = vec![
                self.s("extend schema"),
                self.directives(&schema.directives, false),
            ];
            if !schema.operation_types.is_empty() {
                parts.push(self.braced_block(operation_types));
            }
            return self.concat(parts);
        }
        let body = if schema.operation_types.is_empty() {
            &EMPTY
        } else {
            self.docs
                .indent(self.concat([&HARDLINE, self.docs.join(&HARDLINE, operation_types)]))
        };
        self.concat([
            self.description(schema.description.as_ref(), false),
            self.s("schema"),
            self.directives(&schema.directives, false),
            self.s(" {"),
            body,
            &HARDLINE,
            self.s("}"),
        ])
    }

    fn operation_type(&self, operation: &OperationTypeDefinition<'_>) -> Doc<'a> {
        self.concat([
            self.s(operation.operation.keyword()),
            self.s(": "),
            self.text(operation.ty),
        ])
    }

    /// ` { … }`, always broken — the block of a type, interface, input or
    /// enum definition.
    fn braced_block(&self, items: Vec<Doc<'a>>) -> Doc<'a> {
        let body = self.docs.join(&HARDLINE, items);
        self.concat([
            self.s(" {"),
            self.docs.indent(self.concat([&HARDLINE, body])),
            &HARDLINE,
            self.s("}"),
        ])
    }

    fn type_definition(&self, definition: &TypeDefinition<'_>) -> Doc<'a> {
        match definition.kind {
            TypeKind::Scalar => self.concat([
                self.description(definition.description.as_ref(), false),
                if definition.extend {
                    self.s("extend ")
                } else {
                    &EMPTY
                },
                self.s("scalar "),
                self.text(definition.name),
                self.directives(&definition.directives, false),
            ]),
            TypeKind::Union => self.union(definition),
            TypeKind::Enum => self.concat([
                self.description(definition.description.as_ref(), false),
                if definition.extend {
                    self.s("extend ")
                } else {
                    &EMPTY
                },
                self.s("enum "),
                self.text(definition.name),
                self.directives(&definition.directives, false),
                if definition.values.is_empty() {
                    &EMPTY
                } else {
                    let values =
                        self.sequence(&definition.values, |item| item.span, Self::enum_value);
                    self.braced_block(values)
                },
            ]),
            TypeKind::Object | TypeKind::Interface | TypeKind::Input => self.fielded(definition),
        }
    }

    fn union(&self, definition: &TypeDefinition<'_>) -> Doc<'a> {
        let types = if definition.types.is_empty() {
            &EMPTY
        } else {
            let printed: Vec<Doc<'a>> = definition
                .types
                .iter()
                .map(|name| self.text(name))
                .collect();
            let bar = self.concat([&LINE, self.s("| ")]);
            self.concat([
                self.s(" ="),
                self.docs.if_break(&EMPTY, &SPACE, None),
                self.docs.indent(self.concat([
                    self.docs.if_break(bar, &EMPTY, None),
                    self.docs.join(bar, printed),
                ])),
            ])
        };
        self.docs.group(self.concat([
            self.description(definition.description.as_ref(), false),
            self.docs.group(self.concat([
                if definition.extend {
                    self.s("extend ")
                } else {
                    &EMPTY
                },
                self.s("union "),
                self.text(definition.name),
                self.directives(&definition.directives, false),
                types,
            ])),
        ]))
    }

    /// `type`, `interface` and `input`, which differ only in the keyword,
    /// whether they take `implements`, and what their block holds.
    fn fielded(&self, definition: &TypeDefinition<'_>) -> Doc<'a> {
        let mut parts = Vec::new();
        if definition.extend {
            parts.push(self.s("extend "));
        } else {
            parts.push(self.description(definition.description.as_ref(), false));
        }
        parts.push(match definition.kind {
            TypeKind::Object => self.s("type"),
            TypeKind::Interface => self.s("interface"),
            _ => self.s("input"),
        });
        parts.push(&SPACE);
        parts.push(self.text(definition.name));
        if !definition.interfaces.is_empty() {
            let printed: Vec<Doc<'a>> = definition
                .interfaces
                .iter()
                .map(|name| self.text(name))
                .collect();
            let separator = self.concat([self.s(" &"), &LINE]);
            parts.push(self.s(" implements "));
            parts.push(
                self.docs
                    .indent(self.docs.group(self.docs.join(separator, printed))),
            );
        }
        parts.push(self.directives(&definition.directives, false));
        if !definition.fields.is_empty() {
            let fields =
                self.sequence(&definition.fields, |item| item.span, Self::field_definition);
            parts.push(self.braced_block(fields));
        } else if !definition.input_fields.is_empty() {
            let fields = self.sequence(
                &definition.input_fields,
                |item| item.span,
                Self::input_value_definition,
            );
            parts.push(self.braced_block(fields));
        }
        self.concat(parts)
    }

    fn field_definition(&self, field: &FieldDefinition<'_>) -> Doc<'a> {
        let arguments = if field.arguments.is_empty() {
            &EMPTY
        } else {
            let printed = self.sequence(
                &field.arguments,
                |item| item.span,
                Self::input_value_definition,
            );
            self.bracketed_list("(", ")", printed)
        };
        self.concat([
            self.description(field.description.as_ref(), false),
            self.text(field.name),
            arguments,
            self.s(": "),
            self.ty(&field.ty),
            self.directives(&field.directives, false),
        ])
    }

    fn input_value_definition(&self, definition: &InputValueDefinition<'_>) -> Doc<'a> {
        let mut parts = vec![
            self.description(definition.description.as_ref(), true),
            self.text(definition.name),
            self.s(": "),
            self.ty(&definition.ty),
        ];
        if let Some(default) = &definition.default_value {
            parts.push(self.concat([self.s(" = "), self.value(default)]));
        }
        parts.push(self.directives(&definition.directives, false));
        self.concat(parts)
    }

    fn enum_value(&self, value: &EnumValueDefinition<'_>) -> Doc<'a> {
        self.concat([
            self.description(value.description.as_ref(), false),
            self.text(value.name),
            self.directives(&value.directives, false),
        ])
    }

    fn directive_definition(&self, definition: &DirectiveDefinition<'_>) -> Doc<'a> {
        if definition.extend {
            return self.concat([
                self.s("extend directive @"),
                self.text(definition.name),
                self.directives(&definition.directives, false),
            ]);
        }
        let arguments = if definition.arguments.is_empty() {
            &EMPTY
        } else {
            let printed = self.sequence(
                &definition.arguments,
                |item| item.span,
                Self::input_value_definition,
            );
            self.bracketed_list("(", ")", printed)
        };
        let locations: Vec<Doc<'a>> = definition
            .locations
            .iter()
            .map(|location| self.text(location))
            .collect();
        self.concat([
            self.description(definition.description.as_ref(), false),
            self.s("directive @"),
            self.text(definition.name),
            arguments,
            self.directives(&definition.directives, false),
            if definition.repeatable {
                self.s(" repeatable")
            } else {
                &EMPTY
            },
            self.s(" on "),
            self.docs.join(self.s(" | "), locations),
        ])
    }
}

/// Prettier's `isNextLineEmpty`: whether the author left a blank line after
/// the element that ends at `index`.
///
/// Commas are whitespace in GraphQL, so a trailing one belongs to the line
/// above rather than to the blank line below it.
fn is_next_line_empty(source: &str, index: usize) -> bool {
    let bytes = source.as_bytes();
    let mut at = index.min(bytes.len());
    while at < bytes.len() && matches!(bytes[at], b',' | b';' | b' ' | b'\t') {
        at += 1;
    }
    at = skip_newline(source, at);
    let mut after = at;
    while after < bytes.len() && matches!(bytes[after], b' ' | b'\t') {
        after += 1;
    }
    skip_newline(source, after) != after
}

/// One line terminator, if there is one here. `\r\n` is one.
fn skip_newline(source: &str, at: usize) -> usize {
    let bytes = source.as_bytes();
    match bytes.get(at) {
        Some(b'\n') => at + 1,
        Some(b'\r') => {
            if bytes.get(at + 1) == Some(&b'\n') {
                at + 2
            } else {
                at + 1
            }
        }
        _ => {
            // U+2028 and U+2029 are line terminators too, and three bytes
            // each in UTF-8.
            if source[at..].starts_with('\u{2028}') || source[at..].starts_with('\u{2029}') {
                at + 3
            } else {
                at
            }
        }
    }
}
