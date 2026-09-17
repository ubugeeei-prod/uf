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
    /// The deprecated spelling of the project-wide ignore list.
    ///
    /// It was never `uf lint`'s: `uf fmt`, `uf check`, `uf test` and `uf doc`
    /// walk the project through the same function and have always read it too,
    /// so the key is named after one of the five commands that obey it. The
    /// name it should have had is [`crate::UniflowedConfig::ignore`], and this
    /// one keeps working for as long as alpha lasts — read only when that is
    /// absent, and reported once when it is what answered. See
    /// [`crate::UniflowedConfig::project_ignore`] and ubugeeei-prod/uf#575.
    ///
    /// An `Option` because the distinction it carries is the whole of the
    /// migration: `None` means the project never wrote this key, and there is
    /// nothing to tell anybody about; `Some(vec![])` means it wrote an empty
    /// one, which is a project deliberately walking `node_modules`.
    pub ignore: Option<Vec<CompactString>>,
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
const DEFAULT_LINT_RULES: [(&str, RuleLevel); 130] = [
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
    // "Click here" says nothing in a screen reader's list of links, which is
    // how people using one skim a page. `warn`, because the sentence around a
    // link may still carry its purpose (WCAG 2.4.4 allows that) and a linter
    // cannot read the paragraph a component ends up in.
    ("a11y/anchor-ambiguous-text", RuleLevel::Warn),
    // A link with nothing to announce is read as "link" and nothing more, so
    // one cannot be told from the next.
    ("a11y/anchor-has-content", RuleLevel::Error),
    // An `<a>` without a destination is not a link: no keyboard reaches it,
    // and `href="#"` or `javascript:` is a button without a button's keys.
    ("a11y/anchor-is-valid", RuleLevel::Error),
    // `aria-activedescendant` names the element focus is standing in for, so
    // an element that cannot take focus never gets to say it: the attribute is
    // inert and the option it points at is announced to nobody.
    ("a11y/aria-activedescendant-has-tabindex", RuleLevel::Error),
    // The quietest bug on this list. A misspelled `aria-*` is not rejected by
    // the browser, not reported by React and not read by anything: the control
    // is unlabelled and there is no symptom at all.
    ("a11y/aria-props", RuleLevel::Error),
    // The same silence one level down: a browser handed `aria-hidden="yes"`
    // drops the attribute, and the element it was written to hide is read out.
    ("a11y/aria-proptypes", RuleLevel::Error),
    // A `role` ARIA does not define leaves the element with whatever role HTML
    // gave it — usually none — and nothing anywhere says so.
    ("a11y/aria-role", RuleLevel::Error),
    // `<meta>`, `<script>`, `<title>` and the rest are never rendered, so they
    // are not in the accessibility tree for a role or an `aria-*` to say
    // anything about: whatever was meant is said nowhere.
    ("a11y/aria-unsupported-elements", RuleLevel::Error),
    // A token the browser knows is what lets it fill a field from what the
    // reader has already typed once, which is WCAG 1.3.5 and which matters most
    // to somebody for whom typing an address again is the hard part. An unknown
    // token fills nothing and nothing on the page says so.
    ("a11y/autocomplete-valid", RuleLevel::Error),
    // The other half of `a11y/no-static-element-interactions`: this one takes
    // the element that *has* a `role`, where somebody has said what it is and
    // a keyboard still cannot work it, so the missing handler is the whole of
    // the advice. Exactly one of the two answers any given markup.
    ("a11y/click-events-have-key-events", RuleLevel::Error),
    // A control with no name is announced as "button" and nothing else, so a
    // form of them cannot be told apart. `warn`, and reported only for the
    // shape that is certainly wrong — no `id` for a `<label htmlFor>` to point
    // at, no `<label>` around it, and no name of its own — because the label
    // that names a control is usually a sibling this rule cannot see.
    ("a11y/control-has-associated-label", RuleLevel::Warn),
    // An empty heading is still a heading: a reader jumping by heading lands
    // on it and hears nothing.
    ("a11y/heading-has-content", RuleLevel::Error),
    // Style, and a claim about a document uf cannot see the whole of: a `<h1>`
    // in a layout and a `<h3>` in a card are only a skip if the one renders
    // inside the other. `warn`, and compared within one function body.
    ("a11y/heading-order", RuleLevel::Warn),
    // Without `lang` a screen reader reads the page in the user's own
    // language, whatever it is written in. WCAG 3.1.1, level A.
    ("a11y/html-has-lang", RuleLevel::Error),
    // A frame is announced by its title; without one a reader has to enter it
    // to find out what it holds.
    ("a11y/iframe-has-title", RuleLevel::Error),
    // A screen reader already says "image", so alt text that says it again is
    // wording rather than a defect.
    ("a11y/img-redundant-alt", RuleLevel::Warn),
    // A widget role promises the element behaves like the control it names,
    // and the first thing every control needs is to be reachable: a handler a
    // keyboard never arrives at is a feature it does not have.
    ("a11y/interactive-supports-focus", RuleLevel::Error),
    // A label attached to nothing leaves its field with no accessible name and
    // makes the label itself dead to a click. Both are defects, not opinions.
    ("a11y/label-has-associated-control", RuleLevel::Error),
    // `a11y/html-has-lang` asks whether the page says what language it is in;
    // this asks whether what it says is a language tag at all. A value that is
    // not one leaves a screen reader in whatever voice it was already using.
    ("a11y/lang", RuleLevel::Error),
    // Media with no captions shuts out whoever cannot hear it, but whether it
    // has speech to caption is something only the media knows, so `warn`.
    ("a11y/media-has-caption", RuleLevel::Warn),
    // `onMouseOver` and `onMouseOut` fire for a pointer and nothing else, so a
    // tooltip built on them never opens for somebody tabbing through.
    // `onFocus` and `onBlur` are the same two moments for a keyboard.
    ("a11y/mouse-events-have-key-events", RuleLevel::Error),
    // The shortcut an `accessKey` asks for is the browser's or the screen
    // reader's to give, and which modifier reaches it differs per browser, per
    // platform and per assistive technology: it either does nothing or takes a
    // key away from somebody relying on it.
    ("a11y/no-access-key", RuleLevel::Error),
    // Hidden from assistive technology and still in the tab order is the worst
    // of both: focus lands on an element a screen reader has nothing to say
    // about, and the reader is told nothing about where they are.
    ("a11y/no-aria-hidden-on-focusable", RuleLevel::Error),
    // Focus moved before the reader has been told where they are: a screen
    // reader begins at the control instead of the top of the page, so the
    // heading that says what the page is never gets read.
    ("a11y/no-autofocus", RuleLevel::Error),
    // `<marquee>` and `<blink>` animate forever with no way for the reader to
    // stop them, which WCAG 2.2.2 requires. HTML removed both and browsers
    // still render them, so the markup keeps working and keeps being a problem.
    ("a11y/no-distracting-elements", RuleLevel::Error),
    // A `<button role="presentation">` still focuses, still fires and still
    // sits in the tab order: the role removes the announcement and none of the
    // behaviour, leaving a control nobody is told about.
    (
        "a11y/no-interactive-element-to-noninteractive-role",
        RuleLevel::Error,
    ),
    // The third share of the question `a11y/no-static-element-interactions`
    // and `a11y/click-events-have-key-events` divide: a non-interactive role
    // that has a key handler, where somebody has wired up the keyboard and the
    // element still is not a control, so the advice is the semantics and not
    // another handler.
    (
        "a11y/no-noninteractive-element-interactions",
        RuleLevel::Error,
    ),
    // `<ul role="button">` keeps a list's structure while claiming to be a
    // control. A `<div>` is not reported: it has no semantics of its own, and
    // giving one a widget role is how every custom control is built.
    (
        "a11y/no-noninteractive-element-to-interactive-role",
        RuleLevel::Error,
    ),
    // Every tab stop that is not a control puts the controls further away.
    // `warn` rather than `error`: a scrollable region is given `tabIndex={0}`
    // deliberately, so that a keyboard can scroll it, and that is guidance
    // rather than a defect. A negative `tabIndex` is never reported at all —
    // it is script-only focus, which is how a dialog takes focus when it opens.
    ("a11y/no-noninteractive-tabindex", RuleLevel::Warn),
    // A role the element already has is a second place to keep the same fact
    // correct, and it stops being correct as soon as the markup changes.
    // `<nav role="navigation">` is exempt: w3 recommends it for assistive
    // technology that predates the HTML5 elements.
    ("a11y/no-redundant-roles", RuleLevel::Error),
    // A handler only a mouse can reach is a feature a keyboard user does not
    // have. Reported only where neither a `role` nor a key handler is present,
    // which is where nobody has considered the keyboard at all.
    ("a11y/no-static-element-interactions", RuleLevel::Error),
    // `warn`, and the only rule here the plugin does not put in `recommended`
    // either: `<div role="navigation">` announces exactly what `<nav>` does, so
    // this is about markup that will keep working rather than a defect. uf
    // reports it only where the tag is a real drop-in — never for a widget
    // role, because a `<select>` is not the combobox somebody has built.
    ("a11y/prefer-tag-over-role", RuleLevel::Warn),
    // A role is a promise about what the element is, and the state it is
    // announced by is the other half of it: `role="checkbox"` with no
    // `aria-checked` is a checkbox a screen reader cannot read.
    ("a11y/role-has-required-aria-props", RuleLevel::Error),
    // An `aria-*` the role does not take is inert — the accessibility tree has
    // nowhere to put it — and the state the author described is never
    // announced.
    ("a11y/role-supports-aria-props", RuleLevel::Error),
    // `scope` is what says whether a header names its column or its row, and
    // HTML defines it on `<th>` alone: anywhere else it is ignored, so a table
    // whose headers are `<td scope="col">` has no headers at all as far as a
    // screen reader is concerned.
    ("a11y/scope", RuleLevel::Error),
    // A positive `tabIndex` does not move an element one place forward, it
    // moves it ahead of everything the document orders itself, and every other
    // positive value on the page joins the same queue: the tab order and the
    // reading order stop agreeing, and they disagree more with each one added.
    ("a11y/tabindex-no-positive", RuleLevel::Error),
    // `<p><div>` is a hydration bug rather than a style opinion: the browser's
    // parser repairs it before React sees it, and the repair is the mismatch.
    ("markup/no-invalid-nesting", RuleLevel::Error),
    // `warn`: the registries `rel` draws on are open — IANA's, the HTML
    // standard's, and the microformats wiki the standard points at — so uf
    // reports only the certainly-wrong pair, a keyword that belongs to another
    // element and the spellings the standard calls non-conforming. A browser
    // ignores the keyword rather than breaking, and a build should not fail
    // over a link relation nothing acts on.
    ("markup/no-invalid-rel", RuleLevel::Warn),
    // The official React Compiler's diagnostics: a rule for every category a
    // module can produce under `eslint-plugin-react-hooks`' options, at the
    // level that plugin's `recommended-latest` preset gives the category.
    //
    // Two differ. `hooks` (`error`) and `memo-dependencies` (`warn`) are on
    // although the preset leaves those compiler rules off, because the plugin
    // answers both questions a second time with its classic `rules-of-hooks`
    // (`error`) and `exhaustive-deps` (`warn`), and uf has only the compiler's
    // answer.
    //
    // `exhaustive-effect-dependencies` is off because the plugin ships that
    // validation switched off; turning the rule on turns the validation on. The
    // other rules at `off` are off in the preset.
    ("react-compiler/capitalized-calls", RuleLevel::Off),
    ("react-compiler/error-boundaries", RuleLevel::Error),
    (
        "react-compiler/exhaustive-effect-dependencies",
        RuleLevel::Off,
    ),
    ("react-compiler/fbt", RuleLevel::Off),
    ("react-compiler/globals", RuleLevel::Error),
    ("react-compiler/hooks", RuleLevel::Error),
    ("react-compiler/immutability", RuleLevel::Error),
    ("react-compiler/incompatible-library", RuleLevel::Warn),
    ("react-compiler/invariant", RuleLevel::Off),
    ("react-compiler/memo-dependencies", RuleLevel::Warn),
    (
        "react-compiler/no-deriving-state-in-effects",
        RuleLevel::Off,
    ),
    (
        "react-compiler/preserve-manual-memoization",
        RuleLevel::Error,
    ),
    ("react-compiler/purity", RuleLevel::Error),
    ("react-compiler/refs", RuleLevel::Error),
    ("react-compiler/set-state-in-effect", RuleLevel::Error),
    ("react-compiler/set-state-in-render", RuleLevel::Error),
    ("react-compiler/static-components", RuleLevel::Error),
    ("react-compiler/syntax", RuleLevel::Off),
    ("react-compiler/todo", RuleLevel::Off),
    ("react-compiler/unsupported-syntax", RuleLevel::Warn),
    ("react-compiler/use-memo", RuleLevel::Error),
    ("react-compiler/void-use-memo", RuleLevel::Error),
    // Style preferences during the migration to Flow component/hook syntax.
    ("react/component-syntax", RuleLevel::Warn),
    ("react/hook-syntax", RuleLevel::Warn),
    // A list item with no `key` is matched to the next render by position, so
    // reordering or filtering the list hands one item's state to another.
    ("react/jsx-key", RuleLevel::Error),
    // Text that starts `//` or `/*` between tags renders on the page, and a
    // comment was never meant to be read there.
    ("react/jsx-no-comment-textnodes", RuleLevel::Error),
    // The last of two identical props wins, so the first is code that does
    // nothing while looking like it does something.
    ("react/jsx-no-duplicate-props", RuleLevel::Error),
    // An index is only the wrong key for a list that reorders, filters or
    // grows at the front; a list that never does is correct code, so `warn`.
    ("react/no-array-index-key", RuleLevel::Warn),
    // A `children` prop renders what nesting would. The shape that loses
    // content — the prop and nested children at once — is named in the
    // message, and the rule stays `warn` for the rest.
    ("react/no-children-prop", RuleLevel::Warn),
    // Framework routes are wired by name; `warn` while the scaffold migrates.
    ("react/no-default-export-component", RuleLevel::Warn),
    // A `useMemo`/`useCallback` the official React Compiler removed when it
    // compiled the function around it. `warn`, not `error`: nothing is broken
    // — the compiler already did the work, so the hand-written call is a
    // second dependency array to keep correct rather than a defect — and
    // deleting memoization is a refactor, which is not something a build
    // should fail over. Off when `app.builtins.reactCompiler.enabled` is
    // false, because then the hand-written one is the only memoization there
    // is.
    ("react/no-redundant-memo", RuleLevel::Warn),
    // `warn`: HTML defaults a button to `type="submit"`, so one written for an
    // `onClick` inside a form submits and navigates away. A button outside a
    // form has nothing to submit, and uf cannot see the form a component is
    // rendered into, so this is guidance rather than a defect everywhere.
    ("react/button-has-type", RuleLevel::Warn),
    // `checked` makes an input controlled: React puts the prop's value back
    // after every click, so with neither `onChange` nor `readOnly` the reader
    // cannot move it. React reports it in development and the fix is exact.
    (
        "react/checked-requires-onchange-or-readonly",
        RuleLevel::Error,
    ),
    // JSX reads a lowercase name as an HTML tag and a capitalised one as a
    // value in scope; a namespaced name is neither, so there is nothing for
    // React to render.
    ("react/no-namespace", RuleLevel::Error),
    // `warn`: a `>` or a `}` left in JSX text renders, so this is a suspicion
    // rather than a defect — but each is usually the wreckage of a mistyped tag
    // or a stray brace. `'` and `"` are deliberately not reported: they render
    // exactly as written, and `don't` is the false positive this rule is best
    // known for.
    ("react/no-unescaped-entities", RuleLevel::Warn),
    // The HTML spelling of an attribute React spells differently — `class` for
    // `className`, a lowercase `onclick` for `onClick`. Reported only where the
    // correct spelling is known and can be named, never for an attribute uf has
    // simply not heard of: the platform keeps growing, and a stale table would
    // report markup that had become correct.
    ("react/no-unknown-property", RuleLevel::Error),
    // React throws on a `style` that is not an object, so this is a crash the
    // module can see rather than a matter of taste.
    ("react/style-prop-object", RuleLevel::Error),
    // React throws when it renders a void element such as `<img>` with
    // children or `dangerouslySetInnerHTML`.
    ("react/void-dom-elements-no-children", RuleLevel::Error),
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
    // A `javascript:` URL is the same class by another route: the browser turns
    // the string into code, with everything the page can do.
    ("security/no-script-url", RuleLevel::Error),
    // The half of this defect everyone knows is already closed — browsers give
    // `target="_blank"` a null `window.opener` on their own now — and what is
    // left is the `Referer` header going out, telling wherever the link leads
    // the full URL the reader came from. `error` with the rest of the
    // namespace: the fix is one exact token, `rel="noreferrer"`, and a project
    // that means to send a referrer says so on the line or in this table,
    // rather than every such link passing unnoticed everywhere.
    ("security/no-target-blank", RuleLevel::Error),
    // Where uf departs from eslint-plugin-react, which leaves its own version
    // of this rule out of `recommended`: an unsandboxed frame runs with
    // everything a document gets, and with the whole of this page when it is
    // served from this origin. uf's security namespace does not warn —
    // `uf_lint`'s `security_rules_are_errors_by_default` pins that — so a
    // first-party embed needing no bounds is a decision a project records,
    // rather than a default that lets every frame through unasked.
    ("security/iframe-has-sandbox", RuleLevel::Error),
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
            // Absent, not empty, and the default list is no longer here: it
            // moved to `crate::DEFAULT_IGNORE` with the key. A project that
            // never wrote `lint.ignore` has nothing to be told about, and
            // that is what `None` says.
            ignore: None,
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
