// @flow
//
// Replacing a *module*: `uft.mock` and the five bindings around it.
//
// `uft.spyOn` replaces a method on an object a test can reach, which covers a
// great deal and not the cases a React application is actually made of: a
// component that imports a client and calls it while it is being evaluated, a
// page that imports `useRouter`, a module behind `@uniflowed/relay`. For those
// the import is the only seam, and this file is about the tool for it.
//
// Three rules are asserted here rather than assumed, because each one is a
// thing a person will otherwise find out by being surprised.
//
// **Nothing is hoisted.** Vitest lifts `vi.mock` above the importing file's
// `import` declarations with a Babel pass, and uf has no Babel — `docs/
// architecture.md` says so deliberately. So a static import is never affected
// by a `uft.mock` written below it, a mock takes effect for imports that begin
// after the call, and `await import(…)` is how a test reaches the stand-in.
// The first two cases in "when a mock takes effect" are that rule from both
// sides.
//
// **A mock affects the next import, not the last one.** Nothing can rewrite a
// module Node has already evaluated and linked into its importers. Bun's engine
// can, and one API meaning two things on two hosts is worse than it meaning the
// narrower one on both, so uf takes the narrower one everywhere.
//
// **Registering or removing a mock starts a new module epoch.** A module that
// imported the mocked one computed its own exports from it, so it has to be
// evaluated again too; `uft.resetModules` is the same operation under its
// Vitest name. Modules reached by a path are re-evaluated, packages are not —
// a second copy of `@uniflowed/test` would be a second registry and a second
// set of spies.
//
// The fixtures live in `fixtures/module-mock/` rather than in a temporary
// directory, because they have to be reached by a *relative specifier* from
// this file for the resolution to be the thing under test. The exception is
// "a mock does not outlive its file", which drives a real worker over two real
// files the way `event-generation.test.js` does: a fixture in this workspace
// that registers cases would be collected and run by this very suite.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

// Part of the subject, not a convenience: a `uft.mock` written below this line
// must not change what this binding holds, and the only way to assert that is
// to have the binding.
import * as staticallyImported from "../../tests/library/fixtures/module-mock/client.js";

import { hostName, unsupportedReason } from "./internal/modules.js";

const CLIENT = "../../tests/library/fixtures/module-mock/client.js";
const CONSUMER = "../../tests/library/fixtures/module-mock/consumer.js";
const COUNTER = "../../tests/library/fixtures/module-mock/counter.js";
const SHAPES = "../../tests/library/fixtures/module-mock/shapes.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

/**
 * The budget for a case that starts a process of its own.
 *
 * Two of them do: one drives a worker, one runs `uf check`. Both are seconds of
 * work rather than milliseconds, and both are running beside eleven other
 * workers, so the default per-case timeout is not the right measure of them.
 */
const SUBPROCESS_BUDGET = { timeout: 120_000 };

/**
 * A stand-in for the client that answers without a network.
 *
 * `send` is a spy with no implementation of its own and a return value on top,
 * rather than `uft.fn(() => "stubbed")`, because the two behave differently
 * under `resetAllMocks` and the difference is one of the things asserted below:
 * a reset goes back to the implementation the spy was *made* with, which for
 * this one is none.
 */
function stubClient(): { [string]: mixed } {
  return {
    BASE: "https://stub.test",
    origin: () => "https://stub.test",
    send: uft.fn().mockReturnValue("stubbed"),
  };
}

// Every mock is registered for the rest of the file until it is removed, so a
// test that leaves one behind is a test that decides what the next one sees.
afterEach(() => {
  for (const specifier of [CLIENT, CONSUMER, COUNTER, SHAPES]) {
    uft.unmock(specifier);
  }
});

