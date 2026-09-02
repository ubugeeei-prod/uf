//! The reserved and contextual words the formatter recognizes.
//!
//! Beyond spelling, a keyword answers the three lookahead questions the scanner
//! needs to resolve `/`, `<` and `{`: may an expression start after it, does it
//! open a parenthesised statement header, and does it introduce a type-only
//! declaration.

/// Reserved and contextual keywords the formatter cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    missing_docs,
    reason = "each variant is the identically spelled keyword"
)]
pub enum Keyword {
    As,
    Async,
    Await,
    Break,
    Case,
    Catch,
    Class,
    Component,
    Const,
    Continue,
    Debugger,
    Declare,
    Default,
    Delete,
    Do,
    Else,
    Enum,
    Export,
    Extends,
    False,
    Finally,
    For,
    From,
    Function,
    Get,
    Hook,
    If,
    Implements,
    Import,
    In,
    Instanceof,
    Interface,
    Let,
    Mixins,
    New,
    Null,
    Of,
    Opaque,
    Renders,
    Return,
    Set,
    Static,
    Super,
    Switch,
    This,
    Throw,
    True,
    Try,
    Type,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
}

impl Keyword {
    /// Look up a keyword by its exact spelling.
    #[must_use]
    pub fn lookup(value: &str) -> Option<Self> {
        // A `match` on the string literal compiles to a length switch followed by
        // a memcmp chain, which beats a hash lookup for inputs this short.
        let keyword = match value {
            "as" => Keyword::As,
            "async" => Keyword::Async,
            "await" => Keyword::Await,
            "break" => Keyword::Break,
            "case" => Keyword::Case,
            "catch" => Keyword::Catch,
            "class" => Keyword::Class,
            "component" => Keyword::Component,
            "const" => Keyword::Const,
            "continue" => Keyword::Continue,
            "debugger" => Keyword::Debugger,
            "declare" => Keyword::Declare,
            "default" => Keyword::Default,
            "delete" => Keyword::Delete,
            "do" => Keyword::Do,
            "else" => Keyword::Else,
            "enum" => Keyword::Enum,
            "export" => Keyword::Export,
            "extends" => Keyword::Extends,
            "false" => Keyword::False,
            "finally" => Keyword::Finally,
            "for" => Keyword::For,
            "from" => Keyword::From,
            "function" => Keyword::Function,
            "get" => Keyword::Get,
            "hook" => Keyword::Hook,
            "if" => Keyword::If,
            "implements" => Keyword::Implements,
            "import" => Keyword::Import,
            "in" => Keyword::In,
            "instanceof" => Keyword::Instanceof,
            "interface" => Keyword::Interface,
            "let" => Keyword::Let,
            "mixins" => Keyword::Mixins,
            "new" => Keyword::New,
            "null" => Keyword::Null,
            "of" => Keyword::Of,
            "opaque" => Keyword::Opaque,
            "renders" => Keyword::Renders,
            "return" => Keyword::Return,
            "set" => Keyword::Set,
            "static" => Keyword::Static,
            "super" => Keyword::Super,
            "switch" => Keyword::Switch,
            "this" => Keyword::This,
            "throw" => Keyword::Throw,
            "true" => Keyword::True,
            "try" => Keyword::Try,
            "type" => Keyword::Type,
            "typeof" => Keyword::Typeof,
            "var" => Keyword::Var,
            "void" => Keyword::Void,
            "while" => Keyword::While,
            "with" => Keyword::With,
            "yield" => Keyword::Yield,
            _ => return None,
        };
        Some(keyword)
    }

    /// Whether an expression may start immediately after this keyword.
    ///
    /// Only unambiguously reserved words are listed. Contextual keywords such as
    /// `type` or `get` can legally be plain variable names, and treating them as
    /// expression starters would mis-lex `type / 2` as a regular expression.
    #[must_use]
    pub const fn allows_expression_after(self) -> bool {
        matches!(
            self,
            Keyword::Await
                | Keyword::Case
                | Keyword::Default
                | Keyword::Delete
                | Keyword::Do
                | Keyword::Else
                | Keyword::Extends
                | Keyword::In
                | Keyword::Instanceof
                | Keyword::New
                | Keyword::Of
                | Keyword::Return
                | Keyword::Throw
                | Keyword::Typeof
                | Keyword::Void
                | Keyword::Yield
        )
    }

    /// Whether the keyword introduces a parenthesised statement header, whose
    /// closing `)` may legally be followed by a regular expression.
    #[must_use]
    pub const fn starts_statement_header(self) -> bool {
        matches!(
            self,
            Keyword::Catch
                | Keyword::For
                | Keyword::If
                | Keyword::Switch
                | Keyword::While
                | Keyword::With
        )
    }

    /// Whether the keyword introduces a type-only declaration, where `<` opens a
    /// type parameter list rather than a JSX element.
    #[must_use]
    pub const fn starts_type_declaration(self) -> bool {
        matches!(
            self,
            Keyword::Declare | Keyword::Interface | Keyword::Opaque | Keyword::Type
        )
    }
}
