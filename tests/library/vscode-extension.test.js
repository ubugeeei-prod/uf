// @flow
//
// The VS Code extension's own decisions, without VS Code.
//
// An extension host cannot be started here, so the parts of the extension that
// are *not* the host are separated out and tested directly: which binary gets
// run, what it gets run with, where it gets run, and what the user is told
// when there is nothing to run. Those are the parts that break on a machine
// that is not the author's — a Windows shim, a project that pins uf in
// `node_modules`, a `uf.server.path` typo — and none of them needs an editor.
//
// The modules are loaded with `createRequire`, not `import`, and that is the
// point rather than a workaround: the extension is CommonJS with Flow's
// comment type syntax, so the bytes VS Code loads are the bytes Node loads
// here, with no transform in between that could hide a mistake.
//
// What is *not* covered here, and cannot be: that VS Code registers the
// providers, that `editor.codeActionsOnSave` reaches `source.fixAll.uf`, and
// that the language client and `uf lsp` agree on the wire. The third is
// covered by `lsp.test.js`, which drives the real server. The first two need a
// person with VS Code open; `editors/vscode/README.md` says so.

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

const loadCjs = createRequire(import.meta.url);

const binary = loadCjs("../../editors/vscode/src/binary.js");
const client = loadCjs("../../editors/vscode/src/client.js");
const project = loadCjs("../../editors/vscode/src/project.js");
const release = loadCjs("../../editors/vscode/release/version.js");
const version = loadCjs("../../editors/vscode/src/version.js");
const status = loadCjs("../../editors/vscode/src/status.js");
const workspace = loadCjs("../../editors/vscode/src/workspace.js");

const EXTENSION = path.join(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
  "editors",
  "vscode",
);
const manifest = JSON.parse(fs.readFileSync(path.join(EXTENSION, "package.json"), "utf8"));

const FOLDER: string = path.join("/home", "dev", "app");

/**
 * A fake machine: the files that exist, the environment, and the platform.
 */
const machine = (
  present: Array<string>,
  env: { [string]: string } = {},
  platform: string = "darwin",
) => ({
  exists: (candidate: string) => present.includes(candidate),
  env,
  platform,
});

const bin = (name: string) => path.join(FOLDER, "node_modules", ".bin", name);

