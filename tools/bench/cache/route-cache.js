// @flow
//
// What the route cache is worth in front of a slow loader.
//
//   uf run bench:route-cache
//
// # Why this number exists
//
// ubugeeei-prod/uf#277 says `rendering.cache` was four booleans that reached a
// JSON file and changed nothing, and the answer to that is not "there is a
// cache now" — it is a number that says what having one does. So this is the
// smallest honest shape of the claim: an application whose page waits on a
// loader, served twice, with and without the cache the configuration now turns
// on.
//
// # What is being measured, and what is not
//
// The whole of `createFetchHandler`: the guard, the dispatcher, the render, and
// — with the cache on — buffering the document so it can become an entry. There
// is no socket and no bundler, because neither is what the cache changes, and a
// benchmark that includes them measures the machine.
//
// The loader is a `setTimeout`. That is what makes this a benchmark rather than
// a test: a suite must never sleep, and a measurement of a cache in front of
// something slow has to have something slow in front of it. `LOADER_MS` is the
// knob, and the two columns are the same page with the same loader.
//
// The uncached column is exactly what uf did before #277 — `createFetchHandler`
// with no `cache` option is byte for byte the code path it had — so the "before"
// half of the comparison needs no second checkout.
//
// # How to read it
//
// `firstMs` is a cold request: it renders, and with the cache on it also
// buffers, so it is the same or *slightly slower*. `secondMs` is the request
// after it. `coalescedRenders` is the other half of the story and the one a
// median cannot show: ten simultaneous requests for a page nobody has asked for
// yet are one render with the cache and ten without it.

import { performance } from "node:perf_hooks";
import process from "node:process";

import { cacheLife, createCacheStore } from "@uniflowed/server/cache";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";

/** How long the page's loader takes. */
const LOADER_MS: number = Number.parseInt(process.env.LOADER_MS ?? "50", 10);

/** How many times each column is measured. */
const RUNS: number = Number.parseInt(process.env.BENCH_RUNS ?? "20", 10);

/** How many simultaneous cold requests the coalescing figure uses. */
const SIMULTANEOUS = 10;

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

/** The half of a `ReadableStream` controller this fixture uses. */
type StreamController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  ...
};

function sleep(millis: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, millis);
  });
}

/**
 * A server bundle whose one page waits on a loader.
 *
 * `renders` is the count that makes the cache visible without a clock: a
 * request that did not render is a request the cache answered.
 */
function slowApp(): {|
  renders: Array<string>,
  beginRequest: typeof beginRequest,
  runMiddleware: (request: Request) => Promise<Response | null>,
  dispatch: (request: Request) => Promise<Response | null>,
  render: (url: string) => Promise<mixed>,
|} {
  const renders: Array<string> = [];
  return {
    renders,
    beginRequest,
    runMiddleware: async () => null,
    dispatch: async () => null,
    render: async (url: string) => {
      renders.push(url);
      // The loader. A page that waits on one is the only page a cache is
      // interesting for; a page that does not is a page whose render is
      // already cheaper than a `Map` lookup would save.
      await sleep(LOADER_MS);
      cacheLife({ revalidate: 60 });
      const html = `<!doctype html><title>posts</title><p>${url}</p>`;
      return {
        status: 200,
        pipe: (destination: { write: (chunk: string) => mixed, end: () => mixed, ... }) => {
          destination.write(html);
          destination.end();
        },
        stream: () =>
          new ReadableStream({
            start(controller: StreamController) {
              controller.enqueue(new TextEncoder().encode(html));
              controller.close();
            },
          }),
      };
    },
  };
}

/** Answer one request the way a host does: begin, run, settle. */
async function serve(handle, app, url: string): Promise<Response> {
  const request = new Request(`http://localhost${url}`);
  const { run, settle } = app.beginRequest(request);
  try {
    return await run(() => handle(request));
  } finally {
    await settle();
  }
}

/** The middle value, which is what a run on a shared machine has. */
function median(values: Array<number>): number {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const value =
    sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
  return Number(value.toFixed(2));
}

type Column = {|
  firstMs: number,
  secondMs: number,
  rendersPerPair: number,
  coalescedRenders: number,
|};

/**
 * One column: the same page served twice, `RUNS` times.
 *
 * A fresh store and a fresh URL per pair, so every "first" is genuinely cold
 * and nothing carries over from the run before it.
 */
async function column(cached: boolean): Promise<Column> {
  const firsts: Array<number> = [];
  const seconds: Array<number> = [];
  let renders = 0;

  for (let run = 0; run < RUNS; run += 1) {
    const app = slowApp();
    const store = createCacheStore();
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: cached ? { store, route: true, fetch: false } : undefined,
    });
    const url = `/posts/${String(run)}`;

    const coldAt = performance.now();
    await (await serve(handle, app, url)).text();
    firsts.push(performance.now() - coldAt);

    const warmAt = performance.now();
    await (await serve(handle, app, url)).text();
    seconds.push(performance.now() - warmAt);

    renders += app.renders.length;
  }

  // And what a median cannot show: a cold page asked for by everybody at once.
  const app = slowApp();
  const store = createCacheStore();
  const handle = createFetchHandler({
    app,
    document: assets,
    cache: cached ? { store, route: true, fetch: false } : undefined,
  });
  const many = [];
  for (let at = 0; at < SIMULTANEOUS; at += 1) {
    many.push(serve(handle, app, "/posts/stampede"));
  }
  await Promise.all(many);

  return {
    firstMs: median(firsts),
    secondMs: median(seconds),
    rendersPerPair: renders / RUNS,
    coalescedRenders: app.renders.length,
  };
}

async function main(): Promise<void> {
  // Warm up. The first pass over anything on a JIT measures the JIT, and a
  // benchmark that warms up quietly is a benchmark that can be accused of it.
  await column(true);

  const uncached = await column(false);
  const cached = await column(true);

  process.stdout.write(
    `${JSON.stringify(
      {
        node: process.version,
        platform: `${process.platform} ${process.arch}`,
        loaderMs: LOADER_MS,
        runs: RUNS,
        simultaneous: SIMULTANEOUS,
        results: { uncached, cached },
      },
      null,
      2,
    )}\n`,
  );
}

// Not top-level `await`: the Flow parser uf vendors does not accept it, which
// is a real limit of running a Flow entry point on the Capability JS Host and
// is worth meeting here rather than in somebody's application.
main().catch((error: mixed) => {
  process.stderr.write(`${String(error)}\n`);
  process.exitCode = 1;
});
