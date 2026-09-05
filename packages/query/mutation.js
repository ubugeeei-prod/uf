// @flow
//
// `@uniflowed/query/mutation`: the write, and the lie you tell before it.
//
// A mutation is not a query with a different verb. A query can be repeated,
// abandoned, de-duplicated and garbage-collected because asking twice is free;
// none of that is true of "create the invoice". So this module shares the
// retry loop with queries and nothing else: no cache entry, no de-duplication,
// no staleness, and — deliberately — no cancellation.
//
// # Why an unmount does not cancel a mutation
//
// A request that has left the building cannot be un-sent. Aborting it would
// stop the *answer* arriving, not the write happening, and the application
// would be left unable to say whether the invoice exists. So a mutation runs
// to completion and its callbacks fire even if the component that started it
// has gone. The callbacks are where cache updates and invalidations live,
// which is exactly the work that still needs doing when the reader has moved
// on.
//
// # Why `onMutate` returns a context and `onError` is given it
//
// Optimistic updates are the reason this API has a shape at all. The sequence
// is fixed and each step exists because of a specific failure:
//
// ```js
// onMutate: async (next) => {
//   // A refetch already in flight would land *after* the optimistic write and
//   // put the server's old answer back on screen. Stop it first.
//   await client.cancelQueries({ queryKey: ["todos"] });
//   const previous = client.getQueryData(["todos"]);
//   client.setQueryData(["todos"], (todos) => [...todos, next]);
//   return { previous };            // ← the rollback, captured before the guess
// },
// onError: (_error, _next, context) => {
//   client.setQueryData(["todos"], context.previous);
// },
// onSettled: () => client.invalidateQueries({ queryKey: ["todos"] }),
// ```
//
// The context has to be produced by `onMutate` and handed back to `onError`
// because nothing else can hold it: a `useRef` in the component is gone if the
// component unmounted, and a variable in the caller's closure belongs to one
// call and two mutations can be in flight at once. Passing it through the
// mutation itself is what makes the rollback correct for *this* call.
//
// `onError` runs before the state becomes `error`, and `onSettled` before
// either terminal state, so by the time a component is told the mutation
// failed, the rollback has already happened — the reader never sees the
// optimistic value and the failure message at the same time.
//
// # Why the state is an external store rather than `useState`
//
// So a snapshot is one immutable object with a stable identity, comparable by
// `Object.is`, which is what `useSyncExternalStore` requires and what keeps a
// re-render from being scheduled for a mutation that has not moved. It also
// means the state machine is testable without rendering anything.

import { asError, runWithRetry } from "./retry.js";
import type { RetryDelay, RetryPolicy } from "./retry.js";

export type MutationStatus = "idle" | "pending" | "success" | "error";

export type MutationState<TVariables, TData> = {|
  readonly status: MutationStatus,
  readonly data: TData | void,
  readonly error: Error | null,
  /** What the last call was given, so a retry button can repeat it. */
  readonly variables: TVariables | void,
  readonly failureCount: number,
  readonly submittedAt: number,
|};

/**
 * The callbacks a single call may add.
 *
 * Both sets run — the ones on the hook and the ones on the call — with the
 * hook's first. The hook's are the ones that keep the cache correct and must
 * not be skippable by a caller who only wanted a toast.
 */
export type MutationCallbacks<TVariables, TData, TContext> = {|
  readonly onSuccess?: (data: TData, variables: TVariables, context: TContext | void) => mixed,
  readonly onError?: (error: Error, variables: TVariables, context: TContext | void) => mixed,
  readonly onSettled?: (
    data: TData | void,
    error: Error | null,
    variables: TVariables,
    context: TContext | void,
  ) => mixed,
|};

export type MutationOptions<TVariables, TData, TContext> = {|
  readonly mutationFn: (variables: TVariables) => Promise<TData>,
  /** Runs first; what it returns is handed to the other three. */
  readonly onMutate?: (variables: TVariables) => Promise<TContext | void> | TContext | void,
  readonly onSuccess?: (data: TData, variables: TVariables, context: TContext | void) => mixed,
  readonly onError?: (error: Error, variables: TVariables, context: TContext | void) => mixed,
  readonly onSettled?: (
    data: TData | void,
    error: Error | null,
    variables: TVariables,
    context: TContext | void,
  ) => mixed,
  readonly retry?: RetryPolicy,
  readonly retryDelay?: RetryDelay,
|};