describe("finding the uf binary", () => {
  it("prefers the configured path over everything else", () => {
    const resolution = binary.resolveServer(
      "/opt/uf/bin/uf",
      FOLDER,
      machine(["/opt/uf/bin/uf", bin("uf"), "/usr/local/bin/uf"], {
        PATH: "/usr/local/bin",
      }),
    );

    expect(resolution).toEqual({
      kind: "found",
      command: "/opt/uf/bin/uf",
      source: "setting",
    });
  });

  it("refuses to fall through when the configured path is not there", () => {
    // The alternative would be silently running a different uf than the one
    // the user named, which is the exact situation someone debugging a build
    // of uf would waste an afternoon on.
    const resolution = binary.resolveServer(
      "/opt/uf/bin/uf",
      FOLDER,
      machine([bin("uf"), "/usr/local/bin/uf"], { PATH: "/usr/local/bin" }),
    );

    expect(resolution.kind).toBe("missing");
    expect(resolution.tried).toEqual(["/opt/uf/bin/uf"]);
    expect(resolution.setting).toBe("/opt/uf/bin/uf");
    expect(binary.describeMissing(resolution)).toContain("/opt/uf/bin/uf");
  });

  it("resolves a relative setting against the workspace folder", () => {
    const resolution = binary.resolveServer(
      "./target/debug/uf",
      FOLDER,
      machine([path.join(FOLDER, "target", "debug", "uf")]),
    );

    expect(resolution).toEqual({
      kind: "found",
      command: path.join(FOLDER, "target", "debug", "uf"),
      source: "setting",
    });
  });

  it("expands ${workspaceFolder} and a leading tilde", () => {
    expect(binary.expand("${workspaceFolder}/bin/uf", FOLDER, "/home/dev")).toBe(
      `${FOLDER}/bin/uf`,
    );
    expect(binary.expand("~/bin/uf", FOLDER, "/home/dev")).toBe("/home/dev/bin/uf");
    // No home to expand against: left alone rather than turned into "/bin/uf".
    expect(binary.expand("~/bin/uf", FOLDER, undefined)).toBe("~/bin/uf");
    expect(binary.expand("/absolute/uf", FOLDER, "/home/dev")).toBe("/absolute/uf");
  });

  it("takes the project's own copy before one on PATH", () => {
    // The version a project pinned is the version its files were formatted and
    // linted with. A different global one would disagree with CI.
    const resolution = binary.resolveServer(
      "",
      FOLDER,
      machine([bin("uf"), "/usr/local/bin/uf"], { PATH: "/usr/local/bin" }),
    );

    expect(resolution).toEqual({ kind: "found", command: bin("uf"), source: "workspace" });
  });

  it("falls back to PATH, in PATH order", () => {
    const resolution = binary.resolveServer(
      "",
      FOLDER,
      machine(["/opt/bin/uf", "/usr/local/bin/uf"], {
        PATH: "/usr/local/bin:/opt/bin",
      }),
    );

    expect(resolution).toEqual({
      kind: "found",
      command: "/usr/local/bin/uf",
      source: "path",
    });
  });

  it("says where it looked when nothing has it", () => {
    const resolution = binary.resolveServer("", FOLDER, machine([], { PATH: "/usr/local/bin" }));

    expect(resolution.kind).toBe("missing");
    expect(resolution.setting).toBe(null);
    expect(resolution.tried).toEqual([bin("uf"), "/usr/local/bin/uf"]);
    // The notification names the way out, and the log names every place.
    expect(binary.describeMissing(resolution)).toContain("uf.server.path");
    expect(binary.describeSearch(resolution)).toContain(`  ${bin("uf")}`);
    expect(binary.describeSearch(resolution)).toContain("  /usr/local/bin/uf");
  });

  it("reads a Windows machine's PATH and shim names", () => {
    // A `;`-separated `Path`, and the shim names npm and a global install
    // write. `path.join` is the host's, so the expected candidate is built the
    // same way rather than spelled with a separator this test cannot choose.
    const shim = path.join("C:\\tools", "uf.cmd");
    const windows = machine([shim], { Path: "C:\\Windows;C:\\tools" }, "win32");

    expect(binary.binaryNames("win32")).toEqual(["uf.exe", "uf.cmd", "uf.bat", "uf"]);
    expect(binary.pathDirectories(windows)).toEqual(["C:\\Windows", "C:\\tools"]);
    expect(binary.resolveServer("", FOLDER, windows)).toEqual({
      kind: "found",
      command: shim,
      source: "path",
    });
  });

  it("needs a shell only for the shims Node refuses to execute", () => {
    expect(binary.needsShell("C:\\tools\\uf.cmd")).toBe(true);
    expect(binary.needsShell("C:\\tools\\uf.BAT")).toBe(true);
    expect(binary.needsShell("C:\\tools\\uf.exe")).toBe(false);
    expect(binary.needsShell("/usr/local/bin/uf")).toBe(false);
  });

  it("has no PATH to read without one", () => {
    expect(binary.pathDirectories(machine([], {}))).toEqual([]);
    expect(binary.pathDirectories(machine([], { PATH: "" }))).toEqual([]);
  });
});

