// @flow
//
// `@uniflowed/server/socket`: the upgrade a route handler can accept.
//
// A WebSocket handshake is a `GET` with `Upgrade: websocket` on it, so a route
// handler is already the right place to answer one — the path matching, the
// middleware above it and the `cookies()` a guard reads are all the same. What
// a handler cannot do on its own is take the connection, because taking it
// belongs to the host, and the hosts uf targets disagree about that more than
// they disagree about anything else in this package.
//
//   // app/api/room/[id]/_uf.route.js
//   // @flow
//   import { upgradeWebSocket } from "@uniflowed/server/socket";
//
//   export function GET(request: Request, context: HandlerContext): Response {
//     const { response, socket } = upgradeWebSocket(request);
//     socket.addEventListener("message", (event) => {
//       socket.send(`${context.params.id}: ${String(event.data)}`);
//     });
//     return response;
//   }
//
// # The disagreement, which is the design work
//
// | Runtime | How a socket is taken |
// | --- | --- |
// | Node | no server-side `WebSocket` at all; a library frames one over the raw socket |
// | Deno | `Deno.upgradeWebSocket(request)` answers `{ response, socket }` |
// | Bun | `server.upgrade(request)` answers a boolean; the handlers live on the server |
// | Cloudflare | `new WebSocketPair()`, `server.accept()`, and `webSocket` on a `Response` |
// | Lambda | it cannot: the connection ends with the invocation |
//
// Four spellings and one refusal, and no amount of naming makes them one API.
// So uf implements none of them. What it defines is the shape they all reduce
// to — a function from a `Request` to `{ response, socket }` — and the
// deployment supplies it. Deno's upgrade *is* already that function; Cloudflare's
// and Bun's are four lines each.
//
// That is red line 3 rather than a shortcut. A built-in that wrapped `ws` would
// make uf the thing that decides which WebSocket library a project has and
// which version of it, which is the `react-scripts` structure `docs/red-lines.md`
// is about — and it would put a native dependency in a package four runtimes
// import.
//
// # Why this one takes an argument when `cookies()` does not
//
// Everything in `@uniflowed/server` answers about the request with no argument,
// and this asks for the `Request` the handler was already given. That is not an
// inconsistency: `Deno.upgradeWebSocket` and Bun's `server.upgrade` take *the*
// request object because it is how their runtime finds the connection behind
// it. A request rebuilt from the context would carry the same headers and no
// socket, and would fail on two of the four targets in a way nothing here could
// explain. So the host's rule holds and the binding takes what the host needs.
//
// The context is still what says *whether* there is an upgrader, which is the
// half a handler genuinely cannot be passed.
//
// # Two refusals, at two different moments
//
// **When the host is wired.** `createLambdaHandler` throws if it is handed an
// upgrader, because a serverless target would accept upgrade requests and drop
// every one of them, and the moment to say so is while somebody is looking at
// the wiring. `./internal/capabilities.js` holds that rule.
//
// **When a request asks.** A host with no upgrader answers
// [`upgradeWebSocket`] with [`UpgradeUnavailableError`], naming the target. A
// deployment with no socket handler today is wired exactly as it will be on the
// day somebody adds one, so the wiring-time refusal cannot catch this and
// something has to.
//
// # What is deliberately not here
//
// **An upstream.** `docs/security.md` carries CVE-2026-44578 — SSRF through a
// WebSocket upgrade — whose shape is a request-derived value choosing which
// host to connect *out* to. Nothing here connects anywhere: the upgrade is of
// the inbound connection, the only argument is the request the host is already
// answering, and there is no address for anything to steer. A proxying upgrade
// is a different feature and needs its allow-list in the first commit rather
// than after one.
//
// **A protocol.** No rooms, no presence, no reconnection, no message envelope.
// Every one of those is a decision that would have to be right for every
// application to be worth building in here.

import { OutsideRequestError } from "./index.js";
import type { ServerCapabilities, WebSocketUpgrade } from "./internal/capabilities.js";
import { CapabilityUnavailableError } from "./internal/capabilities.js";
import { currentContext } from "./internal/context.js";

export type {
  ServerCapabilities,
  SocketEvent,
  WebSocketLike,
  WebSocketUpgrade,
  WebSocketUpgrader,
} from "./internal/capabilities.js";

export { CapabilityUnavailableError } from "./internal/capabilities.js";

/**
 * Raised when a handler asks for a socket the host cannot give it.
 *
 * Its own class rather than a `CapabilityUnavailableError` with a longer
 * message, because this is the one a person will search for — and because a
 * handler that would rather answer `426 Upgrade Required` than let a 500 out
 * needs something specific to catch.
 */
export class UpgradeUnavailableError extends CapabilityUnavailableError {
  constructor(target: string) {
    super(
      "WebSocket upgrader",
      target,
      "uf defines the upgrade and the deployment supplies it, because the runtimes spell it " +
        "four incompatible ways — pass `websocket` where the host is built. A serverless " +
        "target cannot be given one at all: the connection ends with the invocation.",
    );
    this.name = "UpgradeUnavailableError";
  }
}

/**
 * Whether this request could be upgraded.
 *
 * For a handler with something else to answer — a page that falls back to
 * polling, or a health check that reports what the deployment can do. A
 * handler with no fallback should call [`upgradeWebSocket`] and let it throw:
 * a `false` that is caught and ignored is how a missing capability becomes a
 * feature that quietly does not work.
 */
export function canUpgrade(): boolean {
  return currentContext()?.capabilities?.websocket != null;
}

/**
 * Whether `request` is asking to be upgraded to a WebSocket.
 *
 * Both values are compared case-insensitively, and `Connection` is searched
 * rather than matched because it is a comma-separated list a proxy is entitled
 * to have added to: `Connection: keep-alive, Upgrade` is what several of them
 * send, and an equality test rejects exactly the requests that arrived through
 * infrastructure.
 */
export function isUpgradeRequest(request: Request): boolean {
  const upgrade = request.headers.get("upgrade");
  const connection = request.headers.get("connection");
  return (
    upgrade != null &&
    upgrade.toLowerCase() === "websocket" &&
    connection != null &&
    connection
      .toLowerCase()
      .split(",")
      .some((token) => token.trim() === "upgrade")
  );
}

/**
 * Take this request's connection, or name the host that would not.
 *
 * The `response` has to be returned from the handler: on every runtime it is
 * what completes the handshake, and on Cloudflare it is the only way the
 * platform learns which end of the pair the worker keeps. Listeners may be
 * attached before or after returning it — the socket is accepted by the time
 * this returns.
 */
export function upgradeWebSocket(request: Request): WebSocketUpgrade {
  const context = currentContext();
  if (context == null) {
    // The same refusal every other binding in this package makes, and the same
    // class: "outside a request" is one failure with one remedy, and a second
    // spelling of it would be a second thing to search for.
    throw new OutsideRequestError("upgradeWebSocket");
  }

  const upgrader = context.capabilities?.websocket;
  if (upgrader == null) {
    throw new UpgradeUnavailableError(context.capabilities?.target ?? "unknown");
  }
  return upgrader(request);
}

/**
 * What the host answering this request can do, or `null` where none said.
 *
 * Exported for a handler that wants to report the deployment's shape rather
 * than discover it — a health check, or the `uf`-shaped answer to "why does
 * this work locally". Everything that *decides* on a capability in this
 * package reads it through a binding of its own, so that the refusal is one
 * message written once rather than an `if` in every handler.
 */
export function currentCapabilities(): ServerCapabilities | null {
  return currentContext()?.capabilities ?? null;
}
