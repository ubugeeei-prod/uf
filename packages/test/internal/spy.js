// @flow
//
// Internal to `@uniflowed/test`: the spy behind `fn` and `uft.spyOn`.
//
// Shaped after Vitest's, because a project moving to uf should not have to
// rewrite its assertions. That means `mock` holds exactly what Vitest's does,
// in the same shapes:
//
// * `calls` — one array of arguments per call, so `mock.calls[0][0]` is the
//   first call's first argument and `mock.calls.map((args) => args[0])` works
//   as it does there. It used to hold `{ args, returned }` objects, which read
//   well and broke every assertion carried over from Vitest or Jest.
// * `results` — `{ type, value }` per call: `"return"` or `"throw"`, and
//   `"incomplete"` while the call has not returned yet (a spy asked about
//   itself from inside its own implementation).
// * `settledResults` — `{ type, value }` per call once the value settled:
//   `"fulfilled"` or `"rejected"` for a promise, `"fulfilled"` at once for
//   anything else, `"incomplete"` until then.
// * `contexts` — the `this` of each call; `instances` — the same, except that
//   a call made with `new` records the instance it constructed, in both.
// * `invocationCallOrder` — a number per call, counted across every spy in the
//   process, so two spies can say which was called first.
// * `lastCall` — the last call's arguments, or `undefined` before any.
//
// And the `Once` variants, and `mockReset` and `mockRestore` meaning the two
// different things they mean there.
//
// The three reset verbs are easy to conflate and are genuinely different:
//
// * `mockClear` forgets the calls, and keeps the implementation.
// * `mockReset` forgets the calls *and* the implementation, leaving the
//   original one a `spyOn` captured — or nothing, for a bare `fn`.
// * `mockRestore` does what `mockReset` does and then puts the real method
//   back on the object, which only a `spyOn` has to put back.
//
// Every spy is registered, so `uft.clearAllMocks` and its siblings can reach the
// ones a test never held a reference to.

import { callable } from "./callable.js";

/** One call's arguments, as Vitest's `mock.calls` holds them. */
export type SpyCall = $ReadOnlyArray<mixed>;

/**
 * One call's outcome, in the shape Vitest reports it.
 *
 * `"incomplete"` is a call that has not returned yet — only ever seen from
 * inside the call itself — and its `value` is `undefined`.
 */
export type SpyResult =
  | { readonly type: "return", readonly value: mixed }
  | { readonly type: "throw", readonly value: mixed }
  | { readonly type: "incomplete", readonly value: void };

/**
 * One call's value once it settled, in the shape Vitest reports it.
 *
 * A promise settles when it does; anything else settles as the call returns.
 */
export type SpySettledResult =
  | { readonly type: "fulfilled", readonly value: mixed }
  | { readonly type: "rejected", readonly value: mixed }
  | { readonly type: "incomplete", readonly value: void };

/**
 * The order calls happened in, across every spy in the process: Vitest's
 * `invocationCallOrder`, which starts at one.
 */
let invocations = 0;

/** Every spy made in this process, so the `All` verbs can reach them. */
const registry: Array<$FlowFixMe> = [];

/** How a spy puts back what it replaced, when it replaced something. */
type Restore = null | (() => void);

/**
 * A spy, optionally standing in for something it can put back.
 *
 * `restore` is what separates `fn()` from `spyOn(object, "method")`: the second
 * took something off an object and owes it back.
 */