describe("when a mock takes effect", () => {
  it("leaves a static import of the module alone, because nothing is hoisted", async () => {
    await uft.mock(CLIENT, stubClient);

    // The `import` declaration at the top of this file ran before the first
    // statement in it, so this binding was resolved long before the mock
    // existed. Vitest would have hoisted the mock above it; uf says what it
    // does instead, and this is that sentence as an assertion.
    expect(staticallyImported.send("/hello")).toBe("real https://api.test/hello");
  });

  it("reaches an import that begins after the call", async () => {
    await uft.mock(CLIENT, stubClient);

    const client = await import(CLIENT);

    expect(client.send("/hello")).toBe("stubbed");
    expect(client.BASE).toBe("https://stub.test");
  });

  it("reaches a module that called the mocked one while it was being evaluated", async () => {
    // The case a spy cannot reach at all: `greeting` is computed at module
    // scope, so by the time a test could patch anything it already holds what
    // the real `send` returned.
    await uft.mock(CLIENT, stubClient);

    const consumer = await import(CONSUMER);

    expect(consumer.greeting).toBe("stubbed");
    expect(consumer.base).toBe("https://stub.test");
  });

  it("stops at unmock, for imports that begin after that", async () => {
    await uft.mock(CLIENT, stubClient);
    expect((await import(CLIENT)).send("/hello")).toBe("stubbed");

    uft.unmock(CLIENT);

    expect((await import(CLIENT)).send("/hello")).toBe("real https://api.test/hello");
    // And the module that computed its exports from the mocked one is
    // evaluated again too, rather than keeping what the stand-in gave it.
    expect((await import(CONSUMER)).greeting).toBe("real https://api.test/hello");
  });

  it("is a function rather than something that throws to say it is missing", () => {
    // These seven were declared and threw `UnsupportedError` — module
    // interception belongs to the loader, and uf had not wired it. This is the
    // assertion that replaced that one.
    for (const binding of [
      "mock",
      "doMock",
      "unmock",
      "doUnmock",
      "importActual",
      "importMock",
      "resetModules",
    ]) {
      expect(typeof (uft: $FlowFixMe)[binding]).toBe("function");
    }
  });

  it("is the same operation under Vitest's un-hoisted names", () => {
    // `vi.doMock` differs from `vi.mock` only in not being hoisted. uf hoists
    // neither, so there is one function; two would be claiming a difference
    // that does not exist.
    expect(uft.doMock).toBe(uft.mock);
    expect(uft.doUnmock).toBe(uft.unmock);
  });
});

describe("the factory", () => {
  it("exports exactly what it returned, and nothing the real module has", async () => {
    await uft.mock(CLIENT, () => ({ send: () => "stubbed" }));

    const client = await import(CLIENT);

    expect(client.send()).toBe("stubbed");
    expect(Object.keys(client)).toEqual(["send"]);
  });

  it("keeps an export name that is not an identifier", async () => {
    await uft.mock(CLIENT, () => ({ "content-type": "application/json" }));

    const client = await import(CLIENT);

    expect(Object.keys(client)).toEqual(["content-type"]);
    expect((client: $FlowFixMe)["content-type"]).toBe("application/json");
  });

  it("carries a default export", async () => {
    await uft.mock(CLIENT, () => ({ default: () => "the default" }));

    const client = await import(CLIENT);

    expect((client: $FlowFixMe).default()).toBe("the default");
  });

  it("may be asynchronous, which is why the call is awaited", async () => {
    await uft.mock(CLIENT, async () => {
      await Promise.resolve();
      return { send: () => "asynchronously stubbed" };
    });

    expect((await import(CLIENT)).send()).toBe("asynchronously stubbed");
  });

  it("refuses to register something that is not a set of exports", () => {
    expect(() => uft.mock(CLIENT, (): $FlowFixMe => 42)).toThrow(
      /the factory must return the module's exports as an object/,
    );
  });

  it("keeps the exports it was given even if the object is changed afterwards", async () => {
    // The export list an ES module has is fixed when it is compiled, so it is
    // written out from the keys the factory produced. A test that went on
    // adding keys would otherwise get a module whose exports depend on when it
    // was first imported, which is not a thing anyone can reason about.
    const exports: { [string]: mixed } = { send: () => "stubbed" };
    await uft.mock(CLIENT, () => exports);
    exports.origin = () => "too late";

    expect(Object.keys(await import(CLIENT))).toEqual(["send"]);
  });
});

