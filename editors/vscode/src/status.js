// @flow
//
// What the status bar says about the language servers in this window.
//
// A server that failed to start used to be a notification that scrolled away
// and an output channel nobody opened; a server that was quietly running an
// old `uf` was not visible at all. The status bar item is the one place that
// always says which it is, and clicking it offers the things to do about it.
//
// Kept apart from `extension.js` so the text for each state is tested without
// VS Code: the state machine is the language client's, and what is decided
// here is only how each state reads.

"use strict";

/*::
export type ServerState =
  | { readonly kind: "starting", readonly folder: string }
  | { readonly kind: "running", readonly folder: string, readonly version: string | null, readonly old: boolean }
  | { readonly kind: "stopped", readonly folder: string, readonly reason: string | null }
  | { readonly kind: "missing", readonly folder: string };

export type StatusView = {
  // With VS Code's `$(icon)` syntax.
  readonly text: string,
  readonly tooltip: string,
  // Which theme colour the item takes: none, a warning or an error.
  readonly severity: "ok" | "warning" | "error",
};
*/

// Worst first: the item shows the worst state among the window's folders,
// because a window with one healthy server and one missing binary has a
// problem, and "uf 0.2.0 ✓" would hide it.
const RANK = { missing: 0, stopped: 1, starting: 3, running: 4 };

function rank(state /*: ServerState */) /*: number */ {
  if (state.kind === "running" && state.old) {
    return 2;
  }
  return RANK[state.kind];
}

function line(state /*: ServerState */) /*: string */ {
  switch (state.kind) {
    case "starting":
      return `${state.folder}: starting`;
    case "running":
      return `${state.folder}: uf ${state.version ?? "(version not reported)"}${state.old ? ", older than this extension supports" : ""}`;
    case "stopped":
      return `${state.folder}: stopped${state.reason != null ? ` — ${state.reason}` : ""}`;
    default:
      return `${state.folder}: no uf binary found`;
  }
}

/**
 * The status bar item for every server in the window, or `null` when the
 * window has no uf project and the item should be hidden.
 */
function statusView(states /*: $ReadOnlyArray<ServerState> */) /*: StatusView | null */ {
  if (states.length === 0) {
    return null;
  }
  const worst = states.slice().sort((a, b) => rank(a) - rank(b))[0];
  const tooltip = ["uf language server", ...states.map(line), "", "Click for uf commands"].join(
    "\n",
  );
  switch (worst.kind) {
    case "starting":
      return { text: "$(sync~spin) uf", tooltip, severity: "ok" };
    case "running":
      if (worst.old) {
        return {
          text: `$(warning) uf ${worst.version ?? ""}`.trimEnd(),
          tooltip,
          severity: "warning",
        };
      }
      return {
        text: `$(check) uf${worst.version != null ? ` ${worst.version}` : ""}`,
        tooltip,
        severity: "ok",
      };
    case "stopped":
      return { text: "$(error) uf", tooltip, severity: "error" };
    default:
      return { text: "$(error) uf: not found", tooltip, severity: "error" };
  }
}

module.exports = { statusView };
