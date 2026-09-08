// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// Serving `./a11y-runtime.js` to the page `uf dev` renders.
//
// The same shape as `./refresh.js`, and deliberately so: a virtual module
// whose source is a file read from disk, and a tag added to every development
// document that imports it. Two mechanisms for "a module uf injects into the
// page" would be two things to keep working.
//
// What is different is the gate. Fast Refresh is always there; this is not.
// axe-core is an *optional* dependency — uf does not make a project that never
// audits carry a megabyte of rule definitions — so the injection is decided by
// asking the project's own `node_modules` whether the engine is installed. The
// question is answered on the server, once, before anything is injected, which
// is why a project without the engine gets no script, no failed import and no
// error in a console it did not open.

import { createRequire } from "node:module";
import path from "node:path";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { DIAGNOSTIC_ENDPOINT } from "./diagnostics.js";

/** Public URL the audit runtime is served from. */
export const AUDIT_PUBLIC_PATH = "/@uf-a11y";

/** The resolved id Vite hands back for it. */
export const AUDIT_RESOLVED_ID = "\0uf:a11y-audit";

const RUNTIME_SOURCE_PATH = fileURLToPath(new URL("./a11y-runtime.js", import.meta.url));

/**
 * How long the DOM has to stop moving before an audit runs, in milliseconds.
 *
 * A React render is hundreds of mutations and axe walks the whole subtree, so
 * without a window this would be a stutter rather than a background job. Long
 * enough to sit out a render and short enough that somebody who has just saved
 * a file hears about it while they are still looking at the page.
 */
export const SETTLE_MS = 750;

/**
 * Whether the project has axe-core.
 *
 * Resolved from the project root rather than from this package, because the
 * dependency is the *project's* — `@uniflowed/vite` declares it as an optional
 * peer, so it is installed beside the application and not beside the plugin.
 *
 * A failure to resolve is the ordinary answer, not an error: "no engine" is
 * what most projects will say, and it is the reason this function exists.
 */
export function auditAvailable(root) {
  try {
    createRequire(path.join(root, "package.json")).resolve("axe-core");
    return true;
  } catch {
    return false;
  }
}

/**
 * The audit module's source, with the project's settings written into its
 * last line.
 *
 * Generated rather than passed at runtime because the module is a *module*:
 * there is nowhere for a caller to hand it arguments, and a global would be a
 * second name to agree on. `JSON.stringify` of a value uf built from its own
 * config — never from anything a page said — is what goes in.
 */
export function auditRuntimeSource(settings) {
  const options = {
    endpoint: DIAGNOSTIC_ENDPOINT,
    settleMs: SETTLE_MS,
    axe: {
      tags: settings?.tags ?? [],
      disabledRules: settings?.disabledRules ?? [],
      minImpact: settings?.minImpact ?? null,
    },
  };
  return `${readFileSync(RUNTIME_SOURCE_PATH, "utf8")}\nstart(${JSON.stringify(options)});\n`;
}

/**
 * The tag that loads it, or `null` when this project has no engine.
 *
 * `injectTo: "body"` rather than the head: the audit reads the rendered tree,
 * so there is nothing for it to do until there is one, and a script in the
 * head would only sit through the same wait with the parser stopped behind it.
 */
export function auditTag(base, available) {
  if (!available) return null;
  return {
    tag: "script",
    attrs: { type: "module", src: `${base}${AUDIT_PUBLIC_PATH.slice(1)}` },
    injectTo: "body",
  };
}
