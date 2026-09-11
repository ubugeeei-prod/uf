"use client";
// @flow

import * as React from "@uniflowed/react";
import { Suspense, startTransition, use, useState } from "@uniflowed/react";
import { promise, runPromiseExit } from "@uniflowed/effect";

import { EmptyState, RetryButton, LoadingState } from "./ui.js";

/** A settled read carries data or a retryable failure, never a thrown transport error. */
type ResourceResult<out T> =
  | {| readonly kind: "ready", readonly value: T |}
  | {| readonly kind: "failed" |};

/** Run a read adapter without exposing its transport error to the rendered page. */
async function settle<T>(read: () => Promise<T>): Promise<ResourceResult<T>> {
  const result = await runPromiseExit(promise(read));

  return match (result) {
    {kind: "success", value: const value} => { kind: "ready", value },
    {kind: "failure", ...} => { kind: "failed" },
  };
}

/**
 * Own the settled promise for one loader response and its explicit retries.
 * Adapting a promise performs no I/O: the loader starts the initial read, and
 * only `retry` calls `load`. A refreshed loader replaces an older retry while
 * state in sibling components, such as an unfinished composition, survives.
 */
export hook useRetryableResource<T>(
  initial: Promise<T>,
  load: () => Promise<T>,
): {|
  readonly resource: Promise<ResourceResult<T>>,
  readonly retry: () => void,
|} {
  const [request, setRequest] = useState(() => ({ initial, resource: settle(() => initial) }));

  if (request.initial !== initial) {
    setRequest({ initial, resource: settle(() => initial) });
  }

  function retry(): void {
    startTransition(() => {
      setRequest({ initial, resource: settle(load) });
    });
  }

  return { resource: request.resource, retry };
}

/** Read beneath Suspense; a failed request renders data-driven recovery in this region. */
component SettledRegion<T>(
  resource: Promise<ResourceResult<T>>,
  label: string,
  retry: () => void,
  children: (T) => React.MixedElement,
) {
  return match (use(resource)) {
    {kind: "ready", value: const value} => children(value),
    {kind: "failed"} =>
      <EmptyState title={`Could not load ${label}`} action={<RetryButton onRetry={retry} />}>
        Please try again. Your other work is still available.
      </EmptyState>,
  };
}

/**
 * Reveal one independently loaded region. Suspense owns only pending work;
 * settled failures use a functional component and an explicit retry action.
 * Unexpected render defects remain the responsibility of the route's $error.js.
 */
export component AsyncRegion<T>(
  resource: Promise<ResourceResult<T>>,
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
