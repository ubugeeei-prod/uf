//! The literal contents of every file `uf create` writes.
//!
//! Each scaffold is one list of `(relative path, contents)` pairs, so the file
//! set a template produces can be read at a glance and changing what a new
//! project looks like never touches the code that puts it on disk. The app
//! template doubles as the crate's worked example of idiomatic `// @flow` uf
//! source.

pub(crate) fn app_react_files(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("package.json", app_package_json(name)),
        ("uf.config.js", app_config()),
        ("app.js", app_entry()),
        ("app/_uf.layout.js", app_layout()),
        ("app/_uf.middleware.js", app_middleware()),
        ("app/_uf.page.js", app_page()),
        ("app/_uf.page.native.js", app_native_page()),
        ("app/_uf.page.test.js", app_test()),
        ("app/client/Counter.js", app_client_counter()),
        ("app/client/useCounter.js", app_client_hook()),
        ("app/styles/tokens.stylex.js", stylex_tokens()),
        ("server/actions.js", app_server_actions()),
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
    "@uniflowed/core": "latest",
    "@uniflowed/react": "latest",
    "@uniflowed/router": "latest",
    "@uniflowed/vite": "latest",
    "react": "^19.2.0",
    "react-dom": "^19.2.0"
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
  "dependencies": {{
    "@uniflowed/core": "latest"
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
    test: { command: "uf test --list" },
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
    test: { command: "uf test --list" },
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

fn app_middleware() -> String {
    r#"// @flow
import { next } from "@uniflowed/router";

export default function middleware() {
  return next();
}
"#
    .to_string()
}

fn app_page() -> String {
    r#"// @flow
import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";
import { createFetch, request } from "@uniflowed/fetch";
import { effect, call } from "@uniflowed/effect";
import { createLoader, useLoader } from "@uniflowed/loader";
import { cell } from "@uniflowed/state";
import { createQuery } from "@uniflowed/query";
import { graphql, useLazyLoadQuery } from "@uniflowed/relay";
import { stylex } from "@uniflowed/stylex";
import { Button, Dialog, Form } from "@uniflowed/ui";
import { v } from "@uniflowed/validator";
import { refreshGreeting } from "../server/actions.js";
import Counter from "./client/Counter.js";
import { tokens } from "./styles/tokens.stylex.js";

const selectedTone = cell<"calm" | "sharp">("calm");
const HomeQuery = graphql("query HomeQuery { viewer { name } }");
const apiBase = "/api";
const api = createFetch({ baseURL: apiBase });
const viewerLoader = createLoader<{| name: string |}>("viewer", () => request(api, "/viewer"));
const contactSchema = v.object({
  name: v.pipe(v.string(), v.minLength(1)),
});

const greetingQuery = createQuery<string>({
  key: ["home", "greeting", apiBase],
  query: () => effect(function* () {
    return yield call(refreshGreeting);
  }),
});

const styles = stylex.create({
  shell: {
    minHeight: "100vh",
    display: "grid",
    placeItems: "center",
    backgroundColor: tokens.canvas,
    color: tokens.ink,
  },
});

component Page() {
  const greeting = greetingQuery.use();
  const viewerState = useLoader(viewerLoader);
  const viewer = use(useLazyLoadQuery<{| viewer: {| name: string |} |}>(HomeQuery, {}));
  const viewerName = viewerState.status === "ready" ? viewerState.value.name : viewer.viewer.name;

  return (
    <main {...stylex.props(styles.shell)}>
      <h1>{greeting.value ?? viewerName}</h1>
      <p>tone: {selectedTone.get()}</p>
      <Counter initial={1} />
      <Form.Root schema={contactSchema}>
        <Form.Field>
          <Form.Label>Name</Form.Label>
          <Form.Control />
          <Form.Message />
        </Form.Field>
        <Form.Submit>Send</Form.Submit>
      </Form.Root>
      <Dialog.Root>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Body>
          <Button>Native UI, preset styles, RSC split</Button>
        </Dialog.Body>
      </Dialog.Root>
    </main>
  );
}

export default Page;
"#
    .to_string()
}

fn app_server_actions() -> String {
    r#""use server";
// @flow
import { serverAction } from "@uniflowed/server";

export const refreshGreeting = serverAction(async (): Promise<string> => {
  return "Flow at native speed";
});
"#
    .to_string()
}

fn app_native_page() -> String {
    r#"// @flow
import * as React from "@uniflowed/react";
import { Text, View } from "@uniflowed/react-native";
import { stylex } from "@uniflowed/stylex";

const styles = stylex.create({
  shell: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
  },
});

component Page() {
  return (
    <View {...stylex.props(styles.shell)}>
      <Text>Flow at native speed</Text>
    </View>
  );
}

export default Page;
"#
    .to_string()
}

fn app_client_counter() -> String {
    r#""use client";
// @flow
import * as React from "@uniflowed/react";
import { Button } from "@uniflowed/ui";
import { useCounter } from "./useCounter.js";

component Counter(initial: number) {
  const [count, increment] = useCounter(initial);

  return <Button onClick={increment}>count: {count}</Button>;
}

export default Counter;
"#
    .to_string()
}

fn app_client_hook() -> String {
    r#""use client";
// @flow
import { useState } from "@uniflowed/react";

export hook useCounter(initial: number): [number, () => void] {
  const [count, setCount] = useState(initial);
  return [count, () => setCount(count + 1)];
}
"#
    .to_string()
}

fn app_test() -> String {
    r#"// @flow
import * as React from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/test";
import { render, screen } from "@uniflowed/react-testing";
import Page from "./_uf.page.js";

describe("Page", () => {
  it("renders the starter headline", async () => {
    render(<Page />);
    await expect(screen.findByText("Flow at native speed")).resolves.toBeVisible();
  });
});
"#
    .to_string()
}

fn stylex_tokens() -> String {
    r##"// @flow
import { stylex } from "@uniflowed/stylex";

export const tokens = stylex.defineVars({
  canvas: "#f7f7f2",
  ink: "#151b1f",
});
"##
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
