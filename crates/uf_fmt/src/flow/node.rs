//! One view over every node in the port's tree, so comments can find the
//! node they belong to.
//!
//! The port's AST is a family of Rust enums and structs with no common
//! trait, which is fine for a printer that matches on each but not for the
//! comment attacher, which needs to ask any node three things: where does it
//! start and end, what is its identity, and what are its children in source
//! order. [`NodeRef`] is the answer — a borrowed, `Copy` handle over the
//! node kinds that Prettier treats as attachment targets, at the same
//! granularity as its ESTree nodes so its attachment rules port unchanged.
//!
//! Identity is the node's address. The tree is immutable and heap-allocated
//! behind `Arc`s for as long as the formatter runs, so an address names one
//! node and only one; the discriminant is folded in so two node kinds that
//! happen to start at the same allocation cannot collide.

use uf_flow::Loc;
use uf_flow::ast::{
    self, Identifier, PrivateName, StringLiteral, Variance, class, expression, function, jsx,
    match_, match_pattern, pattern, statement, types,
};

/// Every generic in the port's tree is instantiated with `Loc` twice.
pub type Program = ast::Program<Loc, Loc>;
/// A statement node.
pub type Statement = statement::Statement<Loc, Loc>;
/// An expression node.
pub type Expression = expression::Expression<Loc, Loc>;
/// A type node.
pub type Type = types::Type<Loc, Loc>;
/// A binding or assignment pattern.
pub type Pattern = pattern::Pattern<Loc, Loc>;
/// A function, in any of the places one can appear.
pub type Function = function::Function<Loc, Loc>;
/// A class, declared or as an expression.
pub type Class = class::Class<Loc, Loc>;
/// A match pattern.
pub type MatchPattern = match_pattern::MatchPattern<Loc, Loc>;
/// A `match` case whose body is an expression.
pub type MatchExpressionCase = match_::Case<Loc, Loc, Expression>;
/// A `match` case whose body is a statement.
pub type MatchStatementCase = match_::Case<Loc, Loc, Statement>;

/// The identity of a node: its address and its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey(usize, u8);

