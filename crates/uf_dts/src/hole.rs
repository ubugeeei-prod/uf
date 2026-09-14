//! What the translation could not say, and where.
//!
//! A hole is the one thing this crate is not allowed to do quietly. Flow's own
//! answer to a declaration it cannot type is `any`, and an `any` nobody
//! reported is indistinguishable from a type that was checked — which is the
//! failure ubugeeei-prod/uf#946 exists to end. So every place the translation
//! writes `any` (or drops something TypeScript said) is recorded here, named by
//! the declaration it happened in, and `uf check --explain-any` is where a
//! reader finds the list.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// One place the translation could not carry a TypeScript declaration into
/// Flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hole {
    /// The declaration the hole is in, outermost name first and joined with
    /// `.`: `ZodType.register`, `util.NoUndefined`, or `(module)` for
    /// something about the file itself.
    pub declaration: CompactString,
    /// Which TypeScript construct it was.
    pub construct: Construct,
    /// What happened instead, as a sentence.
    pub reason: CompactString,
    /// The one-based line in the declaration file.
    pub line: u32,
}

/// The TypeScript constructs a translation can leave a hole for.
///
/// Serialized in kebab-case, because the spelling is part of what
/// `uf check --explain-any` prints and what a golden test pins: renaming a
/// variant is a change a reader of either would notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Construct {
    /// The file is not TypeScript oxc can parse.
    ParseError,
    /// `declare global { … }`: an augmentation of the global scope.
    GlobalAugmentation,
    /// `declare module "x" { … }`: an augmentation of, or an ambient
    /// declaration for, another module.
    ModuleAugmentation,
    /// `asserts x is T` or `asserts x`: Flow has no assertion signatures.
    AssertionSignature,
    /// `[A, ...B[]]`: Flow spreads only tuples into tuples.
    ArrayRestInTuple,
    /// A computed member key other than a well-known symbol.
    ComputedKey,
    /// The `intrinsic` keyword, which only TypeScript's own lib may use.
    IntrinsicType,
    /// A JSDoc type (`?T`, `T!`, `*`) inside a declaration file.
    JsdocType,
    /// `typeof f<T>`: an instantiation expression in a type query.
    InstantiationQuery,
    /// `import x = N.y`: an alias of a namespace member.
    ImportAlias,
    /// `export =` or `export default` of something that is not a name.
    ExportAssignment,
    /// A default export that is only a type.
    DefaultExportType,
    /// A relative import naming no declaration file in the package.
    MissingFile,
    /// An enum member whose value is not a constant the translation can
    /// compute, or an enum mixing strings and numbers.
    EnumMember,
    /// Two declarations of one name that Flow cannot bind together.
    MergedDeclaration,
    /// A `static` index signature on a class.
    StaticIndexSignature,
    /// An index signature keyed by something other than `string`, `number` or
    /// `symbol`.
    IndexSignatureKey,
    /// A class that extends something that is not a name.
    ClassHeritage,
    /// `typeof globalThis`, or a member of it.
    GlobalThis,
}

impl Construct {
    /// Every construct, in declaration order.
    pub const ALL: [Self; 19] = [
        Self::ParseError,
        Self::GlobalAugmentation,
        Self::ModuleAugmentation,
        Self::AssertionSignature,
        Self::ArrayRestInTuple,
        Self::ComputedKey,
        Self::IntrinsicType,
        Self::JsdocType,
        Self::InstantiationQuery,
        Self::ImportAlias,
        Self::ExportAssignment,
        Self::DefaultExportType,
        Self::MissingFile,
        Self::EnumMember,
        Self::MergedDeclaration,
        Self::StaticIndexSignature,
        Self::IndexSignatureKey,
        Self::ClassHeritage,
        Self::GlobalThis,
    ];

    /// The construct's name, as it is serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse-error",
            Self::GlobalAugmentation => "global-augmentation",
            Self::ModuleAugmentation => "module-augmentation",
            Self::AssertionSignature => "assertion-signature",
            Self::ArrayRestInTuple => "array-rest-in-tuple",
            Self::ComputedKey => "computed-key",
            Self::IntrinsicType => "intrinsic-type",
            Self::JsdocType => "jsdoc-type",
            Self::InstantiationQuery => "instantiation-query",
            Self::ImportAlias => "import-alias",
            Self::ExportAssignment => "export-assignment",
            Self::DefaultExportType => "default-export-type",
            Self::MissingFile => "missing-file",
            Self::EnumMember => "enum-member",
            Self::MergedDeclaration => "merged-declaration",
            Self::StaticIndexSignature => "static-index-signature",
            Self::IndexSignatureKey => "index-signature-key",
            Self::ClassHeritage => "class-heritage",
            Self::GlobalThis => "global-this",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_construct_serializes_under_the_name_it_reports() {
        for construct in Construct::ALL {
            let json = serde_json::to_string(&construct).expect("serializes");
            assert_eq!(json, format!("\"{}\"", construct.as_str()));
        }
    }
}
