// @flow
//
// What `@uniflowed/vite` asks of a project before its routes render as React
// Server Components, and what it leaves out of a project whose routes do not.
//
// The router installs beside React 19.2.3 and lists `react-server-dom-parcel`
// as an optional peer (ubugeeei-prod/uf#992). So the two kinds of application
// start from different router entries, and only one kind's entries name React's
// Flight package. The kind that renders Server Components is told what it is
// missing while Vite reads its configuration, before anything is built.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import {
  builtBridgeSource,
  devBridgeSource,
  flightClientSource,
  flightServerSource,
  rscEntrySource,
} from "./internal/flight.js";
import { clientModuleSource, serverModuleSource } from "./internal/routes.js";
import { serverComponentsProblem } from "./internal/server-components.js";

const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/**
 * A project whose `node_modules` holds `@uniflowed/router` and a manifest for
 * each package named, at the version given.
 *
 * Manifests and nothing else, because a version is all the check reads.
 */
function installedIn(versions: { readonly [string]: string }): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-rsc-requirements-"));
  roots.push(root);
  fs.writeFileSync(path.join(root, "package.json"), "{}\n");
  for (const name of ["@uniflowed/router", ...Object.keys(versions)]) {
    const directory = path.join(root, "node_modules", name);
    fs.mkdirSync(directory, { recursive: true });
    const version = name in versions ? versions[name] : "0.0.0-test";
    fs.writeFileSync(path.join(directory, "package.json"), JSON.stringify({ name, version }));
  }
  return root;
}

describe("the router entry an application starts from", () => {
  it("names no Server Components entry when routes render from their modules", () => {
    const sources = [
      clientModuleSource("/project/app.js", { mount: "hydrate" }),
      clientModuleSource("/project/app.js", { mount: "render" }),
      serverModuleSource("/project/app.js"),
    ];

    for (const source of sources) {
      expect(source).not.toContain("@uniflowed/router/rsc");
      expect(source).not.toContain("react-server-dom-parcel");
    }
  });

  it("is a Server Components entry when routes render as Server Components", () => {
    const client = flightClientSource("/project/app.js");
    const server = flightServerSource("/project/app.js", "virtual:uf/routes");

    expect(client).toContain('import { hydrateFlight } from "@uniflowed/router/rsc/client";');
    expect(server).toContain('import { createDocumentRenderer } from "@uniflowed/router/rsc/ssr";');
  });

  it("builds the action endpoint in the rsc graph, beside the pages", () => {
    // An action and the page that shows what it wrote must run the same
    // instance of the modules they share. Building the endpoint in the ssr
    // graph gave the action a copy of its own (ubugeeei-prod/uf#1469).
    const rsc = rscEntrySource("virtual:uf/routes", {}, null, "virtual:uf/actions");
    expect(rsc).toContain('import { actions } from "virtual:uf/actions";');
    expect(rsc).toContain("export const callAction = createActionDispatcher({ actions });");

    const server = flightServerSource("/project/app.js", "virtual:uf/routes");
    expect(server).not.toContain("virtual:uf/actions");
    expect(server).not.toContain("createActionDispatcher");
    expect(server).toContain("callAction as callActionInRsc");

    expect(builtBridgeSource("/out/rsc/index.js")).toContain("callAction");
    expect(devBridgeSource()).toContain("(await load()).callAction(request, settings)");
  });
});

describe("what a project needs before its routes render as Server Components", () => {
  it("is nothing more than React 19.3 and react-server-dom-parcel", () => {
    const ready = installedIn({ react: "19.3.0", "react-server-dom-parcel": "19.3.0" });

    expect(serverComponentsProblem(ready)).toBe(null);
    // And this workspace, whose documentation site renders them.
    expect(serverComponentsProblem(workspace)).toBe(null);
  });

  it("names the React the project has and the package it lacks", () => {
    const problem = serverComponentsProblem(installedIn({ react: "19.2.3" }));

    expect(problem).toContain("This project has React 19.2.3 and no react-server-dom-parcel.");
    expect(problem).toContain("React 19.3.0 or newer");
    expect(problem).toContain("`app.rsc: false`");
  });

  it("names only what is wrong", () => {
    expect(serverComponentsProblem(installedIn({ react: "19.3.0" }))).toContain(
      "This project has no react-server-dom-parcel.",
    );
    expect(
      serverComponentsProblem(
        installedIn({ react: "19.2.3", "react-server-dom-parcel": "19.3.0" }),
      ),
    ).toContain("This project has React 19.2.3.");
  });
});
