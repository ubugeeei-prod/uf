// An in-process PostgreSQL for `pg` and postgres.js: PGlite behind
// PGlite-socket's wire-protocol handler, reached through a duplex pair rather
// than a TCP port.
//
// A socket would be the ordinary way, and the one `@electric-sql/pglite-socket`
// documents. It is not used because a sandboxed runner (and some CI
// containers) may not bind or reach a port, and because a duplex pair makes the
// test independent of both. The cost is two internals of PGLiteSocketServer —
// `active` and `handleConnection` — which is why this file is plain JavaScript
// outside the Flow check and pinned to the version in package.json: an upgrade
// that renames either fails here, loudly, on the first connection.

import { PGLiteSocketServer } from "@electric-sql/pglite-socket";
import { duplexPair } from "node:stream";

/**
 * A factory for sockets into `db`, for `pg`'s `stream` and postgres.js's
 * `socket`, and `settled()`, which resolves once every connection has
 * detached. Closing `db` before that races the handler's own cleanup, which
 * still talks to it.
 */
export function sockets(db) {
  const server = new PGLiteSocketServer({ db, maxConnections: 64 });
  server.active = true;
  const connect = () => {
    const [client, side] = duplexPair();
    for (const socket of [client, side]) {
      socket.setNoDelay = () => socket;
      socket.setKeepAlive = () => socket;
      socket.setTimeout = () => socket;
      socket.ref = () => socket;
      socket.unref = () => socket;
    }
    // `pg` connects the stream it was given and waits for `connect`.
    client.connect = () => {
      process.nextTick(() => client.emit("connect"));
      return client;
    };
    server.handleConnection(side);
    return client;
  };
  connect.settled = async () => {
    for (let spins = 0; server.handlers.size > 0 && spins < 1000; spins += 1) {
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
  return connect;
}
