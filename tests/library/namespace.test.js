// @flow
//
// The `uf` namespace, and the spy behind it.
//
// Shaped after Vitest's, so most of what is asserted here is that a habit
// carried over from a Vitest project still works. The parts worth reading are
// the three reset verbs, which are easy to conflate, and `spyOn`'s restore,
// which has to put an inherited method back without leaving a copy behind.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { UnsupportedError, describe, expect, it, uft } from "@uniflowed/test";

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

describe("a binding this host cannot give", () => {
  // The seven module-mocking bindings used to be here, throwing: interception
  // belongs to the loader, and uf had not wired it. It is wired now
  // (`module-mock.test.js`), and what is left of that arrangement is the error
  // itself, which a host without synchronous module hooks still raises.
  it("names the binding and what it would take", () => {
    const error = new UnsupportedError("mock", "this host has no synchronous module hooks");

    expect(error.name).toBe("UnsupportedError");
    expect(error.binding).toBe("mock");
    expect(String(error)).toContain("uft.mock");
    expect(String(error)).toContain("this host has no synchronous module hooks");
  });
});

// Whether a stub outlives the file that set it, driven through a real worker.
//
// `uft.stubEnv` writes to `process.env` and `uft.stubGlobal` writes to
// `globalThis`, and both of those belong to the process rather than to the file
// being run. A worker serves many files out of one process, so "the stub is
// undone" is a claim about the seam between two files, and there is no way to
// see it from inside one: by the time a case could look, it is the case being
// described. The runner did not undo them at all — `unstubAllEnvs` and
// `unstubAllGlobals` were defined, exported, and called by nothing
// (ubugeeei-prod/uf#417).
//
// So this drives a worker the way `crates/uf_test/src/host.rs` does — a request
// per line on its stdin, one JSON event per line back — exactly as
// `module-mock.test.js` does for the leak one seam over, and for the same
// reason: a worker serving a second file after a first is the whole subject,
// and two workers would have nothing to confuse.
//
// **It cannot pass by luck.** `uf test` fans files across workers by size, so
// under the real scheduler these two files might never share a process and the
// bug would hide behind a timings file. Here both requests are written to one
// worker's stdin and that worker queues them strictly in order, so the second
// file always runs in the process the first one left behind. That is also why
// the bug was worth finding this way rather than waiting for it: under the
// scheduler the file that fails is the one that *read* the leaked value, not
// the one that wrote it.
//
// The fixtures go to a temporary directory rather than beside this file: `uf
// test` discovers by reading a file rather than by naming it, so a fixture in
// this workspace that registers cases would be collected and run by the very
// suite that is supposed to be running it. Nothing out there has a
// `node_modules` to resolve `@uniflowed/test` from, so they reach it by path.

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

/**
 * A variable the worker is started with, so the *restore* is tested too.
 *
 * `unstubAllEnvs` has two branches — put the old value back, or delete a name
 * that was never there — and a fixture that only stubs names nobody set
 * exercises one of them. This one is in the worker's environment before it
 * starts, so the first file replaces something real and the second file has to
 * see the original rather than the stub or nothing.
 */
const PRESENT = "UF_STUB_PRESENT";

/** A variable nothing sets, so putting it back means deleting it. */
const ABSENT = "UF_STUB_ABSENT";

/**
 * The file that stubs, and the two globals it treats differently.
 *
 * `ufStubKept` is assigned before it is stubbed, which makes it the global
 * `stubGlobal` has to *restore* rather than delete. The assignment itself is
 * the file's own doing rather than a stub, so it is deliberately still there
 * for the next file: uf puts back what uf replaced, and nothing else.
 */
const FIRST = `
describe("the file that stubs", () => {
  it("replaces what it was told to replace", () => {
    globalThis.ufStubKept = "the file wrote this";
    uft.stubEnv("${ABSENT}", "stubbed");
    uft.stubEnv("${PRESENT}", "stubbed");
    uft.stubGlobal("ufStubAdded", "stubbed");
    uft.stubGlobal("ufStubKept", "stubbed");
    console.log(
      "first " +
        [
          process.env.${ABSENT},
          process.env.${PRESENT},
          globalThis.ufStubAdded,
          globalThis.ufStubKept,
        ]
          // Rendered one by one rather than by \`join\`, which writes an absent
          // value as an empty string — and "gone" is exactly what this asserts.
          .map((value) => String(value))
          .join(" "),
    );
  });
});
`;

