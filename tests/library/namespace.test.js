// @flow
//
// The `uf` namespace, and the spy behind it.
//
// Shaped after Vitest's, so most of what is asserted here is that a habit
// carried over from a Vitest project still works. The parts worth reading are
// the three reset verbs, which are easy to conflate, and `spyOn`'s restore,
// which has to put an inherited method back without leaving a copy behind.

import { describe, expect, it, uft } from "@uniflowed/test";

describe("uft.fn", () => {
  it("records the calls and what they returned", () => {
    const add = uft.fn((a: number, b: number) => a + b);

    expect(add(1, 2)).toBe(3);
    expect(add(3, 4)).toBe(7);
    expect(add.mock.calls.length).toBe(2);
    expect(add.mock.calls[0].args).toEqual([1, 2]);
    expect(add.mock.results[1]).toEqual({ type: "return", value: 7 });
    expect(add.mock.lastCall).toEqual([3, 4]);
  });

  it("records a throw as a throw rather than a return", () => {
    const boom = uft.fn(() => {
      throw new Error("no");
    });

    expect(() => boom()).toThrow();
    expect(boom.mock.results[0].type).toBe("throw");
  });

  it("has no calls and no lastCall before it is called", () => {
    const spy = uft.fn();

    expect(spy.mock.calls).toEqual([]);
    expect(spy.mock.lastCall).toBe(undefined);
  });

  it("queues the Once variants and falls back to the standing one", () => {
    const spy = uft.fn().mockReturnValue("standing");
    spy.mockReturnValueOnce("first").mockReturnValueOnce("second");

    expect(spy()).toBe("first");
    expect(spy()).toBe("second");
    expect(spy()).toBe("standing");
    expect(spy()).toBe("standing");
  });

  it("resolves and rejects", async () => {
    const spy = uft.fn().mockResolvedValue(1);

    expect(await spy()).toBe(1);

    const failing = uft.fn().mockRejectedValue(new Error("nope"));
    let thrown = null;
    try {
      await failing();
    } catch (error) {
      thrown = error;
    }
    expect(thrown).not.toBe(null);
  });

  it("carries a name", () => {
    const spy = uft.fn().mockName("send");

    expect(spy.getMockName()).toBe("send");
  });
});

describe("the three reset verbs", () => {
  it("mockClear forgets the calls and keeps the implementation", () => {
    const spy = uft.fn().mockReturnValue("kept");
    spy();

    spy.mockClear();

    expect(spy.mock.calls).toEqual([]);
    expect(spy()).toBe("kept");
  });

  it("mockReset forgets the implementation too", () => {
    const spy = uft.fn().mockReturnValue("gone");
    spy();

    spy.mockReset();

    expect(spy.mock.calls).toEqual([]);
    expect(spy()).toBe(undefined);
  });

  it("mockReset restores the original a spyOn captured", () => {
    const object = { greet: () => "real" };
    const spy = uft.spyOn(object, "greet").mockReturnValue("stubbed");

    expect(object.greet()).toBe("stubbed");
    spy.mockReset();
    expect(object.greet()).toBe("real");
  });
});

describe("uft.spyOn", () => {
  it("calls through by default, so watching is not replacing", () => {
    const object = { greet: (name: string) => `hello ${name}` };
    const spy = uft.spyOn(object, "greet");

    expect(object.greet("uf")).toBe("hello uf");
    expect(spy.mock.calls.length).toBe(1);
  });

  it("puts the method back on restore", () => {
    const object = { greet: () => "real" };
    const original = object.greet;
    const spy = uft.spyOn(object, "greet").mockReturnValue("stubbed");

    expect(object.greet()).toBe("stubbed");
    spy.mockRestore();
    expect(object.greet).toBe(original);
  });

  it("removes an inherited method rather than copying it onto the instance", () => {
    // Reassigning would leave a copy the prototype no longer controls, and the
    // next change to the prototype would not be seen.
    const prototype = { greet: () => "from the prototype" };
    const object = Object.create(prototype);

    const spy = uft.spyOn(object, "greet");
    spy.mockRestore();

    expect(Object.hasOwn(object, "greet")).toBe(false);
    expect(object.greet()).toBe("from the prototype");
  });

  it("refuses what cannot be spied on, and says what it found", () => {
    expect(() => uft.spyOn(null, "x")).toThrow();
    expect(() => uft.spyOn({ a: 1 }, "a")).toThrow();
  });

  it("records the receiver", () => {
    const object = {
      value: 7,
      read() {
        return this.value;
      },
    };
    uft.spyOn(object, "read");

    object.read();

    expect(object.read.mock.instances[0]).toBe(object);
  });
});

describe("stubbing the environment", () => {
  it("replaces a variable and puts it back", () => {
    const before = process.env.UF_VI_TEST;

    uft.stubEnv("UF_VI_TEST", "stubbed");
    expect(process.env.UF_VI_TEST).toBe("stubbed");

    uft.unstubAllEnvs();
    expect(process.env.UF_VI_TEST).toBe(before);
  });

  it("removes a variable when the value is undefined", () => {
    uft.stubEnv("UF_VI_GONE", "here");
    uft.stubEnv("UF_VI_GONE", undefined);

    expect(process.env.UF_VI_GONE).toBe(undefined);
    uft.unstubAllEnvs();
  });

  it("remembers the first value across repeated stubs", () => {
    uft.stubEnv("UF_VI_ONCE", "one");
    uft.stubEnv("UF_VI_ONCE", "two");

    uft.unstubAllEnvs();

    expect(process.env.UF_VI_ONCE).toBe(undefined);
  });
});

describe("stubbing a global", () => {
  it("replaces and puts back", () => {
    uft.stubGlobal("__ufViProbe", 1);
    expect((globalThis: $FlowFixMe).__ufViProbe).toBe(1);

    uft.unstubAllGlobals();
    expect(Object.hasOwn(globalThis, "__ufViProbe")).toBe(false);
  });
});

describe("uft.waitFor", () => {
  it("returns once the body stops throwing", async () => {
    let attempts = 0;

    const out = await uft.waitFor(
      () => {
        attempts += 1;
        if (attempts < 3) {
          throw new Error("not yet");
        }
        return attempts;
      },
      { interval: 1 },
    );

    expect(out).toBe(3);
  });

  it("raises the last failure rather than a timeout", async () => {
    // "expected 2, got 1" says what went wrong; "timed out" says only that
    // something did.
    let thrown = null;
    try {
      await uft.waitFor(
        () => {
          throw new Error("the real reason");
        },
        { timeout: 20, interval: 1 },
      );
    } catch (error) {
      thrown = error;
    }

    expect(String(thrown)).toContain("the real reason");
  });

  it("waitUntil waits for something truthy", async () => {
    let ready = false;
    setTimeout(() => {
      ready = true;
    }, 5);

    expect(await uft.waitUntil(() => ready, { interval: 1 })).toBe(true);
  });
});

describe("what is deliberately missing", () => {
  it("says what uft.mock would take rather than doing nothing", () => {
    // A binding that silently did nothing would be worse than not having it: a
    // test would pass while mocking nothing at all.
    for (const binding of ["mock", "unmock", "importActual", "importMock", "resetModules"]) {
      expect(() => (uft: $FlowFixMe)[binding]()).toThrow();
    }

    let message = "";
    try {
      uft.mock("./somewhere.js");
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("loader");
  });
});
