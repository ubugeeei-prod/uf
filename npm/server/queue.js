// @flow
//
// `@uniflowed/server/queue`: work that outlives the request that asked for it.
//
// `after()` is the small version of this and says so: it runs a callback once
// the response has been sent, in the same process, with no retry and no record
// that it was ever registered. That is right for a metric and wrong for
// anything a user would notice missing. A welcome email that failed because
// the mail provider was restarting is gone; so is every job in flight when the
// process is redeployed, which on most deployments is several times a day.
//
// A queue is the durable version of the same idea, and it is four things:
// a job, a payload, a retry policy, and somewhere the work waits. uf owns the
// first three. The fourth is the deployment's, and the honest part of this
// module is saying so rather than shipping something that looks like an answer.
//
// # What uf provides
//
//   * **`defineJob`** — a name, a function, and how often to try it. The name
//     is what a worker in another process uses to find the function again,
//     which is why a job is a value with a name rather than a closure.
//   * **`enqueue`** — from inside a request, and only from inside one. It
//     serialises the payload, stamps the record, and hands it to the backend
//     the host installed. It resolves when the backend has the work, not when
//     the work is done.
//   * **The retry policy and its backoff**, applied by [`createRunner`], which
//     is the half a backend calls when a record comes back out. A failed
//     attempt is re-pushed with a later `notBefore` rather than retried in
//     place, because a retry that ignores the clock is a hot loop against
//     whatever was already failing.
//   * **The record shape**, which is deliberately five plain fields: an
//     implementation over SQS, a Redis list or a Postgres table is a `push`
//     and a loop, not an adapter.
//   * **One backend**, [`memoryQueue`], which runs in this process.
//
// # What the deployment must provide
//
// Two things, and no configuration flag can supply either.
//
// **Durable storage.** `memoryQueue` is an array. A restart loses everything in
// it, two instances behind a load balancer each have their own, and nothing is
// acknowledged. That is a legitimate small deployment — one process, work that
// can be lost — and it is stated on the value itself as `durable: false` so
// that an adapter can refuse it rather than a person having to have read this
// paragraph.
//
// **Something that keeps running.** A queue needs a consumer, and a consumer is
// a process that exists when no request is being answered. A container or a
// long-lived Node process has one. A serverless function does not: it is frozen
// between invocations, so work pushed into its own memory is dropped when the
// response goes out, and `./internal/capabilities.js` refuses that wiring
// outright. A worker isolate does not either — the platform may tear it down
// the moment the response is written, which is the same reason `after()` there
// goes through `ctx.waitUntil`. On both of those the queue's storage has to be
// somewhere else *and* the drain has to be something else: a scheduled
// invocation, a queue-triggered function, a container beside the site.
//
// uf does not pretend to have that. What it has is the seam: `enqueue` writes
// through `QueueBackend.push`, `createRunner` is what the consumer calls per
// record, and neither of them cares which of the two processes it is in.
//
// # At least once, so a job has to be idempotent
//
// A record that has been handed to a runner and whose process then dies is a
// record the backend still holds. Every durable queue works this way, and the
// alternative — acknowledging before running — loses work instead of repeating
// it. So a job may run twice for the same record, and a job that charges a card
// needs its own idempotency key. This is written here rather than left to be
// discovered, because it cannot be fixed later by a library.
//
// # The payload is JSON, and `enqueue` refuses anything else
//
// A record crosses a process boundary in every backend that is worth having,
// so it is serialised at the point it is enqueued rather than at the point it
// is stored. That makes `memoryQueue` behave exactly like a durable one: a
// `Date` arrives as a string in both, a cyclic object is refused in both, and a
// job that mutates its payload cannot reach back into the request that sent it.
// Serialising in the backend instead would have made the in-process one the
// permissive outlier, which is the failure mode this module exists inside.

import { OutsideRequestError } from "./index.js";
import type { JobRecord, QueueBackend } from "./internal/capabilities.js";
import { CapabilityUnavailableError } from "./internal/capabilities.js";
import { currentContext } from "./internal/context.js";

export type { JobRecord, QueueBackend } from "./internal/capabilities.js";

/** How often a failing job is tried again, and how long it waits. */
export type RetryPolicy = {|
  /** Runs in total, including the first. `1` never retries. */
  readonly attempts: number,
  /** Milliseconds before the second attempt; doubles each time after. */
  readonly backoff: number,
  /** The ceiling that doubling stops at. */
  readonly maxBackoff: number,
|};

