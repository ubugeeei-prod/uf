// @flow
//
// The URL `uf dev` names a client module by, and the file a server reads back
// from it.
//
// Two halves of one convention, Vite's: a file inside the project is served at
// its project-relative path, and a file outside it — a workspace package — at
// `/@fs/` followed by its path. A client reference carries the URL to the
// browser, and the ssr graph turns the same URL back into the file to render a
// client component with, so the two halves have to agree on every platform.
// See ubugeeei-prod/uf#519.

import { describe, expect, it } from "@uniflowed/test";

import { devUrlOf, fsFileOf } from "./internal/flight.js";

describe("a development URL for a client module", () => {
  it("is the project-relative path for a file inside the project", () => {
    expect(devUrlOf("/repo/docs", "/", "/repo/docs/app/counter.js")).toBe("/app/counter.js");
  });

  it("is Vite's /@fs/ URL for a file outside it, and reads back to the file", () => {
    const url = devUrlOf("/repo/docs", "/", "/repo/npm/ui/button.js");
    expect(url).toBe("/@fs/repo/npm/ui/button.js");
    expect(fsFileOf(url)).toBe("/repo/npm/ui/button.js");
  });

  it("reads a Windows path back from its drive letter, with no slash in front of it", () => {
    // What `devUrlOf` writes on Windows, where a file outside the project is
    // `C:\work\button.js` and Vite serves it at `/@fs/C:/work/button.js`.
    expect(fsFileOf("/@fs/C:/work/button.js")).toBe("C:/work/button.js");
  });
});
