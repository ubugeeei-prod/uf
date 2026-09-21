"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { RelayEnvironmentProvider } from "@uniflowed/relay";

import { environment } from "./relay-environment.js";

/**
 * One browser environment for the mounted document. Server components stay outside Relay:
 * they preload, and each client island commits its own reference into this store.
 */

export component RelayRoot(children: React.Node) {
  const [client] = useState(() => environment("/graphql"));

  return <RelayEnvironmentProvider environment={client}>{children}</RelayEnvironmentProvider>;
}
