use std::collections::BTreeMap;
use std::fmt;

use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct LintConfig {
    pub engine: LintEngine,
    pub files: Vec<CompactString>,
    pub flow: FlowLintConfig,
    pub ignore: Vec<CompactString>,
    /// Rule levels, **merged over** [`DEFAULT_LINT_RULES`] rather than
    /// replacing it. See [`rules_over_defaults`].
    #[serde(deserialize_with = "rules_over_defaults")]
    pub rules: BTreeMap<CompactString, RuleLevel>,
}

/// Read `lint.rules` as changes to uf's table, not as the whole of it.
///
/// A map field deserializes to exactly what the document said, and for this
/// one field that is the wrong answer in a way nothing reports. `lint: { rules:
/// { "flow/unclear-type": "warn" } }` used to *be* the rule table: the other
/// fifty-odd rules were not lowered or turned off, they stopped existing, and
/// `uf lint` stopped looking for them. `flow/syntax` went with them, so a
/// project that lowered one rule to a warning also stopped being told its
/// sources do not parse. The run then passed, which is the part that makes it
/// severe rather than surprising — a green build that means nothing looks
/// exactly like a green build that means something. See
/// ubugeeei-prod/uf#475.
///
/// So the defaults are laid down first and the document is applied on top of
/// them. Every level in the table is still reachable: `"off"` is what switches
/// a rule off, and it says so at the point it is written. What is no longer
/// reachable is switching a rule off *by not mentioning it*, which nobody
/// could have meant to ask for.
///
/// A rule id uf does not know is kept rather than rejected. Deserialization is
/// not where that is decided: `uf_lint` owns the catalogue, a config may name
/// a rule a newer uf added or an older one retired, and refusing the whole
/// config over one unknown key would make a uf downgrade unrunnable.
fn rules_over_defaults<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<CompactString, RuleLevel>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut rules = default_lint_rules();
    for (rule, level) in BTreeMap::<CompactString, RuleLevel>::deserialize(deserializer)? {
        rules.insert(rule, level);
    }
    Ok(rules)
}