describe("a partial mock", () => {
  it("keeps every export the factory did not name", async () => {
    await uft.mock(CLIENT, async () => ({
      ...(await uft.importActual<$FlowFixMe>(CLIENT)),
      send: () => "stubbed",
    }));

    const client = await import(CLIENT);

    expect(client.send("/hello")).toBe("stubbed");
    // Untouched, and still the real implementation rather than a stand-in.
    expect(client.origin()).toBe("https://api.test");
    expect(client.BASE).toBe("https://api.test");
  });
});

describe("the automatic form", () => {
  it("turns every exported function into a spy that records and returns nothing", async () => {
    await uft.mock(SHAPES);

    const shapes = await import(SHAPES);

    expect(shapes.greet("uf")).toBe(undefined);
    expect((shapes.greet: $FlowFixMe).mock.calls[0].args).toEqual(["uf"]);
  });

  it("empties an array and keeps a primitive", async () => {
    await uft.mock(SHAPES);

    const shapes = await import(SHAPES);

    expect(shapes.ROLES).toEqual([]);
    expect(shapes.RETRIES).toBe(3);
  });

  it("follows a nested object", async () => {
    await uft.mock(SHAPES);

    const shapes = await import(SHAPES);

    expect(shapes.config.name).toBe("production");
    expect(shapes.config.load()).toBe(undefined);
  });

  it("keeps a class constructible, with its methods replaced", async () => {
    await uft.mock(SHAPES);

    const shapes = await import(SHAPES);
    const session = new (shapes.Session: $FlowFixMe)("abc");

    expect(session.identify()).toBe(undefined);
    expect((shapes.Session: $FlowFixMe).mock.calls.length).toBe(1);
  });
});

describe("uft.importActual", () => {
  it("reaches the real module while a stand-in is registered", async () => {
    await uft.mock(CLIENT, stubClient);

    const actual = await uft.importActual<$FlowFixMe>(CLIENT);

    expect(actual.send("/hello")).toBe("real https://api.test/hello");
    expect((await import(CLIENT)).send("/hello")).toBe("stubbed");
  });

  it("reaches past the stand-in for the module it names and for no other", async () => {
    await uft.mock(CLIENT, stubClient);

    const consumer = await uft.importActual<$FlowFixMe>(CONSUMER);

    // `consumer.js` was never mocked, so this is its real source running — and
    // the `client.js` it imports is resolved the way it is anywhere else, which
    // is to the stand-in. Anything else would make a partial mock impossible:
    // the factory asks for the real module while its own stand-in is being
    // built, and it must not get a graph with the mock cut out of it.
    expect(consumer.greeting).toBe("stubbed");
  });

  it("hands back the same module twice rather than compiling it again", async () => {
    await uft.mock(COUNTER, () => ({ bump: () => 0, current: () => 0 }));

    const first = await uft.importActual<$FlowFixMe>(COUNTER);
    first.bump();
    const second = await uft.importActual<$FlowFixMe>(COUNTER);

    expect(second.current()).toBe(1);
  });
});

describe("uft.importMock", () => {
  it("hands back the module with its functions replaced, registering nothing", async () => {
    const mocked = await uft.importMock<$FlowFixMe>(CLIENT);

    expect(mocked.send("/hello")).toBe(undefined);
    // Nothing was registered, so everyone else still gets the real module.
    expect((await import(CLIENT)).send("/hello")).toBe("real https://api.test/hello");
  });
});