/** The file that observes, in the process the first one left behind. */
const SECOND = `
describe("the next file in the same worker", () => {
  it("sees the process the worker started with", () => {
    console.log(
      "second " +
        [
          process.env.${ABSENT},
          process.env.${PRESENT},
          globalThis.ufStubAdded,
          globalThis.ufStubKept,
        ]
          // Rendered one by one rather than by \`join\`, which writes an absent
          // value as an empty string — and "gone" is exactly what this asserts.
          .map((value) => String(value))
          .join(" "),
    );
  });
});
`;

/** One request, in the shape `host.rs` writes it. */
type StubRequest = {|
  readonly file: string,
  readonly timeoutMs: number,
  readonly generation: number,
|};

/** One line the worker wrote back. */
type StubEvent = { event: string, status?: string, text?: string };

/**
 * How this host starts a worker, mirroring `HostCommand::with_flow_loader`.
 *
 * The same shape as `module-mock.test.js` and `event-generation.test.js`: the
 * worker imports Flow, so it needs the host's loader, and each host registers
 * one its own way. Deno has none in `@uniflowed/host` yet, so it cannot run
 * this at all; a named failure is better than a skip that reads like a pass.
 */
function stubLoaderArguments(): Array<string> {
  const host = path.basename(process.execPath);
  if (host.startsWith("node")) {
    const register = path.join(repository, "packages", "host", "register.js");
    return ["--enable-source-maps", "--import", pathToFileURL(register).href];
  }
  if (host.startsWith("bun")) {
    return ["--preload", path.join(repository, "packages", "host", "bun-preload.js")];
  }
  throw new Error(`no Flow loader for ${host}: this test drives the worker uf would have started`);
}

/**
 * Run `requests` in one worker and collect every event it wrote.
 *
 * The environment is this process's, plus [`PRESENT`] and minus [`ABSENT`], so
 * the two names mean what the fixtures assume however the suite was started.
 * The rest is inherited because that is where `UF_PROJECT_ROOT` and `UF_BINARY`
 * are: `uf test` sets both, so the nested worker transforms through the same
 * binary and shares its transform cache instead of building a second one.
 */
function runStubsInWorker(requests: Array<StubRequest>): Promise<Array<StubEvent>> {
  const environment = { ...process.env, [PRESENT]: "the worker started with this" };
  delete environment[ABSENT];

  return new Promise((resolve, reject) => {
    const worker = path.join(repository, "packages", "test", "worker.js");
    const child = spawn(process.execPath, [...stubLoaderArguments(), worker], {
      env: environment,
      // Its stderr is the host's own noise and is not part of the report;
      // inherited so a person debugging this sees it, as `host.rs` does.
      stdio: ["pipe", "pipe", "inherit"],
    });
    let written = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.on("error", reject);
    child.on("close", () => {
      try {
        resolve(
          written
            .split("\n")
            .filter((line) => line !== "")
            .map((line) => JSON.parse(line)),
        );
      } catch (error) {
        reject(new Error(`the worker wrote something that is not an event: ${String(error)}`));
      }
    });
    child.stdin.end(requests.map((request) => `${JSON.stringify(request)}\n`).join(""));
  });
}

/**
 * How long a case that starts a worker may take.
 *
 * A Node start, the Flow loader and two file imports are seconds of work rather
 * than milliseconds, and they run beside eleven other workers, so the default
 * per-case budget is not the right measure of them.
 */
const WORKER_BUDGET = { timeout: 120_000 };

describe("a stub does not outlive the file that set it", () => {
  it(
    "is undone by the time the same worker runs the next file",
    async () => {
      const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-stub-lifetime-"));
      try {
        const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
        const write = (name: string, source: string) => {
          const file = path.join(directory, name);
          fs.writeFileSync(file, `import { describe, it, uft } from "${entry}";\n${source}`);
          return file;
        };

        const events = await runStubsInWorker([
          { file: write("first.js", FIRST), generation: 1, timeoutMs: 60_000 },
          { file: write("second.js", SECOND), generation: 2, timeoutMs: 60_000 },
        ]);

        const printed = events
          .filter((event) => event.event === "output")
          .map((event) => String(event.text).trimEnd());

        // The first line is here so that a `stubEnv` which stubbed nothing
        // could not make the second one true. The second line is the issue: the
        // name that was not there is gone rather than left holding "stubbed",
        // the one that was there is back to what the worker started with, and
        // the global the file assigned itself is untouched.
        expect(printed).toEqual([
          "first stubbed stubbed stubbed stubbed",
          "second undefined the worker started with this undefined the file wrote this",
        ]);
        expect(
          events.filter((event) => event.event === "test").map((event) => String(event.status)),
        ).toEqual(["passed", "passed"]);
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    },
    WORKER_BUDGET,
  );
});
