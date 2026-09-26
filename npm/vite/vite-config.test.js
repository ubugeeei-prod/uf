// @flow
//
// `@uniflowed/vite`'s configuration merge.
//
// This is the red line that matters most (`docs/red-lines.md`, line 2): uf
// must not re-declare an upstream tool's options, because every option it
// enumerates is one nobody can use until uf ships a release naming it. These
// tests are the guarantee that a project can reach Vite directly.

import { describe, expect, it } from "@uniflowed/test";
import { mergeConfig, withProjectConfig } from "@uniflowed/vite/merge";

const generated = () => ({
  root: "/project",
  configFile: false,
  mode: "development",
  server: { host: "127.0.0.1", port: 5173, fs: { allow: ["."] } },
  build: { outDir: "dist", manifest: true, sourcemap: true },
  plugins: ["uf:flow"],
});

describe("reaching Vite directly", () => {
  it("passes through an option uf has never heard of", () => {
    const merged = withProjectConfig(generated(), {
      server: { warmup: { clientFiles: ["./src/big.js"] } },
    });
    // The whole point: uf does not know what `warmup` is, and a project can
    // still use it without waiting for a uf release.
    expect(merged.server.warmup).toEqual({ clientFiles: ["./src/big.js"] });
  });

  it("lets a project override what uf chose", () => {
    const merged = withProjectConfig(generated(), { server: { port: 4000 } });
    // A default is a convenience, not an architecture.
    expect(merged.server.port).toBe(4000);
    expect(merged.server.host).toBe("127.0.0.1");
  });

  it("merges deeply rather than replacing a whole section", () => {
    const merged = withProjectConfig(generated(), { build: { target: "es2022" } });
    expect(merged.build).toEqual({
      outDir: "dist",
      manifest: true,
      sourcemap: true,
      target: "es2022",
    });
  });

  it("replaces an array rather than concatenating it", () => {
    const merged = withProjectConfig(generated(), {
      server: { fs: { allow: ["/elsewhere"] } },
    });
    // `fs.allow: ["/elsewhere"]` means that list, not that list plus uf's.
    expect(merged.server.fs.allow).toEqual(["/elsewhere"]);
  });

  it("keeps uf's plugins and adds the project's after them", () => {
    const merged = withProjectConfig(generated(), { plugins: ["svgr"] });
    // uf's first: the Flow transform has to see a module before anything that
    // expects JavaScript does. And a project cannot drop it by accident —
    // that would leave source that no longer compiles.
    expect(merged.plugins).toEqual(["uf:flow", "svgr"]);
  });

  it("keeps the project root and the config file uf resolved", () => {
    const merged = withProjectConfig(generated(), {
      root: "/somewhere-else",
      configFile: "./vite.config.ts",
    });
    // A `vite.config.ts` beside `uf.config.js` is two files disagreeing about
    // one project, and `root` is what uf resolved.
    expect(merged.root).toBe("/project");
    expect(merged.configFile).toBe(false);
  });

  it("changes nothing when a project sets nothing", () => {
    expect(withProjectConfig(generated(), undefined)).toEqual(generated());
  });
});

describe("mergeConfig", () => {
  it("ignores an explicit undefined rather than erasing a value", () => {
    expect(mergeConfig({ a: 1 }, { a: undefined })).toEqual({ a: 1 });
  });

  it("does not merge into a value that is not a plain object", () => {
    // A logger, a plugin or a URL is a value to replace, not a shape to walk.
    const logger = { info: () => {} };
    expect(mergeConfig({ customLogger: logger }, { customLogger: null })).toEqual({
      customLogger: null,
    });
  });
});
