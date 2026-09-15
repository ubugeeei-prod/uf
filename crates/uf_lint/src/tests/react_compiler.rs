//! `react-compiler/*`: the official React Compiler's diagnostics.
//!
//! What the compiler decides is the compiler's, and `uf_transform::lint`'s own
//! tests pin how it is asked. These pin what `uf lint` does with the answer:
//! the rule a category is filed under, its level, where it is reported, what
//! the retired `react/*` ids still do, and that the findings are the ones
//! `eslint-plugin-react-hooks` reports on the same code.

use super::*;

const HOOKS: &str = "react-compiler/hooks";
const PURITY: &str = "react-compiler/purity";
const GLOBALS: &str = "react-compiler/globals";

/// `(line, column)` of every diagnostic `rule` produced.
fn at(diagnostics: &[Diagnostic], rule: &str) -> Vec<(usize, usize)> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == rule)
        .map(|diagnostic| (diagnostic.line, diagnostic.column))
        .collect()
}

#[test]
fn a_hook_called_conditionally_is_reported_where_it_is_called() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\ncomponent Page(flag: boolean) {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return null;\n}\n",
    );

    assert_eq!(at(&diagnostics, HOOKS), [(4, 17)], "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .starts_with("Hooks must always be called in a consistent order"),
        "the message is the compiler's: {diagnostics:?}"
    );
}

#[test]
fn a_hook_called_in_a_callback_is_reported() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\ncomponent List(items: Array<string>) {\n  items.forEach(() => {\n    useEffect(() => {});\n  });\n  return null;\n}\n",
    );

    assert_eq!(at(&diagnostics, HOOKS), [(4, 5)], "{diagnostics:?}");
}

/// ubugeeei-prod/uf#477: a hook called in an object literal is not a
/// conditional call.
#[test]
fn a_hook_called_in_an_object_literal_is_not_conditional() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\nimport { useState } from \"react\";\n\nexport hook useThing(): { a: number, b: number } {\n  const bag = {\n    a: useState(0)[0],\n    b: useState(1)[0],\n  };\n\n  return bag;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// And the same literal inside a condition is a conditional call.
#[test]
fn an_object_literal_built_conditionally_is_conditional() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\nimport { useState } from \"react\";\n\nexport hook useThing(flag: boolean): { a: number } | null {\n  if (flag) {\n    return { a: useState(0)[0] };\n  }\n  return null;\n}\n",
    );

    assert_eq!(at(&diagnostics, HOOKS).len(), 1, "{diagnostics:?}");
}

#[test]
fn hooks_called_unconditionally_are_accepted() {
    for source in [
        "// @flow\ncomponent Page() {\n  const [a, setA] = useState(0);\n  useEffect(() => {});\n  return null;\n}\n",
        "// @flow\nhook useThing(): number {\n  const [a] = useState(0);\n  return a;\n}\n",
        "// @flow\nexport const useThing = (): number => {\n  const [a] = useState(0);\n  return a;\n};\n",
        "// @flow\ncomponent Page() {\n  const s = \"}\";\n  const [a] = useState(0);\n  return s;\n}\n",
    ] {
        let diagnostics = lint_js(HOOKS, source);
        assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
    }
}

/// A method named like a hook, on a value that can change between renders, is
/// a hook that may not be the same function every render — which the compiler
/// reports, and `eslint-plugin-react-hooks` with it.
#[test]
fn a_hook_named_method_on_a_prop_is_a_dynamic_hook() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\ncomponent Page(api: Api) {\n  api.useThing();\n  return null;\n}\n",
    );

    assert_eq!(at(&diagnostics, HOOKS), [(3, 3)], "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .starts_with("Hooks must be the same function on every render"),
        "{diagnostics:?}"
    );
}

/// The compiler checks the functions it compiles — components and hooks, by
/// declaration or by name — and a hook called anywhere else is not in one.
#[test]
fn a_hook_called_outside_components_and_hooks_is_not_the_compilers_to_report() {
    for source in [
        "// @flow\nconst value = useState(0);\n",
        "// @flow\nfunction helper() {\n  return useState(0);\n}\n",
        "// @flow\nfunction useThing(): number {\n  return 1;\n}\n",
    ] {
        let diagnostics = lint_js(HOOKS, source);
        assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
    }
}

/// `eslint-plugin-react-hooks` compiles only a module whose top-level
/// statements declare something named like a component or a hook, and `uf lint`
/// takes the same modules — a component made only by `memo(…)` is in neither.
#[test]
fn a_module_the_eslint_plugin_would_not_compile_is_not_reported() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\nimport { memo, useState } from \"react\";\n\nexport const Card = memo((props: { flag: boolean }) => {\n  if (props.flag) {\n    useState(0);\n  }\n  return <p />;\n});\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn purity_and_globals_are_errors_by_default() {
    let report = lint_source(
        &at_path(
            "app/index.js",
            "// @flow\nlet renders = 0;\nexport component Clock() {\n  renders = renders + 1;\n  return <p>{Date.now()}</p>;\n}\n",
        ),
        &UniflowedConfig::default(),
    )
    .expect("lint");

    for rule in [GLOBALS, PURITY] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == rule && diagnostic.severity == Severity::Error),
            "{rule}: {:?}",
            report.diagnostics
        );
    }
}

