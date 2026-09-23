// @flow
//
// The VS Code half of `uf lsp`: start one server per uf project, wire up the
// four things the server actually serves, and say something useful when there
// is no server to start.
//
// # Language
//
// JavaScript with Flow types written in Flow's comment syntax. The extension
// host loads CommonJS from disk with no transform, so annotations that are not
// valid JavaScript would need a build step between this file and the one VS
// Code loads — and a build step is a place for the two to differ. `/*: T */`
// is checked by Flow, formatted by `uf fmt`, linted by `uf lint`, and run by
// Node unmodified. See `src/binary.js` for the longer version.
//
// # What is wired, and to what
//
// Everything here is answered by `uf lsp` itself, from the same crates the
// command line uses. Nothing in this extension implements a language feature:
//
// * **Diagnostics** — pushed by the server on open and on every change
//   (`textDocument/publishDiagnostics`, from `uf_lint`).
// * **Formatting** — `textDocument/formatting`, from `uf_fmt`. Registered by
//   the language client, so it is available as "Format Document"; the
//   `uf.formatOnSave` setting below asks for it on save as well.
// * **Quick fixes and fix-all** — `textDocument/codeAction`, kinds `quickfix`
//   and `source.fixAll.uf`.
// * **Hover** — `textDocument/hover`.
// * **Definitions and completion** — registered by the language client from
//   the capabilities the server advertises.
//
// Around that, the extension's own work: finding the binary (`binary.js`),
// checking the version the server reports (`version.js`), a status bar item
// with each server's state (`status.js`), and — in a uf project only — the
// settings that stop VS Code's built-in TypeScript service reporting Flow
// syntax as errors (`workspace.js`). The syntax highlighting for
// `component`, `hook`, `match` and `renders` is a TextMate injection
// grammar, `syntaxes/flow.injection.json`, contributed from package.json.
//
// # One server per folder, started in that folder
//
// `uf lsp` reads `uf.config.js` from its working directory, once, at start-up.
// That is the only channel it has for a project's formatter width, quote
// style and lint levels. So each uf project in the window gets its own server
// process whose `cwd` is that project, and a change to `uf.config.js` restarts
// it — the server has no way to be told about the change, and a stale one is
// a server formatting to the wrong width without saying so.

"use strict";

const vscode = require("vscode");
const nodeFs = require("node:fs");
const { LanguageClient, DocumentFormattingRequest, State } = require("vscode-languageclient/node");

const { describeMissing, describeSearch, resolveServer } = require("./binary");
const {
  CLIENT_ID,
  CLIENT_NAME,
  readSettings,
  serverExecutable,
  shouldFormatOnSave,
} = require("./client");
const { CONFIG_FILE, FLOW_GLOB, isFlowFile, isUfProject } = require("./project");
const { statusView } = require("./status");
const { checkVersion } = require("./version");
const { AUTOMATIC, RECOMMENDED, describePlan, describeSetting, plan } = require("./workspace");

/*::
import type { ServerState } from "./status";
import type { Inspection, Setting, Step } from "./workspace";

type Session = {
  client: LanguageClient,
  folder: string,
  watcher: vscode.FileSystemWatcher,
};
*/

// One session per workspace folder path. A map rather than a list because
// folders are added and removed while the window is open.
const sessions /*: Map<string, Session> */ = new Map();

let output /*: vscode.OutputChannel | null */ = null;
let statusItem /*: vscode.StatusBarItem | null */ = null;
let extensionContext /*: vscode.ExtensionContext | null */ = null;

// What the status bar shows, per folder. Kept apart from `sessions` because a
// folder whose binary is missing, or whose server stopped, has a state and no
// session.
const states /*: Map<string, ServerState> */ = new Map();

function setState(root /*: string */, state /*: ServerState | null */) /*: void */ {
  if (state == null) {
    states.delete(root);
  } else {
    states.set(root, state);
  }
  renderStatus();
}

function renderStatus() /*: void */ {
  const item = statusItem;
  if (item == null) {
    return;
  }
  const view = statusView(Array.from(states.values()));
  if (view == null) {
    item.hide();
    return;
  }
  item.text = view.text;
  item.tooltip = view.tooltip;
  item.backgroundColor =
    view.severity === "error"
      ? new vscode.ThemeColor("statusBarItem.errorBackground")
      : view.severity === "warning"
        ? new vscode.ThemeColor("statusBarItem.warningBackground")
        : undefined;
  item.show();
}

/**
 * Whether a path exists and is a file we could execute.
 *
 * `accessSync` with `X_OK` rather than `existsSync`, because a directory named
 * `uf` on `PATH` exists and is not a program.
 */
