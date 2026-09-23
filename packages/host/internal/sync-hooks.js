// @noflow
//
// Plain JavaScript: this *is* the loader, so it cannot be Flow.
//
// Node's in-thread module hooks, transforming Flow on import.
//
// `node:module`'s `registerHooks` runs a hook synchronously, in the thread that
// is importing, and `../register.js` installs these wherever Node has it. The
// alternative — `register()` and `./node-hooks.js` — puts the hooks on a loader
// thread of Node's own, and that thread is not free: it is a second V8 isolate
// started before the first line of a program runs, and every module the program
// imports is a request to it and a reply back. `uf test` starts one Node
// process per worker, so it paid for that thread once per worker per run.
//
// Measured on the 50-file suite `docs/app/guide/testing` reports, on an 8-core
// M3 (4 performance cores), interleaved with the loader thread in the same
// minutes: one worker from spawn to its first file went from 61 ms to 42 ms,
// each further file from 1.9 ms to 1.8 ms, and a whole `uf test` from 181 ms to
// 121 ms of wall clock and from 1.2 s to 0.8 s of CPU. On Node 26 it also ends
// the `DEP0205` warning `register()` printed from every worker.
//
// # The one thing an in-thread hook cannot do
//
// Wait. A synchronous hook has to hand the module back before it returns, and
// compiling one is a request to `uf transform` and a reply some time later. A
// module already in `.uf/cache/transform` is a file read, which is the whole
// of a warm run and needs nothing else. A module that is not has to be
// compiled while this thread is stopped, so a miss is sent to a transform
// thread (`./transform-thread.js`) that owns the `uf transform` process, and
// this thread sleeps on a shared cell until the thread says the answer is
// waiting on the port.
//
// The transform thread is started on the first miss and never otherwise, so a
// fully warm run starts no thread and no `uf` at all — the property the
// loader thread could never have, because it existed before the question of
// whether anything needed compiling could be asked. A cold run pays for one
// thread and one `uf transform`, which is what the loader thread cost every
// run.
//
// # Deno runs these too
//
// Deno implements `registerHooks` from 2.8 and never implemented `register()`,
// so these are the only hooks it can take: `../deno-preload.js` installs them
// for `uf test`'s workers, and `@uniflowed/vite`'s driver installs them on Deno
// where it would call `register()` on Node. The hooks, the cache and the
// `"import"` condition an `import` carries hold there exactly as here. The one
// difference is how a miss waits: Deno compiles it in a short-lived child
// process rather than on the transform thread, because on Deno that thread has
// crashed the process — see `spawnedCompiler` below.
//
// One thing a registered `load` hook does on Deno and not on Node: while one is
// installed, `require()` of a native `.node` addon fails with `Invalid or
// unexpected token`, whatever the hook answers — even a hook that only calls
// `nextLoad`, and even for an addon these hooks decline. So on Deno a native
// addon has to be loaded before these are installed, which is why the driver
// installs them after its static imports have loaded Vite. That is Deno's to
// fix; see `docs/hosts.md`.

import * as nodeModule from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  TransformError,
  environmentVariable,
  isCompiledOutput,
  isFlowModule,
  transformFlowSync,
  ufBinary,
  ufBinaryIdentity,
} from "../transform.js";
import {
  cacheDirectoryFor,
  cacheEntryFor,
  compileOptions,
  compiledOutputContext,
  framed,
  readCached,
  writeCached,
} from "./flow-cache.js";

/**
 * Install the hooks for the rest of this thread, compiling under `root`.
 *
 * Returns what `registerHooks` returned, so a caller that has to take them
 * back out — a test, not `../register.js` — can.
 */