describe("what counts as a uf project", () => {
  it("is a folder with uf.config.js at its root", () => {
    // uf has one configuration surface, and `uf_config::CONFIG_FILES` is this
    // one name.
    expect(project.CONFIG_FILE).toBe("uf.config.js");

    const exists = (candidate: string) => candidate === path.join(FOLDER, "uf.config.js");
    expect(project.isUfProject(FOLDER, exists)).toBe(true);
    expect(project.isUfProject("/home/dev/other", exists)).toBe(false);
  });

  it("keeps the folders that are uf projects and drops the rest", () => {
    const roots = ["/a", "/b", "/c"];
    const exists = (candidate: string) =>
      candidate === "/a/uf.config.js" || candidate === "/c/uf.config.js";

    expect(project.ufProjectFolders(roots, exists)).toEqual(["/a", "/c"]);
    // A window with no uf project is a window where the extension does
    // nothing, which is the intended outcome.
    expect(project.ufProjectFolders(["/b"], exists)).toEqual([]);
  });

  it("claims exactly the extensions the linter parses", () => {
    // `uf_lint`'s `flow/syntax` claims `.js`, `.jsx`, `.mjs` and `.cjs`, and
    // nothing else. Claiming more would put a squiggle on a file uf never
    // read; claiming fewer would leave one silent.
    expect(project.FLOW_EXTENSIONS).toEqual([".js", ".jsx", ".mjs", ".cjs"]);
    // The glob the document selector uses is built from that list, so the
    // files the server is sent and the files the save handler formats cannot
    // drift apart.
    expect(project.FLOW_GLOB).toBe("**/*.{js,jsx,mjs,cjs}");

    for (const name of ["a.js", "a.jsx", "a.mjs", "a.cjs"]) {
      expect(project.isFlowFile(`/project/${name}`)).toBe(true);
    }
    // `.js.flow` is a declaration sidecar, which `flow/syntax` does not claim
    // either — it ends in `.flow`, not in `.js`.
    for (const name of ["a.ts", "a.md", "a.json", "a.js.flow"]) {
      expect(project.isFlowFile(`/project/${name}`)).toBe(false);
    }
  });
});

describe("how the server gets started", () => {
  const found = { kind: "found", command: "/usr/local/bin/uf", source: "path" };

  it("runs `uf lsp` and nothing else", () => {
    expect(client.SERVER_ARGUMENTS).toEqual(["lsp"]);
  });

  it("starts the server in the project folder rather than passing --cwd", () => {
    // One statement of which project this is, not two: the child process's
    // own working directory is the project folder, and that is what the
    // server reads when no `--cwd` names another.
    expect(client.SERVER_ARGUMENTS).not.toContain("--cwd");

    const executable = client.serverExecutable(found, FOLDER);
    expect(executable.command).toBe("/usr/local/bin/uf");
    expect(executable.args).toEqual(["lsp"]);
    expect(executable.options.cwd).toBe(FOLDER);
    expect(executable.options.shell).toBe(undefined);
  });

  it("asks for a shell only when the binary is a Windows shim", () => {
    const shim = { kind: "found", command: "C:\\tools\\uf.cmd", source: "path" };
    expect(client.serverExecutable(shim, FOLDER).options.shell).toBe(true);
  });

  it("will not build a command without a binary", () => {
    const missing = { kind: "missing", tried: [], setting: null };
    expect(() => client.serverExecutable(missing, FOLDER)).toThrow();
  });

  it("keeps the client id the trace setting is named after", () => {
    // `vscode-languageclient` reads `<id>.trace.server`, so the id and the
    // contributed `uf.trace.server` setting are the same string or the setting
    // does nothing.
    expect(client.CLIENT_ID).toBe("uf");
  });
});

describe("reading the settings", () => {
  const from = (values: { [string]: mixed }) => client.readSettings((key) => values[key]);

  it("treats an unset path as `decide for me`", () => {
    expect(from({}).serverPath).toBe("");
    expect(from({ "server.path": "   " }).serverPath).toBe("");
    expect(from({ "server.path": "  /opt/uf  " }).serverPath).toBe("/opt/uf");
  });

  it("does not format on save unless asked", () => {
    // An extension that reformats a file the moment it is installed is an
    // extension that gets uninstalled.
    expect(from({}).formatOnSave).toBe(false);
    expect(from({ formatOnSave: true }).formatOnSave).toBe(true);
    expect(client.shouldFormatOnSave(from({ formatOnSave: true }))).toBe(true);
  });

  it("falls back rather than trusting a settings file's types", () => {
    expect(from({ "server.path": 7, formatOnSave: "yes" })).toEqual({
      serverPath: "",
      formatOnSave: false,
    });
  });
});

