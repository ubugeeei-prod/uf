// @flow
//
// What `uf build --adapter edge` says about a Node built-in a Worker does not
// provide. The table itself is checked against a real workerd by
// `tools/ci/edge-worker-smoke.sh`; this is the sentence built from it.

import { describe, expect, it } from "@uniflowed/test";

import {
  MAX_NAMED,
  UNAVAILABLE_ON_WORKERS,
  survivingImports,
  unavailableOnWorkers,
  workerBuiltinWarnings,
} from "./internal/worker-builtins.js";

const ROOT = "/work/app";

describe("the Node built-ins a Worker does not provide", () => {
  it("knows a module by any spelling an import can use", () => {
    for (const specifier of ["node:fs", "fs", "node:fs/promises", "fs/promises"]) {
      expect(unavailableOnWorkers(specifier)?.module).toBe("fs");
    }
    expect(unavailableOnWorkers("node:async_hooks")).toBe(null);
    expect(unavailableOnWorkers("./fs.js")).toBe(null);
    expect(unavailableOnWorkers("node:net")).toBe(null);
  });

  it("names the module, the file that reached it, and the date", () => {
    const warnings = workerBuiltinWarnings(
      [{ specifier: "node:fs", importer: `${ROOT}/app/api/files/$route.js` }],
      ROOT,
    );

    expect(warnings.length).toBe(1);
    expect(warnings[0]).toContain("app/api/files/$route.js imports node:fs");
    expect(warnings[0]).toContain("2024-09-23");
    expect(warnings[0]).toContain("answer 500");
  });

  it("names the project's own code before a dependency's, and a dependency by its name", () => {
    const warnings = workerBuiltinWarnings(
      [
        { specifier: "node:http", importer: `${ROOT}/node_modules/@scope/agent/index.js` },
        { specifier: "child_process", importer: `${ROOT}/app/api/run/$route.js` },
      ],
      ROOT,
    );

    expect(warnings[0]).toContain("app/api/run/$route.js imports node:child_process");
    expect(warnings[1]).toContain("the dependency @scope/agent imports node:http");
  });

  it("says a module once per importer, and counts past its bound", () => {
    const reached = [];
    for (let at = 0; at < MAX_NAMED + 3; at += 1) {
      reached.push({ specifier: "node:fs", importer: `${ROOT}/app/${at}.js` });
      reached.push({ specifier: "node:fs/promises", importer: `${ROOT}/app/${at}.js` });
    }

    const warnings = workerBuiltinWarnings(reached, ROOT);

    expect(warnings.length).toBe(MAX_NAMED + 1);
    expect(warnings[MAX_NAMED]).toContain("and 3 more");
  });

  it("counts an import only when the importer rendered code into a chunk that still imports the module", () => {
    const route = `${ROOT}/app/api/files/$route.js`;
    const shaken = `${ROOT}/node_modules/@uniflowed/test/internal/snapshot.js`;
    const reached = [
      { specifier: "node:fs", importer: route },
      { specifier: "node:fs", importer: shaken },
      { specifier: "node:child_process", importer: route },
    ];
    const chunks = [
      {
        imports: ["node:fs", "node:async_hooks"],
        modules: { [route]: { renderedLength: 120 }, [shaken]: { renderedLength: 0 } },
      },
    ];

    const surviving = survivingImports(reached, chunks);

    // The route still imports `node:fs` in the output; the shaken module
    // rendered nothing, and no chunk imports `node:child_process` any more.
    expect(surviving).toEqual([{ specifier: "node:fs", importer: route }]);
  });

  it("carries a call for every module, so the smoke can prove each is a stub", () => {
    for (const entry of UNAVAILABLE_ON_WORKERS) {
      expect(typeof entry.member).toBe("string");
      expect(Array.isArray(entry.args)).toBe(true);
    }
  });
});
