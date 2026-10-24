// @flow
//
// Next.js loads `next.config.js` and the Turbopack loader in plain Node.
// This suite runs under uf's hooks, so the tests that matter load the helper
// with `require` and, for the one that has to prove the hooks are not
// involved, spawn Node without them.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import { ufBinaryIdentity } from "./transform.js";

const load = createRequire(import.meta.url);
const DIR = path.dirname(fileURLToPath(import.meta.url));
const {
  FLOW_PRAGMA,
  NOFLOW_PRAGMA,
  UNIFLOWED_PACKAGE,
  flowRules,
  turbopackLoaderPath,
  withUniflowed,
} = load(path.join(DIR, "turbopack.cjs"));
const { compile, insertAfterDirectivePrologue, shiftSourceMap, shouldTransform, styleModuleFor } =
  load(path.join(DIR, "turbopack-loader.cjs"));

const REAL_UF =
  typeof process.env.UF_BINARY === "string" && ufBinaryIdentity(process.env.UF_BINARY) != null
    ? process.env.UF_BINARY
    : null;

const FLOW_COMPONENT =
  "// @flow\n" +
  'import { useState } from "react";\n' +
  "export component Counter(label: string) {\n" +
  "  const [count] = useState(1);\n" +
  "  return <span>{label}{count}</span>;\n" +
  "}\n";

const STYLEX_MODULE =
  "// @flow\n" +
  'import { stylex } from "@uniflowed/stylex";\n' +
  "export const styles = stylex.create({\n" +
  '  root: { color: "red", padding: 12 },\n' +
  "});\n";

const STYLEX_CLIENT =
  "// @flow\n" +
  '"use client";\n' +
  'import { stylex } from "@uniflowed/stylex";\n' +
  "export const styles = stylex.create({\n" +
  '  root: { color: "red" },\n' +
  "});\n";

