//! The GraphQL syntax tree, shaped like the one Prettier prints from.
//!
//! The node names are graphql-js's, because Prettier's GraphQL printer is a
//! `switch` over exactly those `kind` strings and the two are meant to be
//! read side by side. Where this tree differs it is to say something the
//! `kind` string leaves implicit:
//!
//! * [`OperationDefinition::operation`] is an [`Option`], because Prettier
//!   decides whether to print the `query` keyword by looking at whether the
//!   source starts the definition with `{` — graphql-js fills `operation` in
//!   with `query` for the shorthand form. Modelling the absence is the same
//!   question asked of the grammar instead of of the source text.
//! * Only the nodes that appear in a *sequence* carry a [`Span`], because
//!   the only thing the printer asks the source is whether a blank line
//!   follows a sequence element (Prettier's `isNextLineEmpty`). Nothing else
//!   needs a location, and a location nobody reads is a field that can go
//!   stale.

/// A half-open byte range of the document text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

/// A whole document: the text between two `${}` holes, or the whole
/// template when it has none.
#[derive(Debug, Clone, PartialEq)]
pub struct Document<'a> {
    /// Its definitions, in source order. Never empty — a document with no
    /// definition is a syntax error, which is what GraphQL says too.
    pub definitions: Vec<Definition<'a>>,
}

/// One top-level definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition<'a> {
    /// Which definition it is.
    pub kind: DefinitionKind<'a>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// The definitions a document can hold: the executable ones, the type
/// system ones, and the type system extensions.
#[derive(Debug, Clone, PartialEq)]
pub enum DefinitionKind<'a> {
    /// `query Q { … }`, or the bare `{ … }` shorthand.
    Operation(OperationDefinition<'a>),
    /// `fragment F on T { … }`.
    Fragment(FragmentDefinition<'a>),
    /// `schema { … }` and `extend schema …`.
    Schema(SchemaDefinition<'a>),
    /// `scalar S`, `type T`, `interface I`, `union U`, `enum E`, `input I`,
    /// and the `extend` form of each.
    Type(TypeDefinition<'a>),
    /// `directive @d on FIELD`.
    Directive(DirectiveDefinition<'a>),
}

/// `query`, `mutation` or `subscription`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// `query`
    Query,
    /// `mutation`
    Mutation,
    /// `subscription`
    Subscription,
}

impl OperationType {
    /// The keyword, as it is written.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

/// `query Q($x: Int) @dir { … }`, or `{ … }` with `operation` absent.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationDefinition<'a> {
    /// The keyword, or [`None`] for the anonymous shorthand.
    pub operation: Option<OperationType>,
    /// The operation name.
    pub name: Option<&'a str>,
    /// `($x: Int = 1)`.
    pub variable_definitions: Vec<VariableDefinition<'a>>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `{ … }`.
    pub selection_set: SelectionSet<'a>,
}

/// `fragment F on T { … }`.
///
/// `variable_definitions` is the fragment-arguments extension
/// (`fragment F($x: Int) on T`), which Prettier's printer prints and Relay
/// uses.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentDefinition<'a> {
    /// The fragment name.
    pub name: &'a str,
    /// `($x: Int)`, the fragment-arguments extension.
    pub variable_definitions: Vec<VariableDefinition<'a>>,
    /// The type after `on`.
    pub type_condition: &'a str,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `{ … }`.
    pub selection_set: SelectionSet<'a>,
}

/// `$x: Int! = 3 @dir`.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableDefinition<'a> {
    /// The variable name, without the `$`.
    pub variable: &'a str,
    /// Its type.
    pub ty: Type<'a>,
    /// `= 3`.
    pub default_value: Option<Value<'a>>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
}

/// `{ … }`, which the printer always breaks.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionSet<'a> {
    /// Its selections, in source order. Never empty.
    pub selections: Vec<Selection<'a>>,
}

/// One selection inside a selection set.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection<'a> {
    /// Which selection it is.
    pub kind: SelectionKind<'a>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// The three kinds of selection.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionKind<'a> {
    /// `alias: name(args) @dir { … }`.
    Field(Field<'a>),
    /// `...Frag(args) @dir`.
    FragmentSpread(FragmentSpread<'a>),
    /// `... on T @dir { … }`.
    InlineFragment(InlineFragment<'a>),
}

/// `alias: name(args) @dir { … }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<'a> {
    /// The alias before the `:`.
    pub alias: Option<&'a str>,
    /// The field name.
    pub name: &'a str,
    /// `(x: 1)`.
    pub arguments: Vec<Argument<'a>>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `{ … }`, absent on a leaf.
    pub selection_set: Option<SelectionSet<'a>>,
}

/// `...Frag(args) @dir`. The arguments are the fragment-arguments
/// extension, which Prettier's printer prints.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentSpread<'a> {
    /// The fragment name.
    pub name: &'a str,
    /// `(x: 1)`.
    pub arguments: Vec<Argument<'a>>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
}

/// `... on T @dir { … }`.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineFragment<'a> {
    /// The type after `on`, absent for `... @dir { … }`.
    pub type_condition: Option<&'a str>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `{ … }`.
    pub selection_set: SelectionSet<'a>,
}

