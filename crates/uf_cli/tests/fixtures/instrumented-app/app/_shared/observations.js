// @flow
const key = Symbol.for("uf.instrumentation.fixture");
globalThis[key] ??= { starts: 0, errors: [], spans: () => [] };
export const observations = globalThis[key];
