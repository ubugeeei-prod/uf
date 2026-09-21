// @flow

import * as React from "react";
import { Suspense, use } from "react";

import type { Settled } from "./client.js";

import { Button, EmptyState, LoadingState } from "./ui.native.js";

/** Read beneath Suspense; a failed read renders its own recovery inside this region. */

component SettledRegion<T>(
  resource: Promise<Settled<T>>,
  label: string,
  retry: () => void,
  children: (T) => React.MixedElement,
) {
  return match (use(resource)) {
    {kind: "ready", value: const value} => children(value),
    {kind: "failed"} =>
      <EmptyState
        title={`Could not load ${label}`}
        action={
          <Button onPress={retry} primary={false}>
            Try again
          </Button>
        }
      >
        Your other work is still available.
      </EmptyState>,
  };
}

/**
 * Reveal one independently loaded region, as the web examples' `AsyncRegion` does. Suspense owns
 * only the wait; a settled failure is data, matched like any other, with an explicit retry.
 */

export component AsyncRegion<T>(
  resource: Promise<Settled<T>>,
  label: string,
  retry: () => void,
  pending: renders LoadingState,
  children: (T) => React.MixedElement,
) {
  return (
    <Suspense fallback={pending}>
      <SettledRegion resource={resource} label={label} retry={retry}>
        {children}
      </SettledRegion>
    </Suspense>
  );
}
