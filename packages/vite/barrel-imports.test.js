// @flow
//
// Named imports from `@uniflowed/ui`, rewritten to the files that define them
// before the rsc pass records client modules (ubugeeei-prod/uf#1118).
//
// Each case calls a hook directly, as `flight.test.js` does, against this
// repository's real barrel: the reading has to agree with what
// `packages/ui/index.js` exports at run time, and a copy of the barrel here
// would only agree with itself. What a real `uf build` and `uf dev` ship
// because of the rewrite is `crates/uf_cli/tests/vite.rs`.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { afterAll, describe, expect, it } from "@uniflowed/test";
import { parseAst } from "vite";

import uniflowed from "./index.js";
import {
  NAMESPACE_QUERY,
  barrelExports,
  barrelImportsPlugin,
  namespaceViewOf,
  namespaceViewSource,
} from "./internal/barrel-imports.js";

/** This repository's barrel, the file a project's `@uniflowed/ui` resolves to. */
const BARREL: string = fileURLToPath(new URL("../ui/index.js", import.meta.url));

/** A file of the package, spelled the way a rewritten import names it. */
function uiFile(name: string): string {
  return path.join(path.dirname(BARREL), name).split(path.sep).join("/");
}

/** The id of the view that serves one of the barrel's namespaces. */
function viewOf(name: string): string {
  return `${uiFile("index.js")}?${NAMESPACE_QUERY}=${name}`;
}

/** The barrel as the plugin reads it. */
function reading(): $FlowFixMe {
  return barrelExports(fs.readFileSync(BARREL, "utf8"), BARREL);
}

/** A plugin context whose `resolve` finds `barrel`, as Vite's does for a project. */
function contextFor(barrel: string = BARREL): $FlowFixMe {
  const warnings: Array<string> = [];
  return {
    warnings,
    resolve: async (id: string) => (id === "@uniflowed/ui" ? { id: barrel } : null),
    addWatchFile: () => {},
    warn: (message: string) => {
      warnings.push(message);
    },
  };
}

/** `code` after the rewrite, or `null` when the plugin left it alone. */
async function rewritten(code: string, context: $FlowFixMe = contextFor()): Promise<string | null> {
  const out = await barrelImportsPlugin().transform.call(context, code, "/project/app/$page.js");
  return out == null ? null : out.code;
}

/** A module's namespace object, imported by absolute path. */
async function load(file: string): Promise<$FlowFixMe> {
  return import(pathToFileURL(file).href);
}

const directories: Array<string> = [];