describe("uft.resetModules", () => {
  it("makes the next import evaluate the module again", async () => {
    const before = await import(COUNTER);
    before.bump();
    before.bump();
    expect(before.current()).toBe(2);

    uft.resetModules();

    const after = await import(COUNTER);
    expect(after.current()).toBe(0);
    expect(after).not.toBe(before);
  });

  it("does nothing of the sort without the call", async () => {
    const before = await import(COUNTER);
    before.bump();

    const again = await import(COUNTER);

    expect(again).toBe(before);
    expect(again.current()).toBe(before.current());
  });

  it("leaves a package alone, so the runner is not loaded twice", async () => {
    const before = await import("@uniflowed/test");

    uft.resetModules();

    const after = await import("@uniflowed/test");
    // Two copies of this package would be two registries, two sets of spies,
    // and a test file talking to neither of the ones running it.
    expect(after.uft).toBe(before.uft);
    expect(after.uft).toBe(uft);
  });

  it("does not run a mock's factory again", async () => {
    // The factory ran when `uft.mock` was called; a reset moves the module to
    // a new epoch, and the module it evaluates there exports the same values.
    // Registering the mock again is how a test gets new stand-ins, and saying
    // so is better than a rule that depends on which of two things happened
    // last.
    let factories = 0;
    await uft.mock(CLIENT, () => {
      factories += 1;
      return { send: () => "stubbed" };
    });
    const before = await import(CLIENT);

    uft.resetModules();
    const after = await import(CLIENT);

    expect(factories).toBe(1);
    expect(after.send).toBe(before.send);
  });
});

describe("clear, reset and restore, over a module mock", () => {
  // The three verbs are easy to conflate and none of them is `unmock`. What
  // they reach is the *spies*, wherever those live — including the ones a
  // factory put in a module's exports — and what they never do is put the
  // module back, because a module mock is not something a spy captured.

  it("clearAllMocks forgets the calls and keeps the stand-in", async () => {
    await uft.mock(CLIENT, stubClient);
    const client = await import(CLIENT);
    client.send("/hello");

    uft.clearAllMocks();

    expect((client.send: $FlowFixMe).mock.calls).toEqual([]);
    expect(client.send("/hello")).toBe("stubbed");
    expect((await import(CLIENT)).send("/hello")).toBe("stubbed");
  });

  it("resetAllMocks forgets the implementation too, and still does not unmock", async () => {
    await uft.mock(CLIENT, stubClient);
    const client = await import(CLIENT);

    uft.resetAllMocks();

    // The spy is still the module's `send`; it has just forgotten what to do.
    expect(client.send("/hello")).toBe(undefined);
    expect((await import(CLIENT)).send).toBe(client.send);
  });

  it("restoreAllMocks puts back what spyOn took, and leaves the module mocked", async () => {
    await uft.mock(CLIENT, stubClient);
    const client = await import(CLIENT);
    const object = { greet: () => "real" };
    uft.spyOn(object, "greet").mockReturnValue("spied");
    expect(object.greet()).toBe("spied");

    uft.restoreAllMocks();

    expect(object.greet()).toBe("real");
    expect((await import(CLIENT)).send).toBe(client.send);
  });

  it("unmock is the one that puts the module back", async () => {
    await uft.mock(CLIENT, stubClient);
    expect((await import(CLIENT)).send("/hello")).toBe("stubbed");

    uft.unmock(CLIENT);

    expect((await import(CLIENT)).send("/hello")).toBe("real https://api.test/hello");
  });
});

describe("a host that cannot intercept a module", () => {
  it("is not this one", () => {
    // Node's synchronous module hooks are the whole requirement, and the suite
    // above would be meaningless if this were a host without them.
    expect(hostName()).toBe("this host");
  });

  it("would be told which host it is and what to do instead", () => {
    const reason = unsupportedReason("Bun");

    expect(reason).toContain("Bun");
    expect(reason).toContain("registerHooks");
    expect(reason).toContain("uft.spyOn");
  });
});

/** The fixture a type-checking run is given, as `<name>.js` and its source. */
const TYPED_FIXTURES: $ReadOnlyArray<[string, string]> = [
  [
    "dep.js",
    "// @flow\n" +
      "export function send(path: string): string {\n" +
      "  return path;\n" +
      "}\n" +
      'export const BASE: string = "https://api.test";\n',
  ],
  [
    // The shape the type is meant to accept: a partial mock, one export
    // replaced by something of the right type.
    "right.js",
    "// @flow\n" +
      'import typeof * as Dep from "./dep.js";\n' +
      'import { uft } from "../index.js";\n' +
      "\n" +
      "export async function replace(): Promise<void> {\n" +
      '  await uft.mock<Dep>("./dep.js", () => ({ BASE: "https://stub.test" }));\n' +
      "}\n",
  ],
  [
    // And the misuse: `BASE` is a string in the real module.
    "wrong.js",
    "// @flow\n" +
      'import typeof * as Dep from "./dep.js";\n' +
      'import { uft } from "../index.js";\n' +
      "\n" +
      "export async function replace(): Promise<void> {\n" +
      '  await uft.mock<Dep>("./dep.js", () => ({ BASE: 42 }));\n' +
      "}\n",
  ],
];

