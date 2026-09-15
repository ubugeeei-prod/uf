// @flow
//
// The registry, rendered: every component `uf ui add` writes, its example
// running, and the source of both.
//
// Read from `registry/ui/` when the site is built, the directory `uf` embeds
// when uf is built, so this section and `uf ui list` cannot disagree about what
// exists. A server module, so the sources reach the page as text and go no
// further: only the examples, through `registry-examples.js`, reach the browser
// as code.

import * as React from "@uniflowed/react";

import { Command } from "../../_design/parts.js";
import { RegistryExample } from "./registry-examples.js";

/**
 * Every component file, as text, by path.
 *
 * The exclusions are spelled from `registry/ui/` like the pattern they narrow.
 * A `!**` exclusion is resolved from this file's directory, so it matches
 * nothing in the registry, and every example and test would be read as a
 * component of its own.
 */
const COMPONENTS = import.meta.glob<string>(
  [
    "../../../../registry/ui/*.js",
    "!../../../../registry/ui/*.example.js",
    "!../../../../registry/ui/*.test.js",
  ],
  { eager: true, import: "default", query: "?raw" },
);

/** Every example file, as text, by path. */
const EXAMPLES = import.meta.glob<string>("../../../../registry/ui/*.example.js", {
  eager: true,
  import: "default",
  query: "?raw",
});

type Entry = {|
  readonly name: string,
  readonly description: string,
  readonly source: string,
  readonly example: string,
|};

/** Every component in the registry, in alphabetical order. */
function entries(): $ReadOnlyArray<Entry> {
  const found: Array<Entry> = [];
  for (const path of Object.keys(COMPONENTS)) {
    const name = path.slice(path.lastIndexOf("/") + 1, -".js".length);
    const source = COMPONENTS[path];
    found.push({
      name,
      description: description(source),
      source,
      example: EXAMPLES[`../../../../registry/ui/${name}.example.js`] ?? "",
    });
  }
  return found.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

/**
 * What a component's header says it is: the text after its `// Title: `, up to
 * the first blank comment line. It is the rule `uf ui list` reads a description
 * by, in `crates/uf_ui/src/registry.rs`.
 */
function description(source: string): string {
  const lines = source.split("\n").map((line) => line.trimEnd());
  const first = lines.findIndex(
    (line) => !(line === "" || line === '"use client";' || line === "// @flow" || line === "//"),
  );
  const opening = first < 0 ? "" : lines[first];
  const colon = opening.indexOf(": ");
  if (!opening.startsWith("// ") || colon < 0) {
    return "";
  }
  const words = [opening.slice(colon + ": ".length).trim()];
  for (const line of lines.slice(first + 1)) {
    const more = line.startsWith("// ") ? line.slice("// ".length).trim() : "";
    if (more === "") {
      break;
    }
    words.push(more);
  }
  return words.join(" ");
}

/** `text`, with its backquoted spans set as code. */
function inline(text: string): React.Node {
  return text
    .split("`")
    .map((part, index) => (index % 2 === 1 ? <code key={`${index}:${part}`}>{part}</code> : part));
}

/**
 * An entry for every component in the registry: its name, what it is, the
 * command that writes it, its example running, and the two files folded under
 * it.
 */
export component Registry() {
  return (
    <>
      {entries().map((entry) => (
        <div className="registry-entry" key={entry.name}>
          <h3 id={`registry-${entry.name}`}>
            <code>{entry.name}</code>
          </h3>
          <p>{inline(entry.description)}</p>
          <Command>{`uf ui add ${entry.name}`}</Command>
          <div className="registry-example">
            <RegistryExample name={entry.name} />
          </div>
          <details className="registry-source">
            <summary>
              The example, <code>{`${entry.name}.example.js`}</code>
            </summary>
            <pre>
              <code>{entry.example}</code>
            </pre>
          </details>
          <details className="registry-source">
            <summary>
              The component, <code>{`${entry.name}.js`}</code>
            </summary>
            <pre>
              <code>{entry.source}</code>
            </pre>
          </details>
        </div>
      ))}
    </>
  );
}