describe("@uniflowed/host/turbopack", () => {
  it("recognises a pragma and not a mention, and @noflow does not match @flow", () => {
    expect(FLOW_PRAGMA.test("// @flow\nexport const x = 1;\n")).toBe(true);
    expect(FLOW_PRAGMA.test("/* @flow */\nexport const x = 1;\n")).toBe(true);
    expect(FLOW_PRAGMA.test("// @noflow\nexport const x = 1;\n")).toBe(false);
    expect(FLOW_PRAGMA.test('const note = "see @flow";\n')).toBe(false);
    expect(FLOW_PRAGMA.test("export const x = 1;\n// not a pragma @flow\n")).toBe(false);
    expect(NOFLOW_PRAGMA.test("// @noflow\n")).toBe(true);
    expect(NOFLOW_PRAGMA.test("// @flow\n")).toBe(false);
    expect(UNIFLOWED_PACKAGE.test("node_modules/@uniflowed/stylex/index.js")).toBe(true);
    expect(UNIFLOWED_PACKAGE.test("app/page.js")).toBe(false);
    expect(UNIFLOWED_PACKAGE.test("node_modules/react/index.js")).toBe(false);
  });

  it("keeps a non-Flow module, including @noflow, out of the loader", () => {
    expect(shouldTransform("export const x = 1;\n", "/app/page.js")).toBe(false);
    expect(
      shouldTransform("// @noflow\nexport component A() { return <b />; }\n", "/app/a.js"),
    ).toBe(false);
    expect(shouldTransform("// @flow\nexport const x = 1;\n", "/app/page.js")).toBe(true);
    expect(shouldTransform("// @flow\nexport const x = 1;\n", "/app/page.css")).toBe(false);
  });

  it("builds one pair of rules per extension glob, with foreign packages separate", () => {
    const rules = flowRules({ identity: "bin", root: "/app" });
    for (const extension of ["*.js", "*.jsx", "*.mjs", "*.cjs"]) {
      expect(rules[extension].length).toBe(2);
      expect(rules[extension][0].as).toBe("*.js");
      expect(rules[extension][0].loaders[0].loader).toBe(turbopackLoaderPath);
      expect(rules[extension][0].loaders[0].options).toEqual({
        identity: "bin",
        root    : "/app",
        binary  : "",
      });
      expect(rules[extension][0].condition.all).toEqual([
        { not: "foreign" },
        { not: { path: UNIFLOWED_PACKAGE } },
        { content: FLOW_PRAGMA },
      ]);
      expect(rules[extension][1].condition.all[0]).toEqual({ path: UNIFLOWED_PACKAGE });
    }
    expect(rules["*.js"]).not.toBe(rules["*.jsx"]);
  });

  it("appends to a project's own rules and does not invent experimental.turbo", () => {
    return withProject((root) => {
      const config = withUniflowed(
        {
          turbopack: {
            resolveAlias: { left: "right" },
            rules: {
              "*.js" : { loaders: ["other"], as: "*.js" },
              "*.svg": { loaders: ["svgr"], as: "*.js" },
            },
          },
          experimental: { typedRoutes: true },
          webpack     : undefined,
        },
        root,
      );
      expect(config.turbopack.resolveAlias).toEqual({ left: "right" });
      expect(config.turbopack.rules["*.svg"]).toEqual({ loaders: ["svgr"], as: "*.js" });
      expect(config.turbopack.rules["*.js"][0]).toEqual({ loaders: ["other"], as: "*.js" });
      expect(config.turbopack.rules["*.js"].length).toBe(3);
      expect(config.turbopack.rules["*.uf.css"]).toEqual({ type: "css" });
      expect(config.experimental.turbo).toBe(undefined);
      expect(config.experimental.typedRoutes).toBe(true);
      expect(config.webpack).toBe(undefined);
    });
  });

  it("merges experimental.turbo only when the config already has it", () => {
    return withProject((root) => {
      const config = withUniflowed(
        { experimental: { turbo: { rules: { "*.svg": { loaders: ["svgr"] } } } } },
        root,
      );
      expect(config.experimental.turbo.rules["*.svg"]).toEqual({ loaders: ["svgr"] });
      expect(config.experimental.turbo.rules["*.js"].length).toBe(2);
      expect(config.turbopack.rules["*.js"].length).toBe(2);
    });
  });

  it("lists installed @uniflowed packages after the ones the project named", () => {
    return withProject((root) => {
      fs.mkdirSync(path.join(root, "node_modules/@uniflowed/stylex"), { recursive: true });
      fs.mkdirSync(path.join(root, "node_modules/@uniflowed/core"), { recursive: true });
      const config = withUniflowed({ transpilePackages: ["left-pad"] }, root);
      expect(config.transpilePackages[0]).toBe("left-pad");
      const names = config.transpilePackages.slice(1);
      expect(names).toContain("@uniflowed/core");
      expect(names).toContain("@uniflowed/stylex");
      expect(names.indexOf("@uniflowed/core") < names.indexOf("@uniflowed/stylex")).toBe(true);
    });
  });

  it("composes twice as once, and wraps a function config including a promise", async () => {
    await withProject(async (root) => {
      const once = withUniflowed({ turbopack: { root: "/repo" } }, root);
      expect(withUniflowed(once, root)).toBe(once);

      const wrapped = withUniflowed((phase) => ({ phase }), root);
      expect(typeof wrapped).toBe("function");
      const sync = wrapped("phase-production-build");
      expect(sync.phase).toBe("phase-production-build");
      expect(sync.turbopack.rules["*.jsx"].length).toBe(2);

      const asyncWrapped = withUniflowed(async () => ({ ready: true }), root);
      const resolved = await asyncWrapped();
      expect(resolved.ready).toBe(true);
      expect(resolved.turbopack.rules["*.js"].length).toBe(2);
    });
  });

  it("installs webpack only for --webpack, and calls the function that was already there", async () => {
    await withProject((root) => {
      const argv = process.argv;
      process.argv = [...argv, "--webpack"];
      try {
        const config = withUniflowed({}, root);
        const compiled = config.webpack({ module: { rules: [] } }, {});
        expect(compiled.module.rules[0].enforce).toBe("pre");
        expect(compiled.module.rules[0].use[0].loader).toBe(turbopackLoaderPath);
        expect(config.webpack(compiled, {}).module.rules.length).toBe(1);
      } finally {
        process.argv = argv;
      }
    });
    await withProject((root) => {
      let seen = false;
      const config = withUniflowed(
        {
          webpack(webpackConfig) {
            seen = true;
            webpackConfig.marked = true;
            return webpackConfig;
          },
        },
        root,
      );
      const compiled = config.webpack({ module: { rules: [{ test: /\.svg$/ }] } }, {});
      expect(seen).toBe(true);
      expect(compiled.marked).toBe(true);
      expect(compiled.module.rules[0].use[0].loader).toBe(turbopackLoaderPath);
      expect(compiled.module.rules[1].test).toEqual(/\.svg$/);
      const banned = {
        oneOf: [
          {
            test: /\.css$/,
            use : { loader: "error-loader", options: { reason: "Global CSS cannot be imported" } },
          },
        ],
      };
      const opened = config.webpack({ module: { rules: [banned] } }, {});
      expect(opened.module.rules[1].oneOf[0].exclude).toEqual(/\.uf\.css$/);
      expect(config.webpack(opened, {}).module.rules[1].oneOf[0].exclude).toEqual(/\.uf\.css$/);
    });
  });

  it("changes the loader identity when the uf binary file changes", () => {
    return withProject((root) => {
      const binary = fakeUf(root);
      const identity = (config: LoaderIdentityConfig): string =>
        config.turbopack.rules["*.js"][0].loaders[0].options.identity;
      return withEnv({ UF_BINARY: binary }, () => {
        const before = identity(withUniflowed({}, root));
        expect(before).toContain(binary);
        fs.writeFileSync(binary, "#!/bin/sh\n# rebuilt\n");
        const after = identity(withUniflowed({}, root));
        expect(after).not.toBe(before);
      });
    });
  });

  it("finds uf where the installer put it, and names every place it looked otherwise", async () => {
    await withProject((root) => {
      const installed = path.join(root, "installed-bin");
      fs.mkdirSync(installed);
      const binary = path.join(installed, "uf");
      fs.writeFileSync(binary, "#!/bin/sh\n");
      fs.chmodSync(binary, 0o755);
      fs.mkdirSync(path.join(root, "empty-path"));
      return withEnv(
        { UF_BINARY: undefined, PATH: path.join(root, "empty-path"), UF_BIN_DIR: installed },
        () => {
          const config = withUniflowed({}, root);
          expect(process.env.UF_BINARY).toBe(binary);
          expect(config.turbopack.rules["*.js"][0].loaders[0].options.identity).toContain(binary);
        },
      );
    });
    await withProject((root) => {
      fs.mkdirSync(path.join(root, "empty-path"));
      fs.mkdirSync(path.join(root, "empty-bin"));
      const missing = path.join(root, "empty-bin", "uf");
      return withEnv(
        {
          UF_BINARY : undefined,
          PATH      : path.join(root, "empty-path"),
          UF_BIN_DIR: path.join(root, "empty-bin"),
        },
        () => {
          expect(() => withUniflowed({}, root)).toThrow(
            "UF_BINARY is not set, `uf` is not on PATH " +
              `(${path.join(root, "empty-path")}), and there is no executable at ${missing}`,
          );
        },
      );
    });
  });

  it("loads in plain Node, from CommonJS and from an ES module", () => {
    return withProject((root) => {
      const helper = path.join(DIR, "turbopack.cjs");
      const report =
        "process.stdout.write(JSON.stringify({" +
        " loader: config.turbopack.rules['*.js'][0].loaders[0].loader," +
        " count: config.turbopack.rules['*.js'].length," +
        " webpack: Object.hasOwn(config, 'webpack')," +
        " }));";
      const env: Env = {
        ...process.env,
        UF_BINARY      : fakeUf(root),
        UF_PROJECT_ROOT: root,
        NODE_OPTIONS   : undefined,
      };
      const commonjs = runPlainNode(
        [
          "-e",
          `const { withUniflowed } = require(${JSON.stringify(helper)});\n` +
            `const config = withUniflowed({}, ${JSON.stringify(root)});\n` +
            report,
        ],
        env,
      );
      const esm = runPlainNode(
        [
          "--input-type=module",
          "-e",
          `import { withUniflowed } from ${JSON.stringify(pathToFileURL(helper).href)};\n` +
            `const config = withUniflowed({}, ${JSON.stringify(root)});\n` +
            report,
        ],
        env,
      );
      for (const run of [commonjs, esm]) {
        expect(run.stderr).toBe("");
        expect(run.status).toBe(0);
        expect(JSON.parse(run.stdout)).toEqual({
          loader : turbopackLoaderPath,
          count  : 2,
          webpack: false,
        });
      }
    });
  });

  it("inserts a stylesheet import after the directive prologue and shifts the map", () => {
    const inserted = insertAfterDirectivePrologue(
      '"use client";\n// @flow\nexport const value = 1;\n',
      'import "./page.js.uf.css";',
    );
    expect(inserted.code.indexOf('"use client";')).toBe(0);
    expect(inserted.code.indexOf('import "./page.js.uf.css";')).toBeLessThan(
      inserted.code.indexOf("export const value"),
    );
    expect(inserted.code.indexOf('"use client";')).toBeLessThan(
      inserted.code.indexOf('import "./page.js.uf.css";'),
    );
    const shifted = shiftSourceMap(
      { mappings: "AAAA;AAAA;AAAA", names: [] },
      inserted.line,
      inserted.addedLines,
    );
    expect(shifted.mappings).toBe("AAAA;AAAA;;AAAA");
    expect(shifted.names).toEqual([]);
  });

  it("names a plain stylesheet beside project source and a hashed one for a dependency", () => {
    const project = styleModuleFor("/app/page.js", "/app");
    expect(project).toEqual({
      file     : "/app/page.js.uf.css",
      specifier: "./page.js.uf.css",
    });
    const packaged = styleModuleFor("/app/node_modules/@uniflowed/ui/button.js", "/app");
    expect(packaged.file.startsWith("/app/.uf/turbopack/")).toBe(true);
    expect(packaged.file.endsWith(".css")).toBe(true);
    expect(packaged.file.includes(".module.")).toBe(false);
    expect(packaged.specifier.startsWith(".")).toBe(true);
    expect(fs.existsSync(packaged.file)).toBe(false);
  });

  it("creates an empty stylesheet for each Flow file before the build resolves it", () => {
    return withProject((root) => {
      const page = path.join(root, "app", "page.js");
      const plain = path.join(root, "app", "plain.js");
      const mark = path.join(root, "components", "mark.js");
      const packaged = path.join(root, "node_modules", "@uniflowed", "ui", "button.js");
      fs.mkdirSync(path.dirname(page), { recursive: true });
      fs.mkdirSync(path.dirname(mark), { recursive: true });
      fs.mkdirSync(path.dirname(packaged), { recursive: true });
      fs.writeFileSync(page, "// @flow\nexport const page = 1;\n");
      fs.writeFileSync(plain, "export const plain = 1;\n");
      fs.writeFileSync(mark, "// @flow\nexport const mark = 1;\n");
      fs.writeFileSync(packaged, "// @flow\nexport const button = 1;\n");
      const kept = `${page}.uf.css`;
      fs.writeFileSync(kept, "/* kept */\n");

      withUniflowed({}, root);

      expect(fs.readFileSync(kept, "utf8")).toBe("/* kept */\n");
      expect(fs.existsSync(`${mark}.uf.css`)).toBe(true);
      expect(fs.existsSync(`${plain}.uf.css`)).toBe(false);
      expect(fs.existsSync(`${packaged}.uf.css`)).toBe(false);
      const generated = fs.readdirSync(path.join(root, ".uf", "turbopack"));
      expect(generated.length).toBe(1);
      expect(generated[0].endsWith(".css")).toBe(true);
      expect(generated[0].includes(".module.")).toBe(false);
    });
  });
});

