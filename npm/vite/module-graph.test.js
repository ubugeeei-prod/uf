// @noflow
//
// The module graph `uf build --analyze` reads, recorded from a bundle shaped
// the way Vite hands one to `generateBundle`. What uf makes of it is tested in
// `crates/uf_bundle/src/analysis/tests.rs`, and a real build of both halves in
// `crates/uf_cli/tests/vite.rs`.

import { describe, expect, it } from "@uniflowed/test";

import { createModuleGraphCollector, moduleId } from "./internal/module-graph.js";

const ROOT = "/project";

describe("moduleId", () => {
  it("spells a project file relative, a virtual module bare, and keeps a query", () => {
    expect(moduleId(ROOT, "/project/app/$page.js")).toBe("app/$page.js");
    expect(moduleId(ROOT, "\0virtual:uf/client")).toBe("virtual:uf/client");
    expect(moduleId(ROOT, "/project/app/style.css?direct")).toBe("app/style.css?direct");
    expect(moduleId(ROOT, "/npm/react/index.js")).toBe("../npm/react/index.js");
  });
});

describe("createModuleGraphCollector", () => {
  it("records a bundle's graph, with a client reference kept out of its entries", () => {
    const collector = createModuleGraphCollector(ROOT, {
      isReference: (file) => file === "/project/app/Plot.js",
    });
    const infos = {
      "\0virtual:uf/client": {
        importedIds: ["/project/app/runtime.js"],
        dynamicallyImportedIds: ["/project/app/$page.js"],
      },
      "/project/app/runtime.js": { importedIds: [], dynamicallyImportedIds: [] },
      "/project/app/Plot.js": { importedIds: [], dynamicallyImportedIds: [] },
    };
    const context = {
      environment: { name: "client" },
      getModuleIds: () => Object.keys(infos),
      getModuleInfo: (id) => infos[id] ?? null,
    };
    const bundle = {
      "assets/plot.js": {
        type: "chunk",
        fileName: "assets/plot.js",
        isEntry: true,
        facadeModuleId: "/project/app/Plot.js",
        modules: { "/project/app/Plot.js": { code: "export function Plot() {}" } },
      },
      "assets/client.js": {
        type: "chunk",
        fileName: "assets/client.js",
        isEntry: true,
        facadeModuleId: "\0virtual:uf/client",
        modules: {
          "\0virtual:uf/client": { code: "import './runtime.js';" },
          "/project/app/runtime.js": { code: "export const ready = true;" },
          "/project/app/unused.js": { code: null },
        },
      },
      "assets/style.css": { type: "asset", fileName: "assets/style.css" },
    };

    collector.plugin.generateBundle.call(context, {}, bundle);

    const { version, builds } = collector.graph();
    expect(version).toBe(1);
    expect(builds.length).toBe(1);
    const [build] = builds;
    expect(build.environment).toBe("client");
    expect(build.entries).toEqual(["virtual:uf/client"]);
    expect(build.modules.find((module) => module.id === "virtual:uf/client")).toEqual({
      id: "virtual:uf/client",
      imports: ["app/runtime.js"],
      dynamicImports: ["app/$page.js"],
    });
    expect(build.chunks.map((chunk) => chunk.file)).toEqual(["assets/client.js", "assets/plot.js"]);
    expect(build.chunks[0].modules.map((module) => module.id)).toEqual([
      "virtual:uf/client",
      "app/runtime.js",
    ]);
    expect(build.chunks[1].facade).toBe("app/Plot.js");
  });
});
