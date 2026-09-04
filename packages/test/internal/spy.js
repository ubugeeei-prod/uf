// @flow
//
// Internal to `@uniflowed/test`: the spy behind `fn` and `vi.spyOn`.
//
// Shaped after Vitest's, because a project moving to uf should not have to
// rewrite its assertions. That means `mock.calls`, `mock.results`,
// `mock.lastCall`, the `Once` variants, and `mockReset` and `mockRestore`
// meaning the two different things they mean there.
//
// The three reset verbs are easy to conflate and are genuinely different:
//
// * `mockClear` forgets the calls, and keeps the implementation.
// * `mockReset` forgets the calls *and* the implementation, leaving the
//   original one a `spyOn` captured — or nothing, for a bare `fn`.
// * `mockRestore` does what `mockReset` does and then puts the real method
//   back on the object, which only a `spyOn` has to put back.
//
// Every spy is registered, so `vi.clearAllMocks` and its siblings can reach the
// ones a test never held a reference to.

/** One call: what went in, and what came out. */
export type SpyCall = {
  readonly args: $ReadOnlyArray<mixed>,
  readonly returned?: mixed,
  readonly threw?: mixed,
};

/** One call's outcome, in the shape Vitest reports it. */
export type SpyResult =
  | { readonly type: "return", readonly value: mixed }
  | { readonly type: "throw", readonly value: mixed };

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
  const results: Array<SpyResult> = [];
  const instances: Array<mixed> = [];
  // Implementations queued by the `Once` variants, taken from the front.
  const queued: Array<mixed> = [];

  const original = implementation;
  let current = implementation;
  let mockName = name;

  const spy: $FlowFixMe = function (...args: $ReadOnlyArray<mixed>) {
    // `this` is recorded because a spy on a method is often called as one, and
    // `mock.instances` is how a test asserts on the receiver.
    instances.push(this);
    const body = queued.length > 0 ? queued.shift() : current;
    try {
      const returned = typeof body === "function" ? body.apply(this, args) : undefined;
      calls.push({ args, returned });
      results.push({ type: "return", value: returned });
      return returned;
    } catch (thrown) {
      calls.push({ args, threw: thrown });
      results.push({ type: "throw", value: thrown });
      throw thrown;
    }
  };

  spy.mock = {
    calls,
    results,
    instances,
    get lastCall(): $ReadOnlyArray<mixed> | void {
      return calls.length === 0 ? undefined : calls[calls.length - 1].args;
    },
  };

  spy.mockClear = () => {
    calls.length = 0;
    results.length = 0;
    instances.length = 0;
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
      if (out != null && typeof (out: $FlowFixMe).then === "function") {
        return (out: $FlowFixMe).finally(() => {
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
    spy.mockImplementation(function () {
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
 * The original is put back by `mockRestore`, and by `vi.restoreAllMocks`.
 */
export function spyOn(object: mixed, method: string): $FlowFixMe {
  if (object == null || (typeof object !== "object" && typeof object !== "function")) {
    throw new TypeError(`vi.spyOn: cannot spy on ${describe(object)}`);
  }
  const target = object as $FlowFixMe;
  const original = target[method];
  if (typeof original !== "function") {
    throw new TypeError(
      `vi.spyOn: ${method} is ${describe(original)}, and only a method can be spied on`,
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
  return typeof value === "function" && (value: $FlowFixMe).mock != null;
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
