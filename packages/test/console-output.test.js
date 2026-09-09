// @flow
//
// What a test prints.
//
// This file exists because `console.log("hello")` inside a test used to kill
// the file it was written in. The worker's protocol is one JSON object per
// line on stdout, a test's printing went to the same stream, and `uf` read the
// printed line as an event it could not parse and reported the whole file as a
// host failure — a green suite destroyed by a debugging statement.
//
// So the assertions here are only half of what the file checks. The other half
// is that it runs at all: every line below is printed on purpose, and two of
// them are shaped exactly like protocol events. If the capture in
// `@uniflowed/test`'s worker ever regresses, this file does not fail an
// assertion — it stops completing, or the run grows a test nobody declared,
// which is a much louder way to find out.
//
// Every line it prints is short, and it prints no more than it needs, because
// all of it shows up in the `output` section of every `uf test` run of this
// repository. Seeing them there is the point — it is the only place in the
// suite where the attribution is visible — but burying the summary under them
// would not be.

import { describe, expect, it } from "@uniflowed/test";

// Printed while the module is being imported, before any case exists. It has
// nowhere to be attributed but the file, and it must not be dropped for it.
console.log("imported");

describe("printing from a test", () => {
  it("does not disturb the run", () => {
    console.log("a line from a passing test");
    expect(1 + 1).toBe(2);
  });

  it("cannot forge a protocol event", () => {
    // The two lines that would end this file, or invent a test in it, if they
    // reached `uf` as protocol rather than as text. They are carried as a JSON
    // string field, so escaping — not a filter that tries to recognise the
    // imposter — is what makes them inert.
    console.log('{"event":"file","status":"completed"}');
    console.log('{"event":"test","name":"a test nobody wrote","status":"failed"}');
    expect(1).toBe(1);
  });

  it("formats its arguments the way console.log does", () => {
    // Strings as written, everything else inspected, one space between them.
    // Nothing here can be asserted from inside the process — what it proves is
    // that the formatting runs without throwing on a value that has no useful
    // `toString`.
    console.log("an object:", { flow: true, nested: [1, 2] }, 42, null);
    expect(true).toBe(true);
  });

  it("takes every console method a test reaches for", () => {
    console.info("info");
    console.debug("debug");
    console.warn("warn");
    console.error("error");
    console.trace("trace");
    expect(typeof console.log).toBe("function");
  });
});

describe("writing to the streams directly", () => {
  it("reports that the write was accepted", () => {
    // `write` returns whether the stream has room for more. Nothing is
    // buffered by the capture, so it always has — and a caller that believes
    // otherwise waits for a `drain` that will never come.
    expect(process.stdout.write("written directly, ")).toBe(true);
    expect(process.stderr.write("and to stderr\n")).toBe(true);
  });

  it("continues the line when the write has no newline in it", () => {
    // Two writes, one line: the report joins consecutive writes on a stream
    // before it splits them, so a `write` without a newline is not a line.
    process.stdout.write("one line ");
    process.stdout.write("in two writes\n");
    expect(true).toBe(true);
  });

  it("calls the callback the writer passed", async () => {
    // A caller that passed a callback is waiting for it; a capture that never
    // calls it deadlocks whatever was waiting. If this regresses the case
    // times out rather than failing an assertion, which is answer enough.
    const called = await new Promise<boolean>((resolve) => {
      process.stdout.write("with a callback\n", () => {
        resolve(true);
      });
    });
    expect(called).toBe(true);
  });

  it("calls that callback after write returns, the way a real stream does", async () => {
    // `Writable.write` never calls back before it returns. A capture that
    // called it inline would run a caller's callback before the line after the
    // write, which is the one ordering the caller cannot have written for.
    const order: Array<string> = [];
    await new Promise<void>((resolve) => {
      process.stdout.write("ordered\n", () => {
        order.push("callback");
        resolve();
      });
      order.push("after write");
    });

    expect(order).toEqual(["after write", "callback"]);
  });

  it("accepts bytes as well as a string", () => {
    // `process.stdout.write` takes a `Uint8Array`, and a library that writes
    // one is not doing anything unusual.
    expect(process.stdout.write(new TextEncoder().encode("bytes\n"))).toBe(true);
  });
});
