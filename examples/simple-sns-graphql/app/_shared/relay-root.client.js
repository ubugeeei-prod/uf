"use client";
// @flow
import * as React from "@uniflowed/react";
import { Suspense, useState } from "@uniflowed/react";
import { RelayEnvironmentProvider } from "@uniflowed/relay";
import { environment } from "./relay-environment.js";
import { LoadingState } from "./ui.js";

/** One environment for this request's tree. Each route owns its query and fragments. */
export component RelayRoot(children: React.Node) {
  const [client] = useState(() => environment("/graphql"));
  return (
    <RelayEnvironmentProvider environment={client}>
      <Suspense fallback={<LoadingState kind="feed" />}>{children}</Suspense>
    </RelayEnvironmentProvider>
  );
}
