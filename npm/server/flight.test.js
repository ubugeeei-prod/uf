// @flow
//
// The payload URL, spelled twice.
//
// `@uniflowed/router` writes `<route>/__uf.flight` and every host reads it
// back, and the two cannot share a constant: the router depends on this
// package. So both spellings are imported here and held to one answer — the
// failure this exists to stop is a browser that fetches a URL no host
// recognises, which looks like navigation silently turning into full page
// loads. See ubugeeei-prod/uf#519.

import { describe, expect, it } from "@uniflowed/test";

import {
  FLIGHT_SEGMENT as ROUTER_SEGMENT,
  INTERCEPTED_FROM_HEADER as ROUTER_INTERCEPTED_FROM_HEADER,
  NOT_FOUND_HEADER as ROUTER_NOT_FOUND_HEADER,
  flightUrl,
} from "../router/internal/flight.js";
import {
  FLIGHT_SEGMENT,
  INTERCEPTED_FROM_HEADER,
  NOT_FOUND_HEADER,
  flightDocumentPath,
} from "./internal/flight.js";

describe("the payload URL", () => {
  it("is the same segment in the router and in the server", () => {
    expect(FLIGHT_SEGMENT).toBe(ROUTER_SEGMENT);
  });

  it("uses the same intercepted-from header in the router and in the server", () => {
    expect(INTERCEPTED_FROM_HEADER).toBe(ROUTER_INTERCEPTED_FROM_HEADER);
  });

  it("uses the same not-found header in the router and in the server", () => {
    expect(NOT_FOUND_HEADER).toBe(ROUTER_NOT_FOUND_HEADER);
  });

  it("leads every URL the router writes back to the document it is for", () => {
    for (const pathname of ["/", "/guide", "/guide/rendering", "/a/b/c"]) {
      const written = new URL(flightUrl(pathname), "http://uf.test").pathname;
      expect(flightDocumentPath(written)).toBe(pathname);
    }
  });

  it("is the same segment in the Vite plugin, which answers it under uf dev", async () => {
    // Plain JavaScript that Vite imports before any Flow transform exists, so it
    // spells the segment a third time.
    const vite = await import("../vite/internal/flight.js");
    expect(vite.FLIGHT_SEGMENT).toBe(FLIGHT_SEGMENT);
    expect(vite.INTERCEPTED_FROM_HEADER).toBe(INTERCEPTED_FROM_HEADER);
    expect(vite.NOT_FOUND_HEADER).toBe(NOT_FOUND_HEADER);
    for (const pathname of ["/", "/guide", "/guide/rendering"]) {
      const written = new URL(flightUrl(pathname), "http://uf.test").pathname;
      expect(vite.flightDocumentPath(written)).toBe(pathname);
    }
  });

  it("is not recognised in a path that only resembles it", () => {
    expect(flightDocumentPath("/guide")).toBe(null);
    expect(flightDocumentPath("/guide/__uf.flightx")).toBe(null);
    expect(flightDocumentPath("/guide__uf.flight")).toBe(null);
  });
});