describe("uf transform through the loader", () => {
  it("lowers a Flow component and leaves a plain module untouched", async () => {
    const binary = requireRealUf();
    await withProject(async (root) => {
      await withEnv({ UF_BINARY: binary }, async () => {
        const flow = path.join(root, "Counter.js");
        fs.writeFileSync(flow, FLOW_COMPONENT);
        const lowered = await compile(FLOW_COMPONENT, flow, { root });
        expect(lowered.code).toContain("function Counter(");
        expect(lowered.code).not.toContain("component Counter(");

        const plain = path.join(root, "plain.js");
        const source = "export const value = 1;\n";
        fs.writeFileSync(plain, source);
        expect(await compile(source, plain, { root })).toBe(null);

        const skipped = path.join(root, "skipped.js");
        const noflow = "// @noflow\nexport component A() { return <b />; }\n";
        fs.writeFileSync(skipped, noflow);
        expect(await compile(noflow, skipped, { root })).toBe(null);
      });
    });
  });

  it("fails a syntax error with file, line and column", async () => {
    const binary = requireRealUf();
    await withProject(async (root) => {
      await withEnv({ UF_BINARY: binary }, async () => {
        const file = path.join(root, "broken.js");
        const source = "// @flow\nexport const value: = ;\n";
        fs.writeFileSync(file, source);
        await expect(compile(source, file, { root })).rejects.toThrow(
          new RegExp(`${escapeRegExp(file)}:\\d+:\\d+:`),
        );
      });
    });
  });

  it("writes StyleX as a global CSS module and removes it when the module no longer has any", async () => {
    const binary = requireRealUf();
    await withProject(async (root) => {
      await withEnv({ UF_BINARY: binary }, async () => {
        const file = path.join(root, "brand.js");
        fs.writeFileSync(file, STYLEX_MODULE);
        const styled = await compile(STYLEX_MODULE, file, { root });
        expect(styled.cssPath).toBe(`${file}.uf.css`);
        const css = fs.readFileSync(styled.cssPath, "utf8");
        const classes = styled.code.match(/x[0-9a-z]{13}/g) ?? [];
        expect(classes.length > 0).toBe(true);
        for (const name of classes) {
          expect(css).toContain(`.${name}`);
        }
        expect(css).not.toContain(":global");
        const written = fs.statSync(styled.cssPath).mtimeMs;
        await compile(STYLEX_MODULE, file, { root });
        expect(fs.statSync(styled.cssPath).mtimeMs).toBe(written);

        const client = path.join(root, "client.js");
        fs.writeFileSync(client, STYLEX_CLIENT);
        const marked = await compile(STYLEX_CLIENT, client, { root });
        expect(marked.code.indexOf('"use client";')).toBeLessThan(marked.code.indexOf(".uf.css"));

        fs.writeFileSync(file, "// @flow\nexport const value = 1;\n");
        const cleared = await compile("// @flow\nexport const value = 1;\n", file, { root });
        expect(cleared.cssPath).toBe(null);
        expect(cleared.code).not.toContain(".uf.css");
        expect(fs.existsSync(`${file}.uf.css`)).toBe(false);
      });
    });
  });
});

