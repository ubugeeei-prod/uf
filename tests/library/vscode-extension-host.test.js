// @flow
//
// The VS Code extension's wiring, against a stand-in for VS Code.
//
// `vscode-extension.test.js` tests the decisions; this file tests that
// `extension.js` acts on them: that activating it in a uf project starts a
// server in that folder, that the status bar says what the server reported,
// that an old `uf` is called old, that VS Code's built-in JavaScript
// validation is turned off in a uf project once and only where the project
// set nothing, and that every contributed command is registered.
//
// The stand-in is a module loader hook that answers `require("vscode")` and
// `require("vscode-languageclient/node")` with fakes recording what they were
// asked. It is not VS Code: whether the real editor honours what was asked
// (a folder-level `javascript.validate.enable`, an injected grammar) is a
// person's check, and `editors/vscode/README.md` lists it. What this catches
// is the extension asking for the wrong thing, or for nothing.

import fs from "node:fs";
import Module, { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

const loadCjs = createRequire(import.meta.url);
const EXTENSION = path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
  "editors",
  "vscode",
);
const manifest = JSON.parse(fs.readFileSync(path.join(EXTENSION, "package.json"), "utf8"));

type ServerOptions = {
  readonly run: {
    readonly command: string,
    readonly args: Array<string>,
    readonly options: { readonly cwd: string, ... },
  },
  ...
};

type Disposable = { dispose(): void };

type StatusItem = {
  text: string,
  tooltip: string,
  visible: boolean,
  command?: string,
  show(): void,
  hide(): void,
  dispose(): void,
  ...
};

/** The language client, as far as `extension.js` uses it. */
class FakeClient {
  // Set by `fake()` before each activation.
  static version: ?string = null;
  static created: Array<FakeClient> = [];

  server: ServerOptions;
  initializeResult: ?{ serverInfo: { name: string, version: ?string } } = null;
  listeners: Array<(event: { newState: number }) => void> = [];
  started: boolean = false;

  constructor(id: string, name: string, server: ServerOptions) {
    this.server = server;
    FakeClient.created.push(this);
  }

  onDidChangeState(listener: (event: { newState: number }) => void): Disposable {
    this.listeners.push(listener);
    return { dispose() {} };
  }

  start(): Promise<void> {
    this.started = true;
    this.initializeResult = { serverInfo: { name: "uf-lsp", version: FakeClient.version } };
    return Promise.resolve();
  }

  stop(): Promise<void> {
    return Promise.resolve();
  }
}

type Fake = {
  vscode: mixed,
  languageclient: mixed,
  commands: Map<string, () => mixed>,
  messages: Array<{ level: string, text: string }>,
  updates: Array<{ key: string, value: mixed, target: number, language: boolean }>,
  clients: Array<FakeClient>,
  statusItem: StatusItem,
  workspaceState: Map<string, mixed>,
  // `WorkspaceFolder.uri.toString()` for each folder, in order.
  folderUris: Array<string>,
};

/** A folder with `uf.config.js`, and a `uf` in `node_modules/.bin`. */
function ufProject(): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-vscode-host-"));
  fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
  const bin = path.join(root, "node_modules", ".bin");
  fs.mkdirSync(bin, { recursive: true });
  fs.writeFileSync(path.join(bin, "uf"), "#!/bin/sh\n");
  fs.chmodSync(path.join(bin, "uf"), 0o755);
  return root;
}

