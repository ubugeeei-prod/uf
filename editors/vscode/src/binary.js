// @flow
//
// Where the `uf` binary is, on the machine the editor is actually running on.
//
// # Why the types are in comments
//
// The VS Code extension host is Node.js loading CommonJS. It has no Flow
// transform and no build step is run before it loads an extension, so a file
// with `function f(x: string)` in it would be a syntax error at activation.
// Flow's comment syntax — `/*:: … */` and `/*: T */` — is the same language
// with the same checking, and it is also plain JavaScript that Node runs
// unmodified. That is what lets `uf lint` and `uf fmt` treat this like every
// other file in the repository while VS Code loads the very same bytes, with
// nothing in between that could drift.
//
// # The order, and why it is that order
//
// 1. `uf.server.path`, when the user set one. A setting that names a file that
//    is not there is an error naming it, not a quiet fall-through to something
//    else: someone who points at a build of uf they are debugging would
//    otherwise silently get the released one. This is the same judgement
//    `uf fmt` makes about `fmt.nonFlow.formatter`.
// 2. `<workspace>/node_modules/.bin/uf`, the copy this project pinned. The
//    version a project installed is the version its files were formatted and
//    linted with, so a different global one would disagree with CI.
// 3. `PATH`, for a global install (`curl … | sh`, a package manager).
//
// `PATH` is searched here rather than left to the process spawn so that
// "nothing has it" is something the extension can say, with the list of
// places it looked, instead of an ENOENT the language client swallows.

"use strict";

const path = require("node:path");

/*::
export type Probe = {
  // Whether a path exists and can be executed. Injected so this module can be
  // tested without a file system.
  +exists: (candidate: string) => boolean,
  +env: { +[name: string]: string | void, ... },
  // `process.platform`.
  +platform: string,
};

export type Source = "setting" | "workspace" | "path";

export type Resolution =
  | {
      +kind: "found",
      // The program to spawn.
      +command: string,
      // Which of the three rules above produced it.
      +source: Source,
    }
  | {
      +kind: "missing",
      // Every place that was looked at, in order, for the message.
      +tried: $ReadOnlyArray<string>,
      // Set when the user named a path and it was not there: the message for
      // that case is a different message.
      +setting: string | null,
    };
*/

// The names `uf` can have on disk. npm writes a `.cmd` shim next to the
// extension-less shell script on Windows, and a global install is `uf.exe`.
const POSIX_NAMES = ["uf"];
const WINDOWS_NAMES = ["uf.exe", "uf.cmd", "uf.bat", "uf"];

/**
 * The file names the binary may have on `platform`, most specific first.
 */
function binaryNames(platform /*: string */) /*: Array<string> */ {
  return platform === "win32" ? WINDOWS_NAMES.slice() : POSIX_NAMES.slice();
}

/**
 * Whether spawning `command` needs a shell.
 *
 * Node refuses to execute a `.cmd` or `.bat` directly, so the npm shim on
 * Windows only runs through `cmd.exe`. Nothing else does, and turning the
 * shell on where it is not needed would put the arguments through a second
 * round of quoting.
 */
function needsShell(command /*: string */) /*: boolean */ {
  const extension = path.extname(command).toLowerCase();
  return extension === ".cmd" || extension === ".bat";
}

/**
 * Expand the substitutions VS Code users expect in a path setting.
 *
 * `${workspaceFolder}` is VS Code's own variable and people write it out of
 * habit; a leading `~` is the shell's. Neither is expanded for us, because a
 * setting is read as a literal string.
 */
function expand(
  value /*: string */,
  workspaceFolder /*: string */,
  home /*: string | void */,
) /*: string */ {
  let expanded = value.split("${workspaceFolder}").join(workspaceFolder);
  if (expanded === "~") {
    expanded = home == null ? expanded : home;
  } else if (expanded.startsWith("~/") && home != null) {
    expanded = path.join(home, expanded.slice(2));
  }
  return expanded;
}

