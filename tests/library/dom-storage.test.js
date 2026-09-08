// @flow
//
// How `@uniflowed/react-testing` asks the host about `localStorage`.
//
// Installing a document has to decide whether the host's storage works, because
// Node 25 defines `globalThis.localStorage` and leaves it without a `setItem`
// unless the process was started with `--localstorage-file`. Asking used to mean
// reading the property — and on Node 25 that property is an own accessor whose
// getter prints
//
//     (node:62028) Warning: `--localstorage-file` was provided without a valid path
//     (Use `node --trace-warnings ...` to show where the warning was created)
//
// once per worker that rendered anything, into the `output` section of a
// `uf test` report that is otherwise composed line by line. A reader's first
// component test looked like something had gone wrong. See
// ubugeeei-prod/uf#308.
//
// # Why this drives processes of its own
//
// `installDom` installs once per process and remembers that it has, and a
// worker runs more than one file — so a case that defined a global at module
// scope would be defining it *after* whichever file ran before it had already
// installed a document, and would then measure nothing. A process with no
// document in it is the only place the question can be asked.
//
// # Why there are two of them
//
// They check the two halves, and each is vacuous where the other is not.
//
// `PLAIN` renders and nothing else, and what is read is its **stderr**: on
// Node 25 that is the paragraph #308 is about, asserted where it appears rather
// than described. On Node 24 there is no `localStorage` to warn about, so it
// says nothing either way.
//
// `COUNTING` installs an accessor of its own in Node 25's shape and counts the
// reads, which is the *cause* and is checkable on every runtime — including the
// Node 24 that CI runs. It cannot be the one that reads stderr: its own accessor
// replaces the one Node warns from, so a warning could not appear in it however
// broken the code under test was.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

/** A package's entry point, as a URL a module outside this tree can import. */
function entry(name: string): string {
  return pathToFileURL(path.join(repository, "packages", name, "index.js")).href;
}

/**
 * What both probes do: render a component, then use the storage that was left.
 *
 * By file URL rather than by name, and `createElement` rather than JSX, because
 * the temporary directory a probe is written to has no `node_modules` to
 * resolve a bare `@uniflowed/…` specifier from.
 *
 * The import is dynamic so that a probe with something to set up can set it up
 * first: a static `import` is evaluated before any statement in the module body,
 * and what the counting probe asks is what `installDom` does with a global that
 * is already there.
 */
function render(report: string): string {
  return `Promise.all([
  import("${entry("react")}"),
  import("${entry("react-testing")}"),
]).then(([react, testing]) => {
  testing.render(react.createElement("p", null, "stored"));
${report}
});
`;
}

/** Renders, and is read for what its process wrote to stderr. */
const PLAIN = render(`  globalThis.localStorage.setItem("uf", "308");
  process.stdout.write(
    JSON.stringify({
      setItem: typeof globalThis.localStorage.setItem,
      readBack: globalThis.localStorage.getItem("uf"),
    }) + "\\n",
  );`);

/**
 * Renders with Node 25's `localStorage` in place, and counts what read it.
 *
 * An own get/set pair whose getter hands back an object that is not a Storage.
 * On the real Node it also prints a warning every time it is called; here it
 * counts, which is the same event without the runtime.
 */
const COUNTING = `let reads = 0;
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  enumerable: true,
  get() {
    reads += 1;
    return {};
  },
  set(value) {},
});

${render(`  // Captured before this module touches storage itself: what is counted is
  // what installing a document read, not what asking about it afterwards does.
  const duringInstall = reads;
  globalThis.localStorage.setItem("uf", "308");
  process.stdout.write(JSON.stringify({ duringInstall }) + "\\n");`)}`;

/**
 * How this host starts a module that imports Flow, mirroring
 * `HostCommand::with_flow_loader`.
 *
 * Deno has no loader in `@uniflowed/host` yet, so it cannot run this at all; a
 * named failure is better than a skip that reads like a pass.
 */
function loaderArguments(): Array<string> {
  const host = path.basename(process.execPath);
  if (host.startsWith("node")) {
    return ["--import", pathToFileURL(path.join(repository, "packages/host/register.js")).href];
  }
  if (host.startsWith("bun")) {
    return ["--preload", path.join(repository, "packages/host/bun-preload.js")];
  }
  throw new Error(`no Flow loader for ${host}: this test drives a process uf would have started`);
}

/** What a probe reported, and everything its process wrote to stderr. */
type Report = {
  duringInstall?: number,
  setItem?: string,
  readBack?: string | null,
  stderr: string,
};

function runProbe(source: string): Promise<Report> {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-dom-storage-"));
  const file = path.join(directory, "probe.js");
  fs.writeFileSync(file, source);

  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [...loaderArguments(), file], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let written = "";
    let errors = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      errors += String(chunk);
    });
    child.on("error", reject);
    child.on("close", (code) => {
      fs.rmSync(directory, { recursive: true, force: true });
      if (code !== 0) {
        reject(new Error(`the probe exited ${String(code)}: ${errors}`));
        return;
      }
      try {
        resolve({ ...JSON.parse(written), stderr: errors });
      } catch (error) {
        reject(new Error(`the probe wrote something that is not a report: ${String(error)}`));
      }
    });
  });
}

const runs: { [string]: Promise<Report> } = {};

/** One run per probe, started on first use and shared by the cases. */
function probe(name: string, source: string): Promise<Report> {
  const started = runs[name] ?? runProbe(source);
  runs[name] = started;
  return started;
}

/** How long one of these cases may take, process start and all. */
const PROBE_BUDGET = { timeout: 30000 };

describe("installing a document beside the host's localStorage", () => {
  it(
    "prints no `--localstorage-file` warning",
    async () => {
      expect((await probe("plain", PLAIN)).stderr).not.toContain("localstorage-file");
    },
    PROBE_BUDGET,
  );

  it(
    "decides without invoking the accessor",
    async () => {
      // The cause, and the half that is checkable on every runtime: reading the
      // property is what prints the warning above, so the fix is to read the
      // descriptor and never call the getter.
      expect((await probe("counting", COUNTING)).duringInstall).toBe(0);
    },
    PROBE_BUDGET,
  );

  it(
    "leaves a localStorage a test can use",
    async () => {
      // The answer is unchanged — the host's is still rejected and the
      // document's is still installed. Only the way the question was asked
      // changed, and this is the half that must not have moved with it.
      const seen = await probe("plain", PLAIN);
      expect(seen.setItem).toBe("function");
      expect(seen.readBack).toBe("308");
    },
    PROBE_BUDGET,
  );
});
