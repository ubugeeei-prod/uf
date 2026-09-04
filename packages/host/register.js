// @noflow
//
// Plain JavaScript: this file registers the loader, so it cannot need one.
//
// `node --import @uniflowed/host/register app.js` runs a Flow project on
// Node.js without a build step. Importing this module installs the hooks in
// `./internal/node-hooks.js` for the rest of the process.

import { register } from "node:module";

register("./internal/node-hooks.js", import.meta.url, {
  data: { root: process.env.UF_PROJECT_ROOT ?? process.cwd() },
});
