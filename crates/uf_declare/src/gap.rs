//! What the declarations could not say, and where.
//!
//! A gap is the one thing this crate is not allowed to do quietly. A `.d.ts`
//! is read by a compiler that trusts it completely: TypeScript does not check
//! a declaration file against the code it describes, so a declaration that
//! overstates what it knows is not a weaker type — it is a wrong one, and the
//! consumer finds out at runtime. That is the failure ubugeeei-prod/uf#969
//! exists to prevent, and it is why every place the translation widened a
//! type, dropped a guarantee or refused a declaration outright is recorded
//! here and named in the build report.
//!
//! The mirror image is `uf_dts`'s `Hole`, which records the same thing for the
//! other direction — a TypeScript declaration uf could not carry into Flow.
//! The two vocabularies are deliberately separate: what has no meaning in
//! TypeScript is not the complement of what has no meaning in Flow, and one
//! enum covering both would name constructs that cannot occur.
//!
//! # One gap per declaration, per construct
//!
//! Flow has been exact-by-default since 2023 and rejects
//! `exact_by_default=false` as deprecated — `crates/uf_config/src/lint.rs`
//! says so where it turns the `implicit-inexact-object` rule off — which means
//! an ordinary `{ a: string }` in a Flow library *is* an exact object type and
//! every one of them loses its exactness on the way to TypeScript. Reporting
//! each type node would bury the report in a fact the reader learns once. So
//! the emitter records a construct at most once per declaration: the list says
//! which exports are affected, which is what an author acts on.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// One place the translation could not carry a Flow declaration into
/// TypeScript exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    /// The declaration the gap is in, outermost name first and joined with
    /// `.`: `Store.subscribe`, `Options.retries`, or `(module)` for something
    /// about the file itself.
    pub declaration: CompactString,
    /// Which Flow construct it was.
    pub construct: Construct,
    /// What the consumer gets instead, as a sentence.
    ///
    /// The sentence is the whole value of the report. "Flow's exact object
    /// type has no TypeScript spelling" tells a reader nothing they can act
    /// on; "published as an ordinary object type, so TypeScript will not
    /// reject an extra property" tells them exactly which guarantee they lost
    /// and lets them decide whether they care.
    pub reason: CompactString,
    /// The one-based line in the Flow source.
    pub line: u32,
}

/// The Flow constructs a translation can leave a gap for.
///
/// Serialized in kebab-case, because the spelling is part of what the build
/// report prints and what a golden test pins: renaming a variant is a change
/// a reader of either would notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Construct {
    /// The file is not Flow the parser can read.
    ParseError,
    /// An exact object type. TypeScript's object types are inexact, and its
    /// excess-property check applies to object literals rather than to the
    /// type, so the one thing exactness buys — rejecting an extra property on
    /// a value that already has a type — does not survive.
    ExactObject,
    /// `-x: T`: a write-only property. TypeScript has `readonly` and nothing
    /// on the other side.
    WriteOnlyProperty,
    /// `{ ...A, ...B }`: a spread of one object type into another. An
    /// intersection is not the same operation — a spread overwrites a
    /// property and an intersection keeps both types of it — so the spread is
    /// refused rather than approximated.
    ObjectSpread,
    /// `opaque type T = …`: published as a branded type, which is what
    /// carries the nominality; the underlying type is deliberately not
    /// published, because publishing it is exactly the lie `opaque` exists to
    /// prevent.
    OpaqueType,
    /// An indexer keyed by something TypeScript cannot key on.
    IndexerKey,
    /// A Flow utility type with no TypeScript equivalent that reduces the
    /// same way: `$Diff`, `$Rest`, `$Shape`, `$ObjMap`, `$TupleMap`, `$Call`,
    /// `$Exports`, `$CharSet`, `Class`.
    FlowUtility,
    /// The existential type `*`, which asks Flow to infer a type argument.
    ExistentialType,
    /// `renders`, `renders?` or `renders*`: a claim about what a component
    /// renders, which TypeScript has no way to state.
    RendersType,
    /// Flow's `component` syntax, as a declaration or as a type.
    ComponentType,
    /// A React type that has no name in TypeScript without `@types/react`:
    /// `React$Node`, `React.Node`, `React.Element` and the rest. Translating
    /// these means deciding which `@types/react` a consumer has, which is a
    /// decision this crate does not get to make.
    ReactType,
    /// A `%checks` predicate. Flow's newer `x is T` guard is translated; the
    /// legacy predicate is not, because what it asserts lives in the function
    /// body rather than in its type.
    PredicateFunction,
    /// A Flow enum: a runtime object with `cast`, `isValid`, `members` and
    /// `getName` that TypeScript's own `enum` is not.
    FlowEnum,
    /// An internal slot — `[[Call]]` and the rest.
    InternalSlot,
    /// A private field in an object or class type.
    PrivateField,
    /// `declare module "x" { … }` or `declare namespace`: an ambient
    /// declaration about something other than this module.
    AmbientModule,
    /// `[A, B, ...]`: an inexact tuple. TypeScript's tuples are exact unless
    /// they end in a rest element, which says something different.
    InexactTuple,
    /// `T?.[K]`: an optional indexed access. TypeScript indexes a type that
    /// cannot be null, so there is no spelling that means the same thing.
    OptionalIndexedAccess,
    /// An exported binding with no type annotation, whose type Flow infers.
    /// A declaration file cannot infer, and this crate does not type-check.
    MissingAnnotation,
    /// A relative import naming no module in the project.
    MissingFile,
}