function isExecutable(candidate /*: string */) /*: boolean */ {
  try {
    const stats = nodeFs.statSync(candidate);
    if (!stats.isFile()) {
      return false;
    }
    nodeFs.accessSync(candidate, nodeFs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function exists(candidate /*: string */) /*: boolean */ {
  return nodeFs.existsSync(candidate);
}

/**
 * This extension's settings, scoped to one resource.
 *
 * `getConfiguration(...).get(key)` rather than property access: VS Code
 * exposes `uf.server.path` as a nested `server` object, so reading
 * `configuration["server.path"]` would always be `undefined`.
 */
function settingsFor(resource /*: vscode.Uri */) {
  const configuration = vscode.workspace.getConfiguration("uf", resource);
  return readSettings((key) => configuration.get(key));
}

function log(line /*: string */) /*: void */ {
  if (output != null) {
    output.appendLine(line);
  }
}

/**
 * Start a server for one folder, or explain why there is none.
 */
async function start(folder /*: vscode.WorkspaceFolder */) /*: Promise<void> */ {
  const root = folder.uri.fsPath;
  if (sessions.has(root)) {
    return;
  }
  if (!isUfProject(root, exists)) {
    return;
  }

  const settings = settingsFor(folder.uri);
  const resolution = resolveServer(settings.serverPath, root, {
    exists: isExecutable,
    env: process.env,
    platform: process.platform,
  });

  for (const line of describeSearch(resolution)) {
    log(line);
  }

  if (resolution.kind === "missing") {
    setState(root, { kind: "missing", folder: folder.name });
    // Loud, and with somewhere to go. The alternative is a language client
    // that fails to spawn and reports it into a channel nobody opened.
    const choice = await vscode.window.showErrorMessage(
      describeMissing(resolution),
      "Open Settings",
      "Show Log",
    );
    if (choice === "Open Settings") {
      await vscode.commands.executeCommand("workbench.action.openSettings", "uf.server.path");
    } else if (choice === "Show Log" && output != null) {
      output.show(true);
    }
    return;
  }

  const executable = serverExecutable(resolution, root);
  const serverOptions = {
    run: executable,
    debug: executable,
  };
  const clientOptions = {
    // Scoped to this folder: a multi-root window with two uf projects gets two
    // servers, and neither is sent the other's files.
    documentSelector: [{ scheme: "file", pattern: new vscode.RelativePattern(folder, FLOW_GLOB) }],
    workspaceFolder: folder,
    outputChannel: output ?? undefined,
    // The server has no `workspace/configuration` and no
    // `didChangeConfiguration` handling; its settings come from
    // `uf.config.js`, which the watcher below restarts it for.
    initializationOptions: {},
  };

  const client = new LanguageClient(CLIENT_ID, CLIENT_NAME, serverOptions, clientOptions);

  // `uf lsp` reads the config once, at start-up. Editing `uf.config.js` with
  // the window open is the ordinary way to change a lint level, so restart
  // rather than leave the editor showing answers from the old settings.
  const watcher = vscode.workspace.createFileSystemWatcher(
    new vscode.RelativePattern(folder, CONFIG_FILE),
  );
  const onConfigChanged = () => {
    log(`uf: ${CONFIG_FILE} changed in ${root}; restarting the language server`);
    void restart(folder);
  };
  watcher.onDidChange(onConfigChanged);
  watcher.onDidCreate(onConfigChanged);
  watcher.onDidDelete(onConfigChanged);

  sessions.set(root, { client, folder: root, watcher });
  setState(root, { kind: "starting", folder: folder.name });

  // The client restarts a crashed server a few times on its own, and gives up
  // after that; either way the status bar should say what it is doing now.
  client.onDidChangeState((event) => {
    if (sessions.get(root)?.client !== client) {
      return;
    }
    if (event.newState === State.Stopped) {
      setState(root, { kind: "stopped", folder: folder.name, reason: null });
    } else if (event.newState === State.Starting) {
      setState(root, { kind: "starting", folder: folder.name });
    } else if (event.newState === State.Running) {
      reportVersion(root, folder.name, client, false);
    }
  });

  try {
    await client.start();
    log(`uf: language server ready for ${root}`);
    reportVersion(root, folder.name, client, true);
    await applyAutomaticSettings(folder);
  } catch (error) {
    sessions.delete(root);
    watcher.dispose();
    const detail = error != null && error.message != null ? error.message : String(error);
    setState(root, { kind: "stopped", folder: folder.name, reason: detail });
    log(`uf: the language server for ${root} did not start: ${detail}`);
    const choice = await vscode.window.showErrorMessage(
      `uf: the language server did not start (${detail}).`,
      "Show Log",
    );
    if (choice === "Show Log" && output != null) {
      output.show(true);
    }
  }
}

// Folders already told their `uf` is old, so a restart does not repeat it.
const warnedOld /*: Set<string> */ = new Set();

/**
 * Read the version the server reported in `initialize`, show it, and say so
 * once when it is older than this extension supports.
 */
function reportVersion(
  root /*: string */,
  name /*: string */,
  client /*: LanguageClient */,
  announce /*: boolean */,
) /*: void */ {
  const info = client.initializeResult?.serverInfo;
  const check = checkVersion(info?.version);
  const version = check.version;
  setState(root, { kind: "running", folder: name, version, old: check.kind === "old" });
  if (check.kind === "old") {
    log(check.message);
    if (announce && !warnedOld.has(root)) {
      warnedOld.add(root);
      void vscode.window.showWarningMessage(check.message, "Show Log").then((choice) => {
        if (choice === "Show Log" && output != null) {
          output.show(true);
        }
      });
    }
  } else if (check.kind === "unknown") {
    log(`uf: the server for ${root} did not report a version uf writes (${String(version)})`);
  } else {
    log(`uf: ${root} is served by uf ${check.version}`);
  }
}

/**
 * `WorkspaceConfiguration.inspect` for one of the settings `workspace.js`
 * plans, in the folder's scope and, for a language-scoped one, that language's.
 */
function inspectSetting(
  folder /*: vscode.WorkspaceFolder */,
  setting /*: Setting */,
) /*: Inspection */ {
  const scope =
    setting.language != null ? { uri: folder.uri, languageId: setting.language } : folder.uri;
  const inspected = vscode.workspace.getConfiguration(undefined, scope).inspect(setting.key);
  return inspected ?? {};
}

/**
 * Write the steps a plan says to write, to this folder's settings.
 *
 * `WorkspaceFolder` first — `.vscode/settings.json` in that folder, which is
 * the file a team commits. A setting VS Code registered as window-scoped
 * refuses a folder target, and for that one the workspace is the narrowest
 * place there is.
 */
async function writeSteps(
  folder /*: vscode.WorkspaceFolder */,
  steps /*: $ReadOnlyArray<Step> */,
) /*: Promise<number> */ {
  let written = 0;
  for (const step of steps) {
    if (step.kind !== "write") {
      continue;
    }
    const setting = step.setting;
    const scope =
      setting.language != null ? { uri: folder.uri, languageId: setting.language } : folder.uri;
    const configuration = vscode.workspace.getConfiguration(undefined, scope);
    const inLanguage = setting.language != null;
    try {
      await configuration.update(
        setting.key,
        setting.value,
        vscode.ConfigurationTarget.WorkspaceFolder,
        inLanguage,
      );
      written += 1;
      continue;
    } catch (error) {
      log(
        `uf: ${describeSetting(setting)} cannot go in folder settings (${String(error)}); trying the workspace`,
      );
    }
    try {
      await configuration.update(
        setting.key,
        setting.value,
        vscode.ConfigurationTarget.Workspace,
        inLanguage,
      );
      written += 1;
    } catch (error) {
      // A settings file VS Code cannot parse, or a read-only one: say so and
      // go on, rather than failing the server's start over a preference.
      log(`uf: could not write ${describeSetting(setting)}: ${String(error)}`);
    }
  }
  return written;
}

/**
 * In a uf project, turn off VS Code's built-in JavaScript validation for that
 * folder — once, and never over a value the project set. See `workspace.js`.
 */
async function applyAutomaticSettings(folder /*: vscode.WorkspaceFolder */) /*: Promise<void> */ {
  const context = extensionContext;
  if (context == null) {
    return;
  }
  const enabled = vscode.workspace
    .getConfiguration("uf", folder.uri)
    .get("workspace.disableBuiltinValidation");
  if (enabled === false) {
    return;
  }
  const key = `uf.automaticSettings:${folder.uri.toString()}`;
  if (context.workspaceState.get(key) === true) {
    return;
  }
  const steps = plan(AUTOMATIC, (setting) => inspectSetting(folder, setting));
  await context.workspaceState.update(key, true);
  if (!steps.some((step) => step.kind === "write")) {
    return;
  }
  const written = await writeSteps(folder, steps);
  log(`uf: in ${folder.uri.fsPath}, wrote ${written} setting(s) to .vscode/settings.json:`);
  for (const line of describePlan(steps)) {
    log(line);
  }
  const choice = await vscode.window.showInformationMessage(
    `uf: turned off VS Code's built-in JavaScript validation in ${folder.name} (.vscode/settings.json), so Flow syntax is not reported as TypeScript errors. uf reports this folder's problems.`,
    "Undo",
    "Show Log",
  );
  if (choice === "Undo") {
    for (const step of steps) {
      if (step.kind !== "write") {
        continue;
      }
      const language = step.setting.language;
      const configuration = vscode.workspace.getConfiguration(
        undefined,
        language != null ? { uri: folder.uri, languageId: language } : folder.uri,
      );
      // Both levels: `writeSteps` may have fallen back to the workspace, and
      // the plan only wrote where the project had set nothing.
      for (const target of [
        vscode.ConfigurationTarget.WorkspaceFolder,
        vscode.ConfigurationTarget.Workspace,
      ]) {
        try {
          await configuration.update(step.setting.key, undefined, target, language != null);
        } catch (error) {
          log(`uf: could not clear ${step.setting.key}: ${String(error)}`);
        }
      }
    }
    await vscode.workspace
      .getConfiguration("uf", folder.uri)
      .update(
        "workspace.disableBuiltinValidation",
        false,
        vscode.ConfigurationTarget.WorkspaceFolder,
      );
  } else if (choice === "Show Log" && output != null) {
    output.show(true);
  }
}

/**
 * The uf projects in this window, for commands that act on one.
 */
function ufFolders() /*: Array<vscode.WorkspaceFolder> */ {
  return (vscode.workspace.workspaceFolders ?? []).filter((folder) =>
    isUfProject(folder.uri.fsPath, exists),
  );
}

async function pickFolder() /*: Promise<vscode.WorkspaceFolder | null> */ {
  const folders = ufFolders();
  if (folders.length <= 1) {
    return folders[0] ?? null;
  }
  const picked = await vscode.window.showQuickPick(
    folders.map((folder) => ({ label: folder.name, description: folder.uri.fsPath, folder })),
    { placeHolder: "Which uf project?" },
  );
  return picked != null ? picked.folder : null;
}

/**
 * "uf: Configure Workspace for Flow": the full per-project set, after saying
 * what it will write.
 */
async function configureWorkspace() /*: Promise<void> */ {
  const folder = await pickFolder();
  if (folder == null) {
    void vscode.window.showInformationMessage(
      "uf: no folder in this window has a uf.config.js, so there is nothing to configure.",
    );
    return;
  }
  const steps = plan(RECOMMENDED, (setting) => inspectSetting(folder, setting));
  const lines = describePlan(steps);
  log(`uf: settings for ${folder.uri.fsPath}:`);
  for (const line of lines) {
    log(line);
  }
  if (!steps.some((step) => step.kind === "write")) {
    void vscode.window.showInformationMessage(`uf: ${folder.name} is already configured for Flow.`);
    return;
  }
  const choice = await vscode.window.showInformationMessage(
    `uf: write these to ${folder.name}/.vscode/settings.json?`,
    { modal: true, detail: lines.join("\n") },
    "Write",
  );
  if (choice !== "Write") {
    return;
  }
  const written = await writeSteps(folder, steps);
  log(`uf: wrote ${written} setting(s)`);
}

/**
 * The status bar item's menu.
 */
async function showMenu() /*: Promise<void> */ {
  const items = [
    { label: "$(debug-restart) Restart Language Server", command: "uf.restartServer" },
    { label: "$(output) Show Language Server Log", command: "uf.showOutput" },
    { label: "$(settings-gear) Configure Workspace for Flow", command: "uf.configureWorkspace" },
    {
      label: "$(gear) Open uf Settings",
      command: "workbench.action.openSettings",
      argument: "@ext:uniflowed.uf",
    },
  ];
  const picked = await vscode.window.showQuickPick(items, { placeHolder: "uf" });
  if (picked == null) {
    return;
  }
  if (picked.argument != null) {
    await vscode.commands.executeCommand(picked.command, picked.argument);
  } else {
    await vscode.commands.executeCommand(picked.command);
  }
}

/**
 * Stop the server for one folder, if there is one.
 */
async function stop(root /*: string */) /*: Promise<void> */ {
  setState(root, null);
  const session = sessions.get(root);
  if (session == null) {
    return;
  }
  sessions.delete(root);
  session.watcher.dispose();
  try {
    await session.client.stop();
  } catch (error) {
    // A server that has already gone is the outcome we wanted.
    log(`uf: stopping the server for ${root}: ${String(error)}`);
  }
}

/**
 * Restart one folder's server, re-resolving the binary.
 *
 * Re-resolving is the point: someone who has just installed uf, or just
 * pointed `uf.server.path` at a build of their own, should not have to reload
 * the window. A language server that needs a window reload is a language
 * server people turn off.
 */
async function restart(folder /*: vscode.WorkspaceFolder */) /*: Promise<void> */ {
  await stop(folder.uri.fsPath);
  await start(folder);
}

async function restartAll() /*: Promise<void> */ {
  const folders = vscode.workspace.workspaceFolders ?? [];
  for (const root of Array.from(sessions.keys())) {
    await stop(root);
  }
  for (const folder of folders) {
    await start(folder);
  }
  if (sessions.size === 0) {
    log("uf: nothing to restart — no folder in this window has a uf.config.js");
  }
}

/**
 * The session that owns a document, if any.
 */
function sessionFor(document /*: vscode.TextDocument */) /*: Session | null */ {
  const folder = vscode.workspace.getWorkspaceFolder(document.uri);
  if (folder == null) {
    return null;
  }
  return sessions.get(folder.uri.fsPath) ?? null;
}

/**
 * Format a document on save, through the server's own formatter.
 *
 * Asked as a `textDocument/formatting` request rather than by running
 * `uf fmt`, so what a save does and what "Format Document" does are the same
 * code path in the same process, over the buffer as it is now rather than the
 * file as it was.
 */
async function formatOnSave(
  event /*: vscode.TextDocumentWillSaveEvent */,
) /*: Promise<Array<vscode.TextEdit>> */ {
  // Only the files this extension claims: the handler is called for every
  // save in the window, including files no uf server has ever been sent.
  if (!isFlowFile(event.document.uri.fsPath)) {
    return [];
  }
  const session = sessionFor(event.document);
  if (session == null) {
    return [];
  }
  if (!shouldFormatOnSave(settingsFor(event.document.uri))) {
    return [];
  }

  const client = session.client;
  try {
    const edits = await client.sendRequest(DocumentFormattingRequest.type, {
      textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(event.document),
      options: {
        // The server formats to `uf.config.js`, not to the editor's tab size.
        // These are sent because the protocol requires them.
        tabSize: 2,
        insertSpaces: true,
      },
    });
    if (edits == null) {
      return [];
    }
    return client.protocol2CodeConverter.asTextEdits(edits);
  } catch (error) {
    // A save is not the place to raise. The server may have stopped between
    // the check above and the request, and a rejected `waitUntil` is a modal
    // on every keystroke that reaches disk.
    log(`uf: could not format ${event.document.uri.fsPath} on save: ${String(error)}`);
    return [];
  }
}

/**
 * Start everything this window needs.
 */
async function activate(context /*: vscode.ExtensionContext */) /*: Promise<void> */ {
  extensionContext = context;
  output = vscode.window.createOutputChannel(CLIENT_NAME);
  context.subscriptions.push(output);
  statusItem = vscode.window.createStatusBarItem("uf.status", vscode.StatusBarAlignment.Right, 100);
  statusItem.name = "uf";
  statusItem.command = "uf.showMenu";
  context.subscriptions.push(statusItem);

  context.subscriptions.push(
    vscode.commands.registerCommand("uf.restartServer", async () => {
      log("uf: restarting the language server");
      await restartAll();
    }),
    vscode.commands.registerCommand("uf.showOutput", () => {
      if (output != null) {
        output.show(true);
      }
    }),
    vscode.commands.registerCommand("uf.configureWorkspace", configureWorkspace),
    vscode.commands.registerCommand("uf.showMenu", showMenu),
    vscode.workspace.onDidChangeWorkspaceFolders(async (event) => {
      for (const removed of event.removed) {
        await stop(removed.uri.fsPath);
      }
      for (const added of event.added) {
        await start(added);
      }
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      // Only the setting that decides which process to run needs a restart;
      // `uf.formatOnSave` is read on every save and `uf.trace.server` is the
      // language client's own.
      if (event.affectsConfiguration("uf.server.path")) {
        await restartAll();
      }
    }),
    vscode.workspace.onWillSaveTextDocument((event) => {
      event.waitUntil(formatOnSave(event));
    }),
  );

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    await start(folder);
  }
}

async function deactivate() /*: Promise<void> */ {
  for (const root of Array.from(sessions.keys())) {
    await stop(root);
  }
}

module.exports = { activate, deactivate };
