// @flow
//
// Internal to `@uniflowed/server`: what the host answering this request can do.
//
// Three of the four front doors in this package can hold a connection open and
// one cannot, and until this module existed there was nowhere to say so. A
// route handler that returns an event stream is correct on `uf start`, correct
// on a worker, and on a Lambda is a response buffered until the invocation
// times out — the same code, the same build, a failure that appears only in
// the deployment nobody checked. `docs/security.md` rule 3 is the shape of the
// fix: deny by default, allow by table, and let the table be the host's rather
// than the application's.
//
// So a host states what it is, once, where it is wired. `./context.js` carries
// the answer on the request beside the cache — for the same reason the cache is
// there and not in a module variable: two requests are answered at once, and
// anything reached for by name has to be scoped to the request or it is scoped
// to whichever request set it last.
//
// # Two questions, not one flag
//
// `stream` and `persistent` look like the same fact about a host and are not,
// and collapsing them would get one of the two targets wrong. A Cloudflare
// Worker streams a body perfectly and does not reliably outlive the response —
// `./../edge.js` already says so about `after()`, which is handed to
// `ctx.waitUntil` because there is no later moment to hand it to. A Lambda is
// the other way round in the half that matters: it buffers, and while it is
// running it is an ordinary process. One boolean would have refused event
// streams on the worker or accepted an in-process queue on the function, and
// both are wrong.
//
// # A string, not an enumeration
//
// `target` is the adapter's own name for itself, deliberately not a closed
// union. `uf_config::DeployAdapter` already enumerates the seven targets, it is
// Rust, and a second copy of that list in Flow would be a second thing to keep
// in step with nothing checking that they agree — which is how two names for
// one setting drift apart. What this module needs from `target` is a word to
// put in an error message, and a word is what it takes.
//
// # Why the refusals happen where the host is wired
//
// [`assertCapable`] is called by the adapters when they are built, not when a
// request arrives. A serverless handler holding a WebSocket upgrader is a
// deployment that will accept upgrade requests and drop every one of them, and
// the moment to say so is while somebody is looking at the wiring — not on the
// first connection, in production, as a timeout.

/** An event a socket delivers; inexact, because `close` carries no `data`. */
export type SocketEvent = { readonly data?: mixed, ... };

/**
 * A socket the host handed back from an upgrade.
 *
 * The three members a handler actually uses, declared structurally so this
 * package holds no copy of another project's `WebSocket` type: what arrives is
 * Cloudflare's, Deno's, Bun's or `ws`'s, and all four are this much.
 */
export type WebSocketLike = {
  readonly send: (data: string) => mixed,
  readonly close: (code?: number, reason?: string) => mixed,
  readonly addEventListener: (
    type: "message" | "close" | "error",
    listener: (event: SocketEvent) => mixed,
  ) => mixed,
  ...
};

/** What a host's upgrade answered with: the response to send, and the socket. */
export type WebSocketUpgrade = {|
  /** The handshake response to return from the handler. */
  readonly response: Response,
  /** This end of the connection, already accepted. */
  readonly socket: WebSocketLike,
|};

/**
 * A host's WebSocket upgrade.
 *
 * Supplied by the deployment rather than implemented here, and `../socket.js`
 * argues that at length: the runtimes uf targets spell this four incompatible
 * ways, and Node does not spell it at all.
 */
export type WebSocketUpgrader = (request: Request) => WebSocketUpgrade;

/** One unit of deferred work as it is stored; see `../queue.js`. */
export type JobRecord = {|
  readonly id: string,
  /** The name `defineJob` gave, which is how a worker finds the function. */
  readonly job: string,
  /**
   * The payload as JSON text.
   *
   * Text rather than a value, and that is the load-bearing decision in this
   * whole type. Every backend a real deployment uses puts the record through a
   * process boundary — a Redis list, an SQS message, a row — so an in-process
   * backend that passed the object straight through would be the one backend
   * where a `Date` is still a `Date`, a shared array is still shared, and a
   * job that mutates its payload is visible to the caller. It would work
   * locally and be wrong in production, which is the only kind of difference
   * worth designing against.
   */
  readonly payload: string,
  /** 1 for the first run, 2 for the first retry. */
  readonly attempt: number,
  /** Epoch milliseconds before which this must not run. */
  readonly notBefore: number,
|};

/**
 * Where enqueued work goes.
 *
 * The producer half only. A backend that can also *run* work says so by having
 * somewhere to call `../queue.js`'s runner from; nothing here requires it,
 * because the deployment that stores the work is not always the process that
 * drains it — which is the whole difference between a queue and `after()`.
 *
 * Inexact, and that is the point rather than laziness: a backend is written by
 * the deployment and is entitled to carry whatever else it needs — a poll loop,
 * a connection, the `drain` that `../queue.js`'s own in-process one exposes.
 * These three are what uf reads.
 */
