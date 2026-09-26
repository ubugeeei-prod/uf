// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// What a project needs before its routes can render as React Server Components.
//
// `@uniflowed/router` installs beside React 19.2.3, the React that Expo SDK 57
// and React Native 0.87 ship, and lists `react-server-dom-parcel` as an optional
// peer (ubugeeei-prod/uf#992). An application rendered from its modules
// (`app.rsc: false`) needs nothing more. One whose routes render as Server
// Components, which is the default, needs that package and the React it was
// released with. Without the package a build fails on an unresolved import
// somewhere inside the router, and on too old a React it fails later, inside a
// render, in terms of React's internals. So `uf:flow` asks here once, while Vite
// reads its configuration, and stops with one sentence that says what the
// project has and what to change.
//
// The router's own Server Components entries refuse the same React at run time
// (`npm/router/internal/react-version.js`). This is the earlier half of the
// same rule, for a project that builds with uf.

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** The oldest React, and `react-server-dom-parcel`, that Server Components render on. */
export const SERVER_COMPONENTS_REACT = "19.3.0";

/**
 * What stops the project at `root` from rendering React Server Components, as a
 * sentence, or `null` when nothing does.
 *
 * Both packages are resolved the way the router's own imports resolve them: from
 * the `@uniflowed/router` the project resolves, or from the one this package
 * depends on when the project names none. A prerelease tag is ignored, so a 19.3
 * canary counts as 19.3.
 *
 * @param {string} root
 * @returns {string | null}
 */
export function serverComponentsProblem(root) {
  const router =
    resolveFrom(path.join(root, "package.json"), "@uniflowed/router/package.json") ??
    resolveFrom(fileURLToPath(import.meta.url), "@uniflowed/router/package.json") ??
    path.join(root, "package.json");
  const react = versionOf(resolveFrom(router, "react/package.json"));
  const flight = versionOf(resolveFrom(router, "react-server-dom-parcel/package.json"));

  const found = [];
  if (react == null) {
    found.push("no react");
  } else if (!isRecentEnough(react)) {
    found.push(`React ${react}`);
  }
  if (flight == null) {
    found.push("no react-server-dom-parcel");
  } else if (!isRecentEnough(flight)) {
    found.push(`react-server-dom-parcel ${flight}`);
  }
  if (found.length === 0) {
    return null;
  }
  return (
    "uf: routes render as React Server Components unless `app.rsc` is false, and that needs " +
    `react-server-dom-parcel and React ${SERVER_COMPONENTS_REACT} or newer. This project has ` +
    `${found.join(" and ")}. Install react, react-dom and react-server-dom-parcel at ^19.3.0, ` +
    "or set `app.rsc: false` in uf.config.js to render routes from their modules, which the " +
    "router supports from React 19.2.3."
  );
}

/** Where `specifier` resolves from the file at `from`, or `null`. */
function resolveFrom(from, specifier) {
  try {
    return createRequire(from).resolve(specifier);
  } catch {
    return null;
  }
}

/** The `version` in the manifest at `file`, or `null`. */
function versionOf(file) {
  if (file == null) {
    return null;
  }
  try {
    const { version } = JSON.parse(fs.readFileSync(file, "utf8"));
    return typeof version === "string" ? version : null;
  } catch {
    return null;
  }
}

/** Whether `version` is at least `SERVER_COMPONENTS_REACT`, ignoring a prerelease tag. */
function isRecentEnough(version) {
  const found = /^(\d+)\.(\d+)\.(\d+)/.exec(version);
  if (found == null) {
    return false;
  }
  const needed = SERVER_COMPONENTS_REACT.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    const part = Number(found[index + 1]);
    if (part !== needed[index]) {
      return part > needed[index];
    }
  }
  return true;
}
