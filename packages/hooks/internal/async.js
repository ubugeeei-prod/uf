// @flow
//
// Running a promise from a component.
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

import { useCallback, useEffect, useState } from "@uniflowed/react";

/** What an in-flight, settled or failed call looks like. */
export type Async<T> = {|
  readonly value: T | null,
  readonly error: Error | null,
  readonly pending: boolean,
  /** Run it again, keeping whatever is on screen until the new value lands. */
  readonly reload: () => void,
|};

/**
 * Call `body` when `deps` change, and report what happened.
 *
 * The previous value stays on screen while a reload is in flight, because
 * blanking the page to show a spinner every time a filter changes is worse
 * than showing slightly stale data for a moment. `pending` says which it is.
 */
export function useAsync<T>(body: () => Promise<T>, deps: $ReadOnlyArray<mixed>): Async<T> {
  const [state, setState] = useState<{|
    value: T | null,
    error: Error | null,
    pending: boolean,
  |}>({ value: null, error: null, pending: true });

  // Changing this is what re-runs the effect, so `reload` is a state change
  // rather than a function the effect has to be told about.
  const [attempt, setAttempt] = useState(0);
  const reload = useCallback(() => setAttempt((current) => current + 1), []);

  useEffect(() => {
    // Set when this effect is superseded — by a dependency change, a reload,
    // or an unmount. React runs the cleanup before the next run, so the
    // request that is no longer wanted knows not to write.
    let ignore = false;
    setState((current) => ({ ...current, pending: true }));

    body().then(
      (value) => {
        if (!ignore) {
          setState({ value, error: null, pending: false });
        }
      },
      (thrown) => {
        if (!ignore) {
          setState({
            value: null,
            error: thrown instanceof Error ? thrown : new Error(String(thrown)),
            pending: false,
          });
        }
      },
    );

    return () => {
      ignore = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, attempt]);

  return { ...state, reload };
}
