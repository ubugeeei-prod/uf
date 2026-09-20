// @flow

import {
  context as telemetry,
  propagation,
  SpanKind,
  SpanStatusCode,
  trace,
} from "@opentelemetry/api";
import { beginRequest, currentContext, runWithContext } from "./internal/context.js";
import { processWide } from "./internal/process-state.js";

import type { RequestContext, RequestLifecycle } from "./internal/context.js";
import type { Context, Span } from "@opentelemetry/api";

export type RequestPhase = "request" | "middleware" | "route" | "loader" | "render" | "action";

export type ErrorContext = {|
  readonly phase: RequestPhase,
  readonly method: string,
  readonly route: string | null,
  readonly requestId: string,
|};

export type Instrumentation = {|
  /** Configure the application's SDK/exporter here. Called once per module instance. */
  readonly register?: () => void | Promise<void>,
  readonly onRequestError?: (error: mixed, context: ErrorContext) => void | Promise<void>,
|};

type State = {
  hooks: Instrumentation,
  method: string,
  reported: Set<mixed>,
  span: Span,
  traceContext: Context,
};

// A server render and its RSC graph must see the same request's hooks.
const states: WeakMap<RequestContext, State> = processWide(
  "instrumentation@1",
  () => new WeakMap(),
);
const getter = {
  keys: (headers: Headers) => [...headers.keys()],
  get: (headers: Headers, key: string) => headers.get(key) ?? undefined,
};

/** Report an exception once, including errors recovered by a framework boundary. */
export function reportRequestError(error: mixed, phase: RequestPhase): void {
  const request = currentContext();
  const state = request == null ? null : states.get(request);
  if (request == null || state == null || state.reported.has(error)) return;
  state.reported.add(error);
  fail(state.span, error);

  const hook = state.hooks.onRequestError;
  if (hook == null) return;
  const details = { phase, method: state.method, route: request.route, requestId: request.id };
  // A failed observer cannot replace the response or recursively report itself.
  const pending = Promise.resolve()
    .then(() => telemetry.with(state.traceContext, () => hook(error, details)))
    .catch((failure) => {
      console.error("uf instrumentation: onRequestError failed", failure);
    });
  if (request.waitUntil != null) request.waitUntil(pending);
  else request.deferred.push(() => pending);
}

/** Trace one request phase. The API is a no-op until the application installs an SDK. */
export async function traceRequestPhase<T>(
  phase: RequestPhase,
  body: () => Promise<T>,
  expected?: (error: mixed) => boolean,
): Promise<T> {
  return trace
    .getTracer("@uniflowed/server")
    .startActiveSpan(`uf.${phase}`, async (span: Span): Promise<T> => {
      try {
        const result = await body();
        const request = currentContext();
        if (request?.route != null) span.setAttribute("http.route", request.route);
        if (result != null && typeof result === "object" && typeof result.status === "number") {
          const status = result.status;
          span.setAttribute("http.response.status_code", status);
          const root = request == null ? null : states.get(request)?.span;
          root?.setAttribute("http.response.status_code", status);
        }
        return result;
      } catch (error) {
        if (expected?.(error) !== true) {
          fail(span, error);
          reportRequestError(error, phase);
        }
        throw error;
      } finally {
        span.end();
      }
    });
}

/** One application instance: startup is shared even when its first requests race. */
export function createInstrumentation(hooks: Instrumentation = {}): {|
  beginRequest: (request: Request) => RequestLifecycle,
|} {
  let startup: Promise<void> | null = null;

  return {
    beginRequest(request) {
      const lifecycle = beginRequest(request);
      let state: ?State;
      let settled: Promise<void> | null = null;
      let bodyPending = false;
      let didSettle = false;
      let ended = false;
      function inRequest<Value>(body: () => Value): Value {
        return runWithContext(lifecycle.context, () =>
          telemetry.with(state?.traceContext ?? telemetry.active(), body),
        );
      }
      const finish = () => {
        if (bodyPending || !didSettle || ended) return;
        ended = true;
        inRequest(() => {
          if (lifecycle.context.route != null)
            state?.span.setAttribute("http.route", lifecycle.context.route);
          state?.span.end();
        });
      };

      function observeBody<T>(result: T): T {
        if (!(result instanceof Response)) return result;
        state?.span.setAttribute("http.response.status_code", result.status);
        if (result.body == null) return result;
        const reader = result.body.getReader();
        bodyPending = true;
        let closing = false;
        const complete = () => {
          if (!bodyPending) return;
          bodyPending = false;
          reader.releaseLock();
          finish();
        };
        const body = new ReadableStream(
          {
            async pull(controller) {
              try {
                const chunk = await inRequest(() => reader.read());
                if (closing) return;
                if (chunk.done) {
                  controller.close();
                  complete();
                } else controller.enqueue(chunk.value);
              } catch (error) {
                if (closing) return;
                runWithContext(lifecycle.context, () => reportRequestError(error, "request"));
                controller.error(error);
                complete();
              }
            },
            async cancel(reason) {
              closing = true;
              try {
                await inRequest(() => reader.cancel(reason));
              } finally {
                complete();
              }
            },
          },
          { highWaterMark: 0, size: () => 1 },
        );
        // Hosts return ordinary Responses; preserve the caller's generic type for other values.
        return new Response(body, {
          status: result.status,
          statusText: result.statusText,
          headers: result.headers,
        }) as $FlowFixMe;
      }

      return {
        context: lifecycle.context,
        async run<T>(body: () => Promise<T>): Promise<T> {
          startup ??= Promise.resolve().then(() => hooks.register?.());
          await startup;
          return lifecycle.run(() => {
            const parent = propagation.extract(telemetry.active(), request.headers, getter);
            const span = trace
              .getTracer("@uniflowed/server")
              .startSpan(
                "uf.request",
                {
                  kind: SpanKind.SERVER,
                  attributes: { "http.request.method": request.method },
                },
                parent,
              );
            const traceContext = trace.setSpan(parent, span);
            state = { hooks, method: request.method, reported: new Set(), span, traceContext };
            states.set(lifecycle.context, state);
            return telemetry.with(traceContext, async () => {
              try {
                const result = await body();
                return hooks.onRequestError != null || span.isRecording()
                  ? observeBody(result)
                  : result;
              } catch (error) {
                reportRequestError(error, "request");
                throw error;
              }
            });
          });
        },
        settle() {
          settled ??= Promise.resolve().then(() => {
            didSettle = true;
            finish();
            return lifecycle.settle();
          });
          return settled;
        },
      };
    },
  };
}

/** Both renderers report exceptions after their shell promise has resolved. */
export function instrumentRender<Result extends { readonly error?: mixed, ... }>(
  render: (onError: (error: mixed) => mixed) => Promise<Result>,
  report?: (error: mixed) => mixed,
): Promise<Result> {
  return traceRequestPhase<Result>("render", async (): Promise<Result> => {
    const result = await render((error) => {
      reportRequestError(error, "render");
      return report?.(error);
    });
    if (result.error != null) reportRequestError(result.error, "render");
    return result;
  });
}

function fail(span: ?Span, error: mixed): void {
  span?.setStatus({ code: SpanStatusCode.ERROR });
  if (error instanceof Error) span?.recordException(error);
}