/** A named unit of work. */
export type Job<P> = {|
  /** How a worker in another process finds this function again. */
  readonly name: string,
  readonly retry: RetryPolicy,
  readonly run: (payload: P) => mixed,
|};

/**
 * A job whose payload type has been erased.
 *
 * A registry is a list of jobs with different payloads, and Flow has no
 * existential to say "some `P`" with. `P` is in a parameter position, so
 * `Job<mixed>` is not a supertype and cannot stand in for it.
 */
// Suppressed rather than left to fail `check:lib`: the reason above is the
// whole argument, and it does not end in a change anyone can make to this file.
// uf-lint-disable-next-line flow/unclear-type
export type AnyJob = Job<any>;

/** A retry policy with the parts a definition did not state left out. */
export type PartialRetryPolicy = {|
  readonly attempts?: number,
  readonly backoff?: number,
  readonly maxBackoff?: number,
|};

/** What `defineJob` is given. */
export type JobDefinition<P> = {|
  readonly name: string,
  readonly run: (payload: P) => mixed,
  /** Defaults to three attempts, a second apart, doubling to a minute. */
  readonly retry?: PartialRetryPolicy,
|};

const DEFAULT_RETRY: RetryPolicy = Object.freeze({
  // Three, because the failures a retry actually fixes — a restarting
  // dependency, a rate limit, a lost packet — are almost always over within
  // two intervals, and a job that is still failing on the fourth attempt is
  // failing for a reason that waiting will not change.
  attempts: 3,
  backoff: 1_000,
  maxBackoff: 60_000,
});

/** Raised when work is queued with nowhere to put it. */
export class QueueUnavailableError extends CapabilityUnavailableError {
  constructor(target: string) {
    super(
      "queue",
      target,
      "uf owns the job, the payload and the retry policy; where the work waits is the " +
        "deployment's — pass `queue` where the host is built. `memoryQueue` runs in this " +
        "process and loses everything in it when the process ends, which is why it is not a " +
        "default.",
    );
    this.name = "QueueUnavailableError";
  }
}

/** Raised when a record names a job the runner was not given. */
export class UnknownJobError extends Error {
  /** The name on the record. */
  job: string;

  constructor(job: string, known: $ReadOnlyArray<string>) {
    super(
      `@uniflowed/server: a queued record names the job "${job}", which this runner does not ` +
        `have. It knows: ${known.length === 0 ? "nothing" : known.join(", ")}. A record ` +
        "outliving the deployment that could run it is what a durable queue is for, so this is " +
        "a rename or a rollback rather than a bug in the queue.",
    );
    this.name = "UnknownJobError";
    this.job = job;
  }
}

/**
 * Declare a job.
 *
 * A value with a name rather than a bare function, because the two halves of a
 * queue are usually two processes: what `enqueue` writes is a name and some
 * JSON, and the worker has to find the function from that. A closure cannot be
 * written down.
 */
export function defineJob<P>(definition: JobDefinition<P>): Job<P> {
  const name = definition.name.trim();
  if (name === "") {
    throw new TypeError("@uniflowed/server: a job needs a name, because a record carries one.");
  }
  return {
    name,
    run: definition.run,
    retry: { ...DEFAULT_RETRY, ...(definition.retry ?? {}) },
  };
}

/**
 * Queue `payload` for `job`, and answer with the record's id.
 *
 * Resolves when the backend has the work, which is not when the work is done —
 * that is the entire point, and the reason this returns an id rather than a
 * result. A caller that needs the answer wanted a request.
 *
 * Inside a request, because the backend is the host's and the host installs it
 * on the request. A worker re-queuing a retry does not come through here; it
 * pushes through the backend it is already holding.
 */
export async function enqueue<P>(
  job: Job<P>,
  payload: P,
  options?: {| readonly delay?: number |},
): Promise<string> {
  const context = currentContext();
  if (context == null) {
    // The class every other binding in this package raises for it: "outside a
    // request" is one failure with one remedy, and a second spelling would be
    // a second thing to search for.
    throw new OutsideRequestError("enqueue");
  }

  const backend = context.capabilities?.queue;
  if (backend == null) {
    throw new QueueUnavailableError(context.capabilities?.target ?? "unknown");
  }

  const delay = Math.max(0, options?.delay ?? 0);
  const record: JobRecord = {
    id: newId(),
    job: job.name,
    payload: serialise(job.name, payload),
    attempt: 1,
    notBefore: Date.now() + delay,
  };
  await backend.push(record);
  return record.id;
}