afterAll(() => {
  for (const directory of directories) {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

describe("reading the barrel", () => {
  it("places every name `@uniflowed/ui` exports, at the very value it exports", async () => {
    // The guard on the whole rewrite. A name the reading gets wrong would be a
    // different value in a module that imports it, and a name it cannot place
    // would load the whole package again, so both are failures here rather
    // than a bundle that is quietly larger or quietly broken.
    const exports = reading();
    const barrel = await load(BARREL);
    const names = Object.keys(barrel).sort();
    const placed = [...exports]
      .filter(([, target]) => target.kind !== "opaque")
      .map(([name]) => name)
      .sort();
    expect(placed).toEqual(names);

    for (const name of names) {
      const target = exports.get(name);
      if (target.kind === "binding") {
        expect((await load(target.file))[target.name]).toBe(barrel[name]);
        continue;
      }
      expect(target.parts.map((part) => part.key)).toEqual(Object.keys(barrel[name]));
      for (const part of target.parts) {
        expect((await load(part.file))[part.name]).toBe(barrel[name][part.key]);
      }
    }
  });

  it("names the module each component comes from, and both modules of a namespace that spans two", () => {
    const exports = reading();

    expect(exports.get("Switch")).toEqual({
      kind: "binding",
      file: uiFile("switch.js"),
      name: "Switch",
    });
    expect(exports.get("toast")).toEqual({
      kind: "binding",
      file: uiFile("toast.js"),
      name: "toast",
    });
    expect(exports.get("usePress")?.file).toBe(uiFile("interactions.js"));
    // `ContextMenu` is its own root and trigger over `Menu`'s parts, which is
    // why a view of it imports two files.
    const files = new Set(exports.get("ContextMenu").parts.map((part) => part.file));
    expect([...files].sort()).toEqual([uiFile("context-menu.js"), uiFile("menu.js")]);
    // A type is no value, and nothing imports it at run time.
    expect(exports.has("AccordionType")).toBe(false);
  });
});

describe("a module that imports from the barrel", () => {
  it("imports one component from the file that defines it", async () => {
    const code = await rewritten(
      'import { Switch } from "@uniflowed/ui";\nexport const control = Switch;\n',
    );

    expect(code).toContain(`import { Switch } from ${JSON.stringify(uiFile("switch.js"))};`);
    expect(code).not.toContain('"@uniflowed/ui"');
    expect(code).toContain("export const control = Switch;");
  });

  it("keeps each alias, and imports several names from each of their files", async () => {
    const code = await rewritten(
      'import { Switch as Toggle, Checkbox, toast } from "@uniflowed/ui";\n',
    );

    expect(code).toContain(
      `import { Switch as Toggle } from ${JSON.stringify(uiFile("switch.js"))};`,
    );
    expect(code).toContain(`import { Checkbox } from ${JSON.stringify(uiFile("checkbox.js"))};`);
    expect(code).toContain(`import { toast } from ${JSON.stringify(uiFile("toast.js"))};`);
  });

  it("imports a namespace from a view of the barrel", async () => {
    const code = await rewritten(
      'import { Dialog, ContextMenu as RowMenu } from "@uniflowed/ui";\n',
    );

    expect(code).toContain(`import { Dialog } from ${JSON.stringify(viewOf("Dialog"))};`);
    expect(code).toContain(
      `import { ContextMenu as RowMenu } from ${JSON.stringify(viewOf("ContextMenu"))};`,
    );
  });

  it("re-exports from the defining files too", async () => {
    const code = await rewritten('export { Switch, Dialog as Modal } from "@uniflowed/ui";\n');

    expect(code).toContain(`export { Switch } from ${JSON.stringify(uiFile("switch.js"))};`);
    expect(code).toContain(`export { Dialog as Modal } from ${JSON.stringify(viewOf("Dialog"))};`);
    expect(code).not.toContain('"@uniflowed/ui"');
  });

  it("leaves a name it cannot place on the barrel, and moves the rest", async () => {
    // The bundler's own error for a name the barrel does not export, rather
    // than a rewrite that invents a module for it.
    const code = await rewritten('import { Switch, NotAPart } from "@uniflowed/ui";\n');

    expect(code).toContain('import { NotAPart } from "@uniflowed/ui";');
    expect(code).toContain(`import { Switch } from ${JSON.stringify(uiFile("switch.js"))};`);
  });

  it("leaves every form that binds no name alone, as uf_rsc does", async () => {
    for (const code of [
      'import * as ui from "@uniflowed/ui";\nexport const all = ui;\n',
      'export * from "@uniflowed/ui";\n',
      'export const later = () => import("@uniflowed/ui");\n',
    ]) {
      expect(await rewritten(code)).toBe(null);
    }
  });

  it("drops a bare import, which would load every module for nothing", async () => {
    const code = await rewritten('import "@uniflowed/ui";\nexport const kept = 1;\n');

    expect(code).not.toContain("@uniflowed/ui");
    expect(code).toContain("export const kept = 1;");
  });

  it("keeps every line after an import where it was", async () => {
    // No source map is returned, so a position below the imports has to still
    // be the position the previous map in the chain gave it.
    const source = [
      "import {",
      "  Switch,",
      "  Dialog,",
      '} from "@uniflowed/ui";',
      'export const marker = "here";',
      "",
    ].join("\n");

    const code = (await rewritten(source)) ?? "";

    const line = (text: string) => text.split("\n").findIndex((each) => each.includes("marker"));
    expect(line(code)).toBe(line(source));
  });

  it("leaves imports of anything but the barrel alone", async () => {
    expect(await rewritten('import { Switch } from "@uniflowed/ui/switch";\n')).toBe(null);
    expect(await rewritten('import { thing } from "@uniflowed/uix";\n')).toBe(null);
  });

  it("changes nothing when the barrel cannot be read, and says so once", async () => {
    const context = contextFor(path.join(os.tmpdir(), "uf-no-such-barrel", "index.js"));

    expect(await rewritten('import { Switch } from "@uniflowed/ui";\n', context)).toBe(null);

    expect(context.warnings.length).toBe(1);
    expect(context.warnings[0]).toContain("@uniflowed/ui");
  });
});

describe("a view of a namespace", () => {
  it("is the barrel's path with the namespace in its query, and nothing else is", () => {
    expect(namespaceViewOf(viewOf("Dialog"))).toEqual({ file: uiFile("index.js"), name: "Dialog" });
    expect(namespaceViewOf(`${viewOf("Dialog")}&v=0b1c2d3e`)?.name).toBe("Dialog");
    expect(namespaceViewOf(BARREL)).toBe(null);
    expect(namespaceViewOf(`${uiFile("switch.js")}?v=0b1c2d3e`)).toBe(null);
    expect(namespaceViewOf(`\0${viewOf("Dialog")}`)).toBe(null);
  });

  it("imports its parts relative to the barrel, from each file they come from", () => {
    const source = barrelImportsPlugin().load(viewOf("ContextMenu"));
    const program = parseAst(source);

    const sources = program.body
      .filter((node) => node.type === "ImportDeclaration")
      .map((node) => node.source.value)
      .sort();
    expect(sources).toEqual(["./context-menu.js", "./menu.js"]);
    expect(source).toContain("export const ContextMenu = {");
  });

  it("evaluates to the object the barrel exports, for every namespace", async () => {
    // Written beside nothing and imported, with its relative imports made
    // absolute, so each part is the instance the barrel imports: the same
    // object, not an equal one.
    const barrel = await load(BARREL);
    const exports = reading();
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-barrel-view-"));
    directories.push(directory);

    const namespaces = [...exports].filter(([, target]) => target.kind === "namespace");
    expect(namespaces.length > 0).toBe(true);
    for (const [name] of namespaces) {
      const source = namespaceViewSource(exports, BARREL, name).replace(
        /from "\.\/([^"]+)"/g,
        (_, file) => `from ${JSON.stringify(pathToFileURL(uiFile(file)).href)}`,
      );
      const file = path.join(directory, `${name}.mjs`);
      fs.writeFileSync(file, source);

      const view = await load(file);

      expect(Object.keys(view)).toEqual([name]);
      expect(Object.keys(view[name])).toEqual(Object.keys(barrel[name]));
      for (const key of Object.keys(barrel[name])) {
        expect(view[name][key]).toBe(barrel[name][key]);
      }
    }
  });

  it("re-exports a name that is no namespace from the barrel itself", () => {
    expect(namespaceViewSource(reading(), BARREL, "Switch")).toBe(
      'export { Switch } from "./index.js";\n',
    );
  });

  it("is neither rewritten nor compiled as the barrel's Flow source", async () => {
    const view = viewOf("Dialog");
    expect(
      await barrelImportsPlugin().transform.call(contextFor(), 'import "@uniflowed/ui";\n', view),
    ).toBe(null);

    const flow = uniflowed({ root: "/project", config: {} }).find(
      (plugin) => plugin.name === "uf:flow",
    );
    expect(
      await flow.transform.call({ environment: { name: "client" } }, "export {};\n", view),
    ).toBe(null);
  });
});

describe("the plugin set", () => {
  it("rewrites in every application, whether or not it renders Server Components", () => {
    for (const app of [{}, { rsc: false }]) {
      const names = uniflowed({ root: "/project", config: { app } }).map((plugin) => plugin.name);
      expect(names).toContain("uf:barrel-imports");
    }
  });
});
