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
import uniflowed from "./index.js";
import { barrelExports, barrelImportsPlugin } from "./internal/barrel-imports.js";

/** This repository's barrel, the file a project's `@uniflowed/ui` resolves to. */
const BARREL: string = fileURLToPath(new URL("../ui/index.js", import.meta.url));

/** A file of the package, spelled the way a rewritten import names it. */
function uiFile(name: string): string {
  return path.join(path.dirname(BARREL), name).split(path.sep).join("/");
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
  // A module the test wrote to a scratch directory, by its run-time path.
  // $FlowFixMe[unsupported-syntax]
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
      // A namespace is the module's own namespace object: the same object the
      // barrel re-exports, not an equal one.
      expect(target.kind).toBe("module");
      expect(await load(target.file)).toBe(barrel[name]);
    }
  });

  it("names the module each component and each namespace comes from", () => {
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
    expect(exports.get("Dialog")).toEqual({ kind: "module", file: uiFile("dialog.js") });
    // `ContextMenu` is its own root and trigger over `Menu`'s parts, and still
    // one module: `context-menu.js` re-exports the parts it shares.
    expect(exports.get("ContextMenu")).toEqual({ kind: "module", file: uiFile("context-menu.js") });
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

  it("imports a namespace as the module it is", async () => {
    const code = await rewritten(
      'import { Dialog, ContextMenu as RowMenu, Switch } from "@uniflowed/ui";\n',
    );

    expect(code).toContain(`import * as Dialog from ${JSON.stringify(uiFile("dialog.js"))};`);
    expect(code).toContain(
      `import * as RowMenu from ${JSON.stringify(uiFile("context-menu.js"))};`,
    );
    expect(code).toContain(`import { Switch } from ${JSON.stringify(uiFile("switch.js"))};`);
    expect(code).not.toContain('"@uniflowed/ui"');
  });

  it("re-exports from the defining files too", async () => {
    const code = await rewritten('export { Switch, Dialog as Modal } from "@uniflowed/ui";\n');

    expect(code).toContain(`export { Switch } from ${JSON.stringify(uiFile("switch.js"))};`);
    expect(code).toContain(`export * as Modal from ${JSON.stringify(uiFile("dialog.js"))};`);
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

describe("a barrel that builds a namespace as an object", () => {
  it("leaves that name on the barrel rather than guessing at the object", async () => {
    // The form `@uniflowed/ui` used before #1453. Reading it is not attempted:
    // an object literal is a value, and a rewrite that rebuilt it would be a
    // second definition of it. Left on the barrel it is only larger.
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-barrel-object-"));
    directories.push(directory);
    const barrel = path.join(directory, "index.js");
    fs.writeFileSync(path.join(directory, "dialog.js"), "export const DialogRoot = 1;\n");
    fs.writeFileSync(
      barrel,
      'import { DialogRoot } from "./dialog.js";\nexport const Dialog = { Root: DialogRoot };\n',
    );

    expect(barrelExports(fs.readFileSync(barrel, "utf8"), barrel).get("Dialog")).toEqual({
      kind: "opaque",
    });
    expect(await rewritten('import { Dialog } from "@uniflowed/ui";\n', contextFor(barrel))).toBe(
      null,
    );
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