describe("the version the registries are sent", () => {
  const published = (uf: string) => release.marketplaceVersion(uf);

  it("maps a prerelease onto a plain version flagged as a pre-release", () => {
    // `vsce publish` refuses `0.0.0-alpha.46` itself.
    expect(published("0.0.0-alpha.46")).toEqual({ version: "0.0.46", preRelease: true });
    expect(published("1.2.0-beta.3")).toEqual({ version: "1.2.1003", preRelease: true });
    expect(published("1.2.0-rc.1")).toEqual({ version: "1.2.2001", preRelease: true });
  });

  it("publishes a release as a release", () => {
    expect(published("1.2.0")).toEqual({ version: "1.2.9999", preRelease: false });
    expect(published("1.2.1")).toEqual({ version: "1.2.19999", preRelease: false });
    // The release that ended the alpha series: a plain release, above every
    // pre-release published for `0.0.0-alpha.N`.
    expect(published("0.1.0")).toEqual({ version: "0.1.9999", preRelease: false });
  });

  it("sorts in the order uf's versions do", () => {
    // VS Code updates to the highest number it sees, so the order is the
    // whole contract.
    const order = [
      "0.0.0-alpha.9",
      "0.0.0-alpha.46",
      "0.0.0-beta.0",
      "0.0.0-rc.2",
      "0.0.0",
      "0.0.1-alpha.0",
      "0.0.1",
      "0.1.0-alpha.1",
      "0.1.0",
      "1.0.0-rc.1",
      "1.0.0",
    ];
    const numbers = order.map((uf) => published(uf).version.split(".").map(Number));
    for (let index = 1; index < numbers.length; index++) {
      const [a, b] = [numbers[index - 1], numbers[index]];
      const later = b[0] !== a[0] ? b[0] > a[0] : b[1] !== a[1] ? b[1] > a[1] : b[2] > a[2];
      expect(`${order[index]} after ${order[index - 1]}: ${String(later)}`).toBe(
        `${order[index]} after ${order[index - 1]}: true`,
      );
    }
  });

  it("refuses a version it cannot place rather than guessing", () => {
    for (const uf of ["0.0.0-nightly.1", "0.0.0-alpha.1000", "0.0.0-alpha", "v1.0.0", ""]) {
      expect(() => published(uf)).toThrow("has no extension version");
    }
  });
});

describe("the version a server reports", () => {
  it("accepts the oldest supported release and anything newer", () => {
    expect(version.checkVersion(version.MINIMUM_UF).kind).toBe("supported");
    expect(version.checkVersion("0.2.0").kind).toBe("supported");
    expect(version.checkVersion("1.0.0-rc.1").kind).toBe("supported");
  });

  it("calls a release before the minimum old, and says what to do", () => {
    const check = version.checkVersion("0.0.0-alpha.47");
    expect(check.kind).toBe("old");
    expect(check.message).toContain("0.0.0-alpha.47");
    expect(check.message).toContain(version.MINIMUM_UF);
    expect(check.message).toContain("uf self-update");
    // A prerelease of the minimum is older than the minimum.
    expect(version.checkVersion(`${version.MINIMUM_UF}-rc.1`).kind).toBe("old");
  });

  it("does not nag about a version it cannot read", () => {
    // A local build reporting something odd is more likely new than old.
    expect(version.checkVersion(undefined)).toEqual({ kind: "unknown", version: null });
    expect(version.checkVersion("dev").kind).toBe("unknown");
  });

  it("orders prereleases the way semver does", () => {
    const order = [
      "0.1.0-alpha.2",
      "0.1.0-alpha.10",
      "0.1.0-beta.1",
      "0.1.0-rc.1",
      "0.1.0",
      "0.1.1",
      "0.10.0",
    ];
    for (let index = 1; index < order.length; index++) {
      const older = version.parseVersion(order[index - 1]);
      const newer = version.parseVersion(order[index]);
      expect(
        `${order[index - 1]} < ${order[index]}: ${String(version.compareVersions(older, newer) < 0)}`,
      ).toBe(`${order[index - 1]} < ${order[index]}: true`);
    }
  });
});

describe("the status bar", () => {
  it("is hidden in a window with no uf project", () => {
    expect(status.statusView([])).toBe(null);
  });

  it("shows the version of a running server", () => {
    const view = status.statusView([
      { kind: "running", folder: "app", version: "0.2.0", old: false },
    ]);
    expect(view?.text).toBe("$(check) uf 0.2.0");
    expect(view?.severity).toBe("ok");
    expect(view?.tooltip).toContain("app: uf 0.2.0");
  });

  it("shows the worst folder, and lists every one", () => {
    const view = status.statusView([
      { kind: "running", folder: "web", version: "0.2.0", old: false },
      { kind: "missing", folder: "api" },
    ]);
    expect(view?.text).toBe("$(error) uf: not found");
    expect(view?.severity).toBe("error");
    expect(view?.tooltip).toContain("web: uf 0.2.0");
    expect(view?.tooltip).toContain("api: no uf binary found");
  });

  it("warns about an old server and spins while starting", () => {
    expect(
      status.statusView([{ kind: "running", folder: "app", version: "0.0.9", old: true }])
        ?.severity,
    ).toBe("warning");
    expect(status.statusView([{ kind: "starting", folder: "app" }])?.text).toBe("$(sync~spin) uf");
    expect(
      status.statusView([{ kind: "stopped", folder: "app", reason: "spawn EACCES" }])?.tooltip,
    ).toContain("spawn EACCES");
  });
});

