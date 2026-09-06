//! A recursive-descent parser for the GraphQL grammar.
//!
//! It parses a whole document — executable definitions and type system
//! definitions and extensions alike — and refuses everything else. What it
//! refuses matters as much as what it accepts, because the caller's answer
//! to [`Declined`] is to leave the template exactly as the author wrote it:
//! a document uf cannot reproduce is one uf does not touch.
//!
//! There is deliberately no error recovery and no diagnostic text. Nobody
//! sees these errors — a template that does not parse is simply not
//! formatted — so an offset is all a test needs to say *which* construct was
//! refused.

use super::ast::{
    Argument, Definition, DefinitionKind, Directive, DirectiveDefinition, Document,
    EnumValueDefinition, Field, FieldDefinition, FragmentDefinition, FragmentSpread,
    InlineFragment, InputValueDefinition, OperationDefinition, OperationType,
    OperationTypeDefinition, SchemaDefinition, Selection, SelectionKind, SelectionSet, Span,
    StringValue, Type, TypeDefinition, TypeKind, Value, VariableDefinition,
};
use super::lexer::{LexErrorKind, Lexer, Token, TokenKind};

/// How deep a document may nest before the parser gives up.
///
/// Selection sets and list values both recurse once per level. The formatter
/// runs on a thread with a large stack, but "large" is not "unbounded", and a
/// template is untrusted input like any other source text.
const MAX_DEPTH: u32 = 128;

/// Why a document will not be formatted.
///
/// Every variant means the same thing to the caller — leave the template
/// alone — and they are distinguished so a test can say which rule fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declined {
    /// The text is not a GraphQL document. The byte offset is where the
    /// parser stopped, which is useful in a test and nowhere else.
    Syntax(usize),
    /// The document holds a `#` comment.
    ///
    /// Prettier places GraphQL comments with its generic comment-attachment
    /// pass — the one that decides between a leading, trailing and dangling
    /// comment from the source layout around it. uf does not reproduce that
    /// pass, and a comment attached to the wrong node is a worse answer than
    /// an untouched template, so a document with one is declined whole.
    Comment(usize),
    /// The document nests deeper than [`MAX_DEPTH`].
    TooDeep,
    /// A string value holds a character Prettier prints raw and uf cannot
    /// write back into a template without changing what it says. See the
    /// lexer.
    Unprintable(usize),
}

/// Parse `source` as a GraphQL document.
///
/// # Errors
///
/// Returns [`Declined`] for anything the printer would not reproduce
/// byte for byte; the caller leaves the template unformatted.
pub fn parse(source: &str) -> Result<Document<'_>, Declined> {
    let tokens = tokenize(source)?;
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    let definitions = parser.parse_definitions()?;
    if definitions.is_empty() {
        return Err(Declined::Syntax(source.len()));
    }
    Ok(Document { definitions })
}

