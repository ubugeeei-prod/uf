// @flow
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";
import { relayConfig, relayDependencies, transformRelay } from "./internal/relay.js";

describe("Relay's application-owned artifacts", () => {
  it("uses the application's config even when the process runs elsewhere", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-relay-"));
    try {
      fs.writeFileSync(
        path.join(root, "relay.config.json"),
        JSON.stringify({ artifactDirectory: "generated" }),
      );
      const options = await relayConfig(root);
      const filename = path.join(root, "app/view.js");
      const result = await transformRelay(
        'import { graphql } from "@uniflowed/relay"; export const query = graphql`query ViewerQuery { viewer { id } }`;',
        filename,
        null,
        options,
      );
      expect(result.code).toContain('"../generated/ViewerQuery.graphql"');
      expect(result.code).not.toContain("graphql`");
      expect(result.map.sourcesContent[0]).toContain("query ViewerQuery");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("preserves the directive of a client component", async () => {
    const result = await transformRelay(
      '"use client"; import { graphql } from "react-relay"; export const fragment = graphql`fragment Profile_user on User { id }`;',
      "/project/Profile.js",
      null,
      { eagerEsModules: true, artifactDirectory: null },
    );
    expect(result.code.startsWith('"use client";')).toBe(true);
    expect(result.code).toContain("./__generated__/Profile_user.graphql");
  });

  it("leaves another GraphQL library's tag alone", async () => {
    expect(
      await transformRelay(
        'import { graphql } from "another-client"; graphql`query Viewer { id }`;',
        "/project/view.js",
        null,
        {},
      ),
    ).toBe(null);
  });

  it("uses Relay's validation for anonymous queries", async () => {
    await expect(
      transformRelay(
        'import { graphql } from "react-relay"; graphql`{ viewer { id } }`;',
        "/project/view.js",
        null,
        { eagerEsModules: true },
      ),
    ).rejects.toThrow("must contain names");
  });

  it("does not require Relay in an application that does not install it", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-without-relay-"));
    try {
      fs.writeFileSync(path.join(root, "package.json"), "{}");
      expect(relayDependencies(root)).toEqual([]);
      expect(relayDependencies(root, true)).toEqual([]);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