describe("next build", () => {
  it("compiles Flow with Turbopack when UF_TURBOPACK_E2E=1", async () => {
    if (process.env.UF_TURBOPACK_E2E !== "1") return;
    const binary = requireRealUf();
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-turbopack-next-")));
    const trace = path.join(root, "trace.txt");
    try {
      fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
      fs.writeFileSync(
        path.join(root, "package.json"),
        JSON.stringify(
          {
            private: true,
            dependencies: {
              next               : "16.4.3",
              react              : "19.2.0",
              "react-dom"        : "19.2.0",
              "@uniflowed/core"  : "0.41.0",
              "@uniflowed/stylex": "0.41.0",
            },
          },
          null,
          2,
        ) + "\n",
      );
      fs.writeFileSync(
        path.join(root, "next.config.js"),
        'const { withUniflowed } = require("@uniflowed/host/turbopack");\n' +
          "module.exports = withUniflowed({});\n",
      );
      fs.mkdirSync(path.join(root, "app"));
      fs.mkdirSync(path.join(root, "components"));
      fs.writeFileSync(
        path.join(root, "app/layout.js"),
        "export default function RootLayout({ children }) {\n" +
          "  return <html><body>{children}</body></html>;\n" +
          "}\n",
      );
      fs.writeFileSync(path.join(root, "app/plain.js"), 'export const label = "not @flow";\n');
      fs.writeFileSync(
        path.join(root, "components/mark.js"),
        "// @flow\n" +
          'import { stylex } from "@uniflowed/stylex";\n' +
          'const styles = stylex.create({ mark: { color: "blue", fontWeight: 700 } });\n' +
          "export default function Mark() {\n" +
          "  return <em {...stylex.props(styles.mark)}>mark</em>;\n" +
          "}\n",
      );
      fs.writeFileSync(
        path.join(root, "app/page.js"),
        "// @flow\n" +
          'import { stylex } from "@uniflowed/stylex";\n' +
          'import Mark from "../components/mark.js";\n' +
          'import { label } from "./plain.js";\n' +
          'const styles = stylex.create({ title: { color: "red", fontSize: 32 } });\n' +
          "export default function Page() {\n" +
          "  return <main {...stylex.props(styles.title)}>{label}<Mark /></main>;\n" +
          "}\n",
      );
      const install = runPlainNode(
        ["install", "--ignore-scripts", "--no-audit", "--no-fund"],
        nextEnv(root, binary, trace),
        root,
        "npm",
      );
      expect(commandOutput(install)).toBe("");
      fs.rmSync(path.join(root, "node_modules/@uniflowed/host"), { recursive: true, force: true });
      copyHost(DIR, path.join(root, "node_modules/@uniflowed/host"));
      const built = runNext(root, binary, trace, ["build"]);
      expect(commandOutput(built)).toBe("");
      expect(`${built.stdout}\n${built.stderr}`).toContain("Turbopack");
      assertFlowSurvived(root, trace);

      fs.rmSync(path.join(root, ".next"), { recursive: true, force: true });
      fs.writeFileSync(trace, "");
      const webpack = runNext(root, binary, trace, ["build", "--webpack"]);
      expect(commandOutput(webpack)).toBe("");
      assertFlowSurvived(root, trace);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

type Env        = { [string]: string | void, ... };
type CommandRun = { status: number | null, stdout: string, stderr: string, ... };
type LoaderIdentityConfig = {
  turbopack: {
    rules: {
      [string]: $ReadOnlyArray<{
        loaders: $ReadOnlyArray<{ options: { identity: string, ... }, ... }>,
        ...
      }>,
      ...
    },
    ...
  },
  ...
};

function assertFlowSurvived(root: string, trace: string): void {
  const seen = fs.readFileSync(trace, "utf8");
  expect(seen).toContain(path.join(root, "app/page.js"));
  expect(seen).toContain(path.join(root, "components/mark.js"));
  expect(seen).not.toContain(path.join(root, "app/plain.js"));
  expect(seen).not.toContain(path.join(root, "app/layout.js"));
  const built = filesUnder(path.join(root, ".next"));
  const javascript = built
    .filter((file) => file.endsWith(".js"))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  const stylesheets = built
    .filter((file) => file.endsWith(".css"))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  const html = built
    .filter((file) => file.endsWith(".html"))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  expect(html).not.toContain("[object Object]");
  for (const sheetPath of ["app/page.js.uf.css", "components/mark.js.uf.css"]) {
    const sheet = fs.readFileSync(path.join(root, sheetPath), "utf8");
    const names = sheet.match(/x[0-9a-z]{13}/g) ?? [];
    expect(names.length > 0).toBe(true);
    for (const name of names) {
      expect(sheet).toContain(`.${name}`);
      expect(javascript).toContain(name);
      expect(stylesheets).toContain(`.${name}`);
      expect(html).toContain(name);
    }
  }
}

function filesUnder(directory: string): Array<string> {
  const found: Array<string> = [];
  if (!fs.existsSync(directory)) return found;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const child = path.join(directory, entry.name);
    if (entry.isDirectory()) found.push(...filesUnder(child));
    else if (/\.(?:js|css|html)$/.test(entry.name)) found.push(child);
  }
  return found;
}

function commandOutput(run: CommandRun): string {
  if (run.status === 0) return "";
  return `exit ${String(run.status)}\n${run.stdout}\n${run.stderr}`;
}

function nextEnv(root: string, binary: string, trace: string): Env {
  const env: Env = {
    ...process.env,
    UF_BINARY              : binary,
    UF_TURBOPACK_TRACE     : trace,
    NEXT_TELEMETRY_DISABLED: "1",
  };
  delete env.NODE_OPTIONS;
  env.npm_config_cache = path.join(root, "npm-cache");
  return env;
}

function runNext(
  root  : string,
  binary: string,
  trace : string,
  args  : $ReadOnlyArray<string>,
): CommandRun {
  const next = path.join(root, "node_modules/next/dist/bin/next");
  return runPlainNode(args, nextEnv(root, binary, trace), root, plainNode(), [next, ...args]);
}

function requireRealUf() {
  if (REAL_UF == null) {
    throw new Error(
      "UF_BINARY is not an executable uf, so the transform tests have nothing to run.",
    );
  }
  return REAL_UF;
}

function fakeUf(root: string): string {
  const binary = path.join(root, "bin", "uf");
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, "#!/bin/sh\n");
  fs.chmodSync(binary, 0o755);
  return binary;
}

async function withProject(body: (root: string) => mixed): Promise<mixed> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-turbopack-")));
  try {
    fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
    return await withEnv({ UF_BINARY: fakeUf(root), UF_PROJECT_ROOT: root }, () => body(root));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

async function withEnv(values: Env, body: () => mixed): Promise<mixed> {
  const previous: Map<string, string | void> = new Map();
  for (const name of Object.keys(values)) {
    previous.set(name, process.env[name]);
    const value = values[name];
    if (value == null) delete process.env[name];
    else process.env[name] = value;
  }
  try {
    return await body();
  } finally {
    for (const [name, value] of previous) {
      if (value == null) delete process.env[name];
      else process.env[name] = value;
    }
  }
}

/**
 * Node itself, not whichever host is running this file.
 *
 * The Deno library lane executes the suite, so `process.execPath` is `deno`.
 * `deno -e` cannot `require` the ES module `@uniflowed/host/transform` the way
 * Node 20.19 and later can, and that load is what the plain-Node case proves.
 */
function plainNode(): string {
  const found = spawnSync("node", ["-p", "process.execPath"], { encoding: "utf8" });
  const binary = String(found.stdout ?? "").trim();
  if (found.status === 0 && binary !== "") return binary;
  throw new Error("`node` is not on PATH, so the plain-Node load cannot be proven.");
}

/** `fs.cpSync` has no Flow declaration in this repository's libdef. */
function copyHost(from: string, to: string): void {
  fs.mkdirSync(to, { recursive: true });
  for (const name of fs.readdirSync(from)) {
    if (name.endsWith(".test.js")) continue;
    const source = path.join(from, name);
    const target = path.join(to, name);
    if (fs.statSync(source).isDirectory()) copyHost(source, target);
    else fs.copyFileSync(source, target);
  }
}

function runPlainNode(
  args       : $ReadOnlyArray<string>,
  env        : Env,
  cwd        : ?string,
  command    : string = plainNode(),
  commandArgs: $ReadOnlyArray<string> = args,
): CommandRun {
  const childEnv = { ...env };
  delete childEnv.NODE_OPTIONS;
  const run = spawnSync(command, commandArgs, {
    cwd,
    encoding: "utf8",
    env     : childEnv,
  });
  return {
    status: run.status,
    stdout: String(run.stdout ?? ""),
    stderr: String(run.stderr ?? ""),
  };
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