/** What a backend calls for one record; see [`createRunner`]. */
export type JobRunner = (record: JobRecord) => Promise<void>;

/**
 * The function a consumer calls for each record it takes.
 *
 * This is uf's half of a backend written by somebody else: the loop that reads
 * from SQS belongs to the deployment, and what it does with each message is
 * this. The retry policy is applied here rather than in the backend so that
 * every backend retries the same way — a policy implemented once per backend
 * is a policy that differs per deployment.
 *
 * A failed attempt with tries left is pushed back with a later `notBefore` and
 * this resolves; the record is *handled*, and what is left is scheduled. A
 * failed attempt with none left rejects, so a backend with a dead-letter queue
 * has something to catch. The two are different outcomes and a caller that
 * treated them the same would either retry forever or not at all.
 */
export function createRunner(options: {|
  readonly jobs: $ReadOnlyArray<AnyJob>,
  readonly backend: QueueBackend,
  /** The clock, so a suite can drive the backoff without waiting for it. */
  readonly now?: () => number,
|}): JobRunner {
  const known: Map<string, AnyJob> = new Map(options.jobs.map((job) => [job.name, job]));
  const now = options.now ?? (() => Date.now());

  return async function run(record: JobRecord): Promise<void> {
    const job = known.get(record.job);
    if (job == null) {
      throw new UnknownJobError(record.job, [...known.keys()]);
    }

    try {
      await job.run(JSON.parse(record.payload));
    } catch (error) {
      if (record.attempt >= job.retry.attempts) {
        throw error;
      }
      await options.backend.push({
        ...record,
        attempt: record.attempt + 1,
        notBefore: now() + backoffFor(job.retry, record.attempt),
      });
    }
  };
}

/**
 * How long to wait before attempt `attempt + 1`.
 *
 * Doubling from `backoff`, capped at `maxBackoff`. No jitter, and that is a
 * decision rather than an omission: jitter matters when many clients retry the
 * same failure at once, and the callers of this are workers draining a queue
 * they already take from one at a time. Adding it would make every test of a
 * backoff a test of a random number.
 */
export function backoffFor(retry: RetryPolicy, attempt: number): number {
  const doubled = retry.backoff * 2 ** Math.max(0, attempt - 1);
  return Math.min(doubled, retry.maxBackoff);
}

/** [`memoryQueue`]'s backend, plus the two things only an in-process one has. */
export type MemoryQueue = {|
  readonly durable: boolean,
  readonly name: string,
  readonly push: (record: JobRecord) => Promise<void>,
  /** Records waiting, whether or not they are due yet. */
  readonly size: () => number,
  /**
   * Run everything that is due, once.
   *
   * Records that fail and have attempts left come back with a later
   * `notBefore`, so they are not picked up again by this same call — a drain
   * that retried in place would turn a one-second backoff into a tight loop.
   */
  readonly drain: () => Promise<void>,
  /** Stop the timer. A queue that is not stopped holds nothing else open. */
  readonly stop: () => void,
|};

/**
 * A queue that runs in this process.
 *
 * The one implementation, and it is honest about being the small one:
 * `durable` is `false`, which is a value an adapter refuses on rather than a
 * warning somebody reads. Use it for a single long-lived process whose work
 * can be lost — a container, `uf start`, a development machine — and for the
 * tests of every job in an application, which is the use that makes it worth
 * shipping even to deployments that will never run it in production.
 *
 * The timer is the whole of its scheduling: every `tick` it drains what is due.
 * It is unreferenced where the host allows, so a pending job cannot be the
 * reason a process will not exit — a queue that kept `uf test` alive would be
 * a queue nobody could use in a test.
 *
 * Jobs run one at a time. A pool is a decision about how much load somebody
 * else's database should take, and this module is in no position to make it.
 */