/** Run `uf check --json` over `patterns`, and hand back what it reported. */
function ufCheck(patterns: Array<string>): Promise<$FlowFixMe> {
  const binary = process.env.UF_BINARY;
  if (binary == null) {
    throw new Error("UF_BINARY is unset: `uf test` sets it, and this test needs the binary");
  }
  return new Promise((resolve, reject) => {
    const child = spawn(
      binary,
      ["check", "--cwd", repository, "--color", "never", "--json", ...patterns],
      // Its stderr is captured rather than inherited: this repository's own
      // lint reports errors, so `uf check` always ends by saying so, and a
      // line of somebody else's verdict in the middle of this suite's report
      // reads as this suite failing. It is kept for the message below, which
      // is the only place it could mean anything.
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    let written = "";
    let complaint = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      complaint += String(chunk);
    });
    child.on("error", reject);
    child.on("close", () => {
      try {
        resolve(JSON.parse(written));
      } catch (error) {
        reject(
          new Error(`uf check wrote something that is not JSON: ${String(error)}\n${complaint}`),
        );
      }
    });
  });
}

describe("a package, not only a file", () => {
  it("resolves a bare specifier to the module an import of it would reach", async () => {
    // A path is resolved as a URL; a package goes through the package resolver.
    // The two have to land on the same file, because a mock filed under a path
    // the host's own resolver never produces would never be found — and because
    // a second copy of this package would be a second registry.
    const actual = await uft.importActual<$FlowFixMe>("@uniflowed/test");

    expect(actual.uft).toBe(uft);
  });

  it("stands in for a package", async () => {
    // The cases the issue is actually about are packages: `@uniflowed/router`'s
    // `useRouter`, imported by every page, and anything behind
    // `@uniflowed/relay`. A package keeps its identity across an epoch, so
    // being stood in for is the one thing that has to take a URL of its own.
    try {
      await uft.mock("@uniflowed/brand", () => ({ ufBrand: { name: "stub" } }));

      expect((await import("@uniflowed/brand")).ufBrand.name).toBe("stub");
    } finally {
      uft.unmock("@uniflowed/brand");
    }

    expect((await import("@uniflowed/brand")).ufBrand.name).toBe("uf");
  });
});

