//! The two `react/*` rules that read the module's tree: the effect that should
//! not be an effect, and the memoization the React Compiler already did.
//!
//! Every rule here is tested from both sides. A linter is only as useful as the
//! reports a reader can act on, so a fixture that must fire is worth exactly as
//! much as one that must not.

use super::*;

const DERIVED_STATE: &str = "react/no-derived-state-effect";
const REDUNDANT_MEMO: &str = "react/no-redundant-memo";

/// A module that imports the hooks it uses, so each fixture is a real module.
fn module(body: &str) -> String {
    format!(
        "// @flow\nimport {{useCallback, useEffect, useLayoutEffect, useMemo, useState}} from \"react\";\n\n{body}"
    )
}

fn derived(body: &str) -> Vec<Diagnostic> {
    lint_js(DERIVED_STATE, &module(body))
}

fn memo(body: &str) -> Vec<Diagnostic> {
    lint_js(REDUNDANT_MEMO, &module(body))
}

// --- react/no-derived-state-effect: what it reports -------------------------

#[test]
fn an_effect_that_only_stores_a_value_built_from_its_dependencies_is_reported() {
    // React's own opening example in "You Might Not Need an Effect".
    let diagnostics = derived(
        "component Name(first: string, last: string) {\n  const [full, setFull] = useState(\"\");\n  useEffect(() => {\n    setFull(first + \" \" + last);\n  }, [first, last]);\n  return <p>{full}</p>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (6, 3));
    assert!(
        diagnostics[0].message.contains("during render"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_concise_body_is_the_same_effect() {
    let diagnostics = derived(
        "component Mirror(a: number) {\n  const [b, setB] = useState(0);\n  useEffect(() => setB(a), [a]);\n  return <p>{b}</p>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_ternary_and_a_template_are_still_expressions_over_the_dependencies() {
    let diagnostics = derived(
        "component Label(count: number, name: string) {\n  const [text, setText] = useState(\"\");\n  useEffect(() => {\n    setText(count > 1 ? `${name}s` : name);\n  }, [count, name]);\n  return <p>{text}</p>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_hook_declaration_is_read_the_same_way_as_a_component() {
    let diagnostics = derived(
        "hook useDouble(a: number): number {\n  const [b, setB] = useState(0);\n  useEffect(() => {\n    setB(a * 2);\n  }, [a]);\n  return b;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

// --- react/no-derived-state-effect: what it must not report -----------------

#[test]
fn a_setter_that_is_not_a_use_state_binding_is_not_derived_state() {
    // `onChange` is a prop, and calling a prop from an effect is what effects
    // are for. Nothing in the name says otherwise, so the binding does.
    let diagnostics = derived(
        "component Field(value: string, setValue: (v: string) => void) {\n  useEffect(() => {\n    setValue(value);\n  }, [value]);\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_setter_declared_in_another_component_is_not_in_scope() {
    let diagnostics = derived(
        "component Owner() {\n  const [value, setValue] = useState(0);\n  return <p>{value}</p>;\n}\n\ncomponent Other(a: number) {\n  useEffect(() => {\n    setValue(a);\n  }, [a]);\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn an_effect_that_also_does_something_else_is_left_alone() {
    let diagnostics = derived(
        "component Name(first: string) {\n  const [full, setFull] = useState(\"\");\n  useEffect(() => {\n    setFull(first);\n    track(first);\n  }, [first]);\n  return <p>{full}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn an_effect_with_a_cleanup_is_left_alone() {
    let diagnostics = derived(
        "component Name(first: string) {\n  const [full, setFull] = useState(\"\");\n  useEffect(() => {\n    setFull(first);\n    return () => setFull(\"\");\n  }, [first]);\n  return <p>{full}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_property_read_is_left_alone() {
    // `data.items` and `box.offsetWidth` are the same three tokens, and the
    // second must *not* move into render — reading layout there is the bug
    // `react/no-render-side-effects` exists to catch. Nothing in the source
    // says which one this is, so the rule says nothing.
    let diagnostics = derived(
        "component List(data: Data) {\n  const [items, setItems] = useState([]);\n  useEffect(() => {\n    setItems(data.items);\n  }, [data]);\n  return <ul>{items}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_call_is_left_alone() {
    let diagnostics = derived(
        "component List(items: Array<string>) {\n  const [sorted, setSorted] = useState([]);\n  useEffect(() => {\n    setSorted(sort(items));\n  }, [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_reset_to_a_constant_is_left_alone() {
    // The article's other shape. Its fix is a `key` or a comparison against
    // the previous prop, which is a redesign rather than moving an
    // expression, so a rule that blocks a build must not insist on it.
    let diagnostics = derived(
        "component Table(items: Array<string>) {\n  const [selection, setSelection] = useState(null);\n  useEffect(() => {\n    setSelection(null);\n  }, [items]);\n  return <ul>{items}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_value_that_is_not_a_dependency_is_left_alone() {
    let diagnostics = derived(
        "component Odd(a: number, b: number) {\n  const [c, setC] = useState(0);\n  useEffect(() => {\n    setC(b);\n  }, [a]);\n  return <p>{c}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn an_effect_with_no_dependency_array_is_left_alone() {
    let diagnostics = derived(
        "component Name(first: string) {\n  const [full, setFull] = useState(\"\");\n  useEffect(() => {\n    setFull(first);\n  });\n  return <p>{full}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn an_effect_with_an_empty_dependency_array_is_left_alone() {
    let diagnostics = derived(
        "component Name() {\n  const [ready, setReady] = useState(false);\n  useEffect(() => {\n    setReady(true);\n  }, []);\n  return <p>{ready}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_layout_effect_is_left_alone() {
    // Where measuring belongs, and a measurement is exactly what must not be
    // moved into render.
    let diagnostics = derived(
        "component Box(width: number) {\n  const [scaled, setScaled] = useState(0);\n  useLayoutEffect(() => {\n    setScaled(width * 2);\n  }, [width]);\n  return <p>{scaled}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn an_updater_function_is_not_a_derived_value() {
    let diagnostics = derived(
        "component Counter(step: number) {\n  const [count, setCount] = useState(0);\n  useEffect(() => {\n    setCount((previous) => previous + step);\n  }, [step]);\n  return <p>{count}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_dependency_the_state_itself_feeds_is_left_alone() {
    // `@uniflowed/hooks`' own `useTimeAgo`, reduced: the schedule that is
    // stored chooses the tick, the tick drives the clock, and the clock's
    // reading is what chooses the next schedule. `const schedule = wanted`
    // during render would define the value in terms of itself, so the effect
    // is what the design needs rather than a mistake — and the rule found this
    // one by firing on it.
    let diagnostics = derived(
        "hook useTimeAgo(instant: number): number {\n  const [schedule, setSchedule] = useState(1000);\n  const tick = schedule;\n  const now = useNow(tick);\n  const wanted = now - instant;\n  useEffect(() => {\n    setSchedule(wanted);\n  }, [wanted]);\n  return schedule;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_dependency_that_only_shares_a_method_name_with_the_state_is_still_reported() {
    // The feedback walk must not join two bindings through a property: `left`
    // is read from `bounds.width`, and `width` is the state — the same word,
    // and no path between them.
    let diagnostics = derived(
        "component Bar(bounds: Bounds) {\n  const [width, setWidth] = useState(0);\n  const left = bounds.width;\n  useEffect(() => {\n    setWidth(left);\n  }, [left]);\n  return <p>{width}</p>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_dependency_written_as_a_property_is_left_alone() {
    // `[data.id]` says the effect re-runs on the field, which is a different
    // claim from "derived from `data`".
    let diagnostics = derived(
        "component Row(data: Data, label: string) {\n  const [text, setText] = useState(\"\");\n  useEffect(() => {\n    setText(label);\n  }, [data.id, label]);\n  return <p>{text}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

// --- react/no-redundant-memo ------------------------------------------------

#[test]
fn a_memo_the_compiler_removed_is_reported() {
    let diagnostics = memo(
        "component List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (5, 18));
    assert!(
        diagnostics[0].message.contains("useMemo"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_callback_the_compiler_removed_is_reported() {
    let diagnostics = memo(
        "hook useHandler(id: string): () => void {\n  return useCallback(() => send(id), [id]);\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("useCallback"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn the_namespaced_spelling_is_reported_at_the_hook_and_not_at_react() {
    // `React.useMemo(…)` reported at its callee underlines `React`, which says
    // nothing about which call is meant on a line that holds two of them.
    let diagnostics = lint_js(
        REDUNDANT_MEMO,
        "// @flow\nimport * as React from \"react\";\n\ncomponent List(items: Array<string>) {\n  const sorted = React.useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (5, 24));
}

#[test]
fn a_memo_in_a_function_the_compiler_never_touches_is_left_alone() {
    // `syntax` mode compiles `component` and `hook` declarations. A plain
    // function is an uncompiled boundary, and the memoization in it is the
    // only memoization there is.
    let diagnostics = memo(
        "export function List(props: {items: Array<string>}) {\n  const sorted = useMemo(() => props.items.slice(), [props.items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_memo_the_compiler_could_not_preserve_is_left_alone() {
    // A missing dependency makes the compiler refuse the whole function rather
    // than drop a memo whose guarantees it cannot reproduce — so the report
    // this rule would have made is exactly the report that would be wrong.
    let diagnostics = memo(
        "component Sum(a: number, b: number) {\n  const total = useMemo(() => a + b, [a]);\n  return <p>{total}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_function_that_opted_out_of_the_compiler_keeps_its_memoization() {
    let diagnostics = memo(
        "component List(items: Array<string>) {\n  \"use no memo\";\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_project_with_the_compiler_switched_off_keeps_its_memoization() {
    // Nothing removes it, so it is not redundant — and a rule that said
    // otherwise would be telling a reader to delete the only memoization the
    // build produces.
    let mut config = only(REDUNDANT_MEMO);
    config.app.builtins.react_compiler.enabled = false;
    let file = at(
        "app/index.js",
        &module(
            "component List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
        ),
    );

    let diagnostics = lint_source(&file, &config).expect("lint").diagnostics;

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

// --- Both rules -------------------------------------------------------------

#[test]
fn a_module_that_names_neither_hook_is_not_parsed_at_all() {
    // The gate that keeps the parse off the common path. It is behaviour
    // rather than an optimisation detail: a module with no `useEffect`,
    // `useMemo` or `useCallback` in it cannot violate either rule, and one
    // that fails to parse must be reported by `flow/syntax` and nothing else.
    let broken = "// @flow\ncomponent Page( {\n";

    assert!(lint_js(DERIVED_STATE, broken).is_empty());
    assert!(lint_js(REDUNDANT_MEMO, broken).is_empty());
}

#[test]
fn a_module_that_does_not_parse_reports_nothing_here() {
    let broken = "// @flow\nimport {useEffect, useMemo, useState} from \"react\";\ncomponent Page( {\n  useEffect(() => setA(1), [a]);\n";

    assert!(lint_js(DERIVED_STATE, broken).is_empty());
    assert!(lint_js(REDUNDANT_MEMO, broken).is_empty());
}

#[test]
fn a_non_flow_file_is_not_parsed() {
    assert!(
        lint_one(
            DERIVED_STATE,
            "app/page.md",
            "useEffect(() => setA(1), [a]);\n"
        )
        .is_empty()
    );
}