export function memoryQueue(options: {|
  readonly jobs: $ReadOnlyArray<AnyJob>,
  /** Milliseconds between drains. `0` runs no timer, for a suite. */
  readonly tick?: number,
  /**
   * The clock, so a suite can move time rather than wait for it.
   *
   * It decides when a record is due and how a backoff is measured, and it does
   * not reach `enqueue`, which stamps `notBefore` from `Date.now()` because it
   * has no backend-specific clock to ask. A suite that injects one should push
   * its own records; one that enqueues should leave this alone.
   */
  readonly now?: () => number,
  /** Where a job that ran out of attempts is reported. */
  readonly onFailure?: (record: JobRecord, error: mixed) => void,
|}): MemoryQueue {
  const now = options.now ?? (() => Date.now());
  const onFailure = options.onFailure ?? reportExhausted;
  const pending: Array<JobRecord> = [];
  let draining: Promise<void> | null = null;

  const backend: QueueBackend = {
    durable: false,
    name: "memoryQueue",
    push: async (record: JobRecord): Promise<void> => {
      pending.push(record);
    },
  };

  const runner = createRunner({ jobs: options.jobs, backend, now });

  /**
   * One pass over what is due.
   *
   * The due records are taken out first and the rest left behind, so a retry
   * pushed during this pass — which is always scheduled for later — cannot be
   * picked up by it.
   */
  const pass = async (): Promise<void> => {
    const moment = now();
    const due: Array<JobRecord> = [];
    for (let index = pending.length - 1; index >= 0; index -= 1) {
      if (pending[index].notBefore <= moment) {
        due.unshift(pending.splice(index, 1)[0]);
      }
    }
    for (const record of due) {
      try {
        await runner(record);
      } catch (error) {
        onFailure(record, error);
      }
    }
  };

  const drain = (): Promise<void> => {
    // Serialised against itself, so the timer firing while a drain is still
    // running does not start a second pass over the same records. `finally`
    // rather than `then`, so a pass that threw still releases the next one.
    draining = (draining ?? Promise.resolve()).then(pass, pass);
    return draining;
  };

  const tick = options.tick ?? 50;
  const timer = tick > 0 ? setInterval(() => void drain(), tick) : null;
  (timer as $FlowFixMe)?.unref?.();

  return {
    durable: backend.durable,
    name: backend.name,
    push: backend.push,
    size: () => pending.length,
    drain,
    stop: () => {
      if (timer != null) {
        clearInterval(timer);
      }
    },
  };
}

/**
 * The payload as JSON, or a failure that names the job.
 *
 * `JSON.stringify` answers `undefined` for a function, a symbol and
 * `undefined` itself rather than throwing, so the result is checked as well as
 * the call: a job silently queued with no payload is a job that fails in a
 * worker, hours later, with nothing pointing back here.
 */
function serialise(job: string, payload: mixed): string {
  let text;
  try {
    text = JSON.stringify(payload);
  } catch (cause) {
    throw new TypeError(
      `@uniflowed/server: the payload for "${job}" cannot be JSON: ${String(cause)}. A queued ` +
        "record crosses a process boundary in every backend worth having, so it is serialised " +
        "here rather than wherever it happens to be stored.",
    );
  }
  if (text === undefined) {
    throw new TypeError(
      `@uniflowed/server: the payload for "${job}" serialises to nothing — a function, ` +
        "a symbol or undefined. Pass a value the worker can be given.",
    );
  }
  return text;
}

/** How many ids this process has issued, for the fallback below. */
let issued = 0;

/**
 * An id for a record.
 *
 * `randomUUID` where the runtime has it, which is all four of uf's targets, and
 * a counter where it does not. That the fallback is guessable costs nothing: an
 * id here is how a log line is matched to a record, never a capability — a
 * queue nobody can push to without the backend is a queue an id cannot open.
 */
function newId(): string {
  const source = (globalThis as $FlowFixMe).crypto;
  const uuid = source?.randomUUID;
  if (typeof uuid === "function") {
    return uuid.call(source);
  }
  issued += 1;
  return `job-${String(Date.now())}-${String(issued)}`;
}

/**
 * Report a job that ran out of attempts.
 *
 * The console, for the reason `drainDeferred` gives about `after()`: this runs
 * with no response in hand and nowhere else to put it. A deployment that wants
 * a dead-letter queue passes `onFailure`.
 */
function reportExhausted(record: JobRecord, error: mixed): void {
  // eslint-disable-next-line no-console
  console.error(`uf: the job ${record.job} (${record.id}) failed on its last attempt`, error);
}
