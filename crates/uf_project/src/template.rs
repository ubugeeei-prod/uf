//! The literal contents of every file `uf create` writes.
//!
//! Each scaffold is one list of `(relative path, contents)` pairs, so the file
//! set a template produces can be read at a glance and changing what a new
//! project looks like never touches the code that puts it on disk. The app
//! template doubles as the crate's worked example of idiomatic `// @flow` uf
//! source.

/// The files `uf create app react` writes.
///
/// Every import here resolves to a package that is implemented, so a freshly
/// created project runs. This used to be a showcase — it imported the effect
/// library, the validator, the query client, the UI kit, Relay, StyleX and
/// React Native — and ten of those packages are declarations that throw when
/// called, so the project it produced could not start. A starter that does not
/// start teaches nothing.
pub(crate) fn app_react_files(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("package.json", app_package_json(name)),
        ("uf.config.js", app_config()),
        ("app.js", app_entry()),
        ("app/_uf.layout.js", app_layout()),
        ("app/_uf.page.js", app_page()),
        ("app/_uf.page.test.js", app_test()),
        ("app/Counter.js", app_counter()),
        ("app/useCounter.js", app_counter_hook()),
    ]
}

pub(crate) fn lib_files(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("package.json", lib_package_json(name)),
        ("uf.config.js", lib_config()),
        ("index.js", lib_index()),
        ("index.test.js", lib_test()),
    ]
}

fn app_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "private": true,
  "type": "module",
  "dependencies": {{
    "@uniflowed/config": "latest",
    "@uniflowed/react": "latest",
    "@uniflowed/router": "latest",
    "@uniflowed/vite": "latest",
    "react": "^19.2.0",
    "react-dom": "^19.2.0"
  }},
  "devDependencies": {{
    "@uniflowed/test": "latest"
  }}
}}
"#
    )
}

fn lib_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "type": "module",
  "exports": {{
    ".": "./index.js"
  }},
  "devDependencies": {{
    "@uniflowed/config": "latest",
    "@uniflowed/host": "latest",
    "@uniflowed/test": "latest"
  }}
}}
"#
    )
}

fn app_config() -> String {
    r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  tasks: {
    dev: { command: "uf dev" },
    build: { command: "uf build" },
    check: { command: "uf check" },
    lint: { command: "uf lint" },
    fmt: { command: "uf fmt" },
    test: { command: "uf test" },
  },
});
"#
    .to_string()
}

fn lib_config() -> String {
    r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      enabled: false,
    },
  },
  tasks: {
    build: { command: "uf build" },
    check: { command: "uf check" },
    lint: { command: "uf lint" },
    fmt: { command: "uf fmt" },
    test: { command: "uf test" },
  },
});
"#
    .to_string()
}

fn app_entry() -> String {
    r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#
    .to_string()
}

fn app_layout() -> String {
    r#"// @flow
import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";

export component Layout(children: mixed) {
  return (
    <html lang="en">
      <body>
        <Suspense fallback={null}>{children}</Suspense>
      </body>
    </html>
  );
}
"#
    .to_string()
}

fn app_page() -> String {
    r#"// @flow
import * as React from "@uniflowed/react";

import Counter from "./Counter.js";

/// The states this page can be in. An enum rather than a union of strings so
/// the `match` below is exhaustive: adding a member here stops compiling until
/// every place that reads it has been updated.
enum Mood {
  Calm,
  Sharp,
}

component Headline(mood: Mood) {
  const tone = match (mood) {
    Mood.Calm => "at native speed",
    Mood.Sharp => "without the pile of tools",
  };

  return <h1>Flow {tone}</h1>;
}

export default component Page() {
  return (
    <main>
      <Headline mood={Mood.Calm} />
      <p>
        Edit <code>app/_uf.page.js</code> and this page reloads. There is no second config file to
        keep in step with this one.
      </p>
      <Counter initial={0} />
    </main>
  );
}
"#
    .to_string()
}

fn app_counter() -> String {
    r#""use client";
// @flow
import * as React from "@uniflowed/react";

import { useCounter } from "./useCounter.js";

/// A component declaration: Flow reads the parameter list as the props, so
/// there is no separate props type to keep in step with the signature.
export default component Counter(initial: number) {
  const [count, increment] = useCounter(initial);

  return (
    <button type="button" onClick={increment}>
      count: {count}
    </button>
  );
}
"#
    .to_string()
}

fn app_counter_hook() -> String {
    r#""use client";
// @flow
import { useCallback, useState } from "@uniflowed/react";

/// A hook declaration. Flow refuses a call to this from anywhere that is not a
/// component or another hook, so the rules of hooks are a type error rather
/// than a lint rule you have to remember to install.
export hook useCounter(initial: number): [number, () => void] {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => setCount((value) => value + 1), []);
  return [count, increment];
}
"#
    .to_string()
}

fn app_test() -> String {
    r#"// @flow
import { describe, expect, it } from "@uniflowed/test";

import { useCounter } from "./useCounter.js";

describe("useCounter", () => {
  it("is a hook, so it is only callable from a component or another hook", () => {
    expect(typeof useCounter).toBe("function");
  });
});
"#
    .to_string()
}

fn lib_index() -> String {
    r#"// @flow
export opaque type UniflowedId = string;

export function createId(raw: string): UniflowedId {
  return raw;
}
"#
    .to_string()
}

fn lib_test() -> String {
    r#"// @flow
import { describe, expect, it } from "@uniflowed/test";
import { createId } from "./index.js";

describe("createId", () => {
  it("preserves the source value behind an opaque boundary", () => {
    expect(createId("flow")).toBe("flow");
  });
});
"#
    .to_string()
}
