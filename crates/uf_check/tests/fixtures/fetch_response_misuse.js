// @flow
//
// Every numbered line here must be a type error. A libdef that typed
// `Response.json` loosely would pass `fetch_response.js` and none of these.

export function misuses(): mixed {
  // `init` is `ResponseOptions`, whose `status` is a number.
  const badStatus = Response.json({}, { status: "201" });

  // `json` is a factory, and it answers a `Response`, not the data.
  const notData: { ok: boolean } = Response.json({ ok: true });

  // The factories are statics; a signal has no `timeout` of its own.
  const signal = new AbortController().signal;
  const notAMethod = signal.timeout(10);

  return { badStatus, notData, notAMethod };
}
