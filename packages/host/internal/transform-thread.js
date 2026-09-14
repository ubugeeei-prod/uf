// @noflow
//
// Plain JavaScript: this compiles the loader's modules, so it cannot need one.
//
// The thread `./sync-hooks.js` sends a module to when it is not in the cache.
//
// The in-thread hooks cannot wait for `uf transform` — they have to return a
// module — so they post the request here and sleep on `answered` until this
// thread has put the reply on the port. Everything asynchronous about a
// transform happens on this thread's event loop: the `uf transform` process,
// its pipes, and the queue that pairs a reply with its request.
//
// # Every request is answered
//
// The importing thread is stopped while it waits, so it cannot notice this
// thread failing in any of the ways an event would report. The contract that
// replaces noticing is that every path out of the handler posts a reply and
// wakes the waiter: a compile error, a `uf` that would not start, and a
// `../transform.js` that could not be loaded at all. That last one is why the
// import is dynamic and inside the handler's `try` rather than a declaration at
// the top — a declaration that failed to link would end this module before the
// handler existed, and the importing thread would sleep for good.

import { workerData } from "node:worker_threads";

const { answered, port } = workerData;

/** `../transform.js`, loaded once, with a failure kept for every request. */
const transform = import("../transform.js");
// A rejection is reported to each request that awaits it; this only stops the
// rejection from being reported to the process as well, before one arrives.
transform.catch(() => {});

port.on("message", async (request) => {
  let reply;
  try {
    const { sharedService } = await transform;
    const service = sharedService(request.options.root, {
      configBootstrap: request.options.configBootstrap === true,
    });
    const out = await service.transform(request.filename, request.source, request.options);
    reply =
      out == null
        ? { sequence: request.sequence, code: null }
        : {
            sequence: request.sequence,
            code: out.code,
            map: out.map,
            // The identity of the build that compiled this, which is what the
            // entry is written under. See "The two identities" in
            // `./flow-cache.js`.
            identity: service.identity,
          };
  } catch (thrown) {
    const error = thrown instanceof Error ? thrown : new Error(String(thrown));
    reply = {
      sequence: request.sequence,
      error: {
        name: error.name,
        message: error.message,
        id: error.id ?? null,
        line: error.loc?.line ?? null,
        column: error.loc?.column ?? null,
      },
    };
  }
  port.postMessage(reply);
  Atomics.store(answered, 0, 1);
  Atomics.notify(answered, 0);
});
