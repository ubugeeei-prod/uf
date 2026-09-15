// @flow
//
// `@uniflowed/router/rsc/ssr`: a document, rendered from the payload React
// Server Components wrote.
//
// `virtual:uf/server` imports this entry when routes render as Server
// Components, which is the default, and `createRenderer` from
// `@uniflowed/router/server` when they render from their modules
// (`app.rsc: false`). It is an entry of its own rather than one more export of
// `./server.js` because it reads the payload with React's Flight client,
// `react-server-dom-parcel`. That package is an optional peer and needs React
// 19.3, while the rest of the router runs on the React 19.2.3 that Expo SDK 57
// and React Native 0.87 ship (ubugeeei-prod/uf#992). A bundler resolves every
// import in the graph it is given, whether or not anything calls it, so a
// server bundle that renders no Server Component leaves the package out only if
// nothing it imports names it. `crates/uf_lib/tests/package_surface.rs` holds
// the router to that.
//
// On an older React the renderer refuses to start, naming the version it found
// and the one it needs; see `./internal/react-version.js`.

import * as React from "react";

import { addressOf } from "./internal/base-path.js";
import {
  type ClientModuleLoader,
  installServerModules,
  readPayload,
} from "./internal/flight-ssr.js";
import { FLIGHT_CONTENT_TYPE, flightUrl } from "./internal/flight.js";
import { type StreamRecord, streamReporter } from "./internal/inspector.js";
import { requireServerComponentsReact } from "./internal/react-version.js";
import { RedirectError } from "./internal/routing.js";
import type { AppProps } from "./internal/runtime.js";
import { redirectDocument, redirectResult, shellFor } from "./internal/shell.js";
import { type DocumentBody, prerenderDocument, renderDocument } from "./internal/stream.js";
import type { FlightRenderer } from "./rsc.js";
import type {
  FlightResponse,
  PrerenderResult,
  RenderAssets,
  RenderOptions,
  RenderResult,
  Renderer,
} from "./server.js";

/** What `virtual:uf/server` hands the renderer for React Server Components. */
export type DocumentRendererOptions = {|
  readonly App: React.ComponentType<AppProps>,
  /** The Flight renderer, from the module graph resolved under `react-server`. */
  readonly renderFlight: FlightRenderer,
  /** The server copy of the client module at a browser chunk URL. */
  readonly loadClientModule: ClientModuleLoader,
|};

/**
 * The renderer for an application rendered by React Server Components.
 *
 * [`createRenderer`] renders a route's modules into HTML. This one renders no
 * route module at all: the Flight renderer (`./rsc.js`), in the module graph
 * resolved under `react-server`, renders the route into a payload, and this
 * reads that payload with React's own Flight client and renders what comes
 * out. The same bytes are written into the document as they arrive, so the
 * browser hydrates the tree the server rendered rather than rendering the
 * route's modules a second time — which is what keeps a Server Component's
 * module out of the browser. See ubugeeei-prod/uf#519.
 *
 * The contract with a host is [`Renderer`]'s, unchanged: `render` resolves
 * when the shell is ready with a status and a body, `prerender` resolves with a
 * finished document, and `error` is the exception a route fell back to its
 * error boundary for. It adds `flight`, which answers a browser that is
 * navigating, and `prerender` adds the payload a static host serves for one.
 *
 * # When the shell throws
 *
 * The HTML renderer only ever sees React's serialisation of what a Server
 * Component threw — in a build, a sentence and a digest — and resolving the
 * error boundary needs the exception itself: `forbidden()` is a 403 and a
 * `RedirectError` is a redirect. So the first exception the Flight renderer
 * reports is kept, as thrown, and it is what the error route is resolved for.
 * An exception only the HTML renderer saw — a client component that threw
 * while it rendered on the server — is resolved for as it is.
 */
