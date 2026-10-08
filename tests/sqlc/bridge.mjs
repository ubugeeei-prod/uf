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

const COPY_DATA = 0x64;
const COPY_DONE = 0x63;
const COPY_FAIL = 0x66;
const QUERY = 0x51;

const wrapped = new WeakSet();

/**
 * PGlite runs `COPY FROM STDIN` only when the query, the `CopyData` and the
 * `CopyDone` arrive in one `execProtocolRawStream` call. A query on its own
 * makes the backend exit. The socket handler delivers one frontend message
 * per call and will not read the next until that call returns, so an
 * interactive `COPY` deadlocks.
 *
 * Answer `CopyInResponse` immediately, hold the data messages, and on
 * `CopyDone` run the whole exchange as one call. Drop the `CopyInResponse`
 * that call produces: the client already has the one sent up front. A real
 * PostgreSQL does not need this; the adapters still speak the ordinary
 * protocol on the socket.
 */
function coalesceCopy(db) {
  if (wrapped.has(db) || typeof db.execProtocolRawStream !== "function") {
    return;
  }
  wrapped.add(db);
  const original = db.execProtocolRawStream.bind(db);
  let pending = null;

  db.execProtocolRawStream = (message, options) => {
    const bytes = Buffer.from(message);
    const kind = bytes[0];
    if (pending !== null && kind !== COPY_DATA && kind !== COPY_DONE && kind !== COPY_FAIL) {
      pending = null;
    }
    if (kind === QUERY && copyFromStdin(bytes)) {
      pending = { query: bytes, data: [] };
      options?.onRawData?.(copyInResponse());
      return Promise.resolve();
    }
    if (pending !== null && kind === COPY_DATA) {
      pending.data.push(bytes);
      return Promise.resolve();
    }
    if (pending !== null && (kind === COPY_DONE || kind === COPY_FAIL)) {
      const { query, data } = pending;
      pending = null;
      return original(Buffer.concat([query, ...data, bytes]), {
        ...options,
        onRawData: forwardWithoutCopyIn(options?.onRawData),
      });
    }
    return original(message, options);
  };
}

function copyFromStdin(message) {
  const sql = message.toString("utf8", 5, message.length - 1);
  return /^COPY\s/i.test(sql) && /\bFROM\s+STDIN\b/i.test(sql);
}

/** Text `CopyInResponse` with no per-column codes. `pg` does not read them. */
function copyInResponse() {
  const message = Buffer.alloc(8);
  message[0] = 0x47;
  message.writeInt32BE(7, 1);
  message.writeInt16BE(0, 6);
  return message;
}

function forwardWithoutCopyIn(onRawData) {
  let skipped = false;
  let rest = Buffer.alloc(0);
  return (chunk) => {
    rest = Buffer.concat([rest, Buffer.from(chunk)]);
    const emit = [];
    while (rest.length >= 5) {
      const total = 1 + rest.readInt32BE(1);
      if (rest.length < total) {
        break;
      }
      const message = rest.subarray(0, total);
      rest = rest.subarray(total);
      if (!skipped && message[0] === 0x47) {
        skipped = true;
        continue;
      }
      skipped = true;
      emit.push(message);
    }
    if (emit.length > 0 && onRawData !== undefined) {
      onRawData(Buffer.concat(emit));
    }
  };
}

/**
 * A factory for sockets into `db`, for `pg`'s `stream` and postgres.js's
 * `socket`, and `settled()`, which resolves once every connection has
 * detached. Closing `db` before that races the handler's own cleanup, which
 * still talks to it.
 */
export function sockets(db) {
  coalesceCopy(db);
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