function fake(
  roots: Array<string>,
  options: {
    serverVersion?: string,
    settings?: { [string]: mixed },
    inspections?: { [string]: { [string]: mixed } },
    workspaceState?: Map<string, mixed>,
    // Keys VS Code refuses as a folder setting, as it refuses a
    // window-scoped one in a multi-root workspace.
    refuseInFolder?: Array<string>,
  },
): Fake {
  const commands = new Map<string, () => mixed>();
  const messages = [];
  const updates = [];
  const clients: Array<FakeClient> = [];
  FakeClient.version = options.serverVersion;
  FakeClient.created = clients;
  const workspaceState = options.workspaceState ?? new Map<string, mixed>();
  const statusItem: StatusItem = {
    text: "",
    tooltip: "",
    visible: false,
    show() {
      statusItem.visible = true;
    },
    hide() {
      statusItem.visible = false;
    },
    dispose() {},
  };
  const disposable = { dispose() {} };
  const event = () => disposable;
  const uri = (fsPath: string) => ({ fsPath, toString: () => `file://${fsPath}`, scheme: "file" });
  const folders = roots.map((root, index) => ({
    uri: uri(root),
    name: path.basename(root),
    index,
  }));
  const message = (level: string) => (text: string) => {
    messages.push({ level, text });
    return Promise.resolve(undefined);
  };
  const vscode = {
    Uri: { file: uri },
    RelativePattern: class {
      constructor(base: mixed, pattern: string) {
        this.base = base;
        this.pattern = pattern;
      }
      base: mixed;
      pattern: string;
    },
    ThemeColor: class {
      constructor(id: string) {
        this.id = id;
      }
      id: string;
    },
    StatusBarAlignment: { Left: 1, Right: 2 },
    ConfigurationTarget: { Global: 1, Workspace: 2, WorkspaceFolder: 3 },
    window: {
      createOutputChannel: () => ({ appendLine() {}, show() {}, dispose() {} }),
      createStatusBarItem: () => statusItem,
      showErrorMessage: message("error"),
      showWarningMessage: message("warning"),
      showInformationMessage: message("information"),
      showQuickPick: () => Promise.resolve(undefined),
    },
    workspace: {
      workspaceFolders: folders,
      getWorkspaceFolder: (target: { fsPath: string }) =>
        folders.find((folder) => target.fsPath.startsWith(folder.uri.fsPath)),
      getConfiguration: (section: ?string) => ({
        get: (key: string) => options.settings?.[section != null ? `${section}.${key}` : key],
        // A registered setting nobody set, unless the test says otherwise.
        inspect: (key: string) => options.inspections?.[key] ?? { defaultValue: true },
        update: (key: string, value: mixed, target: number, language: boolean = false) => {
          if (target === 3 && options.refuseInFolder?.includes(key) === true) {
            return Promise.reject(new Error(`${key} does not support the folder resource scope`));
          }
          updates.push({
            key: section != null ? `${section}.${key}` : key,
            value,
            target,
            language,
          });
          return Promise.resolve();
        },
      }),
      createFileSystemWatcher: () => ({
        onDidChange: event,
        onDidCreate: event,
        onDidDelete: event,
        dispose() {},
      }),
      onDidChangeWorkspaceFolders: event,
      onDidChangeConfiguration: event,
      onWillSaveTextDocument: event,
    },
    commands: {
      registerCommand: (name: string, handler: () => mixed) => {
        commands.set(name, handler);
        return disposable;
      },
      executeCommand: () => Promise.resolve(),
    },
  };
  const State = { Stopped: 1, Starting: 3, Running: 2 };
  const languageclient = {
    LanguageClient: FakeClient,
    State,
    DocumentFormattingRequest: { type: {} },
  };
  return {
    vscode,
    languageclient,
    commands,
    messages,
    updates,
    clients,
    statusItem,
    workspaceState,
    folderUris: folders.map((folder) => folder.uri.toString()),
  };
}

/** Load a fresh copy of the extension against `f`, and activate it. */
async function activate(f: Fake): Promise<mixed> {
  const original = Module._load;
  Module._load = function load(this: mixed, request: string, ...rest: Array<mixed>) {
    if (request === "vscode") {
      return f.vscode;
    }
    if (request === "vscode-languageclient/node") {
      return f.languageclient;
    }
    return original.call(this, request, ...rest);
  };
  try {
    const entry = path.join(EXTENSION, "src", "extension.js");
    for (const file of fs.readdirSync(path.join(EXTENSION, "src"))) {
      delete loadCjs.cache[path.join(EXTENSION, "src", file)];
    }
    const extension = loadCjs(entry);
    const context = {
      subscriptions: [],
      workspaceState: {
        get: (key: string) => f.workspaceState.get(key),
        update: (key: string, value: mixed) => {
          f.workspaceState.set(key, value);
          return Promise.resolve();
        },
      },
    };
    await extension.activate(context);
    return extension;
  } finally {
    Module._load = original;
  }
}