describe("the settings a uf project gets", () => {
  const validation = workspace.AUTOMATIC[0];
  // What VS Code's inspect() says about a registered setting nobody set.
  const unset = { defaultValue: true };

  it("only turns off JavaScript validation on its own, never TypeScript's", () => {
    // Both names: 1.110 reads `js/ts.validate.enabled` first, and the old
    // name only while the new one has no value anywhere. Scoped to
    // [javascript], the id VS Code reads it under for .jsx as well.
    expect(workspace.AUTOMATIC.map(workspace.describeSetting)).toEqual([
      '"javascript.validate.enable": false',
      '"[javascript]": { "js/ts.validate.enabled": false }',
    ]);
    for (const setting of workspace.RECOMMENDED) {
      expect(setting.key.startsWith("typescript.")).toBe(false);
      expect(setting.language === "typescript" || setting.language === "typescriptreact").toBe(
        false,
      );
    }
  });

  it("writes a setting the project has not set", () => {
    expect(workspace.planSetting(validation, unset).kind).toBe("write");
    // The user's own setting is theirs for every project; this one still gets its own.
    expect(workspace.planSetting(validation, { ...unset, globalValue: true }).kind).toBe("write");
  });

  it("does not write a key this VS Code does not have", () => {
    // Cursor and VS Code before 1.110 have no `js/ts.validate.enabled`, and
    // VS Code refuses to write a setting nothing registered.
    const unified = workspace.AUTOMATIC[1];
    const step = workspace.planSetting(unified, {});
    expect(step.kind).toBe("unsupported");
    expect(workspace.describePlan([step])[0]).toContain("does not have that setting");
  });

  it("never changes a value the project set, either way", () => {
    expect(workspace.planSetting(validation, { ...unset, workspaceValue: true })).toEqual({
      kind: "kept",
      setting: validation,
      current: true,
    });
    expect(workspace.planSetting(validation, { ...unset, workspaceFolderValue: false }).kind).toBe(
      "already",
    );
  });

  it("reads a language-scoped setting at its language's level", () => {
    const formatter = workspace.RECOMMENDED.find(
      (setting) => setting.language === "javascript" && setting.key === "editor.defaultFormatter",
    );
    expect(formatter?.value).toBe(workspace.EXTENSION_ID);
    if (formatter == null) {
      return;
    }
    // `editor.defaultFormatter` is registered with a default of null.
    const registered = { defaultValue: null };
    // A top-level `editor.defaultFormatter` is not a JavaScript one.
    expect(
      workspace.planSetting(formatter, { ...registered, workspaceValue: "other.formatter" }).kind,
    ).toBe("write");
    expect(
      workspace.planSetting(formatter, { ...registered, workspaceLanguageValue: "other.formatter" })
        .kind,
    ).toBe("kept");
    expect(workspace.describeSetting(formatter)).toBe(
      `"[javascript]": { "editor.defaultFormatter": "${workspace.EXTENSION_ID}" }`,
    );
  });

  it("names itself by the id package.json publishes", () => {
    expect(workspace.EXTENSION_ID).toBe(`${manifest.publisher}.${manifest.name}`);
  });

  it("says what it wrote and what it kept", () => {
    const steps = workspace.plan(workspace.RECOMMENDED, (setting) =>
      setting.language === "javascriptreact"
        ? { defaultValue: null, workspaceLanguageValue: "esbenp.prettier-vscode" }
        : { defaultValue: setting.key === "editor.defaultFormatter" ? null : true },
    );
    const lines = workspace.describePlan(steps);
    expect(lines[0]).toContain('+ "javascript.validate.enable": false');
    expect(lines[1]).toContain('+ "[javascript]": { "js/ts.validate.enabled": false }');
    expect(lines[4]).toContain("esbenp.prettier-vscode");
    expect(lines[4]).toContain("kept");
  });
});

