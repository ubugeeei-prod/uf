// @flow
//
// What of a route's error may cross into a Flight payload.
//
// React's production rule for an `Error` prop is that the digest crosses and
// the message and stack do not. A loader that throws something that is not an
// `Error` — a string, an API client's plain `{ message, details }` object —
// has no such rule, and would be serialised into the payload as the data it
// is. `crossableRouteError` wraps it so the rule applies.

import { describe, expect, it } from "@uniflowed/test";

import { crossableRouteError } from "./internal/flight.js";

describe("a route's error on its way into a payload", () => {
  it("wraps a thrown value that is not an Error, so none of it crosses as data", () => {
    const thrown = { message: "relation users", details: "SELECT secret FROM users", hint: "x" };
    const crossing = crossableRouteError({ kind: "thrown", error: thrown });
    expect(crossing?.kind).toBe("thrown");
    const error = crossing?.kind === "thrown" ? crossing.error : null;
    expect(error instanceof Error).toBe(true);
    // Nothing of the object's fields rides along as a property React would
    // serialise; what is left is a message, which production drops.
    expect(Object.keys(error ?? {})).toEqual([]);

    const fromString = crossableRouteError({ kind: "thrown", error: "token=abc" });
    expect(fromString?.kind === "thrown" ? fromString.error instanceof Error : false).toBe(true);
  });

  it("survives a value whose toString throws", () => {
    const hostile = {
      toString() {
        throw new Error("no");
      },
    };
    const crossing = crossableRouteError({ kind: "thrown", error: hostile });
    expect(crossing?.kind === "thrown" ? crossing.error instanceof Error : false).toBe(true);
  });

  it("leaves an Error, an unauthorized, a forbidden and no error as they were", () => {
    const error = new Error("already an Error");
    const thrown = { kind: "thrown", error };
    expect(crossableRouteError(thrown)).toBe(thrown);
    const unauthorized = { kind: "unauthorized" };
    expect(crossableRouteError(unauthorized)).toBe(unauthorized);
    const forbidden = { kind: "forbidden" };
    expect(crossableRouteError(forbidden)).toBe(forbidden);
    expect(crossableRouteError(null)).toBe(null);
  });
});
