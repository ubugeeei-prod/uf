// @flow
import { context, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { BasicTracerProvider, InMemorySpanExporter, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { observations } from "./_shared/observations.js";

export async function register() {
  observations.starts++;
  await Promise.resolve();
  const exporter = new InMemorySpanExporter();
  const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(exporter)] });
  trace.setGlobalTracerProvider(provider);
  context.setGlobalContextManager(new AsyncLocalStorageContextManager().enable());
  observations.spans = () => exporter.getFinishedSpans().map((span) => ({
    name: span.name,
    spanId: span.spanContext().spanId,
    traceId: span.spanContext().traceId,
    parentId: span.parentSpanContext?.spanId,
    attributes: span.attributes,
    status: span.status.code,
  }));
}

export async function onRequestError(error, context) {
  await Promise.resolve();
  observations.errors.push({ message: String(error?.message ?? error), ...context });
}