/// A borrowed handle over any attachable node.
#[derive(Clone, Copy)]
pub enum NodeRef<'a> {
    /// The whole file.
    Program(&'a Program),
    /// Any statement.
    Statement(&'a Statement),
    /// Any expression.
    Expression(&'a Expression),
    /// Any type.
    Type(&'a Type),
    /// Any binding pattern.
    Pattern(&'a Pattern),
    /// An identifier standing alone: a name, a label, a key.
    Identifier(&'a Identifier<Loc, Loc>),
    /// A `#private` name.
    PrivateName(&'a PrivateName<Loc>),
    /// A string literal standing alone: a module source, a key.
    StringLiteral(&'a Loc, &'a StringLiteral<Loc>),
    /// A number literal standing alone: a key.
    NumberLiteral(&'a Loc, &'a ast::NumberLiteral<Loc>),
    /// A bigint literal standing alone: a key.
    BigIntLiteral(&'a Loc, &'a ast::BigIntLiteral<Loc>),
    /// A boolean literal standing alone: an enum member value.
    BooleanLiteral(&'a Loc, &'a ast::BooleanLiteral<Loc>),
    /// A function that is not itself a statement or expression: a method's
    /// value, a getter, a setter.
    FunctionValue(&'a Loc, &'a Function),
    /// A block that is not itself a statement: a function body, a `try`
    /// block, a `catch` body, a `declare module` body.
    Block(&'a Loc, &'a statement::Block<Loc, Loc>),
    /// A function parameter.
    Param(&'a function::Param<Loc, Loc>),
    /// A function's `...rest` parameter.
    RestParam(&'a function::RestParam<Loc, Loc>),
    /// A function's `this` parameter.
    ThisParam(&'a function::ThisParam<Loc, Loc>),
    /// One `name = init` of a variable declaration.
    Declarator(&'a statement::variable::Declarator<Loc, Loc>),
    /// One `case` of a switch.
    SwitchCase(&'a statement::switch::Case<Loc, Loc>),
    /// A `catch` clause.
    CatchClause(&'a statement::try_::CatchClause<Loc, Loc>),
    /// The `else` branch of an `if`.
    Alternate(&'a statement::if_::Alternate<Loc, Loc>),
    /// A class body.
    ClassBody(&'a class::Body<Loc, Loc>),
    /// One member of a class body.
    ClassMember(&'a class::BodyElement<Loc, Loc>),
    /// A decorator.
    Decorator(&'a class::Decorator<Loc, Loc>),
    /// One interface a class implements.
    ClassImplements(&'a class::implements::Interface<Loc, Loc>),
    /// One property of an object literal.
    ObjectProperty(&'a expression::object::Property<Loc, Loc>),
    /// A `...spread` in an array or argument list.
    Spread(&'a expression::SpreadElement<Loc, Loc>),
    /// One property of an object pattern.
    PatternProperty(&'a pattern::object::Property<Loc, Loc>),
    /// One element of an array pattern carrying a default.
    PatternElement(&'a pattern::array::NormalElement<Loc, Loc>),
    /// A `...rest` in a pattern.
    PatternRest(&'a pattern::RestElement<Loc, Loc>),
    /// A `: T` annotation.
    Annotation(&'a types::Annotation<Loc, Loc>),
    /// A type parameter list.
    TypeParams(&'a types::TypeParams<Loc, Loc>),
    /// One type parameter.
    TypeParam(&'a types::TypeParam<Loc, Loc>),
    /// A type argument list on a type.
    TypeArgs(&'a types::TypeArgs<Loc, Loc>),
    /// A type argument list on a call, `new`, JSX element, or tagged
    /// template.
    CallTypeArgs(&'a expression::CallTypeArgs<Loc, Loc>),
    /// The `_` of an implicit type argument.
    ImplicitTypeArg(&'a expression::CallTypeArgImplicit<Loc, Loc>),
    /// One property of an object type.
    ObjectTypeProperty(&'a types::object::Property<Loc, Loc>),
    /// An object type that is not itself a type node: an interface or
    /// `declare class` body.
    ObjectType(&'a Loc, &'a types::Object<Loc, Loc>),
    /// A function type that is not itself a type node: a call property, an
    /// object type getter, an abstract method.
    FunctionType(&'a Loc, &'a types::Function<Loc, Loc>),
    /// One parameter of a function type.
    FunctionTypeParam(&'a types::function::Param<Loc, Loc>),
    /// The `...rest` of a function type.
    FunctionTypeRest(&'a types::function::RestParam<Loc, Loc>),
    /// The `this` of a function type.
    FunctionTypeThis(&'a types::function::ThisParam<Loc, Loc>),
    /// One parameter of a component type.
    ComponentTypeParam(&'a types::component_params::Param<Loc, Loc>),
    /// The `...rest` of a component type.
    ComponentTypeRest(&'a types::component_params::RestParam<Loc, Loc>),
    /// A labeled or spread tuple element.
    TupleElement(&'a types::tuple::Element<Loc, Loc>),
    /// A variance sigil.
    Variance(&'a Variance<Loc>),
    /// A type guard `x is T`.
    TypeGuard(&'a types::TypeGuard<Loc, Loc>),
    /// A `%checks` predicate.
    Predicate(&'a types::Predicate<Loc, Loc>),
    /// One `extends` of an interface or `declare class`, or a mixin.
    InterfaceExtends(&'a (Loc, types::Generic<Loc, Loc>)),
    /// A qualified type name `A.B`.
    QualifiedType(&'a types::generic::Qualified<Loc, Loc>),
    /// A qualified `typeof` target `a.b`.
    QualifiedTypeof(&'a types::typeof_::Qualified<Loc, Loc>),
    /// An `import("m")` type.
    ImportType(&'a types::generic::ImportType<Loc, Loc>),
    /// A `renders` clause.
    Renders(&'a Loc, &'a types::Renders<Loc, Loc>),
    /// A named import.
    ImportSpecifier(&'a statement::import_declaration::NamedSpecifier<Loc, Loc>),
    /// A default import.
    ImportDefault(&'a statement::import_declaration::DefaultIdentifier<Loc, Loc>),
    /// A namespace import `* as ns`.
    ImportNamespace(&'a (Loc, Identifier<Loc, Loc>)),
    /// One `with { key: "value" }` attribute.
    ImportAttribute(&'a statement::import_declaration::ImportAttribute<Loc, Loc>),
    /// A named export.
    ExportSpecifier(&'a statement::export_named_declaration::ExportSpecifier<Loc, Loc>),
    /// An `export *` or `export * as ns`.
    ExportBatch(&'a statement::export_named_declaration::ExportBatchSpecifier<Loc, Loc>),
    /// The declaration inside a `declare export`.
    DeclareExportInner(&'a statement::declare_export_declaration::Declaration<Loc, Loc>),
    /// A `declare function` that is not itself a statement.
    DeclareFunction(&'a Loc, &'a statement::DeclareFunction<Loc, Loc>),
    /// One parameter of a component declaration.
    ComponentParam(&'a statement::component_params::Param<Loc, Loc>),
    /// The `...rest` of a component declaration.
    ComponentRest(&'a statement::component_params::RestParam<Loc, Loc>),
    /// An enum body.
    EnumBody(&'a statement::enum_declaration::Body<Loc>),
    /// One enum member.
    EnumMember(&'a statement::enum_declaration::Member<Loc>),
    /// One case of a `match` expression.
    MatchExpressionCase(&'a MatchExpressionCase),
    /// One case of a `match` statement.
    MatchStatementCase(&'a MatchStatementCase),
    /// Any match pattern.
    MatchPattern(&'a MatchPattern),
    /// One property of a match object pattern.
    MatchProperty(&'a match_pattern::object_pattern::Property<Loc, Loc>),
    /// The `...rest` of a match object or array pattern.
    MatchRest(&'a match_pattern::RestPattern<Loc, Loc>),
    /// A `const x` binding inside a match pattern.
    MatchBinding(&'a Loc, &'a match_pattern::BindingPattern<Loc, Loc>),
    /// A member pattern used as a base or constructor rather than as a
    /// pattern of its own.
    MatchMember(&'a match_pattern::MemberPattern<Loc, Loc>),
    /// A JSX child that is not an expression: an element, fragment, text,
    /// spread or expression container between tags.
    JsxChild(&'a jsx::Child<Loc, Loc>),
    /// An opening tag.
    JsxOpening(&'a jsx::Opening<Loc, Loc>),
    /// A closing tag.
    JsxClosing(&'a jsx::Closing<Loc, Loc>),
    /// One attribute.
    JsxAttribute(&'a jsx::Attribute<Loc, Loc>),
    /// A `{...spread}` attribute.
    JsxSpreadAttribute(&'a jsx::SpreadAttribute<Loc, Loc>),
    /// A `{expression}` attribute value.
    JsxExpressionContainer(&'a Loc, &'a jsx::ExpressionContainer<Loc, Loc>),
}

/// A `Loc` carried by a variant, stored for the cases where a node's
/// location lives beside it rather than inside it.
fn statement_loc(statement: &Statement) -> &Loc {
    statement.loc()
}

impl<'a> NodeRef<'a> {
    /// Where the node is in the source.
    pub fn loc(&self) -> Loc {
        let loc: &Loc = match *self {
            NodeRef::Program(node) => &node.loc,
            NodeRef::Statement(node) => statement_loc(node),
            NodeRef::Expression(node) => node.loc(),
            NodeRef::Type(node) => node.loc(),
            NodeRef::Pattern(node) => node.loc(),
            NodeRef::Identifier(node) => &node.loc,
            NodeRef::PrivateName(node) => &node.loc,
            NodeRef::StringLiteral(loc, _)
            | NodeRef::NumberLiteral(loc, _)
            | NodeRef::BigIntLiteral(loc, _)
            | NodeRef::BooleanLiteral(loc, _)
            | NodeRef::FunctionValue(loc, _)
            | NodeRef::Block(loc, _)
            | NodeRef::ObjectType(loc, _)
            | NodeRef::FunctionType(loc, _)
            | NodeRef::Renders(loc, _)
            | NodeRef::DeclareFunction(loc, _)
            | NodeRef::MatchBinding(loc, _)
            | NodeRef::JsxExpressionContainer(loc, _) => loc,
            NodeRef::Param(node) => match node {
                function::Param::RegularParam { loc, .. }
                | function::Param::ParamProperty { loc, .. } => loc,
            },
            NodeRef::RestParam(node) => &node.loc,
            NodeRef::ThisParam(node) => &node.loc,
            NodeRef::Declarator(node) => &node.loc,
            NodeRef::SwitchCase(node) => &node.loc,
            NodeRef::CatchClause(node) => &node.loc,
            NodeRef::Alternate(node) => &node.loc,
            NodeRef::ClassBody(node) => &node.loc,
            NodeRef::ClassMember(node) => match node {
                class::BodyElement::Method(member) => &member.loc,
                class::BodyElement::Property(member) => &member.loc,
                class::BodyElement::PrivateField(member) => &member.loc,
                class::BodyElement::StaticBlock(member) => &member.loc,
                class::BodyElement::DeclareMethod(member) => &member.loc,
                class::BodyElement::AbstractMethod(member) => &member.loc,
                class::BodyElement::AbstractProperty(member) => &member.loc,
                class::BodyElement::IndexSignature(member) => &member.loc,
            },
            NodeRef::Decorator(node) => &node.loc,
            NodeRef::ClassImplements(node) => &node.loc,
            NodeRef::ObjectProperty(node) => match node {
                expression::object::Property::NormalProperty(property) => property.loc(),
                expression::object::Property::SpreadProperty(property) => &property.loc,
            },
            NodeRef::Spread(node) => &node.loc,
            NodeRef::PatternProperty(node) => match node {
                pattern::object::Property::NormalProperty(property) => &property.loc,
                pattern::object::Property::RestElement(rest) => &rest.loc,
            },
            NodeRef::PatternElement(node) => &node.loc,
            NodeRef::PatternRest(node) => &node.loc,
            NodeRef::Annotation(node) => &node.loc,
            NodeRef::TypeParams(node) => &node.loc,
            NodeRef::TypeParam(node) => &node.loc,
            NodeRef::TypeArgs(node) => &node.loc,
            NodeRef::CallTypeArgs(node) => &node.loc,
            NodeRef::ImplicitTypeArg(node) => &node.loc,
            NodeRef::ObjectTypeProperty(node) => match node {
                types::object::Property::NormalProperty(property) => &property.loc,
                types::object::Property::SpreadProperty(property) => &property.loc,
                types::object::Property::Indexer(property) => &property.loc,
                types::object::Property::CallProperty(property) => &property.loc,
                types::object::Property::InternalSlot(property) => &property.loc,
                types::object::Property::MappedType(property) => &property.loc,
                types::object::Property::PrivateField(property) => &property.loc,
            },
            NodeRef::FunctionTypeParam(node) => &node.loc,
            NodeRef::FunctionTypeRest(node) => &node.loc,
            NodeRef::FunctionTypeThis(node) => &node.loc,
            NodeRef::ComponentTypeParam(node) => &node.loc,
            NodeRef::ComponentTypeRest(node) => &node.loc,
            NodeRef::TupleElement(node) => node.loc(),
            NodeRef::Variance(node) => &node.loc,
            NodeRef::TypeGuard(node) => &node.loc,
            NodeRef::Predicate(node) => &node.loc,
            NodeRef::InterfaceExtends((loc, _)) => loc,
            NodeRef::QualifiedType(node) => &node.loc,
            NodeRef::QualifiedTypeof(node) => &node.loc,
            NodeRef::ImportType(node) => &node.loc,
            NodeRef::ImportSpecifier(node) => {
                let start = node.kind_loc.as_ref().unwrap_or(&node.remote.loc);
                let end = node
                    .local
                    .as_ref()
                    .map_or(&node.remote.loc, |local| &local.loc);
                return Loc::between(start, end);
            }
            NodeRef::ImportDefault(node) => &node.identifier.loc,
            NodeRef::ImportNamespace((loc, _)) => loc,
            NodeRef::ImportAttribute(node) => &node.loc,
            NodeRef::ExportSpecifier(node) => &node.loc,
            NodeRef::ExportBatch(node) => &node.loc,
            NodeRef::DeclareExportInner(node) => {
                use statement::declare_export_declaration::Declaration;
                match node {
                    Declaration::Variable { loc, .. }
                    | Declaration::Function { loc, .. }
                    | Declaration::Class { loc, .. }
                    | Declaration::Component { loc, .. }
                    | Declaration::NamedType { loc, .. }
                    | Declaration::NamedOpaqueType { loc, .. }
                    | Declaration::Interface { loc, .. }
                    | Declaration::Enum { loc, .. }
                    | Declaration::Namespace { loc, .. } => loc,
                    Declaration::DefaultType { type_ } => type_.loc(),
                }
            }
            NodeRef::ComponentParam(node) => &node.loc,
            NodeRef::ComponentRest(node) => &node.loc,
            NodeRef::EnumBody(node) => &node.loc,
            NodeRef::EnumMember(node) => {
                use statement::enum_declaration::Member;
                match node {
                    Member::BooleanMember(member) => &member.loc,
                    Member::NumberMember(member) => &member.loc,
                    Member::StringMember(member) => &member.loc,
                    Member::BigIntMember(member) => &member.loc,
                    Member::DefaultedMember(member) => &member.loc,
                }
            }
            NodeRef::MatchExpressionCase(node) => &node.loc,
            NodeRef::MatchStatementCase(node) => &node.loc,
            NodeRef::MatchPattern(node) => node.loc(),
            NodeRef::MatchProperty(node) => node.loc(),
            NodeRef::MatchRest(node) => &node.loc,
            NodeRef::MatchMember(node) => &node.loc,
            NodeRef::JsxChild(node) => node.loc(),
            NodeRef::JsxOpening(node) => &node.loc,
            NodeRef::JsxClosing(node) => &node.loc,
            NodeRef::JsxAttribute(node) => &node.loc,
            NodeRef::JsxSpreadAttribute(node) => &node.loc,
        };
        loc.clone()
    }

    /// The node's identity.
    pub fn key(&self) -> NodeKey {
        let discriminant = self.discriminant();
        let address = match *self {
            NodeRef::Program(node) => node as *const Program as usize,
            NodeRef::Statement(node) => std::ptr::from_ref(&**node) as usize,
            NodeRef::Expression(node) => std::ptr::from_ref(&**node) as usize,
            NodeRef::Type(node) => std::ptr::from_ref(&**node) as usize,
            NodeRef::Pattern(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Identifier(node) => std::ptr::from_ref(&**node) as usize,
            NodeRef::PrivateName(node) => std::ptr::from_ref(node) as usize,
            NodeRef::StringLiteral(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::NumberLiteral(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::BigIntLiteral(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::BooleanLiteral(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::FunctionValue(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::Block(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::Param(node) => std::ptr::from_ref(node) as usize,
            NodeRef::RestParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ThisParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Declarator(node) => std::ptr::from_ref(node) as usize,
            NodeRef::SwitchCase(node) => std::ptr::from_ref(node) as usize,
            NodeRef::CatchClause(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Alternate(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ClassBody(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ClassMember(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Decorator(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ClassImplements(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ObjectProperty(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Spread(node) => std::ptr::from_ref(node) as usize,
            NodeRef::PatternProperty(node) => std::ptr::from_ref(node) as usize,
            NodeRef::PatternElement(node) => std::ptr::from_ref(node) as usize,
            NodeRef::PatternRest(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Annotation(node) => std::ptr::from_ref(node) as usize,
            NodeRef::TypeParams(node) => std::ptr::from_ref(node) as usize,
            NodeRef::TypeParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::TypeArgs(node) => std::ptr::from_ref(node) as usize,
            NodeRef::CallTypeArgs(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImplicitTypeArg(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ObjectTypeProperty(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ObjectType(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::FunctionType(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::FunctionTypeParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::FunctionTypeRest(node) => std::ptr::from_ref(node) as usize,
            NodeRef::FunctionTypeThis(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ComponentTypeParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ComponentTypeRest(node) => std::ptr::from_ref(node) as usize,
            NodeRef::TupleElement(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Variance(node) => std::ptr::from_ref(node) as usize,
            NodeRef::TypeGuard(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Predicate(node) => std::ptr::from_ref(node) as usize,
            NodeRef::InterfaceExtends(node) => std::ptr::from_ref(node) as usize,
            NodeRef::QualifiedType(node) => std::ptr::from_ref(node) as usize,
            NodeRef::QualifiedTypeof(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImportType(node) => std::ptr::from_ref(node) as usize,
            NodeRef::Renders(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImportSpecifier(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImportDefault(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImportNamespace(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ImportAttribute(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ExportSpecifier(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ExportBatch(node) => std::ptr::from_ref(node) as usize,
            NodeRef::DeclareExportInner(node) => std::ptr::from_ref(node) as usize,
            NodeRef::DeclareFunction(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::ComponentParam(node) => std::ptr::from_ref(node) as usize,
            NodeRef::ComponentRest(node) => std::ptr::from_ref(node) as usize,
            NodeRef::EnumBody(node) => std::ptr::from_ref(node) as usize,
            NodeRef::EnumMember(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchExpressionCase(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchStatementCase(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchPattern(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchProperty(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchRest(node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchBinding(_, node) => std::ptr::from_ref(node) as usize,
            NodeRef::MatchMember(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxChild(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxOpening(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxClosing(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxAttribute(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxSpreadAttribute(node) => std::ptr::from_ref(node) as usize,
            NodeRef::JsxExpressionContainer(_, node) => std::ptr::from_ref(node) as usize,
        };
        NodeKey(address, discriminant)
    }

    fn discriminant(&self) -> u8 {
        // SAFETY-free: `NodeRef` is a plain enum, so its discriminant is the
        // first byte of its layout only under `repr(u8)`; reading it through
        // a match keeps this portable.
        match self {
            NodeRef::Program(_) => 0,
            NodeRef::Statement(_) => 1,
            NodeRef::Expression(_) => 2,
            NodeRef::Type(_) => 3,
            NodeRef::Pattern(_) => 4,
            NodeRef::Identifier(_) => 5,
            NodeRef::PrivateName(_) => 6,
            NodeRef::StringLiteral(..) => 7,
            NodeRef::NumberLiteral(..) => 8,
            NodeRef::BigIntLiteral(..) => 9,
            NodeRef::BooleanLiteral(..) => 10,
            NodeRef::FunctionValue(..) => 11,
            NodeRef::Block(..) => 12,
            NodeRef::Param(_) => 13,
            NodeRef::RestParam(_) => 14,
            NodeRef::ThisParam(_) => 15,
            NodeRef::Declarator(_) => 16,
            NodeRef::SwitchCase(_) => 17,
            NodeRef::CatchClause(_) => 18,
            NodeRef::Alternate(_) => 19,
            NodeRef::ClassBody(_) => 20,
            NodeRef::ClassMember(_) => 21,
            NodeRef::Decorator(_) => 22,
            NodeRef::ClassImplements(_) => 23,
            NodeRef::ObjectProperty(_) => 24,
            NodeRef::Spread(_) => 25,
            NodeRef::PatternProperty(_) => 26,
            NodeRef::PatternElement(_) => 27,
            NodeRef::PatternRest(_) => 28,
            NodeRef::Annotation(_) => 29,
            NodeRef::TypeParams(_) => 30,
            NodeRef::TypeParam(_) => 31,
            NodeRef::TypeArgs(_) => 32,
            NodeRef::CallTypeArgs(_) => 33,
            NodeRef::ImplicitTypeArg(_) => 34,
            NodeRef::ObjectTypeProperty(_) => 35,
            NodeRef::ObjectType(..) => 36,
            NodeRef::FunctionType(..) => 37,
            NodeRef::FunctionTypeParam(_) => 38,
            NodeRef::FunctionTypeRest(_) => 39,
            NodeRef::FunctionTypeThis(_) => 40,
            NodeRef::ComponentTypeParam(_) => 41,
            NodeRef::ComponentTypeRest(_) => 42,
            NodeRef::TupleElement(_) => 43,
            NodeRef::Variance(_) => 44,
            NodeRef::TypeGuard(_) => 45,
            NodeRef::Predicate(_) => 46,
            NodeRef::InterfaceExtends(_) => 47,
            NodeRef::QualifiedType(_) => 48,
            NodeRef::QualifiedTypeof(_) => 49,
            NodeRef::ImportType(_) => 50,
            NodeRef::Renders(..) => 51,
            NodeRef::ImportSpecifier(_) => 52,
            NodeRef::ImportDefault(_) => 53,
            NodeRef::ImportNamespace(_) => 54,
            NodeRef::ImportAttribute(_) => 55,
            NodeRef::ExportSpecifier(_) => 56,
            NodeRef::ExportBatch(_) => 57,
            NodeRef::DeclareExportInner(_) => 58,
            NodeRef::DeclareFunction(..) => 60,
            NodeRef::ComponentParam(_) => 68,
            NodeRef::ComponentRest(_) => 69,
            NodeRef::EnumBody(_) => 70,
            NodeRef::EnumMember(_) => 71,
            NodeRef::MatchExpressionCase(_) => 72,
            NodeRef::MatchStatementCase(_) => 73,
            NodeRef::MatchPattern(_) => 74,
            NodeRef::MatchProperty(_) => 75,
            NodeRef::MatchRest(_) => 76,
            NodeRef::MatchBinding(..) => 77,
            NodeRef::MatchMember(_) => 78,
            NodeRef::JsxChild(_) => 79,
            NodeRef::JsxOpening(_) => 80,
            NodeRef::JsxClosing(_) => 81,
            NodeRef::JsxAttribute(_) => 82,
            NodeRef::JsxSpreadAttribute(_) => 83,
            NodeRef::JsxExpressionContainer(..) => 84,
        }
    }

    /// The node's children, in the order the tree stores them (which is
    /// source order except where noted by the caller, who sorts).
    pub fn children(&self, out: &mut Vec<NodeRef<'a>>) {
        match *self {
            NodeRef::Program(node) => out.extend(node.statements.iter().map(NodeRef::Statement)),
            NodeRef::Statement(node) => statement_children(node, out),
            NodeRef::Expression(node) => expression_children(node, out),
            NodeRef::Type(node) => type_children(node, out),
            NodeRef::Pattern(node) => pattern_children(node, out),
            NodeRef::Identifier(_)
            | NodeRef::PrivateName(_)
            | NodeRef::StringLiteral(..)
            | NodeRef::NumberLiteral(..)
            | NodeRef::BigIntLiteral(..)
            | NodeRef::BooleanLiteral(..)
            | NodeRef::ImplicitTypeArg(_)
            | NodeRef::Variance(_) => {}
            NodeRef::FunctionValue(_, node) => function_children(node, out),
            NodeRef::Block(_, node) => out.extend(node.body.iter().map(NodeRef::Statement)),
            NodeRef::Param(node) => match node {
                function::Param::RegularParam {
                    argument, default, ..
                } => {
                    out.push(NodeRef::Pattern(argument));
                    if let Some(default) = default {
                        out.push(NodeRef::Expression(default));
                    }
                }
                function::Param::ParamProperty { property, .. } => {
                    class_property_children(property, out);
                }
            },
            NodeRef::RestParam(node) => out.push(NodeRef::Pattern(&node.argument)),
            NodeRef::ThisParam(node) => out.push(NodeRef::Annotation(&node.annot)),
            NodeRef::Declarator(node) => {
                out.push(NodeRef::Pattern(&node.id));
                if let Some(init) = &node.init {
                    out.push(NodeRef::Expression(init));
                }
            }
            NodeRef::SwitchCase(node) => {
                if let Some(test) = &node.test {
                    out.push(NodeRef::Expression(test));
                }
                out.extend(node.consequent.iter().map(NodeRef::Statement));
            }
            NodeRef::CatchClause(node) => {
                if let Some(param) = &node.param {
                    out.push(NodeRef::Pattern(param));
                }
                out.push(NodeRef::Block(&node.body.0, &node.body.1));
            }
            NodeRef::Alternate(node) => out.push(NodeRef::Statement(&node.body)),
            NodeRef::ClassBody(node) => out.extend(node.body.iter().map(NodeRef::ClassMember)),
            NodeRef::ClassMember(node) => class_member_children(node, out),
            NodeRef::Decorator(node) => out.push(NodeRef::Expression(&node.expression)),
            NodeRef::ClassImplements(node) => {
                generic_identifier_children(&node.id, out);
                if let Some(targs) = &node.targs {
                    out.push(NodeRef::TypeArgs(targs));
                }
            }
            NodeRef::ObjectProperty(node) => object_property_children(node, out),
            NodeRef::Spread(node) => out.push(NodeRef::Expression(&node.argument)),
            NodeRef::PatternProperty(node) => match node {
                pattern::object::Property::NormalProperty(property) => {
                    pattern_key_children(&property.key, out);
                    out.push(NodeRef::Pattern(&property.pattern));
                    if let Some(default) = &property.default {
                        out.push(NodeRef::Expression(default));
                    }
                }
                pattern::object::Property::RestElement(rest) => {
                    out.push(NodeRef::Pattern(&rest.argument));
                }
            },
            NodeRef::PatternElement(node) => {
                out.push(NodeRef::Pattern(&node.argument));
                if let Some(default) = &node.default {
                    out.push(NodeRef::Expression(default));
                }
            }
            NodeRef::PatternRest(node) => out.push(NodeRef::Pattern(&node.argument)),
            NodeRef::Annotation(node) => out.push(NodeRef::Type(&node.annotation)),
            NodeRef::TypeParams(node) => out.extend(node.params.iter().map(NodeRef::TypeParam)),
            NodeRef::TypeParam(node) => {
                if let Some(variance) = &node.variance {
                    out.push(NodeRef::Variance(variance));
                }
                out.push(NodeRef::Identifier(&node.name));
                if let types::AnnotationOrHint::Available(bound) = &node.bound {
                    out.push(NodeRef::Annotation(bound));
                }
                if let Some(default) = &node.default {
                    out.push(NodeRef::Type(default));
                }
            }
            NodeRef::TypeArgs(node) => out.extend(node.arguments.iter().map(NodeRef::Type)),
            NodeRef::CallTypeArgs(node) => {
                for argument in node.arguments.iter() {
                    match argument {
                        expression::CallTypeArg::Explicit(ty) => out.push(NodeRef::Type(ty)),
                        expression::CallTypeArg::Implicit(implicit) => {
                            out.push(NodeRef::ImplicitTypeArg(implicit));
                        }
                    }
                }
            }
            NodeRef::ObjectTypeProperty(node) => object_type_property_children(node, out),
            NodeRef::ObjectType(_, node) => {
                out.extend(node.properties.iter().map(NodeRef::ObjectTypeProperty));
            }
            NodeRef::FunctionType(_, node) => function_type_children(node, out),
            NodeRef::FunctionTypeParam(node) => match &node.param {
                types::function::ParamKind::Anonymous(ty) => out.push(NodeRef::Type(ty)),
                types::function::ParamKind::Labeled { name, annot, .. } => {
                    out.push(NodeRef::Identifier(name));
                    out.push(NodeRef::Type(annot));
                }
                types::function::ParamKind::Destructuring(pattern) => {
                    out.push(NodeRef::Pattern(pattern));
                }
            },
            NodeRef::FunctionTypeRest(node) => out.push(NodeRef::FunctionTypeParam(&node.argument)),
            NodeRef::FunctionTypeThis(node) => out.push(NodeRef::Annotation(&node.annot)),
            NodeRef::ComponentTypeParam(node) => {
                component_param_name_children(&node.name, out);
                out.push(NodeRef::Annotation(&node.annot));
            }
            NodeRef::ComponentTypeRest(node) => {
                if let Some(argument) = &node.argument {
                    out.push(NodeRef::Identifier(argument));
                }
                out.push(NodeRef::Type(&node.annot));
            }
            NodeRef::TupleElement(node) => match node {
                types::tuple::Element::UnlabeledElement { annot, .. } => {
                    out.push(NodeRef::Type(annot));
                }
                types::tuple::Element::LabeledElement { element, .. } => {
                    if let Some(variance) = &element.variance {
                        out.push(NodeRef::Variance(variance));
                    }
                    out.push(NodeRef::Identifier(&element.name));
                    out.push(NodeRef::Type(&element.annot));
                }
                types::tuple::Element::SpreadElement { element, .. } => {
                    if let Some(name) = &element.name {
                        out.push(NodeRef::Identifier(name));
                    }
                    out.push(NodeRef::Type(&element.annot));
                }
            },
            NodeRef::TypeGuard(node) => {
                out.push(NodeRef::Identifier(&node.guard.0));
                if let Some(ty) = &node.guard.1 {
                    out.push(NodeRef::Type(ty));
                }
            }
            NodeRef::Predicate(node) => {
                if let types::PredicateKind::Declared(expression) = &node.kind {
                    out.push(NodeRef::Expression(expression));
                }
            }
            NodeRef::InterfaceExtends((_, node)) => generic_children(node, out),
            NodeRef::QualifiedType(node) => {
                generic_identifier_children(&node.qualification, out);
                out.push(NodeRef::Identifier(&node.id));
            }
            NodeRef::QualifiedTypeof(node) => {
                typeof_target_children(&node.qualification, out);
                out.push(NodeRef::Identifier(&node.id));
            }
            NodeRef::ImportType(node) => {
                out.push(NodeRef::StringLiteral(&node.argument.0, &node.argument.1));
            }
            NodeRef::Renders(_, node) => out.push(NodeRef::Type(&node.argument)),
            NodeRef::ImportSpecifier(node) => {
                out.push(NodeRef::Identifier(&node.remote));
                if let Some(local) = &node.local {
                    out.push(NodeRef::Identifier(local));
                }
            }
            NodeRef::ImportDefault(node) => out.push(NodeRef::Identifier(&node.identifier)),
            NodeRef::ImportNamespace((_, node)) => out.push(NodeRef::Identifier(node)),
            NodeRef::ImportAttribute(node) => {
                match &node.key {
                    statement::import_declaration::ImportAttributeKey::Identifier(id) => {
                        out.push(NodeRef::Identifier(id));
                    }
                    statement::import_declaration::ImportAttributeKey::StringLiteral(loc, lit) => {
                        out.push(NodeRef::StringLiteral(loc, lit));
                    }
                }
                out.push(NodeRef::StringLiteral(&node.value.0, &node.value.1));
            }
            NodeRef::ExportSpecifier(node) => {
                out.push(NodeRef::Identifier(&node.local));
                if let Some(exported) = &node.exported {
                    out.push(NodeRef::Identifier(exported));
                }
            }
            NodeRef::ExportBatch(node) => {
                if let Some(specifier) = &node.specifier {
                    out.push(NodeRef::Identifier(specifier));
                }
            }
            NodeRef::DeclareExportInner(node) => {
                use statement::declare_export_declaration::Declaration;
                match node {
                    Declaration::Variable { declaration, .. } => {
                        declare_variable_children(declaration, out);
                    }
                    Declaration::Function { declaration, .. } => {
                        declare_function_children(declaration, out);
                    }
                    Declaration::Class { declaration, .. } => {
                        declare_class_children(declaration, out);
                    }
                    Declaration::Component { declaration, .. } => {
                        declare_component_children(declaration, out);
                    }
                    Declaration::DefaultType { type_ } => out.push(NodeRef::Type(type_)),
                    Declaration::NamedType { declaration, .. } => {
                        type_alias_children(declaration, out);
                    }
                    Declaration::NamedOpaqueType { declaration, .. } => {
                        opaque_type_children(declaration, out);
                    }
                    Declaration::Interface { declaration, .. } => {
                        interface_children(declaration, out);
                    }
                    Declaration::Enum { declaration, .. } => enum_children(declaration, out),
                    Declaration::Namespace { declaration, .. } => {
                        declare_namespace_children(declaration, out);
                    }
                }
            }
            NodeRef::DeclareFunction(_, node) => declare_function_children(node, out),
            NodeRef::ComponentParam(node) => {
                component_param_name_children(&node.name, out);
                out.push(NodeRef::Pattern(&node.local));
                if let Some(default) = &node.default {
                    out.push(NodeRef::Expression(default));
                }
            }
            NodeRef::ComponentRest(node) => out.push(NodeRef::Pattern(&node.argument)),
            NodeRef::EnumBody(node) => out.extend(node.members.iter().map(NodeRef::EnumMember)),
            NodeRef::EnumMember(node) => {
                use statement::enum_declaration::{Member, MemberName};
                let (name, init): (&MemberName<Loc>, Option<NodeRef<'a>>) = match node {
                    Member::BooleanMember(member) => (
                        &member.id,
                        Some(NodeRef::BooleanLiteral(&member.init.0, &member.init.1)),
                    ),
                    Member::NumberMember(member) => (
                        &member.id,
                        Some(NodeRef::NumberLiteral(&member.init.0, &member.init.1)),
                    ),
                    Member::StringMember(member) => (
                        &member.id,
                        Some(NodeRef::StringLiteral(&member.init.0, &member.init.1)),
                    ),
                    Member::BigIntMember(member) => (
                        &member.id,
                        Some(NodeRef::BigIntLiteral(&member.init.0, &member.init.1)),
                    ),
                    Member::DefaultedMember(member) => (&member.id, None),
                };
                match name {
                    MemberName::Identifier(id) => out.push(NodeRef::Identifier(id)),
                    MemberName::StringLiteral(loc, literal) => {
                        out.push(NodeRef::StringLiteral(loc, literal));
                    }
                }
                out.extend(init);
            }
            NodeRef::MatchExpressionCase(node) => {
                out.push(NodeRef::MatchPattern(&node.pattern));
                if let Some(guard) = &node.guard {
                    out.push(NodeRef::Expression(guard));
                }
                out.push(NodeRef::Expression(&node.body));
            }
            NodeRef::MatchStatementCase(node) => {
                out.push(NodeRef::MatchPattern(&node.pattern));
                if let Some(guard) = &node.guard {
                    out.push(NodeRef::Expression(guard));
                }
                out.push(NodeRef::Statement(&node.body));
            }
            NodeRef::MatchPattern(node) => match_pattern_children(node, out),
            NodeRef::MatchProperty(node) => {
                use match_pattern::object_pattern::{Key, Property};
                match node {
                    Property::Valid { property, .. } => {
                        match &property.key {
                            Key::StringLiteral((loc, literal)) => {
                                out.push(NodeRef::StringLiteral(loc, literal));
                            }
                            Key::NumberLiteral((loc, literal)) => {
                                out.push(NodeRef::NumberLiteral(loc, literal));
                            }
                            Key::BigIntLiteral((loc, literal)) => {
                                out.push(NodeRef::BigIntLiteral(loc, literal));
                            }
                            Key::Identifier(id) => out.push(NodeRef::Identifier(id)),
                        }
                        out.push(NodeRef::MatchPattern(&property.pattern));
                    }
                    Property::InvalidShorthand { identifier, .. } => {
                        out.push(NodeRef::Identifier(identifier));
                    }
                }
            }
            NodeRef::MatchRest(node) => {
                if let Some((loc, binding)) = &node.argument {
                    out.push(NodeRef::MatchBinding(loc, binding));
                }
            }
            NodeRef::MatchBinding(_, node) => out.push(NodeRef::Identifier(&node.id)),
            NodeRef::MatchMember(node) => match_member_children(node, out),
            NodeRef::JsxChild(node) => match node {
                jsx::Child::Element { inner, .. } => jsx_element_children(inner, out),
                jsx::Child::Fragment { inner, .. } => {
                    out.extend(inner.frag_children.1.iter().map(NodeRef::JsxChild));
                }
                jsx::Child::ExpressionContainer { inner, .. } => {
                    if let jsx::expression_container::Expression::Expression(expression) =
                        &inner.expression
                    {
                        out.push(NodeRef::Expression(expression));
                    }
                }
                jsx::Child::SpreadChild { inner, .. } => {
                    out.push(NodeRef::Expression(&inner.expression));
                }
                jsx::Child::Text { .. } => {}
            },
            NodeRef::JsxOpening(node) => {
                if let Some(targs) = &node.targs {
                    out.push(NodeRef::CallTypeArgs(targs));
                }
                for attribute in node.attributes.iter() {
                    match attribute {
                        jsx::OpeningAttribute::Attribute(attribute) => {
                            out.push(NodeRef::JsxAttribute(attribute));
                        }
                        jsx::OpeningAttribute::SpreadAttribute(spread) => {
                            out.push(NodeRef::JsxSpreadAttribute(spread));
                        }
                    }
                }
            }
            NodeRef::JsxClosing(_) => {}
            NodeRef::JsxAttribute(node) => match &node.value {
                Some(jsx::attribute::Value::StringLiteral((loc, literal))) => {
                    out.push(NodeRef::StringLiteral(loc, literal));
                }
                Some(jsx::attribute::Value::ExpressionContainer((loc, container))) => {
                    out.push(NodeRef::JsxExpressionContainer(loc, container));
                }
                None => {}
            },
            NodeRef::JsxSpreadAttribute(node) => out.push(NodeRef::Expression(&node.argument)),
            NodeRef::JsxExpressionContainer(_, node) => {
                if let jsx::expression_container::Expression::Expression(expression) =
                    &node.expression
                {
                    out.push(NodeRef::Expression(expression));
                }
            }
        }
    }
}

fn statement_children<'a>(node: &'a Statement, out: &mut Vec<NodeRef<'a>>) {
    use statement::StatementInner as S;
    match &**node {
        S::Block { inner, .. } => out.extend(inner.body.iter().map(NodeRef::Statement)),
        S::Break { inner, .. } => {
            if let Some(label) = &inner.label {
                out.push(NodeRef::Identifier(label));
            }
        }
        S::Continue { inner, .. } => {
            if let Some(label) = &inner.label {
                out.push(NodeRef::Identifier(label));
            }
        }
        S::ClassDeclaration { inner, .. } => class_children(inner, out),
        S::ComponentDeclaration { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.id));
            if let Some(tparams) = &inner.tparams {
                out.push(NodeRef::TypeParams(tparams));
            }
            component_params_children(&inner.params, out);
            renders_children(&inner.renders, out);
            if let Some((loc, body)) = &inner.body {
                out.push(NodeRef::Block(loc, body));
            }
        }
        S::Debugger { .. } | S::Empty { .. } => {}
        S::DeclareClass { inner, .. } => declare_class_children(inner, out),
        S::DeclareComponent { inner, .. } => declare_component_children(inner, out),
        S::DeclareEnum { inner, .. } | S::EnumDeclaration { inner, .. } => {
            enum_children(inner, out);
        }
        S::DeclareExportDeclaration { inner, .. } => {
            if let Some(declaration) = &inner.declaration {
                out.push(NodeRef::DeclareExportInner(declaration));
            }
            export_specifier_children(inner.specifiers.as_ref(), out);
            if let Some((loc, source)) = &inner.source {
                out.push(NodeRef::StringLiteral(loc, source));
            }
        }
        S::DeclareFunction { inner, .. } => declare_function_children(inner, out),
        S::DeclareInterface { inner, .. } | S::InterfaceDeclaration { inner, .. } => {
            interface_children(inner, out);
        }
        S::DeclareModule { inner, .. } => {
            match &inner.id {
                statement::declare_module::Id::Identifier(id) => out.push(NodeRef::Identifier(id)),
                statement::declare_module::Id::Literal((loc, literal)) => {
                    out.push(NodeRef::StringLiteral(loc, literal));
                }
            }
            out.push(NodeRef::Block(&inner.body.0, &inner.body.1));
        }
        S::DeclareModuleExports { inner, .. } => out.push(NodeRef::Annotation(&inner.annot)),
        S::DeclareNamespace { inner, .. } => declare_namespace_children(inner, out),
        S::DeclareTypeAlias { inner, .. } | S::TypeAlias { inner, .. } => {
            type_alias_children(inner, out);
        }
        S::DeclareOpaqueType { inner, .. } | S::OpaqueType { inner, .. } => {
            opaque_type_children(inner, out);
        }
        S::DeclareVariable { inner, .. } => declare_variable_children(inner, out),
        S::DoWhile { inner, .. } => {
            out.push(NodeRef::Statement(&inner.body));
            out.push(NodeRef::Expression(&inner.test));
        }
        S::ExportDefaultDeclaration { inner, .. } => match &inner.declaration {
            statement::export_default_declaration::Declaration::Declaration(declaration) => {
                out.push(NodeRef::Statement(declaration));
            }
            statement::export_default_declaration::Declaration::Expression(expression) => {
                out.push(NodeRef::Expression(expression));
            }
        },
        S::ExportNamedDeclaration { inner, .. } => {
            if let Some(declaration) = &inner.declaration {
                out.push(NodeRef::Statement(declaration));
            }
            export_specifier_children(inner.specifiers.as_ref(), out);
            if let Some((loc, source)) = &inner.source {
                out.push(NodeRef::StringLiteral(loc, source));
            }
        }
        S::ExportAssignment { inner, .. } => match &inner.rhs {
            statement::ExportAssignmentRhs::Expression(expression) => {
                out.push(NodeRef::Expression(expression));
            }
            statement::ExportAssignmentRhs::DeclareFunction(loc, declaration) => {
                out.push(NodeRef::DeclareFunction(loc, declaration));
            }
        },
        S::NamespaceExportDeclaration { inner, .. } => out.push(NodeRef::Identifier(&inner.id)),
        S::Expression { inner, .. } => out.push(NodeRef::Expression(&inner.expression)),
        S::For { inner, .. } => {
            match &inner.init {
                Some(statement::for_::Init::InitDeclaration((loc, declaration))) => {
                    variable_declaration_children(loc, declaration, out);
                }
                Some(statement::for_::Init::InitExpression(expression)) => {
                    out.push(NodeRef::Expression(expression));
                }
                None => {}
            }
            if let Some(test) = &inner.test {
                out.push(NodeRef::Expression(test));
            }
            if let Some(update) = &inner.update {
                out.push(NodeRef::Expression(update));
            }
            out.push(NodeRef::Statement(&inner.body));
        }
        S::ForIn { inner, .. } => {
            match &inner.left {
                statement::for_in::Left::LeftDeclaration((loc, declaration)) => {
                    variable_declaration_children(loc, declaration, out);
                }
                statement::for_in::Left::LeftPattern(pattern) => {
                    out.push(NodeRef::Pattern(pattern));
                }
            }
            out.push(NodeRef::Expression(&inner.right));
            out.push(NodeRef::Statement(&inner.body));
        }
        S::ForOf { inner, .. } => {
            match &inner.left {
                statement::for_of::Left::LeftDeclaration((loc, declaration)) => {
                    variable_declaration_children(loc, declaration, out);
                }
                statement::for_of::Left::LeftPattern(pattern) => {
                    out.push(NodeRef::Pattern(pattern));
                }
            }
            out.push(NodeRef::Expression(&inner.right));
            out.push(NodeRef::Statement(&inner.body));
        }
        S::FunctionDeclaration { inner, .. } => function_children(inner, out),
        S::If { inner, .. } => {
            out.push(NodeRef::Expression(&inner.test));
            out.push(NodeRef::Statement(&inner.consequent));
            if let Some(alternate) = &inner.alternate {
                out.push(NodeRef::Alternate(alternate));
            }
        }
        S::ImportDeclaration { inner, .. } => {
            if let Some(default) = &inner.default {
                out.push(NodeRef::ImportDefault(default));
            }
            match &inner.specifiers {
                Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(named)) => {
                    out.extend(named.iter().map(NodeRef::ImportSpecifier));
                }
                Some(statement::import_declaration::Specifier::ImportNamespaceSpecifier(ns)) => {
                    out.push(NodeRef::ImportNamespace(ns));
                }
                None => {}
            }
            out.push(NodeRef::StringLiteral(&inner.source.0, &inner.source.1));
            if let Some((_, attributes)) = &inner.attributes {
                out.extend(attributes.iter().map(NodeRef::ImportAttribute));
            }
        }
        S::ImportEqualsDeclaration { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.id));
            match &inner.module_reference {
                statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                    loc,
                    literal,
                ) => out.push(NodeRef::StringLiteral(loc, literal)),
                statement::import_equals_declaration::ModuleReference::Identifier(id) => {
                    generic_identifier_children(id, out);
                }
            }
        }
        S::Labeled { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.label));
            out.push(NodeRef::Statement(&inner.body));
        }
        S::Match { inner, .. } => {
            out.push(NodeRef::Expression(&inner.arg));
            out.extend(inner.cases.iter().map(NodeRef::MatchStatementCase));
        }
        S::RecordDeclaration { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.id));
            if let Some(tparams) = &inner.tparams {
                out.push(NodeRef::TypeParams(tparams));
            }
        }
        S::Return { inner, .. } => {
            if let Some(argument) = &inner.argument {
                out.push(NodeRef::Expression(argument));
            }
        }
        S::Switch { inner, .. } => {
            out.push(NodeRef::Expression(&inner.discriminant));
            out.extend(inner.cases.iter().map(NodeRef::SwitchCase));
        }
        S::Throw { inner, .. } => out.push(NodeRef::Expression(&inner.argument)),
        S::Try { inner, .. } => {
            out.push(NodeRef::Block(&inner.block.0, &inner.block.1));
            if let Some(handler) = &inner.handler {
                out.push(NodeRef::CatchClause(handler));
            }
            if let Some((loc, finalizer)) = &inner.finalizer {
                out.push(NodeRef::Block(loc, finalizer));
            }
        }
        S::VariableDeclaration { inner, .. } => {
            out.extend(inner.declarations.iter().map(NodeRef::Declarator));
        }
        S::While { inner, .. } => {
            out.push(NodeRef::Expression(&inner.test));
            out.push(NodeRef::Statement(&inner.body));
        }
        S::With { inner, .. } => {
            out.push(NodeRef::Expression(&inner.object));
            out.push(NodeRef::Statement(&inner.body));
        }
    }
}

fn variable_declaration_children<'a>(
    _loc: &'a Loc,
    declaration: &'a statement::VariableDeclaration<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.extend(declaration.declarations.iter().map(NodeRef::Declarator));
}

fn export_specifier_children<'a>(
    specifiers: Option<&'a statement::export_named_declaration::Specifier<Loc, Loc>>,
    out: &mut Vec<NodeRef<'a>>,
) {
    use statement::export_named_declaration::Specifier;
    match specifiers {
        Some(Specifier::ExportSpecifiers(list)) => {
            out.extend(list.iter().map(NodeRef::ExportSpecifier));
        }
        Some(Specifier::ExportBatchSpecifier(batch)) => out.push(NodeRef::ExportBatch(batch)),
        None => {}
    }
}

fn expression_children<'a>(node: &'a Expression, out: &mut Vec<NodeRef<'a>>) {
    use expression::ExpressionInner as E;
    match &**node {
        E::Array { inner, .. } => {
            for element in inner.elements.iter() {
                match element {
                    expression::ArrayElement::Expression(expression) => {
                        out.push(NodeRef::Expression(expression));
                    }
                    expression::ArrayElement::Spread(spread) => out.push(NodeRef::Spread(spread)),
                    expression::ArrayElement::Hole(_) => {}
                }
            }
        }
        E::ArrowFunction { inner, .. } | E::Function { inner, .. } => {
            function_children(inner, out);
        }
        E::AsConstExpression { inner, .. } => out.push(NodeRef::Expression(&inner.expression)),
        E::AsExpression { inner, .. } => {
            out.push(NodeRef::Expression(&inner.expression));
            out.push(NodeRef::Annotation(&inner.annot));
        }
        E::TSSatisfies { inner, .. } => {
            out.push(NodeRef::Expression(&inner.expression));
            out.push(NodeRef::Annotation(&inner.annot));
        }
        E::Assignment { inner, .. } => {
            out.push(NodeRef::Pattern(&inner.left));
            out.push(NodeRef::Expression(&inner.right));
        }
        E::Binary { inner, .. } => {
            out.push(NodeRef::Expression(&inner.left));
            out.push(NodeRef::Expression(&inner.right));
        }
        E::Logical { inner, .. } => {
            out.push(NodeRef::Expression(&inner.left));
            out.push(NodeRef::Expression(&inner.right));
        }
        E::Call { inner, .. } => call_children(inner, out),
        E::OptionalCall { inner, .. } => call_children(&inner.call, out),
        E::Class { inner, .. } => class_children(inner, out),
        E::Conditional { inner, .. } => {
            out.push(NodeRef::Expression(&inner.test));
            out.push(NodeRef::Expression(&inner.consequent));
            out.push(NodeRef::Expression(&inner.alternate));
        }
        E::Identifier { .. }
        | E::StringLiteral { .. }
        | E::BooleanLiteral { .. }
        | E::NullLiteral { .. }
        | E::NumberLiteral { .. }
        | E::BigIntLiteral { .. }
        | E::RegExpLiteral { .. }
        | E::ModuleRefLiteral { .. }
        | E::Super { .. }
        | E::This { .. } => {}
        E::Import { inner, .. } => {
            out.push(NodeRef::Expression(&inner.argument));
            if let Some(options) = &inner.options {
                out.push(NodeRef::Expression(options));
            }
        }
        E::JSXElement { inner, .. } => jsx_element_children(inner, out),
        E::JSXFragment { inner, .. } => {
            out.extend(inner.frag_children.1.iter().map(NodeRef::JsxChild));
        }
        E::Match { inner, .. } => {
            out.push(NodeRef::Expression(&inner.arg));
            out.extend(inner.cases.iter().map(NodeRef::MatchExpressionCase));
        }
        E::Member { inner, .. } => member_children(inner, out),
        E::OptionalMember { inner, .. } => member_children(&inner.member, out),
        E::MetaProperty { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.meta));
            out.push(NodeRef::Identifier(&inner.property));
        }
        E::New { inner, .. } => {
            out.push(NodeRef::Expression(&inner.callee));
            if let Some(targs) = &inner.targs {
                out.push(NodeRef::CallTypeArgs(targs));
            }
            if let Some(arguments) = &inner.arguments {
                argument_children(arguments, out);
            }
        }
        E::Object { inner, .. } => {
            out.extend(inner.properties.iter().map(NodeRef::ObjectProperty));
        }
        E::Record { inner, .. } => {
            out.push(NodeRef::Expression(&inner.constructor));
            out.extend(
                inner
                    .properties
                    .1
                    .properties
                    .iter()
                    .map(NodeRef::ObjectProperty),
            );
        }
        E::Sequence { inner, .. } => {
            out.extend(inner.expressions.iter().map(NodeRef::Expression));
        }
        E::TaggedTemplate { inner, .. } => {
            out.push(NodeRef::Expression(&inner.tag));
            if let Some(targs) = &inner.targs {
                out.push(NodeRef::CallTypeArgs(targs));
            }
            out.extend(inner.quasi.1.expressions.iter().map(NodeRef::Expression));
        }
        E::TemplateLiteral { inner, .. } => {
            out.extend(inner.expressions.iter().map(NodeRef::Expression));
        }
        E::TypeCast { inner, .. } => {
            out.push(NodeRef::Expression(&inner.expression));
            out.push(NodeRef::Annotation(&inner.annot));
        }
        E::Unary { inner, .. } => out.push(NodeRef::Expression(&inner.argument)),
        E::Update { inner, .. } => out.push(NodeRef::Expression(&inner.argument)),
        E::Yield { inner, .. } => {
            if let Some(argument) = &inner.argument {
                out.push(NodeRef::Expression(argument));
            }
        }
    }
}

fn call_children<'a>(call: &'a expression::Call<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Expression(&call.callee));
    if let Some(targs) = &call.targs {
        out.push(NodeRef::CallTypeArgs(targs));
    }
    argument_children(&call.arguments, out);
}

fn argument_children<'a>(arguments: &'a expression::ArgList<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    for argument in arguments.arguments.iter() {
        match argument {
            expression::ExpressionOrSpread::Expression(expression) => {
                out.push(NodeRef::Expression(expression));
            }
            expression::ExpressionOrSpread::Spread(spread) => out.push(NodeRef::Spread(spread)),
        }
    }
}

fn member_children<'a>(member: &'a expression::Member<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Expression(&member.object));
    match &member.property {
        expression::member::Property::PropertyIdentifier(id) => out.push(NodeRef::Identifier(id)),
        expression::member::Property::PropertyPrivateName(name) => {
            out.push(NodeRef::PrivateName(name));
        }
        expression::member::Property::PropertyExpression(expression) => {
            out.push(NodeRef::Expression(expression));
        }
    }
}

fn object_property_children<'a>(
    node: &'a expression::object::Property<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    use expression::object::{NormalProperty, Property};
    match node {
        Property::NormalProperty(NormalProperty::Init {
            key,
            value,
            shorthand,
            ..
        }) => {
            object_key_children(key, out);
            if !*shorthand {
                out.push(NodeRef::Expression(value));
            }
        }
        Property::NormalProperty(
            NormalProperty::Method { key, value, .. }
            | NormalProperty::Get { key, value, .. }
            | NormalProperty::Set { key, value, .. },
        ) => {
            object_key_children(key, out);
            out.push(NodeRef::FunctionValue(&value.0, &value.1));
        }
        Property::SpreadProperty(spread) => out.push(NodeRef::Expression(&spread.argument)),
    }
}

fn object_key_children<'a>(key: &'a expression::object::Key<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    use expression::object::Key;
    match key {
        Key::StringLiteral((loc, literal)) => out.push(NodeRef::StringLiteral(loc, literal)),
        Key::NumberLiteral((loc, literal)) => out.push(NodeRef::NumberLiteral(loc, literal)),
        Key::BigIntLiteral((loc, literal)) => out.push(NodeRef::BigIntLiteral(loc, literal)),
        Key::Identifier(id) => out.push(NodeRef::Identifier(id)),
        Key::PrivateName(name) => out.push(NodeRef::PrivateName(name)),
        Key::Computed(computed) => out.push(NodeRef::Expression(&computed.expression)),
    }
}

fn pattern_key_children<'a>(key: &'a pattern::object::Key<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    use pattern::object::Key;
    match key {
        Key::StringLiteral((loc, literal)) => out.push(NodeRef::StringLiteral(loc, literal)),
        Key::NumberLiteral((loc, literal)) => out.push(NodeRef::NumberLiteral(loc, literal)),
        Key::BigIntLiteral((loc, literal)) => out.push(NodeRef::BigIntLiteral(loc, literal)),
        Key::Identifier(id) => out.push(NodeRef::Identifier(id)),
        Key::Computed(computed) => out.push(NodeRef::Expression(&computed.expression)),
    }
}

fn function_children<'a>(node: &'a Function, out: &mut Vec<NodeRef<'a>>) {
    if let Some(id) = &node.id {
        out.push(NodeRef::Identifier(id));
    }
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    if let Some(this) = &node.params.this_ {
        out.push(NodeRef::ThisParam(this));
    }
    out.extend(node.params.params.iter().map(NodeRef::Param));
    if let Some(rest) = &node.params.rest {
        out.push(NodeRef::RestParam(rest));
    }
    match &node.return_ {
        function::ReturnAnnot::Missing(_) => {}
        function::ReturnAnnot::Available(annotation) => out.push(NodeRef::Annotation(annotation)),
        function::ReturnAnnot::TypeGuard(guard) => out.push(NodeRef::TypeGuard(&guard.guard)),
    }
    if let Some(predicate) = &node.predicate {
        out.push(NodeRef::Predicate(predicate));
    }
    match &node.body {
        function::Body::BodyBlock((loc, block)) => out.push(NodeRef::Block(loc, block)),
        function::Body::BodyExpression(expression) => out.push(NodeRef::Expression(expression)),
    }
}

fn class_children<'a>(node: &'a Class, out: &mut Vec<NodeRef<'a>>) {
    out.extend(node.class_decorators.iter().map(NodeRef::Decorator));
    if let Some(id) = &node.id {
        out.push(NodeRef::Identifier(id));
    }
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    if let Some(extends) = &node.extends {
        out.push(NodeRef::Expression(&extends.expr));
        if let Some(targs) = &extends.targs {
            out.push(NodeRef::TypeArgs(targs));
        }
    }
    if let Some(implements) = &node.implements {
        out.extend(implements.interfaces.iter().map(NodeRef::ClassImplements));
    }
    out.push(NodeRef::ClassBody(&node.body));
}

fn class_property_children<'a>(node: &'a class::Property<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.extend(node.decorators.iter().map(NodeRef::Decorator));
    if let Some(variance) = &node.variance {
        out.push(NodeRef::Variance(variance));
    }
    object_key_children(&node.key, out);
    if let types::AnnotationOrHint::Available(annotation) = &node.annot {
        out.push(NodeRef::Annotation(annotation));
    }
    if let class::property::Value::Initialized(value) = &node.value {
        out.push(NodeRef::Expression(value));
    }
}

fn class_member_children<'a>(node: &'a class::BodyElement<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    match node {
        class::BodyElement::Method(method) => {
            out.extend(method.decorators.iter().map(NodeRef::Decorator));
            object_key_children(&method.key, out);
            out.push(NodeRef::FunctionValue(&method.value.0, &method.value.1));
        }
        class::BodyElement::Property(property) => class_property_children(property, out),
        class::BodyElement::PrivateField(field) => {
            out.extend(field.decorators.iter().map(NodeRef::Decorator));
            if let Some(variance) = &field.variance {
                out.push(NodeRef::Variance(variance));
            }
            out.push(NodeRef::PrivateName(&field.key));
            if let types::AnnotationOrHint::Available(annotation) = &field.annot {
                out.push(NodeRef::Annotation(annotation));
            }
            if let class::property::Value::Initialized(value) = &field.value {
                out.push(NodeRef::Expression(value));
            }
        }
        class::BodyElement::StaticBlock(block) => {
            out.extend(block.body.iter().map(NodeRef::Statement));
        }
        class::BodyElement::DeclareMethod(method) => {
            object_key_children(&method.key, out);
            out.push(NodeRef::Annotation(&method.annot));
        }
        class::BodyElement::AbstractMethod(method) => {
            object_key_children(&method.key, out);
            out.push(NodeRef::FunctionType(&method.annot.0, &method.annot.1));
        }
        class::BodyElement::AbstractProperty(property) => {
            if let Some(variance) = &property.variance {
                out.push(NodeRef::Variance(variance));
            }
            object_key_children(&property.key, out);
            if let types::AnnotationOrHint::Available(annotation) = &property.annot {
                out.push(NodeRef::Annotation(annotation));
            }
        }
        class::BodyElement::IndexSignature(indexer) => indexer_children(indexer, out),
    }
}

fn indexer_children<'a>(node: &'a types::object::Indexer<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    if let Some(variance) = &node.variance {
        out.push(NodeRef::Variance(variance));
    }
    if let Some(id) = &node.id {
        out.push(NodeRef::Identifier(id));
    }
    out.push(NodeRef::Type(&node.key));
    out.push(NodeRef::Type(&node.value));
}

fn pattern_children<'a>(node: &'a Pattern, out: &mut Vec<NodeRef<'a>>) {
    match node {
        Pattern::Object { inner, .. } => {
            out.extend(inner.properties.iter().map(NodeRef::PatternProperty));
            if let types::AnnotationOrHint::Available(annotation) = &inner.annot {
                out.push(NodeRef::Annotation(annotation));
            }
        }
        Pattern::Array { inner, .. } => {
            for element in inner.elements.iter() {
                match element {
                    pattern::array::Element::NormalElement(element) => {
                        if element.default.is_some() {
                            out.push(NodeRef::PatternElement(element));
                        } else {
                            out.push(NodeRef::Pattern(&element.argument));
                        }
                    }
                    pattern::array::Element::RestElement(rest) => {
                        out.push(NodeRef::PatternRest(rest));
                    }
                    pattern::array::Element::Hole(_) => {}
                }
            }
            if let types::AnnotationOrHint::Available(annotation) = &inner.annot {
                out.push(NodeRef::Annotation(annotation));
            }
        }
        Pattern::Identifier { inner, .. } => {
            out.push(NodeRef::Identifier(&inner.name));
            if let types::AnnotationOrHint::Available(annotation) = &inner.annot {
                out.push(NodeRef::Annotation(annotation));
            }
        }
        Pattern::Expression { inner, .. } => out.push(NodeRef::Expression(inner)),
    }
}

fn type_children<'a>(node: &'a Type, out: &mut Vec<NodeRef<'a>>) {
    use types::TypeInner as T;
    match &**node {
        T::Any { .. }
        | T::Mixed { .. }
        | T::Empty { .. }
        | T::Void { .. }
        | T::Null { .. }
        | T::Number { .. }
        | T::BigInt { .. }
        | T::String { .. }
        | T::Boolean { .. }
        | T::Symbol { .. }
        | T::Exists { .. }
        | T::StringLiteral { .. }
        | T::NumberLiteral { .. }
        | T::BigIntLiteral { .. }
        | T::BooleanLiteral { .. }
        | T::Unknown { .. }
        | T::Never { .. }
        | T::Undefined { .. }
        | T::UniqueSymbol { .. } => {}
        T::Nullable { inner, .. } => out.push(NodeRef::Type(&inner.argument)),
        T::Function { inner, .. } | T::ConstructorType { inner, .. } => {
            function_type_children(inner, out);
        }
        T::Component { inner, .. } => {
            if let Some(tparams) = &inner.tparams {
                out.push(NodeRef::TypeParams(tparams));
            }
            out.extend(inner.params.params.iter().map(NodeRef::ComponentTypeParam));
            if let Some(rest) = &inner.params.rest {
                out.push(NodeRef::ComponentTypeRest(rest));
            }
            renders_children(&inner.renders, out);
        }
        T::Object { inner, .. } => {
            out.extend(inner.properties.iter().map(NodeRef::ObjectTypeProperty));
        }
        T::Interface { inner, .. } => {
            out.extend(inner.extends.iter().map(NodeRef::InterfaceExtends));
            out.push(NodeRef::ObjectType(&inner.body.0, &inner.body.1));
        }
        T::Array { inner, .. } => out.push(NodeRef::Type(&inner.argument)),
        T::Conditional { inner, .. } => {
            out.push(NodeRef::Type(&inner.check_type));
            out.push(NodeRef::Type(&inner.extends_type));
            out.push(NodeRef::Type(&inner.true_type));
            out.push(NodeRef::Type(&inner.false_type));
        }
        T::Infer { inner, .. } => out.push(NodeRef::TypeParam(&inner.tparam)),
        T::Generic { inner, .. } => generic_children(inner, out),
        T::IndexedAccess { inner, .. } => {
            out.push(NodeRef::Type(&inner.object));
            out.push(NodeRef::Type(&inner.index));
        }
        T::OptionalIndexedAccess { inner, .. } => {
            out.push(NodeRef::Type(&inner.indexed_access.object));
            out.push(NodeRef::Type(&inner.indexed_access.index));
        }
        T::Union { inner, .. } => {
            out.push(NodeRef::Type(&inner.types.0));
            out.push(NodeRef::Type(&inner.types.1));
            out.extend(inner.types.2.iter().map(NodeRef::Type));
        }
        T::Intersection { inner, .. } => {
            out.push(NodeRef::Type(&inner.types.0));
            out.push(NodeRef::Type(&inner.types.1));
            out.extend(inner.types.2.iter().map(NodeRef::Type));
        }
        T::Typeof { inner, .. } => {
            typeof_target_children(&inner.argument, out);
            if let Some(targs) = &inner.targs {
                out.push(NodeRef::TypeArgs(targs));
            }
        }
        T::Keyof { inner, .. } => out.push(NodeRef::Type(&inner.argument)),
        T::Renders { inner, .. } => out.push(NodeRef::Type(&inner.argument)),
        T::ReadOnly { inner, .. } => out.push(NodeRef::Type(&inner.argument)),
        T::Tuple { inner, .. } => {
            for element in inner.elements.iter() {
                match element {
                    types::tuple::Element::UnlabeledElement { annot, .. } => {
                        out.push(NodeRef::Type(annot));
                    }
                    _ => out.push(NodeRef::TupleElement(element)),
                }
            }
        }
        T::TemplateLiteral { inner, .. } => out.extend(inner.types.iter().map(NodeRef::Type)),
    }
}

fn generic_children<'a>(node: &'a types::Generic<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    generic_identifier_children(&node.id, out);
    if let Some(targs) = &node.targs {
        out.push(NodeRef::TypeArgs(targs));
    }
}

fn generic_identifier_children<'a>(
    node: &'a types::generic::Identifier<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    match node {
        types::generic::Identifier::Unqualified(id) => out.push(NodeRef::Identifier(id)),
        types::generic::Identifier::Qualified(qualified) => {
            out.push(NodeRef::QualifiedType(qualified));
        }
        types::generic::Identifier::ImportTypeAnnot(import) => {
            out.push(NodeRef::ImportType(import))
        }
    }
}

fn typeof_target_children<'a>(
    node: &'a types::typeof_::Target<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    match node {
        types::typeof_::Target::Unqualified(id) => out.push(NodeRef::Identifier(id)),
        types::typeof_::Target::Qualified(qualified) => {
            out.push(NodeRef::QualifiedTypeof(qualified))
        }
        types::typeof_::Target::Import(import) => out.push(NodeRef::ImportType(import)),
    }
}

fn function_type_children<'a>(node: &'a types::Function<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    if let Some(this) = &node.params.this {
        out.push(NodeRef::FunctionTypeThis(this));
    }
    out.extend(node.params.params.iter().map(NodeRef::FunctionTypeParam));
    if let Some(rest) = &node.params.rest {
        out.push(NodeRef::FunctionTypeRest(rest));
    }
    match &node.return_ {
        types::function::ReturnAnnotation::Missing(_) => {}
        types::function::ReturnAnnotation::Available(annotation) => {
            out.push(NodeRef::Annotation(annotation));
        }
        types::function::ReturnAnnotation::TypeGuard(guard) => out.push(NodeRef::TypeGuard(guard)),
    }
}

fn object_type_property_children<'a>(
    node: &'a types::object::Property<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    use types::object::Property;
    match node {
        Property::NormalProperty(property) => {
            if let Some(variance) = &property.variance {
                out.push(NodeRef::Variance(variance));
            }
            object_key_children(&property.key, out);
            match &property.value {
                types::object::PropertyValue::Init(Some(ty)) => out.push(NodeRef::Type(ty)),
                types::object::PropertyValue::Init(None) => {}
                types::object::PropertyValue::Get(loc, function)
                | types::object::PropertyValue::Set(loc, function) => {
                    out.push(NodeRef::FunctionType(loc, function));
                }
            }
            if let Some(init) = &property.init {
                out.push(NodeRef::Expression(init));
            }
        }
        Property::SpreadProperty(spread) => out.push(NodeRef::Type(&spread.argument)),
        Property::Indexer(indexer) => indexer_children(indexer, out),
        Property::CallProperty(call) => {
            out.push(NodeRef::FunctionType(&call.value.0, &call.value.1));
        }
        Property::InternalSlot(slot) => {
            out.push(NodeRef::Identifier(&slot.id));
            out.push(NodeRef::Type(&slot.value));
        }
        Property::MappedType(mapped) => {
            if let Some(variance) = &mapped.variance {
                out.push(NodeRef::Variance(variance));
            }
            out.push(NodeRef::TypeParam(&mapped.key_tparam));
            out.push(NodeRef::Type(&mapped.source_type));
            if let Some(name) = &mapped.name_type {
                out.push(NodeRef::Type(name));
            }
            out.push(NodeRef::Type(&mapped.prop_type));
        }
        Property::PrivateField(field) => out.push(NodeRef::PrivateName(&field.key)),
    }
}

fn component_params_children<'a>(
    params: &'a statement::component_params::Params<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.extend(params.params.iter().map(NodeRef::ComponentParam));
    if let Some(rest) = &params.rest {
        out.push(NodeRef::ComponentRest(rest));
    }
}

fn component_param_name_children<'a>(
    name: &'a statement::component_params::ParamName<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    match name {
        statement::component_params::ParamName::Identifier(id) => out.push(NodeRef::Identifier(id)),
        statement::component_params::ParamName::StringLiteral((loc, literal)) => {
            out.push(NodeRef::StringLiteral(loc, literal));
        }
    }
}

fn renders_children<'a>(
    renders: &'a types::ComponentRendersAnnotation<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    if let types::ComponentRendersAnnotation::AvailableRenders(loc, renders) = renders {
        out.push(NodeRef::Renders(loc, renders));
    }
}

fn declare_variable_children<'a>(
    node: &'a statement::DeclareVariable<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.extend(node.declarations.iter().map(NodeRef::Declarator));
}

fn declare_function_children<'a>(
    node: &'a statement::DeclareFunction<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    if let Some(id) = &node.id {
        out.push(NodeRef::Identifier(id));
    }
    out.push(NodeRef::Annotation(&node.annot));
    if let Some(predicate) = &node.predicate {
        out.push(NodeRef::Predicate(predicate));
    }
}

fn declare_class_children<'a>(
    node: &'a statement::DeclareClass<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.push(NodeRef::Identifier(&node.id));
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    if let Some((_, extends)) = &node.extends {
        declare_class_extends_children(extends, out);
    }
    out.extend(node.mixins.iter().map(NodeRef::InterfaceExtends));
    if let Some(implements) = &node.implements {
        out.extend(implements.interfaces.iter().map(NodeRef::ClassImplements));
    }
    out.push(NodeRef::ObjectType(&node.body.0, &node.body.1));
}

fn declare_class_extends_children<'a>(
    node: &'a statement::DeclareClassExtends<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    match node {
        statement::DeclareClassExtends::ExtendsIdent(generic) => generic_children(generic, out),
        statement::DeclareClassExtends::ExtendsCall { callee, arg } => {
            generic_children(&callee.1, out);
            declare_class_extends_children(&arg.1, out);
        }
    }
}

fn declare_component_children<'a>(
    node: &'a statement::DeclareComponent<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.push(NodeRef::Identifier(&node.id));
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    component_params_children(&node.params, out);
    renders_children(&node.renders, out);
}

fn type_alias_children<'a>(node: &'a statement::TypeAlias<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Identifier(&node.id));
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    out.push(NodeRef::Type(&node.right));
}

fn opaque_type_children<'a>(node: &'a statement::OpaqueType<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Identifier(&node.id));
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    for bound in [
        &node.lower_bound,
        &node.upper_bound,
        &node.legacy_upper_bound,
    ]
    .into_iter()
    .flatten()
    {
        out.push(NodeRef::Type(bound));
    }
    if let Some(impl_type) = &node.impl_type {
        out.push(NodeRef::Type(impl_type));
    }
}

fn interface_children<'a>(node: &'a statement::Interface<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Identifier(&node.id));
    if let Some(tparams) = &node.tparams {
        out.push(NodeRef::TypeParams(tparams));
    }
    out.extend(node.extends.iter().map(NodeRef::InterfaceExtends));
    out.push(NodeRef::ObjectType(&node.body.0, &node.body.1));
}

fn enum_children<'a>(node: &'a statement::EnumDeclaration<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::Identifier(&node.id));
    out.push(NodeRef::EnumBody(&node.body));
}

fn declare_namespace_children<'a>(
    node: &'a statement::DeclareNamespace<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    match &node.id {
        statement::declare_namespace::Id::Global(id)
        | statement::declare_namespace::Id::Local(id) => out.push(NodeRef::Identifier(id)),
    }
    out.push(NodeRef::Block(&node.body.0, &node.body.1));
}

fn match_pattern_children<'a>(node: &'a MatchPattern, out: &mut Vec<NodeRef<'a>>) {
    use match_pattern::MatchPattern as P;
    match node {
        P::WildcardPattern { .. }
        | P::NumberPattern { .. }
        | P::BigIntPattern { .. }
        | P::StringPattern { .. }
        | P::BooleanPattern { .. }
        | P::NullPattern { .. }
        | P::UnaryPattern { .. } => {}
        P::BindingPattern { inner, .. } => out.push(NodeRef::Identifier(&inner.id)),
        P::IdentifierPattern { inner, .. } => out.push(NodeRef::Identifier(inner)),
        P::MemberPattern { inner, .. } => match_member_children(inner, out),
        P::ObjectPattern { inner, .. } => match_object_pattern_children(inner, out),
        P::ArrayPattern { inner, .. } => {
            out.extend(
                inner
                    .elements
                    .iter()
                    .map(|element| NodeRef::MatchPattern(&element.pattern)),
            );
            if let Some(rest) = &inner.rest {
                out.push(NodeRef::MatchRest(rest));
            }
        }
        P::OrPattern { inner, .. } => {
            out.extend(inner.patterns.iter().map(NodeRef::MatchPattern));
        }
        P::AsPattern { inner, .. } => {
            out.push(NodeRef::MatchPattern(&inner.pattern));
            match &inner.target {
                match_pattern::as_pattern::Target::Identifier(id) => {
                    out.push(NodeRef::Identifier(id));
                }
                match_pattern::as_pattern::Target::Binding { loc, pattern } => {
                    out.push(NodeRef::MatchBinding(loc, pattern));
                }
            }
        }
        P::InstancePattern { inner, .. } => {
            match &inner.constructor {
                match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => {
                    out.push(NodeRef::Identifier(id));
                }
                match_pattern::InstancePatternConstructor::MemberConstructor(member) => {
                    out.push(NodeRef::MatchMember(member));
                }
            }
            match_object_pattern_children(&inner.properties.1, out);
        }
    }
}

fn match_object_pattern_children<'a>(
    node: &'a match_pattern::ObjectPattern<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    out.extend(node.properties.iter().map(NodeRef::MatchProperty));
    if let Some(rest) = &node.rest {
        out.push(NodeRef::MatchRest(rest));
    }
}

fn match_member_children<'a>(
    node: &'a match_pattern::MemberPattern<Loc, Loc>,
    out: &mut Vec<NodeRef<'a>>,
) {
    use match_pattern::member_pattern::{Base, Property};
    match &node.base {
        Base::BaseIdentifier(id) => out.push(NodeRef::Identifier(id)),
        Base::BaseMember(member) => out.push(NodeRef::MatchMember(member)),
    }
    match &node.property {
        Property::PropertyString { loc, literal } => out.push(NodeRef::StringLiteral(loc, literal)),
        Property::PropertyNumber { loc, literal } => out.push(NodeRef::NumberLiteral(loc, literal)),
        Property::PropertyBigInt { loc, literal } => out.push(NodeRef::BigIntLiteral(loc, literal)),
        Property::PropertyIdentifier(id) => out.push(NodeRef::Identifier(id)),
    }
}

fn jsx_element_children<'a>(node: &'a jsx::Element<Loc, Loc>, out: &mut Vec<NodeRef<'a>>) {
    out.push(NodeRef::JsxOpening(&node.opening_element));
    out.extend(node.children.1.iter().map(NodeRef::JsxChild));
    if let Some(closing) = &node.closing_element {
        out.push(NodeRef::JsxClosing(closing));
    }
}