export function installFlowHooks(root) {
  // Named here rather than left to `registerHooks is not a function`, because
  // this is installed on two runtimes and the fix differs: `../register.js`
  // falls back to `register()` on an old Node, and on Deno there is nothing to
  // fall back to — `../deno-preload.js` and `@uniflowed/vite`'s driver call
  // this directly there.
  if (typeof nodeModule.registerHooks !== "function") {
    const deno = globalThis.Deno?.version?.deno;
    throw new Error(
      deno == null
        ? "@uniflowed/host's in-thread Flow loader needs `registerHooks` from `node:module`, which " +
          "Node has from 22.15 and 23.5; on an older Node, `@uniflowed/host/register` installs " +
          "the loader-thread hooks instead."
        : "@uniflowed/host's Flow loader needs `registerHooks` from `node:module`, which Deno " +
          `implemented in 2.8, and this is Deno ${deno}. Run \`deno upgrade\`, or run the ` +
          "project on Node.js or Bun.",
    );
  }
  const cacheDirectory = cacheDirectoryFor(root);
  let compiler = null;

  return nodeModule.registerHooks({
    resolve(specifier, context, nextResolve) {
      try {
        return nextResolve(specifier, context);
      } catch (error) {
        if (isTestCompilerRuntime(specifier) && error.code === "ERR_MODULE_NOT_FOUND") {
          return { url: TEST_COMPILER_RUNTIME_URL, shortCircuit: true };
        }
        throw error;
      }
    },

    load(url, context, nextLoad) {
      if (!url.startsWith("file:") || !isImport(context)) return nextLoad(url, context);
      const filename = fileURLToPath(url);
      if (isCompiledOutput(filename)) {
        return nextLoad(url, compiledOutputContext(filename, context));
      }
      if (!isFlowModule(filename)) return nextLoad(url, context);

      const source = readFileSync(filename, "utf8");
      const cached = readCached(
        cacheEntryFor(cacheDirectory, ufBinaryIdentity(), source, filename),
      );
      if (cached != null) {
        return { format: "module", source: cached, shortCircuit: true };
      }

      compiler ??= globalThis.Deno != null ? spawnedCompiler() : startCompiler();
      const reply = compiler.compile(filename, source, compileOptions(root));
      if (reply.code == null) return nextLoad(url, context);
      const output = framed(reply);
      writeCached(cacheEntryFor(cacheDirectory, reply.identity, source, filename), output);
      // uf projects are ES modules. Forcing the format here means a project
      // whose package.json forgot `"type": "module"` still runs, rather than
      // failing on an `import` in what Node would have guessed was CommonJS.
      return { format: "module", source: output, shortCircuit: true };
    },
  });
}

/**
 * Whether Node is asking about an `import` rather than a `require()`.
 *
 * The loader thread `register()` starts is never asked about a `require()` of
 * a CommonJS module — Node 22.15, 24 and 26 alike load one without consulting
 * it — so every module that loader ever claimed was one an `import` reached.
 * In-thread hooks are asked about both. Claiming a `require()` the way an
 * `import` is claimed forces a CommonJS file into an ES module:
 * `@uniflowed/react-native`'s `metro-transformer.cjs`, which Metro loads with
 * `require`, and the VS Code extension this repository tests the same way, died
 * on their first line with `require is not defined in ES module scope`. So
 * these hooks claim what the loader thread claimed, and nothing it did not.
 *
 * `conditions` is how Node says which it is. An `import` carries `"import"` on
 * every version. A `require()` carries `"require"` on Node 24 and later and an
 * empty object on 22.15, so the test is for the answer every version agrees
 * on rather than for the one that moved.
 */
function isImport(context) {
  const conditions = context?.conditions;
  return Array.isArray(conditions) && conditions.includes("import");
}

const TEST_COMPILER_RUNTIME_URL =
  "data:text/javascript;charset=utf-8," +
  encodeURIComponent(`const sentinel = Symbol.for("react.memo_cache_sentinel");
export function c(size) {
  const cache = new Array(size);
  for (let index = 0; index < size; index += 1) cache[index] = sentinel;
  return cache;
}
`);

function isTestCompilerRuntime(specifier) {
  return (
    specifier === "react/compiler-runtime" && environmentVariable("UF_IN_SOURCE_TESTS") === "1"
  );
}