impl Construct {
    /// Every construct, in declaration order.
    pub const ALL: [Self; 20] = [
        Self::ParseError,
        Self::ExactObject,
        Self::WriteOnlyProperty,
        Self::ObjectSpread,
        Self::OpaqueType,
        Self::IndexerKey,
        Self::FlowUtility,
        Self::ExistentialType,
        Self::RendersType,
        Self::ComponentType,
        Self::ReactType,
        Self::PredicateFunction,
        Self::FlowEnum,
        Self::InternalSlot,
        Self::PrivateField,
        Self::AmbientModule,
        Self::InexactTuple,
        Self::OptionalIndexedAccess,
        Self::MissingAnnotation,
        Self::MissingFile,
    ];

    /// The construct's name, as it is serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse-error",
            Self::ExactObject => "exact-object",
            Self::WriteOnlyProperty => "write-only-property",
            Self::ObjectSpread => "object-spread",
            Self::OpaqueType => "opaque-type",
            Self::IndexerKey => "indexer-key",
            Self::FlowUtility => "flow-utility",
            Self::ExistentialType => "existential-type",
            Self::RendersType => "renders-type",
            Self::ComponentType => "component-type",
            Self::ReactType => "react-type",
            Self::PredicateFunction => "predicate-function",
            Self::FlowEnum => "flow-enum",
            Self::InternalSlot => "internal-slot",
            Self::PrivateField => "private-field",
            Self::AmbientModule => "ambient-module",
            Self::InexactTuple => "inexact-tuple",
            Self::OptionalIndexedAccess => "optional-indexed-access",
            Self::MissingAnnotation => "missing-annotation",
            Self::MissingFile => "missing-file",
        }
    }

    /// Whether a declaration carrying this gap still describes its value.
    ///
    /// The distinction the build report draws, and the reason it prints two
    /// counts rather than one. [`Self::ExactObject`] widens a type that is
    /// otherwise complete — every member is still there and still checked, so
    /// a consumer who misspells one still gets an error — while
    /// [`Self::FlowUtility`] leaves `unknown` where a type used to be, and a
    /// consumer gets no checking at all. A reader deciding what to do about a
    /// list of gaps needs to know which kind each one is.
    #[must_use]
    pub const fn keeps_the_type(self) -> bool {
        matches!(
            self,
            Self::ExactObject
                | Self::WriteOnlyProperty
                | Self::OpaqueType
                | Self::InexactTuple
                | Self::PrivateField
                | Self::InternalSlot
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_construct_serializes_under_the_name_it_reports() {
        for construct in Construct::ALL {
            let json = serde_json::to_string(&construct).expect("serializes");
            assert_eq!(
                json,
                uf_infra::cstr!("\"{}\"", construct.as_str()).into_string()
            );
        }
    }

    #[test]
    fn every_construct_is_listed_once() {
        let mut seen: Vec<&str> = Construct::ALL.iter().map(|it| it.as_str()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "a construct is listed twice");
    }

    /// A gap that keeps its type is a widening; one that does not is a
    /// refusal. Both are reported, and the two are not the same news.
    #[test]
    fn a_refusal_is_not_a_widening() {
        assert!(Construct::ExactObject.keeps_the_type());
        assert!(!Construct::FlowUtility.keeps_the_type());
        assert!(!Construct::ParseError.keeps_the_type());
    }
}