describe("activating in a uf project", () => {
  it("starts `uf lsp` from the project's node_modules, in the project", async () => {
    const root = ufProject();
    const f = fake([root], { serverVersion: "0.2.0" });
    await activate(f);

    expect(f.clients.length).toBe(1);
    const client = f.clients[0];
    expect(client.started).toBe(true);
    expect(client.server.run.command).toBe(path.join(root, "node_modules", ".bin", "uf"));
    expect(client.server.run.args).toEqual(["lsp"]);
    expect(client.server.run.options.cwd).toBe(root);
  });

  it("shows the server's version in the status bar", async () => {
    const f = fake([ufProject()], { serverVersion: "0.2.0" });
    await activate(f);
    expect(f.statusItem.visible).toBe(true);
    expect(f.statusItem.text).toBe("$(check) uf 0.2.0");
    expect(f.statusItem.command).toBe("uf.showMenu");
  });

  it("warns once about a uf older than the extension supports", async () => {
    const f = fake([ufProject()], { serverVersion: "0.0.0-alpha.40" });
    await activate(f);
    const warnings = f.messages.filter((message) => message.level === "warning");
    expect(warnings.length).toBe(1);
    expect(warnings[0].text).toContain("0.0.0-alpha.40");
    expect(f.statusItem.text).toBe("$(warning) uf 0.0.0-alpha.40");
  });

  it("registers every command package.json contributes", async () => {
    const f = fake([ufProject()], { serverVersion: "0.2.0" });
    await activate(f);
    for (const command of manifest.contributes.commands) {
      expect(`${command.command}: ${String(f.commands.has(command.command))}`).toBe(
        `${command.command}: true`,
      );
    }
  });
});

describe("VS Code's built-in JavaScript validation", () => {
  it("is turned off for the folder, once", async () => {
    const workspaceState = new Map<string, mixed>();
    const f = fake([ufProject()], { serverVersion: "0.2.0", workspaceState });
    await activate(f);
    expect(f.updates).toEqual([
      { key: "javascript.validate.enable", value: false, target: 3, language: false },
      { key: "js/ts.validate.enabled", value: false, target: 3, language: true },
    ]);
    expect(
      f.messages.some((message) => message.text.includes("built-in JavaScript validation")),
    ).toBe(true);

    // Deleting the line afterwards is a decision; the next start respects it.
    const again = fake([ufProject()], { serverVersion: "0.2.0", workspaceState });
    workspaceState.set(`uf.automaticSettings:${again.folderUris[0]}`, true);
    await activate(again);
    expect(again.updates).toEqual([]);
  });

  it("is never written to the whole workspace when a folder refuses it", async () => {
    // In a multi-root window the old name is window-scoped; the workspace file
    // would turn validation off for a TypeScript folder beside this one.
    const f = fake([ufProject()], {
      serverVersion: "0.2.0",
      refuseInFolder: ["javascript.validate.enable"],
    });
    await activate(f);
    expect(f.updates).toEqual([
      { key: "js/ts.validate.enabled", value: false, target: 3, language: true },
    ]);
    expect(f.clients[0].started).toBe(true);
  });

  it("is left alone when the project set it", async () => {
    const f = fake([ufProject()], {
      serverVersion: "0.2.0",
      inspections: {
        "javascript.validate.enable": { defaultValue: true, workspaceValue: true },
        "js/ts.validate.enabled": { defaultValue: true, workspaceLanguageValue: true },
      },
    });
    await activate(f);
    expect(f.updates).toEqual([]);
  });

  it("is left alone when the opt-out is set", async () => {
    const f = fake([ufProject()], {
      serverVersion: "0.2.0",
      settings: { "uf.workspace.disableBuiltinValidation": false },
    });
    await activate(f);
    expect(f.updates).toEqual([]);
  });

  it("is left alone outside a uf project", async () => {
    const plain = fs.mkdtempSync(path.join(os.tmpdir(), "uf-vscode-plain-"));
    const f = fake([plain], { serverVersion: "0.2.0" });
    await activate(f);
    expect(f.clients.length).toBe(0);
    expect(f.updates).toEqual([]);
    expect(f.statusItem.visible).toBe(false);
  });
});

describe("a folder with no uf binary", () => {
  it("says so in the status bar and a notification, and starts nothing", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-vscode-missing-"));
    fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
    const f = fake([root], { serverVersion: "0.2.0" });
    const previous = process.env.PATH;
    process.env.PATH = "";
    try {
      await activate(f);
    } finally {
      process.env.PATH = previous;
    }
    expect(f.clients.length).toBe(0);
    expect(f.statusItem.text).toBe("$(error) uf: not found");
    expect(f.messages.some((message) => message.level === "error")).toBe(true);
  });
});
