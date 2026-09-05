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
const { LanguageClient, DocumentFormattingRequest } = require("vscode-languageclient/node");

const { describeMissing, describeSearch, resolveServer } = require("./binary");
const {
  CLIENT_ID,
  CLIENT_NAME,
  readSettings,
  serverExecutable,
  shouldFormatOnSave,
} = require("./client");
const { CONFIG_FILE, FLOW_GLOB, isFlowFile, isUfProject } = require("./project");

/*::
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

  try {
    await client.start();
    log(`uf: language server ready for ${root}`);
  } catch (error) {
    sessions.delete(root);
    watcher.dispose();
    const detail = error != null && error.message != null ? error.message : String(error);
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

/**
 * Stop the server for one folder, if there is one.
 */
async function stop(root /*: string */) /*: Promise<void> */ {
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
  output = vscode.window.createOutputChannel(CLIENT_NAME);
  context.subscriptions.push(output);

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