/** The same options with the client's defaults filled in. */
export type ResolvedMutationOptions<TVariables, TData, TContext> = {|
  ...MutationOptions<TVariables, TData, TContext>,
  readonly retry: RetryPolicy,
  readonly retryDelay: RetryDelay,
|};

/** What a mutation looks like to a component. */
export type MutationResult<TVariables, TData, TContext> = {|
  readonly data: TData | void,
  readonly error: Error | null,
  readonly status: MutationStatus,
  readonly variables: TVariables | void,
  readonly failureCount: number,
  readonly isIdle: boolean,
  readonly isPending: boolean,
  readonly isSuccess: boolean,
  readonly isError: boolean,
  /** Fire and forget. The failure is in `error`, not in a rejected promise. */
  readonly mutate: (
    variables: TVariables,
    callbacks?: MutationCallbacks<TVariables, TData, TContext>,
  ) => void,
  /** The same call, awaited. Rejects, so a caller can branch on the failure. */
  readonly mutateAsync: (
    variables: TVariables,
    callbacks?: MutationCallbacks<TVariables, TData, TContext>,
  ) => Promise<TData>,
  readonly reset: () => void,
|};

const IDLE: MutationState<empty, empty> = Object.freeze({
  status: "idle",
  data: undefined,
  error: null,
  variables: undefined,
  failureCount: 0,
  submittedAt: 0,
});

export class Mutation<TVariables, TData, TContext> {
  state: MutationState<TVariables, TData> = IDLE;
  listeners: Set<() => void> = new Set();

  /**
   * Which call owns the state.
   *
   * Two calls can be in flight — a reader who clicked twice — and the state
   * describes the latest, not whichever settled last. The callbacks of both
   * still run: each of them was a real write with real consequences, and the
   * cache updates in them are not the loser's to skip.
   */
  runId: number = 0;

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Back to idle, forgetting the last result. */
  reset(): void {
    this.runId += 1;
    this.setState(IDLE as $FlowFixMe);
  }

  async execute(
    variables: TVariables,
    options: ResolvedMutationOptions<TVariables, TData, TContext>,
    callbacks?: MutationCallbacks<TVariables, TData, TContext>,
  ): Promise<TData> {
    const id = this.runId + 1;
    this.runId = id;
    this.setState({
      status: "pending",
      data: undefined,
      error: null,
      variables,
      failureCount: 0,
      submittedAt: Date.now(),
    });

    // Declared outside the `try` so the rollback in `onError` can still be
    // given whatever `onMutate` managed to record before things went wrong.
    let context: TContext | void;
    try {
      context = await options.onMutate?.(variables);
      const data = await runWithRetry({
        attempt: () => options.mutationFn(variables),
        retry: options.retry,
        retryDelay: options.retryDelay,
        // Never aborted: see the module docs on why a write is not cancelled.
        signal: new AbortController().signal,
        onFailure: (failureCount) => {
          if (this.runId === id) {
            this.setState({ failureCount });
          }
        },
      });

      // Before the state moves to `success`, so a component told the mutation
      // succeeded is looking at a cache that already knows.
      await options.onSuccess?.(data, variables, context);
      await callbacks?.onSuccess?.(data, variables, context);
      await options.onSettled?.(data, null, variables, context);
      await callbacks?.onSettled?.(data, null, variables, context);

      if (this.runId === id) {
        this.setState({ status: "success", data, error: null });
      }
      return data;
    } catch (thrown) {
      const error = asError(thrown);
      try {
        await options.onError?.(error, variables, context);
        await callbacks?.onError?.(error, variables, context);
        await options.onSettled?.(undefined, error, variables, context);
        await callbacks?.onSettled?.(undefined, error, variables, context);
      } finally {
        // Committed whatever the callbacks did. A callback that throws is the
        // caller's bug and still reaches them — it comes out of `execute` in
        // place of the rethrow below — but a mutation left `pending` for ever
        // because a listener threw would be this module's bug, and the
        // component showing a spinner has no way back from it.
        if (this.runId === id) {
          this.setState({ status: "error", error, data: undefined });
        }
      }
      // Rethrown so `mutateAsync` can be branched on. `mutate` swallows it,
      // which is why the two exist.
      throw error;
    }
  }

  setState(patch: { +[string]: mixed }): void {
    this.state = { ...this.state, ...patch } as $FlowFixMe;
    for (const listener of Array.from(this.listeners)) {
      listener();
    }
  }
}
