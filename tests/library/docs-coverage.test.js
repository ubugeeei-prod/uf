// @flow
//
// `tools/docs/coverage.js`, the check that what uf ships is documented.
//
// The check itself runs in CI as `uf run docs:coverage`, against the real
// binary and the real reference pages. These are its parts, on small inputs,
// so a change to how a heading or a table row is read is caught here with the
// case that broke rather than as a wall of new "gaps" in CI.

import { describe, expect, it } from "@uniflowed/test";

import {
  compare,
  gaps,
  keyDocumented,
  keyPaths,
  namedKeys,
  parseHelp,
  readKnown,
  sections,
} from "../../tools/docs/coverage.js";

const HELP = `Run the project's tests

Usage: uf test [OPTIONS] [PATH]...

Commands:
  watch   Rerun on change
  help    Print this message or the help of the given subcommand(s)

Arguments:
  [PATH]...
          Only these files

Options:
  -t, --test-name-pattern <PATTERN>
          Only tests whose name contains PATTERN

      --shard <INDEX/COUNT>
          One part of the suite

      --cwd <DIR>
          Run as if uf had been started in DIR

  -h, --help
          Print help
`;

describe("parseHelp", () => {
  it("reads subcommands, leaving out clap's own help", () => {
    expect(parseHelp(HELP).subcommands).toEqual(["watch"]);
  });

  it("reads every long flag, short-aliased or not", () => {
    expect(parseHelp(HELP).flags).toEqual(["--test-name-pattern", "--shard", "--cwd", "--help"]);
  });
});

describe("sections", () => {
  it("runs a section to the next heading of its rank or above, and skips fenced code", () => {
    const page = [
      "## Running",
      "### uf test [PATH...]",
      "Takes `--shard`.",
      "#### Shards",
      "```sh",
      "# not a heading",
      "```",
      "### uf dev",
      "## Other",
    ].join("\n");
    const found = sections(page);
    expect(found.map((section) => section.heading)).toEqual([
      "Running",
      "uf test [PATH...]",
      "Shards",
      "uf dev",
      "Other",
    ]);
    expect(found[1].body.includes("# not a heading")).toBe(true);
    expect(found[1].body.includes("uf dev")).toBe(false);
  });
});

describe("namedKeys", () => {
  const page = [
    "## app",
    "",
    "| Key | Default |",
    "| --- | --- |",
    '| `router.entry` | `"app.js"` |',
    "| `rendering.cache.route`, `.fetch` | `false` |",
    "| `react.*` | — |",
    "",
    "## pm and publish",
    "",
    "| `registry` | — |",
    "",
    "```js",
    "| `fenced.key` | — |",
    "```",
  ].join("\n");
  const named = namedKeys(page);

  it("reads a key relative to the section for its first segment", () => {
    expect(keyDocumented(named, "app.router.entry")).toBe(true);
    expect(keyDocumented(named, "app.router.root")).toBe(false);
  });

  it("continues a `.sibling` from the key before it in the row", () => {
    expect(keyDocumented(named, "app.rendering.cache.fetch")).toBe(true);
  });

  it("reads `x.*` as everything under x", () => {
    expect(keyDocumented(named, "app.react.strictMode")).toBe(true);
  });

  it("reads a heading that names several sections as each of them", () => {
    expect(keyDocumented(named, "pm.registry")).toBe(true);
    expect(keyDocumented(named, "publish.registry")).toBe(true);
  });

  it("does not read keys out of fenced code", () => {
    expect(keyDocumented(named, "pm.fenced.key")).toBe(false);
  });
});

describe("keyPaths", () => {
  it("lists leaves, and stops at a map whose keys are the project's", () => {
    expect(
      keyPaths({
        app: { router: { entry: "app.js", basePath: null }, targets: [] },
        lint: { rules: { "a11y/alt-text": "error" }, engine: "rust" },
        tasks: {},
      }),
    ).toEqual([
      "app.router.entry",
      "app.router.basePath",
      "app.targets",
      "lint.rules",
      "lint.engine",
      "tasks",
    ]);
  });
});

describe("gaps", () => {
  const cliPage = [
    "## Global flags",
    "`--cwd DIR`, `--color`, `-h, --help`, `-V, --version`",
    "### uf test [PATH...]",
    "`--shard INDEX/COUNT` splits a suite; `--shard-count` is not `--shard`.",
  ].join("\n");

  it("names each undocumented command, flag, key and package once, sorted", () => {
    expect(
      gaps({
        commands: new Map([
          ["test", ["--shard", "--changed", "--cwd"]],
          ["migrate", ["--dry-run"]],
        ]),
        keys: ["app.router.entry", "app.rsc"],
        packages: ["@uniflowed/state", "@uniflowed/immer"],
        cliPage,
        configPage: "## app\n| `router.entry` | x |",
        packagesPage: "`@uniflowed/state` is atoms. `@uniflowed/immer-extra` is not immer.",
      }),
    ).toEqual([
      "command uf migrate",
      "config app.rsc",
      "flag uf test --changed",
      "package @uniflowed/immer",
    ]);
  });

  it("does not count a flag that only appears inside a longer one", () => {
    expect(
      gaps({
        commands: new Map([["test", ["--shard-count"]]]),
        keys: [],
        packages: [],
        cliPage: cliPage.replace("`--shard-count` is not ", ""),
        configPage: "",
        packagesPage: "",
      }),
    ).toEqual(["flag uf test --shard-count"]);
  });
});

describe("the known list", () => {
  it("reads lines, not comments or blanks", () => {
    expect(readKnown("# header\n\ncommand uf a\n  flag uf b --c  \n")).toEqual([
      "command uf a",
      "flag uf b --c",
    ]);
  });

  it("reports a new gap and a closed one separately", () => {
    expect(compare(["command uf a", "config b"], ["config b", "config c"])).toEqual({
      added: ["config c"],
      closed: ["command uf a"],
    });
  });
});