/// `name: value`, in an argument list or an object value.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument<'a> {
    /// The argument name.
    pub name: &'a str,
    /// Its value.
    pub value: Value<'a>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// `@name(args)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Directive<'a> {
    /// The directive name, without the `@`.
    pub name: &'a str,
    /// `(x: 1)`.
    pub arguments: Vec<Argument<'a>>,
}

/// A value: an argument's, a default's, or one inside a list or object.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// `$x`.
    Variable(&'a str),
    /// `1`, kept as written.
    Int(&'a str),
    /// `1.5e3`, kept as written.
    Float(&'a str),
    /// `"a"` or `"""a"""`.
    String(StringValue),
    /// `true` or `false`.
    Boolean(bool),
    /// `null`.
    Null,
    /// `ENUM_VALUE`.
    Enum(&'a str),
    /// `[1, 2]`.
    List(Vec<Value<'a>>),
    /// `{ a: 1 }`.
    Object(Vec<Argument<'a>>),
}

/// A string value, after the escapes and the block-string indentation have
/// been resolved — which is what Prettier prints from, and why a printed
/// string is not always spelled the way the source spelled it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringValue {
    /// The value the string denotes.
    pub value: String,
    /// Whether it was written as a `"""` block string.
    pub block: bool,
}

/// `Int`, `[Int]`, `Int!`.
#[derive(Debug, Clone, PartialEq)]
pub enum Type<'a> {
    /// A type name.
    Named(&'a str),
    /// `[T]`.
    List(Box<Type<'a>>),
    /// `T!`.
    NonNull(Box<Type<'a>>),
}

/// `schema @dir { query: Q }`, or `extend schema …`.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDefinition<'a> {
    /// Whether it was written `extend schema`.
    pub extend: bool,
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `query: Q`, `mutation: M`, `subscription: S`.
    pub operation_types: Vec<OperationTypeDefinition<'a>>,
}

/// `query: Q` inside a schema definition.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationTypeDefinition<'a> {
    /// Which root it names.
    pub operation: OperationType,
    /// The type name.
    pub ty: &'a str,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// Which of the six type definitions this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    /// `scalar S`.
    Scalar,
    /// `type T`.
    Object,
    /// `interface I`.
    Interface,
    /// `union U`.
    Union,
    /// `enum E`.
    Enum,
    /// `input I`.
    Input,
}

/// A type definition or its `extend` form. The six share enough shape that
/// Prettier prints them from three arms of one `switch`, and keeping them
/// one node keeps the printer readable next to it.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDefinition<'a> {
    /// Which keyword.
    pub kind: TypeKind,
    /// Whether it was written `extend`.
    pub extend: bool,
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// The type name.
    pub name: &'a str,
    /// `implements A & B`, on objects and interfaces.
    pub interfaces: Vec<&'a str>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// `{ … }` on an object or interface.
    pub fields: Vec<FieldDefinition<'a>>,
    /// `{ … }` on an input object.
    pub input_fields: Vec<InputValueDefinition<'a>>,
    /// `{ … }` on an enum.
    pub values: Vec<EnumValueDefinition<'a>>,
    /// `= A | B` on a union.
    pub types: Vec<&'a str>,
}

/// `name(args): T @dir` inside a `type` or `interface`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDefinition<'a> {
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// The field name.
    pub name: &'a str,
    /// `(x: Int = 1)`.
    pub arguments: Vec<InputValueDefinition<'a>>,
    /// The field type.
    pub ty: Type<'a>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// `name: T = 1 @dir` inside an `input`, or in an argument list.
#[derive(Debug, Clone, PartialEq)]
pub struct InputValueDefinition<'a> {
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// The argument or input field name.
    pub name: &'a str,
    /// Its type.
    pub ty: Type<'a>,
    /// `= 1`.
    pub default_value: Option<Value<'a>>,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// `VALUE @dir` inside an `enum`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValueDefinition<'a> {
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// The enum value.
    pub name: &'a str,
    /// `@dir(…)`.
    pub directives: Vec<Directive<'a>>,
    /// Where it sits, for the blank line after it.
    pub span: Span,
}

/// `directive @d(x: Int) repeatable on FIELD | QUERY`, and the
/// `extend directive @d @other` form.
///
/// Neither the directives on a definition nor the `extend` form is in the
/// GraphQL specification, and both are in Relay's schema fixtures and in
/// Prettier's printer, so both are here.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveDefinition<'a> {
    /// Whether it was written `extend directive`, which takes no arguments,
    /// no `repeatable` and no locations.
    pub extend: bool,
    /// The `"""…"""` in front of it.
    pub description: Option<StringValue>,
    /// The directive name, without the `@`.
    pub name: &'a str,
    /// `(x: Int = 1)`.
    pub arguments: Vec<InputValueDefinition<'a>>,
    /// `@dir(…)` on the definition itself.
    pub directives: Vec<Directive<'a>>,
    /// Whether it is `repeatable`.
    pub repeatable: bool,
    /// The locations after `on`. Empty only on the `extend` form.
    pub locations: Vec<&'a str>,
}
