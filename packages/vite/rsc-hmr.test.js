// @flow
// A server-component edit has no browser module to hot replace. The RSC
// environment must tell the client environment to reload the document.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, expect, it } from "@uniflowed/test";

import uniflowed from "./index.js";

const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-rsc-hmr-"));
const closers: Array<() => void> = [];

afterAll(() => {
  for (const close of closers) close();
  fs.rmSync(root, { recursive: true, force: true });
});

it("reloads the client after a server-component edit in the RSC graph", () => {
  fs.mkdirSync(path.join(root, "app"), { recursive: true });
  fs.writeFileSync(path.join(root, "app/$page.js"), "export default function Page() {}\n");
  const plugin: $FlowFixMe = uniflowed({ root, config: {} })[0];
  plugin.configResolved({ root, base: "/", command: "serve", isProduction: false });

  const sent = [];
  plugin.configureServer({
    middlewares: { use() {} },
    httpServer: {
      once(event, close) {
        if (event === "close") closers.push(close);
      },
    },
    watcher: { on() {} },
    environments: {
      client: {
        hot: {
          send(message) {
            sent.push(message);
          },
        },
      },
    },
  });

  const serverComponent = path.join(root, "app/$page.js");
  plugin.hotUpdate.call({ environment: { name: "rsc" } }, { modules: [{ file: serverComponent }] });
  expect(sent).toEqual([{ type: "full-reload", path: "*" }]);

  // Only the server graph asks for this reload. The client graph keeps Fast
  // Refresh responsible for its own edits.
  plugin.hotUpdate.call(
    { environment: { name: "client" } },
    { modules: [{ file: serverComponent }] },
  );
  expect(sent).toHaveLength(1);
});