/// uf's rule table, as a fresh map.
fn default_lint_rules() -> BTreeMap<CompactString, RuleLevel> {
    DEFAULT_LINT_RULES
        .into_iter()
        .map(|(rule, level)| (CompactString::const_new(rule), level))
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LintEngine {
    #[default]
    Rust,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FlowLintConfig {
    pub builtins: FlowBuiltinLintMode,
    pub parser: FlowLintParser,
}

impl Default for FlowLintConfig {
    fn default() -> Self {
        Self {
            builtins: FlowBuiltinLintMode::Mixed,
            parser: FlowLintParser::OfficialFlowRust,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowBuiltinLintMode {
    #[default]
    Mixed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowLintParser {
    #[default]
    OfficialFlowRust,
}

/// Every lint rule `uf lint` ships, with the level uf applies out of the box.
///
/// `uf lint` is the union of Flow's built-in lint set (the `flow/` namespace) and
/// uf's own framework rules, so this table has to name both. Flow itself defaults
/// every built-in lint to `off`; uf does not, because a linter nobody switches on
/// catches nothing. The policy, applied per row below:
///
/// - `error` when the pattern is a bug, an unsound escape hatch, or a rule whose
///   fix is mechanical — the things a large Flow codebase cannot let accumulate.
/// - `warn` when the pattern is only suspicious, is legitimate in some code, or
///   when uf's current check covers a syntactic subset of what Flow checks.
/// - `off` only where leaving a rule on would report the same violation twice.
///
/// Each rule's full rationale, category, and one-line description live on its
/// `uf_lint::RuleDescriptor`; `uf_lint` has a test asserting this table and that
/// catalogue agree exactly, in both directions, so the two cannot drift apart.
///
/// A project's `lint.rules` is merged **over** this table rather than replacing
/// it — see [`rules_over_defaults`] for what naming one rule used to do to the
/// other fifty.
const DEFAULT_LINT_RULES: [(&str, RuleLevel); 61] = [
    // --- Flow built-in lints ------------------------------------------------
    // Exactness must be stated, not inferred from a config flag.
    // Off: the ambiguity is gone. Flow has been exact-by-default since 2023 and
    // rejects `exact_by_default=false` as deprecated, so `{ a: b }` is exact and
    // the `{| |}` this rule asks for is the legacy spelling. See
    // `uf_lint::rules::flow::FLOW_META`.
    ("flow/ambiguous-object-type", RuleLevel::Off),
    // Reading named exports off a default import is a CommonJS interop bug.
    ("flow/default-import-access", RuleLevel::Error),
    // `bool` is a legacy alias for `boolean`; mechanical fix.
    ("flow/deprecated-type", RuleLevel::Error),
    // Legal, just confusing.
    ("flow/export-renamed-default", RuleLevel::Warn),
    // Flow's internal types are unstable across releases.
    ("flow/internal-type", RuleLevel::Error),
    // A namespace object is not a value.
    ("flow/invalid-import-star-use", RuleLevel::Error),
    // Rebinding a method to a foreign receiver is unsound.
    ("flow/invalid-this-arg", RuleLevel::Error),
    // Shadowing a builtin libdef breaks every consumer at once.
    ("flow/libdef-override", RuleLevel::Error),
    // uf ships ESM; mixing module systems defeats static analysis.
    ("flow/mixed-import-and-require", RuleLevel::Error),
    // A nested component remounts its whole subtree every render.
    ("flow/nested-component", RuleLevel::Error),
    // A nested hook gets a new identity every render.
    ("flow/nested-hook", RuleLevel::Error),
    // A mutable export is a live binding consumers cannot reason about.
    ("flow/non-const-var-export", RuleLevel::Error),
    // Only actionable mid-migration, so it must not block one.
    ("flow/nonstrict-import", RuleLevel::Warn),
    // Shadowing `div`/`span` silently changes what JSX means.
    ("flow/react-intrinsic-overlap", RuleLevel::Error),
    // The explicit form is verbose; the implicit form is not itself a bug.
    ("flow/require-explicit-enum-checks", RuleLevel::Warn),
    ("flow/require-explicit-enum-switch-cases", RuleLevel::Warn),
    // The most common Flow-catchable production bug: `if (count)` skipping 0.
    ("flow/sketchy-null", RuleLevel::Error),
    // The typed variants stay off so one violation is not reported twice.
    ("flow/sketchy-null-bigint", RuleLevel::Off),
    ("flow/sketchy-null-bool", RuleLevel::Off),
    ("flow/sketchy-null-mixed", RuleLevel::Off),
    ("flow/sketchy-null-number", RuleLevel::Off),
    ("flow/sketchy-null-string", RuleLevel::Off),
    // `{count && <List />}` renders a literal `0`; user-visible bug.
    ("flow/sketchy-number", RuleLevel::Error),
    // Legal in methods, so warn rather than block.
    ("flow/this-in-exported-function", RuleLevel::Warn),
    // `any`/`Object`/`Function` switch the type checker off.
    ("flow/unclear-type", RuleLevel::Error),
    // Reading a field before the constructor finishes yields `undefined`.
    ("flow/uninitialized-instance-property", RuleLevel::Error),
    // Dead code, not a bug.
    ("flow/unnecessary-invariant", RuleLevel::Warn),
    // uf only sees the syntactic subset today.
    ("flow/unnecessary-optional-chain", RuleLevel::Warn),
    // Accessors hide side effects, but are legitimate in some UI code.
    ("flow/unsafe-getters-setters", RuleLevel::Warn),
    // `Object.assign` mutates its target and is unsound in Flow.
    ("flow/unsafe-object-assign", RuleLevel::Error),
    // An untyped dependency turns everything it exports into `any`.
    ("flow/untyped-import", RuleLevel::Error),
    ("flow/untyped-type-import", RuleLevel::Error),
    // A floating promise swallows rejections and loses ordering.
    ("flow/unused-promise", RuleLevel::Error),
    // --- uf's own rules -----------------------------------------------------
    // A file that does not parse cannot be checked at all.
    ("flow/syntax", RuleLevel::Error),
    ("uniflowed/no-tabs", RuleLevel::Error),
    ("uniflowed/no-trailing-whitespace", RuleLevel::Error),
    // Tasks belong in uf.config.js, never in a shelled-out package manager.
    ("uniflowed/no-npm-script-invocation", RuleLevel::Error),
    // A typo'd suppression silently stops enforcing a rule.
    ("uniflowed/unknown-lint-suppression", RuleLevel::Error),
    // An image with no text alternative is unreadable to the people who need
    // the alternative, and the fix is mechanical: the words the image carries,
    // or `alt=""` when it carries none.
    ("a11y/alt-text", RuleLevel::Error),
    // The quietest bug on this list. A misspelled `aria-*` is not rejected by
    // the browser, not reported by React and not read by anything: the control
    // is unlabelled and there is no symptom at all.
    ("a11y/aria-props", RuleLevel::Error),
    // Style, and a claim about a document uf cannot see the whole of: a `<h1>`
    // in a layout and a `<h3>` in a card are only a skip if the one renders
    // inside the other. `warn`, and compared within one function body.
    ("a11y/heading-order", RuleLevel::Warn),
    // A label attached to nothing leaves its field with no accessible name and
    // makes the label itself dead to a click. Both are defects, not opinions.
    ("a11y/label-has-associated-control", RuleLevel::Error),
    // A handler only a mouse can reach is a feature a keyboard user does not
    // have. Reported only where neither a `role` nor a key handler is present,
    // which is where nobody has considered the keyboard at all.
    ("a11y/no-static-element-interactions", RuleLevel::Error),
    // `<p><div>` is a hydration bug rather than a style opinion: the browser's
    // parser repairs it before React sees it, and the repair is the mismatch.
    ("markup/no-invalid-nesting", RuleLevel::Error),
    // Style preferences during the migration to Flow component/hook syntax.
    ("react/component-syntax", RuleLevel::Warn),
    ("react/hook-syntax", RuleLevel::Warn),
    // Breaking the rules of hooks corrupts React's hook state.
    ("react/hooks-rules", RuleLevel::Error),
    // Framework routes are wired by name; `warn` while the scaffold migrates.
    ("react/no-default-export-component", RuleLevel::Warn),
    // Non-idempotent render breaks streaming SSR and hydration.
    ("react/no-render-side-effects", RuleLevel::Error),
    // Platform branches are a preference, not a correctness problem.
    ("react-native/platform-split", RuleLevel::Warn),
    // Leaking a secret into a client bundle is unrecoverable.
    ("server/no-client-secret", RuleLevel::Error),
    ("server/no-server-only-import-in-client", RuleLevel::Error),
    // A misplaced directive is silently ignored; Next.js has shipped this bug.
    ("server/use-client-directive-position", RuleLevel::Error),
    ("server/use-server-actions", RuleLevel::Error),
    ("router/reserved-files", RuleLevel::Error),
    // `@slot` and `(.)segment` used to be served as literal URLs, which is
    // worse than not supporting them: the project looks like it works.
    ("router/unsupported-segment", RuleLevel::Error),
    ("package/no-npm-scripts", RuleLevel::Error),
    ("fetch/no-global-override", RuleLevel::Error),
    // `import.meta.hot.accept(...)` throws in a production build, and the
    // guard Vite's own documentation writes around it does not refine in Flow.
    // One shape crashes a build and the other fails `uf check`; both block.
    ("vite/hot-needs-optional-chaining", RuleLevel::Error),
    // XSS and arbitrary code execution: never a warning.
    ("security/no-dangerously-set-inner-html", RuleLevel::Error),
    ("security/no-eval", RuleLevel::Error),
];

impl Default for LintConfig {
    fn default() -> Self {
        let rules = default_lint_rules();

        Self {
            engine: LintEngine::Rust,
            files: vec![
                CompactString::const_new("app"),
                CompactString::const_new("npm"),
                CompactString::const_new("server"),
                CompactString::const_new("tests"),
            ],
            flow: FlowLintConfig::default(),
            ignore: vec![
                CompactString::const_new("node_modules"),
                CompactString::const_new("dist"),
                CompactString::const_new("target"),
            ],
            rules,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Off,
    Warn,
    Error,
}

impl RuleLevel {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl<'de> Deserialize<'de> for RuleLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RuleLevel;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("\"off\", \"warn\", \"error\", false, true, 0, 1, or 2")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(if value {
                    RuleLevel::Error
                } else {
                    RuleLevel::Off
                })
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    0 => Ok(RuleLevel::Off),
                    1 => Ok(RuleLevel::Warn),
                    2 => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value}"))),
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "off" => Ok(RuleLevel::Off),
                    "warn" | "warning" => Ok(RuleLevel::Warn),
                    "error" => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value:?}"))),
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}