/// The retired ids point at the compiler's categories, and a level written
/// against one still applies — in a real config, where uf's defaults are
/// already in the table under the new id.
#[test]
fn a_level_written_against_a_retired_id_still_applies() {
    let source = at_path(
        "app/index.js",
        "// @flow\ncomponent Page(flag: boolean) {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return <p>{Date.now()}</p>;\n}\n",
    );
    let mut config = UniflowedConfig::default();
    config.lint.rules.insert(
        CompactString::const_new("react/hooks-rules"),
        RuleLevel::Off,
    );
    config.lint.rules.insert(
        CompactString::const_new("react/no-render-side-effects"),
        RuleLevel::Warn,
    );

    let report = lint_source(&source, &config).expect("lint");

    assert!(
        !fired(&report.diagnostics, HOOKS),
        "{:?}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == PURITY && diagnostic.severity == Severity::Warn),
        "{:?}",
        report.diagnostics
    );

    // Naming the new id at a level other than its default wins over the old.
    config
        .lint
        .rules
        .insert(CompactString::const_new(HOOKS), RuleLevel::Warn);
    let report = lint_source(&source, &config).expect("lint");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == HOOKS && diagnostic.severity == Severity::Warn),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn a_suppression_written_against_a_retired_id_still_applies() {
    let diagnostics = lint_js(
        HOOKS,
        "// @flow\ncomponent Page(flag: boolean) {\n  if (flag) {\n    // uf-lint-disable-next-line react/hooks-rules\n    const [a] = useState(0);\n  }\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Plain JavaScript, linted by `eslint-plugin-react-hooks` 7.1.1 with every
/// compiler rule on, in a scratch project outside this repository. Each
/// expected position below is a finding that run reported, under the plugin's
/// name for the same category (`react-hooks/hooks`, `react-hooks/purity`,
/// `react-hooks/globals`); it reported the other categories in `RENDER` too,
/// which `uf lint` does not file under a rule.
#[test]
fn the_findings_are_eslint_plugin_react_hooks_findings() {
    let hooks = lint_js(HOOKS, ESLINT_HOOKS);
    assert_eq!(
        at(&hooks, HOOKS),
        [(5, 17), (13, 5), (22, 15), (28, 5), (35, 5)],
        "{hooks:?}"
    );

    let mut config = only(PURITY);
    config
        .lint
        .rules
        .insert(CompactString::const_new(GLOBALS), RuleLevel::Error);
    let render = lint_source(&at_path("app/render.js", ESLINT_RENDER), &config)
        .expect("lint")
        .diagnostics;
    assert_eq!(at(&render, PURITY), [(17, 14), (21, 13)], "{render:?}");
    assert_eq!(at(&render, GLOBALS), [(26, 3)], "{render:?}");
}

fn at_path(path: &str, source: &str) -> SourceFile {
    SourceFile {
        path: path.to_owned(),
        source: source.to_owned(),
    }
}

const ESLINT_HOOKS: &str = r"import { useEffect, useState } from 'react';

export function Conditional({ flag }) {
  if (flag) {
    const [a] = useState(0);
    return <p>{a}</p>;
  }
  return null;
}

export function InCallback({ items }) {
  items.forEach(() => {
    useEffect(() => {});
  });
  return <ul />;
}

export function AfterEarlyReturn({ flag }) {
  if (flag) {
    return null;
  }
  const [a] = useState(0);
  return <p>{a}</p>;
}

export function useConditional(flag) {
  if (flag) {
    useEffect(() => {});
  }
  return useState(0);
}

export function InLoop({ items }) {
  for (const item of items) {
    useState(item);
  }
  return <ul />;
}

// Neither a component nor a hook by name, so the compiler never compiles it.
export function helper() {
  return useState(0);
}

export function Fine() {
  const [a, setA] = useState(0);
  useEffect(() => {}, []);
  return <button onClick={() => setA(a + 1)}>{a}</button>;
}
";

const ESLINT_RENDER: &str = r"import { useEffect, useRef, useState } from 'react';

let renders = 0;

export function ReadRefInRender() {
  const ref = useRef(0);
  return <p>{ref.current}</p>;
}

export function WriteRefInRender({ value }) {
  const ref = useRef(null);
  ref.current = value;
  return <p />;
}

export function Clock() {
  return <p>{Date.now()}</p>;
}

export function Dice() {
  const n = Math.random();
  return <p>{n}</p>;
}

export function CountRenders() {
  renders = renders + 1;
  return <p>{renders}</p>;
}

export function MutateProps(props) {
  props.items.push('x');
  return <ul>{props.items}</ul>;
}

export function MutateState() {
  const [list] = useState([]);
  list.push(1);
  return <p>{list.length}</p>;
}

export function SetStateInRender() {
  const [n, setN] = useState(0);
  setN(n + 1);
  return <p>{n}</p>;
}

export function SetStateInEffect({ value }) {
  const [copy, setCopy] = useState(value);
  useEffect(() => {
    setCopy(value);
  }, [value]);
  return <p>{copy}</p>;
}

export function Guarded({ render }) {
  try {
    return <div>{render()}</div>;
  } catch (error) {
    return <p>failed</p>;
  }
}

export function Parent({ label }) {
  const Child = () => <p>{label}</p>;
  return <Child />;
}

export function Fine({ items, url }) {
  const ref = useRef(null);
  const [data, setData] = useState(null);
  useEffect(() => {
    fetch(url).then((response) => setData(response));
  }, [url]);
  const copy = [...items, 'x'];
  return (
    <button ref={ref} onClick={() => { ref.current = Date.now(); }}>
      {copy}
      {data}
    </button>
  );
}
";