/**
 * The directories on `PATH`, in order.
 *
 * The variable is `Path` on Windows as often as `PATH`, and the separator is
 * `;` there. Both are read from the injected environment rather than from
 * `process`, so a test can describe a Windows machine.
 */
function pathDirectories(probe /*: Probe */) /*: Array<string> */ {
  const raw = probe.env.PATH != null ? probe.env.PATH : probe.env.Path;
  if (raw == null || raw === "") {
    return [];
  }
  const separator = probe.platform === "win32" ? ";" : ":";
  return raw.split(separator).filter((entry) => entry !== "");
}

/**
 * Find `uf` for one workspace folder.
 *
 * `settingValue` is `uf.server.path` exactly as configured — empty or absent
 * means "decide for me".
 */
function resolveServer(
  settingValue /*: string | null */,
  workspaceFolder /*: string */,
  probe /*: Probe */,
) /*: Resolution */ {
  const tried = [];
  const names = binaryNames(probe.platform);

  const setting = settingValue == null ? "" : settingValue.trim();
  if (setting !== "") {
    const expanded = expand(setting, workspaceFolder, probe.env.HOME ?? probe.env.USERPROFILE);
    const absolute = path.isAbsolute(expanded) ? expanded : path.join(workspaceFolder, expanded);
    tried.push(absolute);
    if (probe.exists(absolute)) {
      return { kind: "found", command: absolute, source: "setting" };
    }
    // Deliberately not falling through: see the header.
    return { kind: "missing", tried, setting };
  }

  const binDirectory = path.join(workspaceFolder, "node_modules", ".bin");
  for (const name of names) {
    const candidate = path.join(binDirectory, name);
    tried.push(candidate);
    if (probe.exists(candidate)) {
      return { kind: "found", command: candidate, source: "workspace" };
    }
  }

  for (const directory of pathDirectories(probe)) {
    for (const name of names) {
      const candidate = path.join(directory, name);
      tried.push(candidate);
      if (probe.exists(candidate)) {
        return { kind: "found", command: candidate, source: "path" };
      }
    }
  }

  return { kind: "missing", tried, setting: null };
}

/**
 * One sentence for a notification, saying which rule found the binary.
 */
function describeSource(resolution /*: Resolution */) /*: string */ {
  if (resolution.kind !== "found") {
    return "uf was not found.";
  }
  switch (resolution.source) {
    case "setting":
      return `uf: using ${resolution.command} (from the uf.server.path setting).`;
    case "workspace":
      return `uf: using ${resolution.command} (this project's node_modules).`;
    default:
      return `uf: using ${resolution.command} (found on PATH).`;
  }
}

/**
 * What to tell the user when there is no binary to start.
 *
 * Short enough for a notification, and it names the setting to change. The
 * full list of places looked at goes to the output channel, because a
 * notification that quotes twenty directories is a notification nobody reads.
 */
function describeMissing(resolution /*: Resolution */) /*: string */ {
  if (resolution.kind !== "missing") {
    return "";
  }
  if (resolution.setting != null) {
    return (
      `uf: the uf.server.path setting points at \`${resolution.setting}\`, ` +
      "which is not there. Fix the setting or clear it to search " +
      "node_modules/.bin and PATH."
    );
  }
  return (
    "uf: no `uf` binary found in this project's node_modules/.bin or on PATH, " +
    "so diagnostics, formatting, quick fixes and hover are off. Install uf, " +
    "or set uf.server.path."
  );
}

/**
 * The lines for the output channel: every place that was looked at.
 */
function describeSearch(resolution /*: Resolution */) /*: Array<string> */ {
  if (resolution.kind === "found") {
    return [describeSource(resolution)];
  }
  return ["uf: looked for the language server in, and did not find it at:"].concat(
    resolution.tried.map((candidate) => `  ${candidate}`),
  );
}

module.exports = {
  binaryNames,
  describeMissing,
  describeSearch,
  describeSource,
  expand,
  needsShell,
  pathDirectories,
  resolveServer,
};
