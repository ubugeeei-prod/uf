// @flow

import { afterAll, describe, expect, it } from "@uniflowed/test";
import { context, propagation, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { W3CTraceContextPropagator } from "@opentelemetry/core";
import {
  BasicTracerProvider,
  InMemorySpanExporter,
  SimpleSpanProcessor,
} from "@opentelemetry/sdk-trace-base";
import { createFetch } from "@uniflowed/fetch";
import { noteRoute } from "./host.js";
import {
  createInstrumentation,
  instrumentRender,
  reportRequestError,
  traceRequestPhase,
} from "./instrumentation.js";

const exporter = new InMemorySpanExporter();
const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(exporter)] });
const manager = new AsyncLocalStorageContextManager().enable();
trace.setGlobalTracerProvider(provider);
context.setGlobalContextManager(manager);
propagation.setGlobalPropagator(new W3CTraceContextPropagator());
afterAll(async () => {
  await provider.shutdown();
  manager.disable();
  trace.disable();
  context.disable();
  propagation.disable();
});

describe("request instrumentation", () => {
  it("awaits one startup across concurrent requests and isolates their traces", async () => {
    exporter.reset();
    let starts = 0;
    const instrumentation = createInstrumentation({
      async register() {
        starts++;
        await Promise.resolve();
      },
    });
    const client = createFetch({ fetch: async () => new Response("ok") });
    await Promise.all(
      ["/a/:id", "/b/:id"].map(async (route) => {
        const lifecycle = instrumentation.beginRequest(
          new Request("https://example.com/secret?token=secret"),
        );
        await lifecycle.run(() =>
          traceRequestPhase("render", async () => {
            expect(starts).toBe(1);
            noteRoute(route);
            await traceRequestPhase("loader", () =>
              client.raw("https://upstream.example/private?token=secret"),
            );
            return { status: 200 };
          }),
        );
        await lifecycle.settle();
        await lifecycle.settle();
      }),
    );
    const spans = exporter.getFinishedSpans();
    expect(spans.length).toBe(8);
    const requests = spans.filter((span) => span.name === "uf.request");
    expect(requests.length).toBe(2);
    expect(requests[0].spanContext().traceId === requests[1].spanContext().traceId).toBe(false);
    for (const request of requests) {
      const children = spans.filter(
        (span) => span.spanContext().traceId === request.spanContext().traceId,
      );
      const render = children.find((span) => span.name === "uf.render");
      const loader = children.find((span) => span.name === "uf.loader");
      const fetch = children.find((span) => span.name === "uf.fetch");
      expect(render.parentSpanContext.spanId).toBe(request.spanContext().spanId);
      expect(loader.parentSpanContext.spanId).toBe(render.spanContext().spanId);
      expect(fetch.parentSpanContext.spanId).toBe(loader.spanContext().spanId);
      expect(request.attributes["http.request.method"]).toBe("GET");
      expect(request.attributes["http.response.status_code"]).toBe(200);
      expect(fetch.attributes["server.address"]).toBe("upstream.example");
    }
    expect(JSON.stringify(spans.map((span) => span.attributes)).includes("secret")).toBe(false);
  });

  it("joins an incoming W3C trace and propagates the fetch span to the upstream", async () => {
    exporter.reset();
    const traceId = "11112222333344445555666677778888";
    const parentId = "1111222233334444";
    let outgoing = null;
    const client = createFetch({
      fetch: async (_url, options) => {
        outgoing = new Headers(options.headers).get("traceparent");
        return new Response("ok");
      },
    });
    const lifecycle = createInstrumentation().beginRequest(
      new Request("https://example.com/", {
        headers: { traceparent: `00-${traceId}-${parentId}-01` },
      }),
    );
    const response = await lifecycle.run(() => client.raw("https://upstream.example/"));
    await response.text();
    await lifecycle.settle();

    const spans = exporter.getFinishedSpans();
    expect(spans.length).toBe(2);
    const request = spans.find((span) => span.name === "uf.request");
    const fetch = spans.find((span) => span.name === "uf.fetch");
    expect(request.parentSpanContext.spanId).toBe(parentId);
    expect(request.parentSpanContext.isRemote).toBe(true);
    expect(request.spanContext().traceId).toBe(traceId);
    expect(fetch.parentSpanContext.spanId).toBe(request.spanContext().spanId);
    expect(outgoing).toBe(`00-${traceId}-${fetch.spanContext().spanId}-01`);
  });

  it("reports a recovered late render exception once and waits for its async hook", async () => {
    const reports = [];
    const instrumentation = createInstrumentation({
      async onRequestError(error, details) {
        await Promise.resolve();
        reports.push({ error, details });
      },
    });
    const failure = new Error("late render");
    const lifecycle = instrumentation.beginRequest(new Request("https://example.com/"));
    await lifecycle.run(async () => {
      noteRoute("/notes/:id");
      const render = () =>
        instrumentRender(async (onError) => {
          await Promise.resolve();
          onError(failure);
          return { status: 500, error: failure };
        });
      await render();
      reportRequestError(failure, "request");
    });
    await lifecycle.settle();
    expect(reports.length).toBe(1);
    expect(reports[0].error).toBe(failure);
    expect(reports[0].details.phase).toBe("render");
    expect(reports[0].details.route).toBe("/notes/:id");
  });

  it("keeps a failed startup failed without running it again or serving a request", async () => {
    let starts = 0;
    let requests = 0;
    const failure = new Error("startup failed");
    const instrumentation = createInstrumentation({
      register() {
        starts++;
        throw failure;
      },
    });
    for (let attempt = 0; attempt < 2; attempt++) {
      const lifecycle = instrumentation.beginRequest(new Request("https://example.com/"));
      let caught;
      try {
        await lifecycle.run(async () => {
          requests++;
        });
      } catch (error) {
        caught = error;
      }
      await lifecycle.settle();
      expect(caught).toBe(failure);
    }
    expect(starts).toBe(1);
    expect(requests).toBe(0);
  });

  it("keeps a streamed request open through cancellation and a late error hook alive", async () => {
    exporter.reset();
    const reports = [];
    const background = [];
    const failure = new Error("stream failed");
    const instrumentation = createInstrumentation({
      async onRequestError(error) {
        await Promise.resolve();
        reports.push(error);
      },
    });
    const lifecycle = instrumentation.beginRequest(new Request("https://example.com/"));
    lifecycle.context.waitUntil = (work) => {
      background.push(work);
    };
    const response = await lifecycle.run(
      async () =>
        new Response(
          new ReadableStream(
            {
              pull(controller) {
                controller.error(failure);
              },
            },
            { highWaterMark: 0 },
          ),
        ),
    );
    await lifecycle.settle();
    expect(exporter.getFinishedSpans().length).toBe(0);
    let caught;
    try {
      await response.text();
    } catch (error) {
      caught = error;
    }
    await Promise.all(background);
    expect(caught).toBe(failure);
    expect(reports).toEqual([failure]);
    expect(exporter.getFinishedSpans().length).toBe(1);

    const cancelled = instrumentation.beginRequest(new Request("https://example.com/"));
    let cancellations = 0;
    const stream = await cancelled.run(
      async () =>
        new Response(
          new ReadableStream({
            cancel() {
              cancellations++;
            },
          }),
        ),
    );
    await cancelled.settle();
    await stream.body.cancel();
    expect(cancellations).toBe(1);
    expect(exporter.getFinishedSpans().length).toBe(2);
  });
});
