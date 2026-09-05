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

import { createRequire } from "node:module";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

const loadCjs = createRequire(import.meta.url);

const binary = loadCjs("../../editors/vscode/src/binary.js");
const client = loadCjs("../../editors/vscode/src/client.js");
const project = loadCjs("../../editors/vscode/src/project.js");

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
    // `uf lsp` accepts `--cwd` and ignores it: the command reads its
    // configuration with `load_config(".")`, so the flag would look like it
    // worked and quietly give the project uf's default formatter width and
    // lint levels. The child process's own working directory is what the
    // server actually reads.
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
