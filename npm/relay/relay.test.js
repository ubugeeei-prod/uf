// @flow
//
// `@uniflowed/relay`: Relay, re-exported by name.
//
// There is nothing of uf's to test in the library itself — it is Meta's, and
// Relay's own suite is the one that matters. What is worth holding is that this
// package *is* that library rather than a declaration of it, and that the
// surface uf documents is the surface it actually re-exports. A name that
// silently disappeared from `react-relay` would otherwise be an `undefined`
// an application discovered at runtime.

import { describe, expect, it } from "@uniflowed/test";
import * as uf from "@uniflowed/relay";
import * as ReactRelay from "react-relay";
import { createServerEnvironment } from "@uniflowed/relay/rsc_EXPERIMENTAL";
import { createServerEnvironment as upstreamServerEnvironment } from "react-relay/rsc_EXPERIMENTAL.js";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { useQueryFromServer as upstreamQueryFromServer } from "react-relay/rsc-client_EXPERIMENTAL.js";

// Both namespaces read by name. A module namespace cannot be indexed by a
// string, and the questions below are about names read from lists, so each is
// viewed as the record of bindings it is.
const ours: { readonly [string]: mixed } = uf;
const upstream: { readonly [string]: mixed } = ReactRelay;

/** Every binding uf's registry promises for this specifier. */
const DOCUMENTED = [
  "graphql",
  "useFragment",
  "useLazyLoadQuery",
  "usePreloadedQuery",
  "useMutation",
  "commitMutation",
  "RelayEnvironmentProvider",
  "loadQuery",
];

describe("@uniflowed/relay", () => {
  it("preserves the upstream experimental RSC protocol", () => {
    expect(createServerEnvironment).toBe(upstreamServerEnvironment);
    expect(useQueryFromServer).toBe(upstreamQueryFromServer);
  });

  it("exposes every upstream runtime binding through the browser-safe barrel", () => {
    for (const name of Object.keys(upstream)) {
      if (name !== "default" && name !== "module.exports") expect(ours[name]).toBe(upstream[name]);
    }
  });
  it("re-exports the real Relay, not a declaration of it", () => {
    // Identity, not shape: `uf.useFragment` has to be the function Relay
    // exports, or an application is holding two Relays.
    for (const name of DOCUMENTED) {
      expect(ours[name]).toBe(upstream[name]);
    }
  });

  it("exports every binding uf documents", () => {
    for (const name of DOCUMENTED) {
      expect(typeof ours[name]).not.toBe("undefined");
    }
  });

  it("keeps the container API for applications still migrating", () => {
    // Not recommended for new code, and not removed: uf does not get to decide
    // when somebody else's migration is over.
    for (const name of [
      "createFragmentContainer",
      "createPaginationContainer",
      "createRefetchContainer",
    ]) {
      expect(ours[name]).toBe(upstream[name]);
    }
  });

  it("offers the whole namespace as well as the named exports", () => {
    expect(uf.ReactRelay.useFragment).toBe(ReactRelay.useFragment);
  });
});