/**
 * How long one wait for the transform thread sleeps before it looks again.
 *
 * Not a deadline. The thread always answers — every path out of its handler
 * posts a reply, a module that failed to load included — and a compile is
 * allowed to be as slow as the machine is busy, exactly as it was on the
 * loader thread. The bound exists so that a reply which has been announced but
 * not yet delivered to the port is looked for again rather than waited for on
 * a cell that will not change. A thread that dies without answering takes the
 * file with it, and `uf test`'s own wall clock is what ends that, the same as
 * a module that hangs while it evaluates.
 */
const WAIT_SLICE_MS = 50;

/**
 * A compiler with no thread in it: one short-lived `uf transform` per miss.
 *
 * Deno's. The hooks and the cache are the same on both runtimes; what differs
 * is how the importing thread waits for a compile. On Node it sleeps on the
 * transform thread below. On Deno that thread has crashed the process — Deno
 * 2.9.6, Linux x86_64, `Fatal error in :0: unreachable code`, in every test of
 * one CI run that compiled a module through it and in none that read the cache,
 * while a second run of the same commit passed — and a crash that depends on the
 * run is worse than a slower compile. So Deno waits on `transformFlowSync`, a
 * child process and a pipe, which is what its loader was built on and measured
 * with before the transform thread existed. A warm run starts nothing on either.
 *
 * The identity an entry is written under is the binary's as it was read just
 * before the child started, which is the binary that child executed.
 */
function spawnedCompiler() {
  return {
    compile(filename, source, options) {
      const command = ufBinary();
      const identity = ufBinaryIdentity(command);
      const out = transformFlowSync(source, filename, { ...options, command });
      return out == null ? { code: null } : { code: out.code, map: out.map, identity };
    },
  };
}

/**
 * Start the transform thread, and return the one call that talks to it.
 *
 * `execArgv` is emptied on purpose. A thread inherits the process's flags, and
 * the flag that installed these hooks is `--import @uniflowed/host/register`:
 * inherited, it would install them again in the transform thread, which would
 * then meet `../transform.js` — a module these hooks claim — on a cold cache
 * and start a transform thread of its own to compile the module that starts
 * transform threads.
 *
 * Both handles are unreferenced. The thread holds a port and a child process
 * open for as long as it lives, and a program that has finished must be able to
 * exit with a warm `uf transform` still waiting behind it; the thread ends with
 * the process, and `uf transform` ends when its stdin closes.
 */
function startCompiler() {
  const { MessageChannel, Worker, receiveMessageOnPort } =
    process.getBuiltinModule("node:worker_threads");
  const answered = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT));
  const { port1: here, port2: there } = new MessageChannel();
  const thread = new Worker(new URL("./transform-thread.js", import.meta.url), {
    workerData: { answered, port: there },
    transferList: [there],
    execArgv: [],
  });
  thread.unref();
  here.unref();

  let sequence = 0;
  return {
    compile(filename, source, options) {
      sequence += 1;
      Atomics.store(answered, 0, 0);
      here.postMessage({ sequence, filename, source, options });
      for (;;) {
        Atomics.wait(answered, 0, 0, WAIT_SLICE_MS);
        const received = receiveMessageOnPort(here);
        if (received == null) continue;
        const reply = received.message;
        // One request is ever outstanding — this thread is stopped until it
        // is answered — so a reply to any other is a thread that has lost
        // track of the conversation, and trusting it would put one module's
        // code under another module's name.
        if (reply.sequence !== sequence) {
          throw new Error(
            `@uniflowed/host's transform thread answered request ${reply.sequence} while ${sequence} was waiting`,
          );
        }
        if (reply.error != null) throw rebuilt(reply.error, filename);
        return reply;
      }
    },
  };
}

/**
 * The error the transform thread reported, as the importer should meet it.
 *
 * A `TransformError` comes back as one, with its position, because that is
 * what a load failure's code frame is drawn from. Anything else — `uf` that
 * would not start, a thread that could not load `../transform.js` — keeps its
 * name and message, which were written for a person already.
 */
function rebuilt(error, filename) {
  if (error.name === "TransformError") {
    return new TransformError(error.id ?? filename, error.message, error.line, error.column);
  }
  const plain = new Error(error.message);
  plain.name = error.name ?? "Error";
  return plain;
}