describe("the Flow grammar injected into JavaScript", () => {
  const grammar = JSON.parse(
    fs.readFileSync(path.join(EXTENSION, "syntaxes", "flow.injection.json"), "utf8"),
  );
  // TextMate grammars are Oniguruma; every pattern here is also valid
  // JavaScript, which is what lets them be tested without an editor.
  const pattern = (name: string) =>
    new RegExp(grammar.repository[name].match ?? grammar.repository[name].begin, "g");
  const matches = (name: string, source: string): Array<string> =>
    Array.from(source.matchAll(pattern(name)), (match) => match[1]);

  it("is contributed to the grammars VS Code uses for .js and .jsx", () => {
    const contributed = manifest.contributes.grammars.find(
      (entry) => entry.scopeName === grammar.scopeName,
    );
    expect(contributed?.injectTo).toEqual(["source.js", "source.js.jsx", "source.js.flow"]);
    expect(fs.existsSync(path.join(EXTENSION, contributed?.path ?? ""))).toBe(true);
    for (const include of grammar.patterns) {
      expect(grammar.repository[include.include.slice(1)] != null).toBe(true);
    }
  });

  it("marks component and hook declarations", () => {
    expect(
      matches("component-declaration", "export default component Button(label: string) {"),
    ).toEqual(["component"]);
    expect(matches("component-declaration", "declare component Icon<T>(name: T);")).toEqual([
      "component",
    ]);
    expect(matches("hook-declaration", "export hook useCounter(start: number) {")).toEqual([
      "hook",
    ]);
  });

  it("marks renders after a signature or a colon", () => {
    expect(matches("renders", "component List() renders* Item {")).toEqual(["renders*"]);
    expect(matches("renders", "type Slot = { child: renders? Item };")).toEqual(["renders?"]);
  });

  it("marks a match expression, not a match call", () => {
    expect(matches("match", "const label = match (status) {")).toEqual(["match"]);
    expect(matches("match", "const found = text.match(/a/); match(x);")).toEqual([]);
  });

  it("leaves ordinary JavaScript alone", () => {
    const plain = [
      "const component = load(); component.render();",
      "function hook(fn) { return fn; } hook(useless);",
      "const renders = 3; const o = { renders, count: renders };",
      "const opaque = true;",
    ].join("\n");
    for (const { include } of grammar.patterns) {
      const name = include.slice(1);
      expect(`${name}: ${JSON.stringify(matches(name, plain))}`).toBe(`${name}: []`);
    }
  });
});

describe("the manifest", () => {
  const source = fs.readFileSync(path.join(EXTENSION, "src", "extension.js"), "utf8");

  it("registers every command it contributes", () => {
    for (const command of manifest.contributes.commands) {
      expect(
        `${command.command}: ${String(source.includes(`registerCommand("${command.command}"`))}`,
      ).toBe(`${command.command}: true`);
    }
  });

  it("describes every setting it contributes", () => {
    for (const [key, setting] of Object.entries(manifest.contributes.configuration.properties)) {
      const described =
        typeof setting.description === "string" || typeof setting.markdownDescription === "string";
      expect(`${key}: ${String(described)}`).toBe(`${key}: true`);
    }
  });

  it("contributes every uf setting the extension reads", () => {
    const read = new Set<string>();
    for (const file of fs.readdirSync(path.join(EXTENSION, "src"))) {
      const text = fs.readFileSync(path.join(EXTENSION, "src", file), "utf8");
      for (const match of text.matchAll(
        /(?:\bget|\baffectsConfiguration)\("((?:uf\.)?[a-zA-Z.]+)"\)/g,
      )) {
        read.add(match[1].startsWith("uf.") ? match[1] : `uf.${match[1]}`);
      }
    }
    expect(read.size >= 3).toBe(true);
    for (const key of read) {
      expect(`${key}: ${String(key in manifest.contributes.configuration.properties)}`).toBe(
        `${key}: true`,
      );
    }
  });
});