describe("a factory is checked against the module it stands in for", () => {
  it(
    "reports the wrong type for an export as an error, and the right one as nothing",
    async () => {
      // The claim this makes good on: a Flow-first toolchain can check a mock,
      // and a mock that does not fit the module is a mistake found before the
      // suite runs rather than a `TypeError` three tests later. `import typeof *
      // as Dep` is how a test names the module's shape; the checker does the rest.
      //
      // Written to a directory of its own rather than kept in the workspace,
      // because `wrong.js` is a deliberate type error and a deliberate type error
      // that lives in the repository is one this repository's own `uf check`
      // reports forever. Inside the repository rather than in the system's
      // temporary directory, because the fixtures import `@uniflowed/test` by
      // path and the path has to lead somewhere.
      const name = ".uf-module-mock-types";
      const directory = path.join(here, name);
      // Removed before as well as after: a run that was killed between the two
      // — a timeout, a `--bail`, a Ctrl-C — would otherwise leave the wrong
      // module in the workspace, where this repository's own `uf check` would
      // go on reporting it. `.gitignore` covers the same window.
      fs.rmSync(directory, { force: true, recursive: true });
      fs.mkdirSync(directory);
      try {
        for (const [file, source] of TYPED_FIXTURES) {
          fs.writeFileSync(path.join(directory, file), source);
        }

        const report = await ufCheck([`packages/test/${name}`, "packages/test"]);
        const types = report.typeCheck;
        expect(types.status).toBe("checked");

        const named = (file: string) =>
          types.diagnostics.filter(
            (diagnostic: $FlowFixMe) => diagnostic.primary.path === `packages/test/${name}/${file}`,
          );

        expect(named("right.js")).toEqual([]);

        const [wrong, ...rest] = named("wrong.js");
        expect(rest).toEqual([]);
        expect(wrong.code).toBe("incompatible-type");
        const message = wrong.message.map((segment: $FlowFixMe) => String(segment.text)).join("");
        // The message names the call, the export and both types, which is what
        // makes it a usable error rather than a report that something is wrong.
        expect(message).toContain("uft.mock");
        expect(message).toContain("BASE");
        expect(message).toContain("42");
        expect(message).toContain("string");
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    },
    SUBPROCESS_BUDGET,
  );
});

/** What `uf` sends a worker for one file. */
type Request = {|
  readonly file: string,
  readonly timeoutMs: number,
  readonly generation: number,
|};

/** One line a worker wrote back. */
type Event = { event: string, status?: string, text?: string };

/**
 * How this host starts a worker, mirroring `HostCommand::with_flow_loader`.
 *
 * The same shape as `event-generation.test.js`, and for the same reason: what
 * is being asserted is a property of a worker serving two files, so the test
 * has to be a worker serving two files.
 */
function loaderArguments(): Array<string> {
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

/** Run `requests` in one worker, in order, and collect every event it wrote. */
function runInWorker(requests: Array<Request>): Promise<Array<Event>> {
  return new Promise((resolve, reject) => {
    const worker = path.join(repository, "packages", "test", "worker.js");
    const child = spawn(process.execPath, [...loaderArguments(), worker], {
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

describe("a mock does not outlive its file", () => {
  it(
    "is gone by the time the same worker imports the module for the next file",
    async () => {
      // A worker is reused across files and Node's module registry never
      // forgets, so "the mock is cleared" is not enough on its own: the module
      // the first file mocked must not still be sitting at the URL the second
      // file's import resolves to. Two real files, one real worker.
      const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-module-mock-"));
      try {
        const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
        fs.writeFileSync(
          path.join(directory, "client.js"),
          'export function send() {\n  return "real";\n}\n',
        );
        fs.writeFileSync(
          path.join(directory, "consumer.js"),
          'import { send } from "./client.js";\nexport const greeting = send();\n',
        );
        fs.writeFileSync(
          path.join(directory, "first.js"),
          `import { describe, it, uft } from "${entry}";\n` +
            'describe("the file that mocks", () => {\n' +
            '  it("is handed the stand-in", async () => {\n' +
            '    await uft.mock("./client.js", () => ({ send: () => "stubbed" }));\n' +
            '    const client = await import("./client.js");\n' +
            '    const consumer = await import("./consumer.js");\n' +
            "    console.log(`first ${client.send()} ${consumer.greeting}`);\n" +
            "  });\n" +
            "});\n",
        );
        fs.writeFileSync(
          path.join(directory, "second.js"),
          `import { describe, it } from "${entry}";\n` +
            'describe("the next file in the same worker", () => {\n' +
            '  it("is handed the real module", async () => {\n' +
            '    const client = await import("./client.js");\n' +
            '    const consumer = await import("./consumer.js");\n' +
            "    console.log(`second ${client.send()} ${consumer.greeting}`);\n" +
            "  });\n" +
            "});\n",
        );

        const events = await runInWorker([
          { file: path.join(directory, "first.js"), generation: 1, timeoutMs: 20_000 },
          { file: path.join(directory, "second.js"), generation: 2, timeoutMs: 20_000 },
        ]);

        const printed = events
          .filter((event) => event.event === "output")
          .map((event) => String(event.text).trimEnd());

        // The second line is the whole point: the module the first file replaced,
        // and the module that imported it, are both real again.
        expect(printed).toEqual(["first stubbed stubbed", "second real real"]);
        expect(
          events.filter((event) => event.event === "test").map((event) => String(event.status)),
        ).toEqual(["passed", "passed"]);
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    },
    SUBPROCESS_BUDGET,
  );
});
