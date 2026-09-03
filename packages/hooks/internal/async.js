// @flow
//
// Running a promise from a component.
//
// The two bugs a hand-written version has: it sets state after the component
// has gone, and a slow first request overwrites a fast second one. The first is
// a warning; the second is a wrong answer on screen, which is worse and much
// harder to notice. A generation counter fixes both — a result is only used if
// it belongs to the newest run.

import { useCallback, useEffect, useRef, useState } from "@uniflowed/react";

import { useStableCallback } from "./lifecycle.js";

/** What an in-flight, settled or failed call looks like. */
export type Async<T> = {|
  +value: T | null,
  +error: Error | null,
  +pending: boolean,
  /** Run it again, keeping whatever is on screen until the new value lands. */
  +reload: () => void,
|};

/**
 * Call `body` when `deps` change, and report what happened.
 *
 * The previous value stays on screen while a reload is in flight, because
 * blanking the page to show a spinner every time a filter changes is worse
 * than showing slightly stale data for a moment. `pending` says which it is.
 */
export function useAsync<T>(
  body: () => Promise<T>,
  deps: $ReadOnlyArray<mixed>,
): Async<T> {
  const stable = useStableCallback(body);
  const [state, setState] = useState<{| value: T | null, error: Error | null, pending: boolean |}>(
    { value: null, error: null, pending: true },
  );

  // Which run is allowed to write. Incremented on every start, so a result
  // from an older run is dropped rather than overwriting a newer one.
  const generation = useRef(0);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const run = useCallback(() => {
    generation.current += 1;
    const mine = generation.current;
    setState((current) => ({ ...current, pending: true }));

    stable().then(
      (value) => {
        if (live.current && generation.current === mine) {
          setState({ value, error: null, pending: false });
        }
      },
      (error) => {
        if (live.current && generation.current === mine) {
          setState({
            value: null,
            error: error instanceof Error ? error : new Error(String(error)),
            pending: false,
          });
        }
      },
    );
  }, [stable]);

  useEffect(() => {
    run();
    // eslint-disable-next-line
  }, deps);

  return { ...state, reload: run };
}