export function createDocumentRenderer(options: DocumentRendererOptions): Renderer {
  requireServerComponentsReact("@uniflowed/router/rsc/ssr");
  const { App, renderFlight } = options;
  installServerModules(options.loadClientModule);

  /**
   * The document for one payload: React's Flight client reads one copy of the
   * stream while the other copy is written into the document.
   */
  async function documentOf(
    stream: ReadableStream<Uint8Array>,
    url: string,
    assets: RenderAssets,
    settings: {|
      readonly onError: (error: mixed) => void,
      readonly transformHead?: (html: string) => Promise<string>,
      readonly onStream?: (record: StreamRecord) => void,
    |},
  ): Promise<DocumentBody> {
    const [forHtml, forBrowser] = stream.tee();
    try {
      return await renderDocument(<App url={url} flight={readPayload(forHtml)} />, {
        shell: shellFor(assets),
        onError: settings.onError,
        transformHead: settings.transformHead,
        onStream: settings.onStream,
        payload: forBrowser,
      });
    } catch (error) {
      void forBrowser.cancel();
      throw error;
    }
  }

  async function render(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<RenderResult> {
    const report = settings?.onError ?? (() => {});
    const send = settings?.onStream;
    const onStream = send == null ? undefined : streamReporter(url, send);
    const transformHead = settings?.transformHead;
    const ledger = createErrorLedger();

    // Held until the shell is known to have survived, for the reason
    // [`createRenderer`] holds them — and dropped rather than reported once the
    // render they came from has been given up for the error boundary. That
    // render goes on reporting while it stops, and nothing it says then is
    // about the response.
    let streaming = false;
    let abandoned = false;
    let held: Array<mixed> = [];
    const onError = (error: mixed) => {
      if (abandoned) {
        return;
      }
      if (streaming) {
        report(error);
        return;
      }
      held.push(error);
    };

    const stop = new AbortController();
    const rendered = await renderFlight(url, {
      onError: (error: mixed) => {
        onError(error);
        return ledger.record(error);
      },
      signal: stop.signal,
    });
    if (rendered.kind === "redirect") {
      return redirectDocument(new RedirectError(rendered.location, rendered.status === 308));
    }
    let status = rendered.status;
    let failure = rendered.failure;
    let body: DocumentBody;
    try {
      body = await documentOf(rendered.stream, url, assets, {
        onError: (error: mixed) => {
          if (!ledger.isCopy(error)) {
            onError(error);
          }
        },
        transformHead,
        onStream,
      });
      streaming = true;
      for (const error of held) {
        report(error);
      }
      held = [];
    } catch (error) {
      abandoned = true;
      held = [];
      stop.abort();
      const cause = ledger.originalOf(error);
      if (cause instanceof RedirectError) {
        return redirectDocument(cause);
      }
      // Deliberately not caught again: this render is the boundary's own, and a
      // boundary that throws has nothing left to answer with. It reaches
      // `uf dev`'s overlay and fails `uf build`'s route, which is where somebody
      // can fix it.
      const recovered = await renderFlight(url, {
        failure: { error: cause },
        onError: (late: mixed) => {
          report(late);
          return ledger.record(late);
        },
      });
      if (recovered.kind === "redirect") {
        return redirectDocument(new RedirectError(recovered.location, recovered.status === 308));
      }
      status = recovered.status;
      failure = recovered.failure;
      try {
        body = await documentOf(recovered.stream, url, assets, {
          onError: (late: mixed) => {
            if (!ledger.isCopy(late)) {
              report(late);
            }
          },
          transformHead,
          onStream,
        });
      } catch (thrown) {
        // The boundary's own render threw; see `prerender` for why what leaves
        // is the exception as thrown rather than React's copy of it.
        throw ledger.originalOf(thrown);
      }
    }

    return { status, pipe: body.pipe, stream: body.stream, text: body.text, error: failure };
  }

  async function prerender(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<PrerenderResult> {
    const report = settings?.onError ?? (() => {});
    const ledger = createErrorLedger();
    // The Flight renderer's copy of an exception is the one reported, because it
    // is the exception as thrown; the HTML renderer's copy of the same one is
    // recognised by its digest and dropped. See [`createErrorLedger`].
    const onServerError = (error: mixed): string => {
      report(error);
      return ledger.record(error);
    };
    const onHtmlError = (error: mixed) => {
      if (!ledger.isCopy(error)) {
        report(error);
      }
    };

    // `defer: false`, for the reason [`createRenderer`]'s prerender passes it:
    // a file has no fallback to show first.
    const rendered = await renderFlight(url, { defer: false, onError: onServerError });
    if (rendered.kind === "redirect") {
      return redirectResult(
        redirectDocument(new RedirectError(rendered.location, rendered.status === 308)),
      );
    }
    let status = rendered.status;
    let failure = rendered.failure;
    let payload: Uint8Array;
    let html: string;
    try {
      // Inside the `try`, with the document: a payload the Flight renderer
      // could not finish — a function handed to a client component, say —
      // fails the stream itself, and that is this route's failure like any
      // other.
      payload = await bytesOf(rendered.stream);
      html = await staticDocumentOf(payload, url, assets, onHtmlError);
    } catch (error) {
      const cause = ledger.originalOf(error);
      if (cause instanceof RedirectError) {
        return redirectResult(redirectDocument(cause));
      }
      const recovered = await renderFlight(url, {
        defer: false,
        failure: { error: cause },
        onError: onServerError,
      });
      if (recovered.kind === "redirect") {
        return redirectResult(
          redirectDocument(new RedirectError(recovered.location, recovered.status === 308)),
        );
      }
      status = recovered.status;
      failure = recovered.failure;
      try {
        payload = await bytesOf(recovered.stream);
        html = await staticDocumentOf(payload, url, assets, onHtmlError);
      } catch (thrown) {
        // The boundary's own render threw, and nothing is left to answer with.
        // What leaves is the exception as thrown rather than the copy React's
        // client rebuilt from its row, whose message in a build says only that
        // the real one was omitted — which is all `uf build` would then print.
        throw ledger.originalOf(thrown);
      }
    }
    return { status, html, error: failure, payload };
  }

  /** A finished document for a finished payload, with the payload written into it. */
  function staticDocumentOf(
    payload: Uint8Array,
    url: string,
    assets: RenderAssets,
    onError: (error: mixed) => void,
  ): Promise<string> {
    return prerenderDocument(<App url={url} flight={readPayload(streamOf(payload))} />, {
      shell: shellFor(assets),
      onError,
      payload: streamOf(payload),
    });
  }

  async function flight(
    url: string,
    settings?: {| readonly onError?: (error: mixed) => void |},
  ): Promise<FlightResponse> {
    const rendered = await renderFlight(url, { onError: settings?.onError });
    if (rendered.kind === "redirect") {
      const { location } = rendered;
      // A redirect on this origin points at its target's payload, so the
      // browser's `fetch` follows it and lands on one. A redirect elsewhere is
      // left as written: what the browser finds there is not a payload, and it
      // loads the URL as a document instead.
      const onThisOrigin = location.startsWith("/") && !location.startsWith("//");
      return {
        status: rendered.status,
        headers: { location: onThisOrigin ? flightUrl(addressOf(location)) : location },
        stream: null,
      };
    }
    return {
      status: rendered.status,
      headers: { "content-type": FLIGHT_CONTENT_TYPE },
      stream: rendered.stream,
      error: rendered.failure,
    };
  }

  return { render, prerender, flight };
}

/**
 * Pairs each exception the Flight renderer reports with the copy of it the HTML
 * renderer sees.
 *
 * A Server Component that throws is seen twice. The Flight renderer catches the
 * exception as it was thrown and writes an error row where that part of the
 * tree would have been; React's Flight client reads the row and throws an error
 * rebuilt from it, which is what the HTML renderer sees at the same spot — and
 * in a build, that error's message is a sentence saying the real one was
 * omitted. Reporting both tells a host about one failure twice, once uselessly.
 * Resolving the error boundary for the rebuilt one is worse: `forbidden()`
 * would become a 500 and `redirect()` an error page, because neither survives
 * being rebuilt as a plain `Error`.
 *
 * The row carries a digest, and that is React's field for exactly this: what
 * the Flight renderer's `onError` returns is written into the row and set on the
 * rebuilt error as `digest`. So every exception is recorded here under a digest
 * of its own, and an error the HTML renderer reports with a recorded digest is a
 * copy — dropped from the report, and exchanged for the original when the
 * boundary is resolved.
 *
 * The digest reaches the browser inside the payload, and says nothing: it counts
 * the exceptions one render has recorded.
 */
function createErrorLedger(): ErrorLedger {
  const originals: Map<string, mixed> = new Map();
  const recorded = (error: mixed): string | null => {
    const digest = digestOf(error);
    return digest != null && originals.has(digest) ? digest : null;
  };
  return {
    record(error: mixed): string {
      const digest = `uf:${originals.size + 1}`;
      originals.set(digest, error);
      return digest;
    },
    isCopy(error: mixed): boolean {
      return recorded(error) != null;
    },
    originalOf(error: mixed): mixed {
      const digest = recorded(error);
      return digest == null ? error : originals.get(digest);
    },
  };
}

/** See [`createErrorLedger`]. */
type ErrorLedger = {|
  /** Keep an exception the Flight renderer reported, and return its digest. */
  readonly record: (error: mixed) => string,
  /** Whether an error the HTML renderer reported is the copy of a kept one. */
  readonly isCopy: (error: mixed) => boolean,
  /** The exception as thrown, for a copy; any other error, as it is. */
  readonly originalOf: (error: mixed) => mixed,
|};

/** The digest React set on an error, when it set one. */
function digestOf(error: mixed): string | null {
  if (error == null || typeof error !== "object") {
    return null;
  }
  const tagged: { +digest?: mixed, ... } = (error: $FlowFixMe);
  return typeof tagged.digest === "string" ? tagged.digest : null;
}

/** Every byte of a stream, as one array. */
async function bytesOf(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  while (true) {
    const step = await reader.read();
    if (step.done === true) {
      break;
    }
    const chunk = step.value;
    if (chunk != null) {
      chunks.push(chunk);
      total += chunk.byteLength;
    }
  }
  const bytes = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.byteLength;
  }
  return bytes;
}

/** A stream of one array, read as many times as a caller constructs one. */
function streamOf(bytes: Uint8Array): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(bytes);
      controller.close();
    },
  });
}