/// Every token, so the parser can look ahead as far as the grammar needs.
///
/// A document is at most a template literal long, so reading it all up front
/// costs nothing and buys `extend type` and `... on` a second token of
/// lookahead without a pushback buffer.
fn tokenize(source: &str) -> Result<Vec<Token<'_>>, Declined> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().map_err(|error| match error.kind {
            LexErrorKind::Syntax => Declined::Syntax(error.offset),
            LexErrorKind::Unprintable => Declined::Unprintable(error.offset),
        })?;
        if token.kind == TokenKind::Comment {
            return Err(Declined::Comment(token.start));
        }
        let eof = token.kind == TokenKind::Eof;
        tokens.push(token);
        if eof {
            return Ok(tokens);
        }
    }
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
    depth: u32,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token<'a> {
        &self.tokens[self.at.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, ahead: usize) -> &Token<'a> {
        let at = (self.at + ahead).min(self.tokens.len() - 1);
        &self.tokens[at]
    }

    fn offset(&self) -> usize {
        self.peek().start
    }

    fn error<T>(&self) -> Result<T, Declined> {
        Err(Declined::Syntax(self.offset()))
    }

    fn bump(&mut self) -> Token<'a> {
        let token = self.tokens[self.at.min(self.tokens.len() - 1)].clone();
        if self.at < self.tokens.len() - 1 {
            self.at += 1;
        }
        token
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn at_name(&self, name: &str) -> bool {
        let token = self.peek();
        token.kind == TokenKind::Name && token.text == name
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at_kind(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_name(&mut self, name: &str) -> bool {
        if self.at_name(name) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token<'a>, Declined> {
        if self.at_kind(kind) {
            Ok(self.bump())
        } else {
            self.error()
        }
    }

    fn expect_name(&mut self) -> Result<&'a str, Declined> {
        Ok(self.expect(TokenKind::Name)?.text)
    }

    /// The end of the token before the cursor, which is where the node that
    /// just finished ends.
    fn previous_end(&self) -> usize {
        if self.at == 0 {
            0
        } else {
            self.tokens[self.at - 1].end
        }
    }

    fn nest<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, Declined>,
    ) -> Result<T, Declined> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Declined::TooDeep);
        }
        let result = parse(self);
        self.depth -= 1;
        result
    }

    // ---- definitions ----

    fn parse_definitions(&mut self) -> Result<Vec<Definition<'a>>, Declined> {
        let mut definitions = Vec::new();
        while !self.at_kind(TokenKind::Eof) {
            definitions.push(self.parse_definition()?);
        }
        Ok(definitions)
    }

    fn parse_definition(&mut self) -> Result<Definition<'a>, Declined> {
        let start = self.offset();
        let description = self.parse_description();
        let kind = if description.is_some() {
            // Only a type system definition takes a description, and an
            // `extend` never does.
            DefinitionKind::from_type_system(self.parse_type_system(description)?)
        } else if self.at_kind(TokenKind::BraceL) {
            DefinitionKind::Operation(self.parse_operation(None)?)
        } else if self.at_kind(TokenKind::Name) {
            match self.peek().text {
                "query" => {
                    self.bump();
                    DefinitionKind::Operation(self.parse_operation(Some(OperationType::Query))?)
                }
                "mutation" => {
                    self.bump();
                    DefinitionKind::Operation(self.parse_operation(Some(OperationType::Mutation))?)
                }
                "subscription" => {
                    self.bump();
                    DefinitionKind::Operation(
                        self.parse_operation(Some(OperationType::Subscription))?,
                    )
                }
                "fragment" => {
                    self.bump();
                    DefinitionKind::Fragment(self.parse_fragment()?)
                }
                _ => DefinitionKind::from_type_system(self.parse_type_system(None)?),
            }
        } else {
            return self.error();
        };
        Ok(Definition {
            kind,
            span: Span {
                start,
                end: self.previous_end(),
            },
        })
    }

    fn parse_description(&mut self) -> Option<StringValue> {
        let token = self.peek();
        if matches!(token.kind, TokenKind::String | TokenKind::BlockString)
            && self.peek_at(1).kind == TokenKind::Name
        {
            let token = self.bump();
            return token.string;
        }
        None
    }

    fn parse_operation(
        &mut self,
        operation: Option<OperationType>,
    ) -> Result<OperationDefinition<'a>, Declined> {
        let name = if operation.is_some() && self.at_kind(TokenKind::Name) {
            Some(self.bump().text)
        } else {
            None
        };
        let variable_definitions = self.parse_variable_definitions()?;
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(OperationDefinition {
            operation,
            name,
            variable_definitions,
            directives,
            selection_set,
        })
    }

    fn parse_fragment(&mut self) -> Result<FragmentDefinition<'a>, Declined> {
        let name = self.expect_name()?;
        if name == "on" {
            return self.error();
        }
        let variable_definitions = self.parse_variable_definitions()?;
        if !self.eat_name("on") {
            return self.error();
        }
        let type_condition = self.expect_name()?;
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(FragmentDefinition {
            name,
            variable_definitions,
            type_condition,
            directives,
            selection_set,
        })
    }

    // ---- selections ----

    fn parse_selection_set(&mut self) -> Result<SelectionSet<'a>, Declined> {
        self.nest(|parser| {
            parser.expect(TokenKind::BraceL)?;
            let mut selections = Vec::new();
            while !parser.at_kind(TokenKind::BraceR) {
                if parser.at_kind(TokenKind::Eof) {
                    return parser.error();
                }
                selections.push(parser.parse_selection()?);
            }
            parser.expect(TokenKind::BraceR)?;
            if selections.is_empty() {
                return parser.error();
            }
            Ok(SelectionSet { selections })
        })
    }

    fn parse_selection(&mut self) -> Result<Selection<'a>, Declined> {
        let start = self.offset();
        let kind = if self.eat(TokenKind::Spread) {
            // `...Frag` is a spread; `... on T` and `... @dir` and `... {`
            // are inline fragments. `on` can be a fragment name nowhere, so
            // the one name that is not a spread is `on`.
            if self.at_kind(TokenKind::Name) && !self.at_name("on") {
                let name = self.bump().text;
                let arguments = self.parse_arguments()?;
                let directives = self.parse_directives()?;
                SelectionKind::FragmentSpread(FragmentSpread {
                    name,
                    arguments,
                    directives,
                })
            } else {
                let type_condition = if self.eat_name("on") {
                    Some(self.expect_name()?)
                } else {
                    None
                };
                let directives = self.parse_directives()?;
                let selection_set = self.parse_selection_set()?;
                SelectionKind::InlineFragment(InlineFragment {
                    type_condition,
                    directives,
                    selection_set,
                })
            }
        } else {
            let first = self.expect_name()?;
            let (alias, name) = if self.eat(TokenKind::Colon) {
                (Some(first), self.expect_name()?)
            } else {
                (None, first)
            };
            let arguments = self.parse_arguments()?;
            let directives = self.parse_directives()?;
            let selection_set = if self.at_kind(TokenKind::BraceL) {
                Some(self.parse_selection_set()?)
            } else {
                None
            };
            SelectionKind::Field(Field {
                alias,
                name,
                arguments,
                directives,
                selection_set,
            })
        };
        Ok(Selection {
            kind,
            span: Span {
                start,
                end: self.previous_end(),
            },
        })
    }

    // ---- arguments, directives, variables ----

    fn parse_arguments(&mut self) -> Result<Vec<Argument<'a>>, Declined> {
        if !self.eat(TokenKind::ParenL) {
            return Ok(Vec::new());
        }
        let mut arguments = Vec::new();
        while !self.at_kind(TokenKind::ParenR) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            arguments.push(self.parse_argument()?);
        }
        self.expect(TokenKind::ParenR)?;
        if arguments.is_empty() {
            return self.error();
        }
        Ok(arguments)
    }

    fn parse_argument(&mut self) -> Result<Argument<'a>, Declined> {
        let start = self.offset();
        let name = self.expect_name()?;
        self.expect(TokenKind::Colon)?;
        let value = self.parse_value()?;
        Ok(Argument {
            name,
            value,
            span: Span {
                start,
                end: self.previous_end(),
            },
        })
    }

    fn parse_directives(&mut self) -> Result<Vec<Directive<'a>>, Declined> {
        let mut directives = Vec::new();
        while self.eat(TokenKind::At) {
            let name = self.expect_name()?;
            let arguments = self.parse_arguments()?;
            directives.push(Directive { name, arguments });
        }
        Ok(directives)
    }

    fn parse_variable_definitions(&mut self) -> Result<Vec<VariableDefinition<'a>>, Declined> {
        if !self.eat(TokenKind::ParenL) {
            return Ok(Vec::new());
        }
        let mut definitions = Vec::new();
        while !self.at_kind(TokenKind::ParenR) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            self.expect(TokenKind::Dollar)?;
            let variable = self.expect_name()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let default_value = if self.eat(TokenKind::Equals) {
                Some(self.parse_value()?)
            } else {
                None
            };
            let directives = self.parse_directives()?;
            definitions.push(VariableDefinition {
                variable,
                ty,
                default_value,
                directives,
            });
        }
        self.expect(TokenKind::ParenR)?;
        if definitions.is_empty() {
            return self.error();
        }
        Ok(definitions)
    }

    // ---- values and types ----

    fn parse_value(&mut self) -> Result<Value<'a>, Declined> {
        self.nest(|parser| {
            let token = parser.peek().clone();
            match token.kind {
                TokenKind::Dollar => {
                    parser.bump();
                    Ok(Value::Variable(parser.expect_name()?))
                }
                TokenKind::Int => {
                    parser.bump();
                    Ok(Value::Int(token.text))
                }
                TokenKind::Float => {
                    parser.bump();
                    Ok(Value::Float(token.text))
                }
                TokenKind::String | TokenKind::BlockString => {
                    parser.bump();
                    match token.string {
                        Some(string) => Ok(Value::String(string)),
                        None => parser.error(),
                    }
                }
                TokenKind::Name => {
                    parser.bump();
                    Ok(match token.text {
                        "true" => Value::Boolean(true),
                        "false" => Value::Boolean(false),
                        "null" => Value::Null,
                        name => Value::Enum(name),
                    })
                }
                TokenKind::BracketL => {
                    parser.bump();
                    let mut values = Vec::new();
                    while !parser.at_kind(TokenKind::BracketR) {
                        if parser.at_kind(TokenKind::Eof) {
                            return parser.error();
                        }
                        values.push(parser.parse_value()?);
                    }
                    parser.expect(TokenKind::BracketR)?;
                    Ok(Value::List(values))
                }
                TokenKind::BraceL => {
                    parser.bump();
                    let mut fields = Vec::new();
                    while !parser.at_kind(TokenKind::BraceR) {
                        if parser.at_kind(TokenKind::Eof) {
                            return parser.error();
                        }
                        fields.push(parser.parse_argument()?);
                    }
                    parser.expect(TokenKind::BraceR)?;
                    Ok(Value::Object(fields))
                }
                _ => parser.error(),
            }
        })
    }

    fn parse_type(&mut self) -> Result<Type<'a>, Declined> {
        self.nest(|parser| {
            let inner = if parser.eat(TokenKind::BracketL) {
                let inner = parser.parse_type()?;
                parser.expect(TokenKind::BracketR)?;
                Type::List(Box::new(inner))
            } else {
                Type::Named(parser.expect_name()?)
            };
            if parser.eat(TokenKind::Bang) {
                Ok(Type::NonNull(Box::new(inner)))
            } else {
                Ok(inner)
            }
        })
    }

    // ---- the type system ----

    fn parse_type_system(
        &mut self,
        description: Option<StringValue>,
    ) -> Result<TypeSystem<'a>, Declined> {
        let extend = description.is_none() && self.at_name("extend");
        if extend {
            self.bump();
        }
        if !self.at_kind(TokenKind::Name) {
            return self.error();
        }
        let keyword = self.bump().text;
        match keyword {
            "schema" => {
                let directives = self.parse_directives()?;
                let operation_types = self.parse_operation_types()?;
                // `schema` on its own says nothing: the grammar requires
                // the braces on a definition, and an extension needs at
                // least a directive or a root type to add.
                if operation_types.is_empty() && (!extend || directives.is_empty()) {
                    return self.error();
                }
                Ok(TypeSystem::Schema(SchemaDefinition {
                    extend,
                    description,
                    directives,
                    operation_types,
                }))
            }
            "directive" => {
                self.expect(TokenKind::At)?;
                let name = self.expect_name()?;
                if extend {
                    let directives = self.parse_directives()?;
                    if directives.is_empty() {
                        return self.error();
                    }
                    return Ok(TypeSystem::Directive(DirectiveDefinition {
                        extend,
                        description,
                        name,
                        arguments: Vec::new(),
                        directives,
                        repeatable: false,
                        locations: Vec::new(),
                    }));
                }
                let arguments = self.parse_input_value_definitions(TokenKind::ParenL)?;
                let directives = self.parse_directives()?;
                let repeatable = self.eat_name("repeatable");
                if !self.eat_name("on") {
                    return self.error();
                }
                let mut locations = Vec::new();
                self.eat(TokenKind::Pipe);
                locations.push(self.expect_name()?);
                while self.eat(TokenKind::Pipe) {
                    locations.push(self.expect_name()?);
                }
                Ok(TypeSystem::Directive(DirectiveDefinition {
                    extend,
                    description,
                    name,
                    arguments,
                    directives,
                    repeatable,
                    locations,
                }))
            }
            "scalar" | "type" | "interface" | "union" | "enum" | "input" => {
                let kind = match keyword {
                    "scalar" => TypeKind::Scalar,
                    "type" => TypeKind::Object,
                    "interface" => TypeKind::Interface,
                    "union" => TypeKind::Union,
                    "enum" => TypeKind::Enum,
                    _ => TypeKind::Input,
                };
                self.parse_type_definition(kind, extend, description)
                    .map(TypeSystem::Type)
            }
            _ => self.error(),
        }
    }

    fn parse_type_definition(
        &mut self,
        kind: TypeKind,
        extend: bool,
        description: Option<StringValue>,
    ) -> Result<TypeDefinition<'a>, Declined> {
        let name = self.expect_name()?;
        let mut definition = TypeDefinition {
            kind,
            extend,
            description,
            name,
            interfaces: Vec::new(),
            directives: Vec::new(),
            fields: Vec::new(),
            input_fields: Vec::new(),
            values: Vec::new(),
            types: Vec::new(),
        };

        if matches!(kind, TypeKind::Object | TypeKind::Interface) && self.eat_name("implements") {
            self.eat(TokenKind::Amp);
            definition.interfaces.push(self.expect_name()?);
            while self.eat(TokenKind::Amp) {
                definition.interfaces.push(self.expect_name()?);
            }
        }
        definition.directives = self.parse_directives()?;

        match kind {
            TypeKind::Scalar => {}
            TypeKind::Object | TypeKind::Interface => {
                definition.fields = self.parse_field_definitions()?;
            }
            TypeKind::Input => {
                definition.input_fields = self.parse_input_value_definitions(TokenKind::BraceL)?;
            }
            TypeKind::Enum => {
                definition.values = self.parse_enum_values()?;
            }
            TypeKind::Union => {
                if self.eat(TokenKind::Equals) {
                    self.eat(TokenKind::Pipe);
                    definition.types.push(self.expect_name()?);
                    while self.eat(TokenKind::Pipe) {
                        definition.types.push(self.expect_name()?);
                    }
                }
            }
        }

        // `extend type T` with nothing after the name adds nothing, and
        // GraphQL rejects it.
        if extend
            && definition.interfaces.is_empty()
            && definition.directives.is_empty()
            && definition.fields.is_empty()
            && definition.input_fields.is_empty()
            && definition.values.is_empty()
            && definition.types.is_empty()
        {
            return self.error();
        }
        Ok(definition)
    }

    fn parse_operation_types(&mut self) -> Result<Vec<OperationTypeDefinition<'a>>, Declined> {
        if !self.eat(TokenKind::BraceL) {
            return Ok(Vec::new());
        }
        let mut operation_types = Vec::new();
        while !self.at_kind(TokenKind::BraceR) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            let start = self.offset();
            let operation = match self.expect_name()? {
                "query" => OperationType::Query,
                "mutation" => OperationType::Mutation,
                "subscription" => OperationType::Subscription,
                _ => return self.error(),
            };
            self.expect(TokenKind::Colon)?;
            let ty = self.expect_name()?;
            operation_types.push(OperationTypeDefinition {
                operation,
                ty,
                span: Span {
                    start,
                    end: self.previous_end(),
                },
            });
        }
        self.expect(TokenKind::BraceR)?;
        if operation_types.is_empty() {
            return self.error();
        }
        Ok(operation_types)
    }

    fn parse_field_definitions(&mut self) -> Result<Vec<FieldDefinition<'a>>, Declined> {
        if !self.eat(TokenKind::BraceL) {
            return Ok(Vec::new());
        }
        let mut fields = Vec::new();
        while !self.at_kind(TokenKind::BraceR) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            let start = self.offset();
            let description = self.parse_description();
            let name = self.expect_name()?;
            let arguments = self.parse_input_value_definitions(TokenKind::ParenL)?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let directives = self.parse_directives()?;
            fields.push(FieldDefinition {
                description,
                name,
                arguments,
                ty,
                directives,
                span: Span {
                    start,
                    end: self.previous_end(),
                },
            });
        }
        self.expect(TokenKind::BraceR)?;
        if fields.is_empty() {
            return self.error();
        }
        Ok(fields)
    }

    /// The argument list of a field or directive definition (`open` is
    /// `(`), and the field list of an `input` (`open` is `{`). The two are
    /// the same production with different brackets.
    fn parse_input_value_definitions(
        &mut self,
        open: TokenKind,
    ) -> Result<Vec<InputValueDefinition<'a>>, Declined> {
        if !self.eat(open) {
            return Ok(Vec::new());
        }
        let close = if open == TokenKind::ParenL {
            TokenKind::ParenR
        } else {
            TokenKind::BraceR
        };
        let mut definitions = Vec::new();
        while !self.at_kind(close) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            let start = self.offset();
            let description = self.parse_description();
            let name = self.expect_name()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let default_value = if self.eat(TokenKind::Equals) {
                Some(self.parse_value()?)
            } else {
                None
            };
            let directives = self.parse_directives()?;
            definitions.push(InputValueDefinition {
                description,
                name,
                ty,
                default_value,
                directives,
                span: Span {
                    start,
                    end: self.previous_end(),
                },
            });
        }
        self.expect(close)?;
        if definitions.is_empty() {
            return self.error();
        }
        Ok(definitions)
    }

    fn parse_enum_values(&mut self) -> Result<Vec<EnumValueDefinition<'a>>, Declined> {
        if !self.eat(TokenKind::BraceL) {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        while !self.at_kind(TokenKind::BraceR) {
            if self.at_kind(TokenKind::Eof) {
                return self.error();
            }
            let start = self.offset();
            let description = self.parse_description();
            let name = self.expect_name()?;
            if matches!(name, "true" | "false" | "null") {
                return self.error();
            }
            let directives = self.parse_directives()?;
            values.push(EnumValueDefinition {
                description,
                name,
                directives,
                span: Span {
                    start,
                    end: self.previous_end(),
                },
            });
        }
        self.expect(TokenKind::BraceR)?;
        if values.is_empty() {
            return self.error();
        }
        Ok(values)
    }
}

/// What [`Parser::parse_type_system`] returns before it becomes a
/// [`DefinitionKind`].
enum TypeSystem<'a> {
    Schema(SchemaDefinition<'a>),
    Type(TypeDefinition<'a>),
    Directive(DirectiveDefinition<'a>),
}

impl<'a> DefinitionKind<'a> {
    fn from_type_system(definition: TypeSystem<'a>) -> Self {
        match definition {
            TypeSystem::Schema(schema) => Self::Schema(schema),
            TypeSystem::Type(ty) => Self::Type(ty),
            TypeSystem::Directive(directive) => Self::Directive(directive),
        }
    }
}
