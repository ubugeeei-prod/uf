// @flow
//
// `@uniflowed/hooks/async`: running a promise from a component.
//
// Two bugs a hand-written version has, and only one of them is a warning:
// setting state after the component has gone, and a slow first request
// overwriting a fast second one. The second is the dangerous one — it puts a
// wrong answer on screen and nothing says so.
//
// Both are fixed by the effect's own cleanup rather than by a ref: the effect
// that started a request is the thing that knows it has been superseded,
// because React runs its cleanup before running it again. That is the shape
// React's own documentation uses, and it means there is no "latest" anything
// to keep in a ref and no generation counter to keep in step.
//
// # What belongs in this module
//
// A hook that starts one call and holds its pending, resolved and failed
// states. One file for one hook, because the subject is neither the
// component's life nor a timer nor a DOM node, and hiding it inside one of
// those would make all three harder to name.
//
// # Where the line with `@uniflowed/query` is, exactly
//
// Not here, and not in this package at all: caching, deduplication between
// components, invalidation, stale-while-revalidate. Those are
// `@uniflowed/query`'s, and the boundary is what keeps `useAsync` small enough
// to read in one sitting. When a caller needs a cache they should change
// packages, not discover that this hook grew one.
//
// Cancellation and retry are on this side of that line, and the reason is that
// neither of them needs a cache to mean anything. A component that unmounts
// while a request is in flight should stop the request, not merely ignore it —
// the connection is the cost, and `AbortSignal` is how the platform says so.
// A call that failed once on a flaky connection should be able to try again
// without the caller writing a loop that has to know about the cleanup flag.
// What query owns is the *shared* version of both: one backoff across every
// component watching a key, cancellation that has to decide whether another
// observer still wants the answer. Nothing here is shared, so nothing here has
// to decide that.

import { useCallback, useEffect, useState } from "@uniflowed/react";

import { useStableCallback } from "./lifecycle.js";

/** What an in-flight, settled or failed call looks like. */
export type Async<T> = {|
  readonly value: T | null,
  readonly error: Error | null,
  readonly pending: boolean,
  /** Run it again, keeping whatever is on screen until the new value lands. */
  readonly reload: () => void,
|};

/** How hard to try. */
export type AsyncOptions = {|
  /** How many times to try *again* after a failure. Zero, by default. */
  readonly retry?: number,
  /**
   * How long to wait before attempt `attempt` (counting from zero).
   *
   * Exponential with a ceiling, by default. A fixed delay is `() => 200`.
   */
  readonly retryDelay?: (attempt: number) => number,
|};

/** Exponential backoff, capped so a long-lived page does not wait for minutes. */
function backoff(attempt: number): number {
  return Math.min(200 * 2 ** attempt, 5_000);
}

/**
 * Call `body` when `deps` change, and report what happened.
 *
 * The previous value stays on screen while a reload is in flight, because
 * blanking the page to show a spinner every time a filter changes is worse
 * than showing slightly stale data for a moment. `pending` says which it is.
 *
 * `body` is handed an `AbortSignal` that is aborted when the call is
 * superseded — by a dependency change, a `reload`, or an unmount. Passing it
 * to `fetch` is what makes a cancelled request actually stop; a `body` that
 * ignores it is still correct, because the effect's own flag is what decides
 * whether a result is written.
 *
 * Nothing here runs during a prerender: `useEffect` does not run on a server,
 * so a server-rendered page shows `pending` with no value, which is the same
 * thing the client's first render shows. Data a page needs *in* its HTML
 * belongs in a route loader, not in this hook.
 */
export hook useAsync<T>(
  body: (signal: AbortSignal) => Promise<T>,
  deps: $ReadOnlyArray<mixed>,
  options?: AsyncOptions,
): Async<T> {
  const [state, setState] = useState<{|
    value: T | null,
    error: Error | null,
    pending: boolean,
  |}>({ value: null, error: null, pending: true });

  // Changing this is what re-runs the effect, so `reload` is a state change
  // rather than a function the effect has to be told about.
  const [attempt, setAttempt] = useState(0);
  const reload = useCallback(() => setAttempt((current) => current + 1), []);

  const retries = options?.retry ?? 0;
  const call = useStableCallback(body);
  const wait = useStableCallback<[number], number>(options?.retryDelay ?? backoff);

  useEffect(() => {
    // Set when this effect is superseded — by a dependency change, a reload,
    // or an unmount. React runs the cleanup before the next run, so the
    // request that is no longer wanted knows not to write.
    let ignore = false;
    let sleeping: TimeoutID | null = null;
    const controller = new AbortController();

    // Guarded rather than unconditional: a reload that arrives while a call is
    // already in flight would otherwise build a new state object, and a new
    // object is a re-render that changes nothing anyone can see.
    setState((current) => (current.pending ? current : { ...current, pending: true }));

    const run = (tries: number) => {
      call(controller.signal).then(
        (value) => {
          if (!ignore) {
            setState({ value, error: null, pending: false });
          }
        },
        (thrown) => {
          if (ignore) {
            return;
          }
          // An abort is not a failure to report: the only reason this call was
          // aborted is that nobody wants its answer any more.
          if (controller.signal.aborted) {
            return;
          }
          if (tries < retries) {
            sleeping = setTimeout(() => {
              sleeping = null;
              if (!ignore) {
                run(tries + 1);
              }
            }, wait(tries));
            return;
          }
          setState({
            value: null,
            error: thrown instanceof Error ? thrown : new Error(String(thrown)),
            pending: false,
          });
        },
      );
    };

    run(0);

    return () => {
      ignore = true;
      if (sleeping != null) {
        clearTimeout(sleeping);
      }
      controller.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, attempt, retries, call, wait]);

  return { ...state, reload };
}
