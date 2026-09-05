// @flow
//
// `@uniflowed/query/retry`: trying again without making the outage worse.
//
// A request fails for two unrelated reasons and the difference decides
// everything: the network dropped a packet, in which case trying again in a
// moment works; or the server said 404, in which case trying again works
// exactly as well as the first time and costs the same. So the policy is a
// function of the failure, not a constant, and [`RetryPolicy`] accepts a
// predicate for the case where only the caller can tell the two apart.
//
// # Why the delay grows
//
// The failure mode a fixed delay produces is a thundering herd: a server comes
// back up, every client that was retrying every second hits it in the same
// second, and it goes down again. Doubling the wait spreads the recovery out,
// and the cap keeps a tab left open overnight from drifting into hour-long
// silences. The default is `1s, 2s, 4s, 8s…` to thirty seconds.
//
// Jitter is deliberately absent. It is the right answer for a fleet of servers
// retrying each other, and close to irrelevant for browser tabs, which are
// already spread out by the reader's own timing; adding it here would make
// every test of this module probabilistic in exchange for nothing measurable.
//
// # Why this module owns the sleeping too
//
// A retry loop that cannot be interrupted is a leak with a delay on it: the
// reader has navigated away, the query is unobserved, and the loop still wakes
// up in eight seconds to make a request nobody will read. So [`runWithRetry`]
// takes the same `AbortSignal` the request does, and the sleep between
// attempts rejects the moment it fires. Splitting "how long to wait" from
// "waiting" would put the half that matters in whichever module happened to
// call this one.

/**
 * Whether a failed attempt should be tried again.
 *
 * `false` never retries, `true` retries forever, a number is a count of
 * *retries* — `2` means three attempts in total — and a predicate is given the
 * number of failures so far and the last error, which is the only form that
 * can tell a 500 from a 404.
 */
export type RetryPolicy = boolean | number | ((failureCount: number, error: Error) => boolean);

/** How long to wait before the next attempt, in milliseconds. */
export type RetryDelay = number | ((failureCount: number, error: Error) => number);

/**
 * What a cancelled request rejects with.
 *
 * A real class rather than a string, because the code that swallows a
 * cancellation must not also swallow a genuine failure — telling them apart by
 * message would break the first time someone's API said "aborted".
 */
export class CancelledError extends Error {
  constructor(message: string = "the request was cancelled") {
    super(message);
    this.name = "CancelledError";
  }
}

/** Doubling backoff from one second, capped at thirty. */
export function backoffDelay(failureCount: number): number {
  return Math.min(1000 * 2 ** (failureCount - 1), 30_000);
}

/**
 * Run `attempt` until it succeeds, the policy gives up, or the signal fires.
 *
 * `onFailure` is called after every failed attempt with the running count, so
 * the caller can put "retrying, attempt 2" on screen. It is not called for a
 * cancellation: nobody is waiting for that answer, so there is nothing to
 * report.
 *
 * The error a caller finally sees is the *last* one. An early failure that was
 * retried past is not the reason the request failed; the one that exhausted
 * the policy is.
 */
export async function runWithRetry<T>(options: {|
  readonly attempt: (failureCount: number) => Promise<T>,
  readonly retry: RetryPolicy,
  readonly retryDelay: RetryDelay,
  readonly signal: AbortSignal,
  readonly onFailure?: (failureCount: number, error: Error) => void,
|}): Promise<T> {
  let failureCount = 0;
  for (;;) {
    if (options.signal.aborted) {
      throw cancellation(options.signal);
    }
    try {
      return await race(options.attempt, failureCount, options.signal);
    } catch (thrown) {
      // A request that failed *because* it was cancelled is not a failure this
      // policy has an opinion about. Retrying it would restart work the caller
      // just asked to stop.
      if (options.signal.aborted) {
        throw cancellation(options.signal);
      }

      const error = asError(thrown);
      failureCount += 1;
      options.onFailure?.(failureCount, error);
      if (!shouldRetry(options.retry, failureCount, error)) {
        throw error;
      }
      await sleep(delayFor(options.retryDelay, failureCount, error), options.signal);
    }
  }
}

/** Whatever was thrown, as an `Error`, because state has to hold one shape. */
export function asError(thrown: mixed): Error {
  return thrown instanceof Error ? thrown : new Error(String(thrown));
}

function shouldRetry(policy: RetryPolicy, failureCount: number, error: Error): boolean {
  if (typeof policy === "function") {
    return policy(failureCount, error);
  }
  if (typeof policy === "number") {
    return failureCount <= policy;
  }
  return policy;
}

function delayFor(delay: RetryDelay, failureCount: number, error: Error): number {
  return typeof delay === "function" ? delay(failureCount, error) : delay;
}

/**
 * One attempt, but no longer than the caller wants to wait for it.
 *
 * Aborting a signal does not, on its own, settle anything: `AbortController`
 * is a request, and a function that ignores it goes on running. So the
 * cancellation has to be a promise of its own, raced against the attempt —
 * otherwise cancelling a query whose function does not take a signal hangs
 * forever, and the reader is left watching a spinner for a request nobody is
 * waiting for.
 *
 * The attempt is not stopped by this; nothing can stop it. It is abandoned,
 * and the entry's fetch id makes sure a late answer cannot write.
 */
async function race<T>(
  attempt: (failureCount: number) => Promise<T>,
  failureCount: number,
  signal: AbortSignal,
): Promise<T> {
  let onAbort = () => {};
  const cancelled: Promise<empty> = new Promise((_resolve, reject) => {
    onAbort = () => reject(cancellation(signal));
    signal.addEventListener("abort", onAbort, { once: true });
  });
  try {
    return await Promise.race([attempt(failureCount), cancelled]);
  } finally {
    // So a signal aborted after this attempt succeeded cannot reject a promise
    // nothing is listening to any more.
    signal.removeEventListener("abort", onAbort);
  }
}

function cancellation(signal: AbortSignal): Error {
  const reason = signal.reason;
  return reason instanceof Error ? reason : new CancelledError();
}

/**
 * Wait, unless the signal fires first.
 *
 * The timer is unreferenced where the host supports it: a process whose only
 * remaining work is a backoff nobody is waiting for should exit, and a test
 * that finishes while a retry is pending should not hang for eight seconds.
 */
function sleep(millis: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(cancellation(signal));
      return;
    }
    const onAbort = () => {
      clearTimeout(timer);
      reject(cancellation(signal));
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    }, millis);
    (timer as $FlowFixMe)?.unref?.();
    signal.addEventListener("abort", onAbort, { once: true });
  });
}
