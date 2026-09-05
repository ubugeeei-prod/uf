// @flow
//
// What counts as a uf project, and which files in one this extension claims.
//
// # `uf.config.js`, and nothing else
//
// uf has exactly one configuration surface, and `uf_config::CONFIG_FILES` is
// the list of names it will read: `["uf.config.js"]`. So a folder is a uf
// project when it has that file at its root, and the extension starts one
// server per such folder — no server at all in a folder without one.
//
// This matters more than a file-name check usually does, because of where the
// server reads its configuration from. `uf lsp` calls `load_config(".")` once
// at start-up: its options come from its *working directory*, and there is no
// request that can tell it otherwise. So the folder that has the config is
// also the folder the server has to be started in, and starting one anywhere
// else would give a project the default formatter width and the default lint
// levels while looking exactly like it was working.
//
// The alternative — activate on any JavaScript file and hunt upwards for a
// config — would start a uf server in projects that are not uf projects, and
// report uf's lint rules against code that never opted into them.
//
// # Which files
//
// The four extensions `uf_lint`'s `flow/syntax` claims: `.js`, `.jsx`, `.mjs`
// and `.cjs`. Matching by glob rather than by VS Code language id is
// deliberate — `.mjs` and `.cjs` are `javascript` to VS Code, but so is a
// `.js` file in a folder with no `uf.config.js`, and the pattern is scoped to
// the folder so a multi-root window does not send one project's files to
// another project's server.

"use strict";

const path = require("node:path");

// uf's single configuration surface. Keep in step with
// `uf_config::CONFIG_FILES`, which is the authority.
const CONFIG_FILE = "uf.config.js";

// The files `flow/syntax` parses; see `uf_lint::runner::flow_syntax`. The glob
// is built from the list rather than written twice, because the document
// selector and the save handler have to claim the same files: a save handler
// that claimed more would ask the server to format a document it was never
// sent, and one that claimed fewer would skip a file the squiggles are on.
const FLOW_EXTENSIONS = [".js", ".jsx", ".mjs", ".cjs"];
const FLOW_GLOB = `**/*.{${FLOW_EXTENSIONS.map((extension) => extension.slice(1)).join(",")}}`;

/*::
export type Exists = (candidate: string) => boolean;
*/

/**
 * Whether a path is one of the files uf parses.
 */
function isFlowFile(filePath /*: string */) /*: boolean */ {
  return FLOW_EXTENSIONS.some((extension) => filePath.endsWith(extension));
}

/**
 * Whether `folder` is the root of a uf project.
 */
function isUfProject(folder /*: string */, exists /*: Exists */) /*: boolean */ {
  return exists(path.join(folder, CONFIG_FILE));
}

/**
 * The uf projects among the folders open in this window, in the order given.
 *
 * A window with none of them is a window where this extension does nothing,
 * which is the intended outcome and not a failure to report.
 */
function ufProjectFolders(
  folders /*: $ReadOnlyArray<string> */,
  exists /*: Exists */,
) /*: Array<string> */ {
  return folders.filter((folder) => isUfProject(folder, exists));
}

module.exports = {
  CONFIG_FILE,
  FLOW_EXTENSIONS,
  FLOW_GLOB,
  isFlowFile,
  isUfProject,
  ufProjectFolders,
};
