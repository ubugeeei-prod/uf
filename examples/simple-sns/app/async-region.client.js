"use client";
// @flow
import * as React from "@uniflowed/react";
import { Suspense, startTransition, useState } from "@uniflowed/react";
import { EmptyState, RetryButton, LoadingState } from "./ui.js";

type BoundaryProps = {|
  readonly children: React.MixedElement,
  readonly label: string,
  readonly onRetry: () => void,
|};
class RegionErrorBoundary extends React.Component<BoundaryProps, {| failed: boolean |}> {
  state: {| failed: boolean |} = { failed: false };
  static getDerivedStateFromError(): {| failed: boolean |} {
    return { failed: true };
  }
  render(): React.MixedElement {
    return this.state.failed ? (
      <EmptyState
        title={`Could not load ${this.props.label}`}
        action={<RetryButton onRetry={this.props.onRetry} />}
      >
        Please try again. Your other work is still available.
      </EmptyState>
    ) : (
      this.props.children
    );
  }
}
export hook useRetryableResource<T>(
  initial: Promise<T>,
  load: () => Promise<T>,
): {|
  readonly resource: Promise<T>,
  readonly generation: number,
  readonly retry: () => void,
|} {
  // Initial work is owned by the route loader. Only an explicit retry starts a new read.
  const [request, setRequest] = useState({ initial, resource: initial, generation: 0 });
  // A loader refresh hands over a new promise. Adopt it without starting work
  // in render, and discard an error/retry belonging to the previous response.
  if (request.initial !== initial) {
    setRequest({ initial, resource: initial, generation: request.generation + 1 });
  }
  function retry(): void {
    startTransition(() =>
      setRequest({ initial, resource: load(), generation: request.generation + 1 }),
    );
  }
  return { resource: request.resource, generation: request.generation, retry };
}
export component AsyncRegion(
  generation: number,
  label: string,
  retry: () => void,
  pending: renders LoadingState,
  ...{ children }: React.ElementConfig<typeof Suspense>
) {
  return (
    <RegionErrorBoundary key={generation} label={label} onRetry={retry}>
      <Suspense fallback={pending}>{children}</Suspense>
    </RegionErrorBoundary>
  );
}