function makeSpy(implementation: mixed, restore: Restore, name: string): $FlowFixMe {
  const calls: Array<SpyCall> = [];
  const results: Array<$FlowFixMe> = [];
  const settledResults: Array<$FlowFixMe> = [];
  const contexts: Array<mixed> = [];
  const instances: Array<mixed> = [];
  const invocationCallOrder: Array<number> = [];
  // Implementations queued by the `Once` variants, taken from the front.
  const queued: Array<mixed> = [];

  const original = implementation;
  let current = implementation;
  let mockName = name;

  const spy: $FlowFixMe = function (this: mixed, ...args: $ReadOnlyArray<mixed>) {
    // Recorded before the implementation runs, as Vitest records it, so a spy
    // that asks about itself from inside its own call sees that call already.
    calls.push(args);
    invocations += 1;
    invocationCallOrder.push(invocations);
    const result: $FlowFixMe = { type: "incomplete", value: undefined };
    const settled: $FlowFixMe = { type: "incomplete", value: undefined };
    results.push(result);
    settledResults.push(settled);
    // `this` is recorded because a spy on a method is often called as one, and
    // `mock.contexts` is how a test asserts on the receiver. A construction has
    // no receiver yet; the instance it makes is recorded once there is one.
    const constructing = new.target !== undefined;
    const context = constructing ? undefined : this;
    const at = contexts.push(context) - 1;
    instances.push(context);
    const body = queued.length > 0 ? queued.shift() : current;
    let returned;
    try {
      if (constructing) {
        returned =
          typeof body === "function"
            ? Reflect.construct(body as $FlowFixMe, [...args], new.target)
            : this;
      } else {
        const run = callable(body);
        returned = run != null ? run.apply(this, args) : undefined;
      }
    } catch (thrown) {
      result.type = "throw";
      result.value = thrown;
      settled.type = "rejected";
      settled.value = thrown;
      throw thrown;
    }
    result.type = "return";
    result.value = returned;
    if (constructing) {
      contexts[at] = returned;
      instances[at] = returned;
    }
    if (returned instanceof Promise) {
      returned.then(
        (value) => {
          settled.type = "fulfilled";
          settled.value = value;
        },
        (reason) => {
          settled.type = "rejected";
          settled.value = reason;
        },
      );
    } else {
      settled.type = "fulfilled";
      settled.value = returned;
    }
    return returned;
  };

  spy.mock = {
    calls,
    results,
    settledResults,
    contexts,
    instances,
    invocationCallOrder,
    get lastCall(): SpyCall | void {
      return calls.length === 0 ? undefined : calls[calls.length - 1];
    },
  };

  spy.mockClear = () => {
    calls.length = 0;
    results.length = 0;
    settledResults.length = 0;
    contexts.length = 0;
    instances.length = 0;
    invocationCallOrder.length = 0;
    return spy;
  };
  spy.mockReset = () => {
    spy.mockClear();
    queued.length = 0;
    current = original;
    return spy;
  };
  spy.mockRestore = () => {
    spy.mockReset();
    if (restore != null) {
      restore();
    }
    return spy;
  };

  spy.mockImplementation = (next: mixed) => {
    current = next;
    return spy;
  };
  spy.mockImplementationOnce = (next: mixed) => {
    queued.push(next);
    return spy;
  };
  spy.withImplementation = (next: mixed, body: () => mixed) => {
    const previous = current;
    current = next;
    try {
      const out = body();
      // An async body has to put the implementation back when it settles, not
      // when it starts, or the next test runs against this one's stand-in.
      if (out != null && typeof (out as $FlowFixMe).then === "function") {
        return (out as $FlowFixMe).finally(() => {
          current = previous;
        });
      }
      current = previous;
      return out;
    } catch (thrown) {
      current = previous;
      throw thrown;
    }
  };

  spy.mockReturnValue = (value: mixed) => spy.mockImplementation(() => value);
  spy.mockReturnValueOnce = (value: mixed) => spy.mockImplementationOnce(() => value);
  spy.mockResolvedValue = (value: mixed) => spy.mockImplementation(() => Promise.resolve(value));
  spy.mockResolvedValueOnce = (value: mixed) =>
    spy.mockImplementationOnce(() => Promise.resolve(value));
  spy.mockRejectedValue = (reason: mixed) => spy.mockImplementation(() => Promise.reject(reason));
  spy.mockRejectedValueOnce = (reason: mixed) =>
    spy.mockImplementationOnce(() => Promise.reject(reason));
  spy.mockReturnThis = () =>
    spy.mockImplementation(function (this: mixed) {
      return this;
    });

  spy.mockName = (next: string) => {
    mockName = next;
    return spy;
  };
  spy.getMockName = () => mockName;

  registry.push(spy);
  return spy;
}

/**
 * A spy with no original behind it.
 *
 * `fn()` records and returns `undefined`; `fn(body)` records and runs `body`.
 */
export function fn(implementation?: mixed): $FlowFixMe {
  return makeSpy(implementation, null, "spy");
}

/**
 * Replace `object[method]` with a spy that calls through to it.
 *
 * Calls through by default, which is what makes `spyOn` an observation rather
 * than a replacement — a test that only wants to know a method was called does
 * not have to reimplement it. `mockImplementation` is how a test says it wants
 * the other thing.
 *
 * The original is put back by `mockRestore`, and by `uft.restoreAllMocks`.
 */
export function spyOn(object: mixed, method: string): $FlowFixMe {
  if (object == null || (typeof object !== "object" && typeof object !== "function")) {
    throw new TypeError(`uft.spyOn: cannot spy on ${describe(object)}`);
  }
  const target = object as $FlowFixMe;
  const original = target[method];
  if (typeof original !== "function") {
    throw new TypeError(
      `uft.spyOn: ${method} is ${describe(original)}, and only a method can be spied on`,
    );
  }

  const owned = Object.hasOwn(target, method);
  const spy = makeSpy(
    original,
    () => {
      // Deleting rather than reassigning when the method was inherited: writing
      // the original onto the instance would leave a copy the prototype no longer
      // controls, and the next change to the prototype would not be seen.
      if (owned) {
        target[method] = original;
      } else {
        delete target[method];
      }
    },
    method,
  );

  target[method] = spy;
  return spy;
}

/** Whether `value` is one of these spies. */
export function isSpy(value: mixed): boolean {
  return typeof value === "function" && (value as $FlowFixMe).mock != null;
}

/** Forget every spy's calls, keeping their implementations. */
export function clearAllMocks(): void {
  for (const spy of registry) {
    spy.mockClear();
  }
}

/** Forget every spy's calls and implementations. */
export function resetAllMocks(): void {
  for (const spy of registry) {
    spy.mockReset();
  }
}

/** Put back everything `spyOn` replaced. */
export function restoreAllMocks(): void {
  for (const spy of registry) {
    spy.mockRestore();
  }
}

/** A readable name for a value, for an error message. */
function describe(value: mixed): string {
  if (value === null) {
    return "null";
  }
  return typeof value;
}