export type QueueBackend = {
  /**
   * Whether the work survives the process that pushed it.
   *
   * Read by the adapters rather than by the queue. An in-process backend on a
   * host that keeps running is a legitimate small deployment; the same backend
   * on a target that ends with the response is work that is dropped without a
   * word. One field, so the difference is a value the wiring can be refused
   * over rather than a paragraph somebody has to have read.
   */
  readonly durable: boolean,
  /** A name for this backend, for the message when one is refused. */
  readonly name: string,
  /** Take one record. Resolves when the backend has it, not when it has run. */
  readonly push: (record: JobRecord) => Promise<void>,
  ...
};

/** What the host answering this request can do. */
export type ServerCapabilities = {|
  /** The adapter's own name for itself: `node`, `edge`, `serverless`, `dev`. */
  readonly target: string,
  /**
   * Whether a response body reaches the client as it is produced.
   *
   * False on a target that reads the whole body before answering — which is
   * every serverless invocation in this package, because a Lambda response in
   * payload format 2.0 is a JSON value. A document survives that as a slower
   * document; an event stream does not survive it at all.
   */
  readonly stream: boolean,
  /**
   * Whether the process is still there once the response has been written.
   *
   * False for a serverless invocation, which is billed until it returns and
   * frozen afterwards, and false for a worker isolate, which the platform may
   * tear down the moment the response is out — the reason `after()` on that
   * target goes through `ctx.waitUntil` rather than being awaited.
   */
  readonly persistent: boolean,
  /** The host's upgrade, or `null` where it has none. */
  readonly websocket: WebSocketUpgrader | null,
  /** Where `enqueue` puts work, or `null` where the deployment named none. */
  readonly queue: QueueBackend | null,
|};

/** What a deployment may hand an adapter; everything else is the target's. */
export type CapabilityOptions = {|
  readonly websocket?: WebSocketUpgrader | null,
  readonly queue?: QueueBackend | null,
|};

/** Raised when a deployment is wired with something its target cannot do. */
export class CapabilityRefusedError extends Error {
  /** The adapter that refused, e.g. `serverless`. */
  target: string;
  /** The capability it was handed, e.g. `websocket`. */
  capability: string;

  constructor(target: string, capability: string, because: string) {
    super(
      `@uniflowed/server: the ${target} adapter was given a ${capability}, and ${because}. ` +
        "Remove it, or deploy this application to a target that can keep it — " +
        "`uf explain build` names the adapter a build will use.",
    );
    this.name = "CapabilityRefusedError";
    this.target = target;
    this.capability = capability;
  }
}

/**
 * Raised when a request asks for something the host answering it cannot do.
 *
 * The other half of the pair, and the one that catches what a wiring-time
 * refusal cannot: a deployment with no WebSocket handler today is wired
 * exactly as it will be on the day somebody adds one.
 */
export class CapabilityUnavailableError extends Error {
  /** The capability that was asked for, e.g. `websocket`. */
  capability: string;
  /** The adapter answering, or `unknown` outside a request that named one. */
  target: string;

  constructor(capability: string, target: string, remedy: string) {
    super(
      `@uniflowed/server: this request is being answered by the ${target} host, which has no ` +
        `${capability}. ${remedy}`,
    );
    this.name = "CapabilityUnavailableError";
    this.capability = capability;
    this.target = target;
  }
}

/** How an adapter describes itself before the deployment's half is added. */
export type CapabilityDefaults = {|
  readonly stream: boolean,
  readonly persistent: boolean,
|};

/**
 * The capabilities of `target`, with whatever the deployment supplied.
 *
 * `websocket` and `queue` default to `null` and there is no default that would
 * be better: those are the deployment's to supply, and inventing one would be
 * uf pretending to own an implementation it does not have.
 */
export function capabilitiesFor(
  target: string,
  defaults: CapabilityDefaults,
  options?: CapabilityOptions,
): ServerCapabilities {
  return {
    target,
    stream: defaults.stream,
    persistent: defaults.persistent,
    websocket: options?.websocket ?? null,
    queue: options?.queue ?? null,
  };
}

/**
 * Refuse `capabilities` this target cannot honour, or hand them back.
 *
 * What it checks is never "did the caller want a socket" but "can this target
 * keep one", which is why the rules live beside the target's own flags rather
 * than in each adapter: a fifth adapter then inherits the reasoning instead of
 * re-deriving it, and a target that gets one of the two booleans wrong is
 * wrong in one place.
 */
export function assertCapable(capabilities: ServerCapabilities): ServerCapabilities {
  if (capabilities.websocket != null && !capabilities.stream) {
    throw new CapabilityRefusedError(
      capabilities.target,
      "websocket upgrader",
      "a target that buffers a response cannot hold a socket open",
    );
  }
  const queue = capabilities.queue;
  if (queue != null && !queue.durable && !capabilities.persistent) {
    throw new CapabilityRefusedError(
      capabilities.target,
      `queue (${queue.name})`,
      "the work would be pushed into a process that ends with this response and lost with it",
    );
  }
  return capabilities;
}
