// @flow
//
// How the language client is configured: the process to start, and the
// settings that change what the extension does with it.
//
// Kept apart from `extension.js` because none of it needs an editor. The
// decisions here — which arguments, which working directory, whether a shell
// is involved, what an unset setting means — are the ones that go wrong on a
// machine that is not the author's, and they are the ones that can be tested
// without VS Code running.

"use strict";

const { needsShell } = require("./binary");

/*::
import type { Resolution } from "./binary";

export type Executable = {
  +command: string,
  +args: Array<string>,
  +options: {
    +cwd: string,
    +shell?: boolean,
    ...
  },
};

export type Settings = {
  // `uf.server.path`, trimmed; empty string means "decide for me".
  +serverPath: string,
  // `uf.formatOnSave`.
  +formatOnSave: boolean,
};
*/

// The one subcommand. `uf lsp` speaks JSON-RPC over stdio and takes no
// options that matter here.
//
// Notably *not* `--cwd`: `uf lsp` accepts the flag — it is a global option, so
// clap prints it in `uf lsp --help` — and then ignores it, because the command
// reads its configuration with `load_config(".")` rather than from the
// resolved directory. Passing it would look like it worked and silently give
// the project uf's defaults. Setting the child process's own working
// directory is the thing that actually works, and it is what `serverExecutable`
// does. See the report in the repository for the reproduction.
const SERVER_ARGUMENTS = ["lsp"];

// The name of the output channel and the language client's id. The id is what
// `vscode-languageclient` appends `.trace.server` to when it reads the trace
// level, so it has to stay `uf` for the `uf.trace.server` setting to work.
const CLIENT_ID = "uf";
const CLIENT_NAME = "uf";

/**
 * The process to start for one uf project.
 *
 * The working directory is the project folder, and that is load-bearing rather
 * than tidy: `uf lsp` reads `uf.config.js` from wherever it was started, once,
 * and every formatter option and lint level for the session comes from that
 * read.
 *
 * @throws when the binary was not found; callers check `resolution.kind`
 *   first and show the message from `describeMissing` instead.
 */
function serverExecutable(resolution /*: Resolution */, folder /*: string */) /*: Executable */ {
  if (resolution.kind !== "found") {
    throw new Error("uf: cannot build a server command without a binary");
  }
  const options = needsShell(resolution.command) ? { cwd: folder, shell: true } : { cwd: folder };
  return { command: resolution.command, args: SERVER_ARGUMENTS.slice(), options };
}

/**
 * The extension's settings, read one key at a time.
 *
 * `get` is `WorkspaceConfiguration.get` — the method, not property access,
 * because VS Code exposes a dotted setting as a nested object and
 * `configuration["server.path"]` is therefore always `undefined`.
 *
 * Written to take unknown values because that is what a settings file
 * contains: a user can put a number in a boolean setting, and an extension
 * that then does something surprising is worse than one that falls back.
 */
function readSettings(get /*: (key: string) => mixed */) /*: Settings */ {
  const configured = get("server.path");
  return {
    serverPath: typeof configured === "string" ? configured.trim() : "",
    formatOnSave: get("formatOnSave") === true,
  };
}

/**
 * Whether a saved document should be formatted by the server first.
 *
 * Off unless asked for. An extension that reformats a file the moment it is
 * installed is an extension that gets uninstalled, and VS Code's own
 * `editor.formatOnSave` already exists for people who want it globally.
 */
function shouldFormatOnSave(settings /*: Settings */) /*: boolean */ {
  return settings.formatOnSave;
}

module.exports = {
  CLIENT_ID,
  CLIENT_NAME,
  SERVER_ARGUMENTS,
  readSettings,
  serverExecutable,
  shouldFormatOnSave,
};
