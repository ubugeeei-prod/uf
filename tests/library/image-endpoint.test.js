// @flow
//
// Three spellings of one default, held to each other.
//
// `app.builtins.images.widths` and `.quality` default to the same numbers in
// three places, and each has a reason to be its own copy. `uf_assets` applies
// them when it reads the configuration, which is where `uf assets` gets them.
// `@uniflowed/server/image` applies them for the request-time endpoint, which
// reads the configuration as JavaScript and never sees the Rust default. And
// `@uniflowed/web`'s `internal/image-endpoint.js` carries them for an `Image`
// rendered outside a uf build, where nothing generates that module.
//
// A width one of them has and another does not is a `srcset` rung the
// endpoint refuses with a `400` — a broken image, in production, on the one
// image a project cared enough about to serve through the endpoint. So the
// three are compared here rather than trusted.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";
import { DEFAULT_QUALITY, DEFAULT_WIDTHS } from "@uniflowed/server/image";

import { IMAGE_ENDPOINT } from "../../packages/web/internal/image-endpoint.js";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/** A constant's value in `crates/uf_assets/src/config.rs`, as written. */
function rustConstant(name: string): string {
  const source = fs.readFileSync(path.join(REPO, "crates/uf_assets/src/config.rs"), "utf8");
  const line = source.split("\n").find((each) => each.startsWith(`pub const ${name}:`));
  if (line == null) throw new Error(`crates/uf_assets/src/config.rs has no ${name}`);
  return line
    .slice(line.indexOf("=") + 1)
    .replace(";", "")
    .trim();
}

describe("the image defaults", () => {
  it("are the same widths in Rust, in the endpoint and in Image", () => {
    const rust = rustConstant("DEFAULT_WIDTHS")
      .replace("&[", "")
      .replace("]", "")
      .split(",")
      .map((width) => Number(width.trim()));
    expect(DEFAULT_WIDTHS).toEqual(rust);
    expect(IMAGE_ENDPOINT.widths).toEqual(rust);
  });

  it("are the same quality in Rust, in the endpoint and in Image", () => {
    const rust = Number(rustConstant("DEFAULT_QUALITY"));
    expect(DEFAULT_QUALITY).toBe(rust);
    expect(IMAGE_ENDPOINT.quality).toBe(rust);
  });

  it("say there is no endpoint until a build says otherwise", () => {
    expect(IMAGE_ENDPOINT.path).toBe(null);
  });
});
