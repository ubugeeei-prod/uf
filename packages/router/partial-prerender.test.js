// @flow
//
// A static shell, and the request's part of it.
//
// `uf build` prerenders a page that reads the request inside a `<Suspense>`
// boundary as a shell with the boundary left as a hole, and a server sends the
// shell and then resumes React on the hole (ubugeeei-prod/uf#950). The three
// halves that are uf's own are tested here without a build: the shell
// `prerenderShell` writes and where it stops, what `resumeDocument` sends and
// in what order, and how `./internal/flight-rows.js` takes the rows a read left
// behind out of a payload. That a real page's first bytes are its shell, under
// every server that serves a build, is `crates/uf_cli/tests/vite.rs`'s.

import * as React from "react";
import { describe, expect, it } from "@uniflowed/test";

import { flightChunkBytes } from "./internal/flight-chunks.js";
import { withoutErrorRows } from "./internal/flight-rows.js";
import { type DocumentShell, prerenderShell, resumeDocument } from "./internal/stream.js";

const shell: DocumentShell = {
  head: '<link rel="stylesheet" href="/app.css">',
  open: '<!doctype html>\n<html lang="en"><head><meta charset="utf-8">',
  body: '<link rel="stylesheet" href="/app.css"></head><body><div id="uf-root">',
  close: "</div></body></html>\n",
};

const END_MARKER = '<script type="application/json" data-uf-flight>null</script>';

const encode = (text: string): Uint8Array => new TextEncoder().encode(text);
const decode = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);

/** A promise nothing ever settles: what a hole waits on while the shell is written. */
const never: Promise<string> = new Promise(() => {});

/** A few turns of the event loop, which is all a render of nothing slow takes. */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 4; turn += 1) {
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

/** The part of a page that reads the request: it waits during the build. */
component Who(name: Promise<string>) {
  return <p id="hole">{`signed in as ${React.use(name)}`}</p>;
}

/** A page with a static part around one hole. */
component Page(name: Promise<string>, document: boolean) {
  const content = (
    <main>
      <h1>Your account</h1>
      <React.Suspense fallback={<p>checking who you are</p>}>
        <Who name={name} />
      </React.Suspense>
      <footer>the same for everybody</footer>
    </main>
  );
  if (!document) {
    return content;
  }
  return (
    <html lang="en">
      <head />
      <body>{content}</body>
    </html>
  );
}

/** A payload stream holding `rows`, already ended. */
function payloadOf(rows: string): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encode(rows));
      controller.close();
    },
  });
}

/** Every chunk out of a document body, as the host receives them. */
async function chunksOf(stream: ReadableStream<Uint8Array>): Promise<Array<string>> {
  const reader = stream.getReader();
  const chunks = [];
  while (true) {
    const step = await reader.read();
    if (step.done === true) return chunks;
    if (step.value != null) chunks.push(decode(step.value));
  }
  return chunks;
}

describe("a static shell", () => {
  it("is what React finished, with the hole as its fallback and no closing tags", async () => {
    const built = await prerenderShell(<Page name={never} document={true} />, {
      shell,
      onError: (error) => {
        throw error;
      },
      settle,
    });
    expect(built.html.startsWith("<!doctype html>")).toBe(true);
    expect(built.html).toContain('<link rel="stylesheet" href="/app.css">');
    expect(built.html).toContain("checking who you are");
    expect(built.html).toContain("the same for everybody");
    expect(built.html).not.toContain("signed in as");
    // React's `resume` writes a document's closing tags itself.
    expect(built.html).not.toContain("</body>");
    expect(built.close).toBe("");
    expect(built.postponed).not.toBe(null);
    // Plain JSON, which is what lets a build write it and a server read it.
    expect(JSON.parse(JSON.stringify(built.postponed))).toEqual(built.postponed);
  });

  it("stops inside uf's own container for an application that renders no document", async () => {
    const built = await prerenderShell(<Page name={never} document={false} />, {
      shell,
      onError: () => {},
      settle,
    });
    expect(built.html.startsWith(shell.open)).toBe(true);
    expect(built.html).toContain('<div id="uf-root"><main>');
    expect(built.html.endsWith(shell.close)).toBe(false);
    expect(built.close).toBe(shell.close);
    expect(built.rootDepth).toBe(1);
  });

  it("is a whole document when nothing was left waiting", async () => {
    const built = await prerenderShell(<Page name={Promise.resolve("ada")} document={true} />, {
      shell,
      onError: () => {},
      settle,
    });
    expect(built.postponed).toBe(null);
    expect(built.html).toContain("signed in as ada");
    expect(built.html).toContain("</body></html>");
  });

  it("does not report the stop as an error", async () => {
    const errors = [];
    await prerenderShell(<Page name={never} document={true} />, {
      shell,
      onError: (error) => errors.push(error),
      settle,
    });
    expect(errors).toEqual([]);
  });
});

/**
 * A shell of `Page`, then a resume of it, read the way a host reads a body:
 * the shell must be out before the request's part has happened.
 */
async function assertResumedInOrder(document: boolean): Promise<void> {
  const built = await prerenderShell(<Page name={never} document={document} />, {
    shell,
    onError: () => {},
    settle,
  });
  let answer: (name: string) => void = () => {};
  const name = new Promise<string>((resolve) => {
    answer = resolve;
  });
  const body = resumeDocument(<Page name={name} document={document} />, built, {
    onError: (error) => {
      throw error;
    },
    payload: payloadOf('0:"the request\'s payload"\n'),
  });
  const reader = body.stream().getReader();
  const first = await reader.read();
  // Nothing about the request has happened yet, and the shell is out.
  expect(decode(first.value ?? new Uint8Array())).toBe(built.html);
  answer("ada");
  const rest = [];
  while (true) {
    const step = await reader.read();
    if (step.done === true) break;
    rest.push(decode(step.value ?? new Uint8Array()));
  }
  const tail = rest.join("");
  expect(tail).toContain("signed in as ada");
  expect(tail).toContain('$RC("B:0","S:0")');
  // The payload is the request's, written after the shell and ended
  // before the document closes.
  const payload = /<script type="application\/json" data-uf-flight>(.*?)<\/script>/.exec(tail);
  expect(payload == null ? null : decode(flightChunkBytes(payload[1]) ?? new Uint8Array())).toBe(
    '0:"the request\'s payload"\n',
  );
  expect(tail.indexOf(END_MARKER)).toBeGreaterThan(-1);
  expect(tail.trimEnd().endsWith("</body></html>")).toBe(true);
  expect(tail.lastIndexOf("</body>")).toBeGreaterThan(tail.indexOf(END_MARKER));
  expect(tail.split("</body>").length).toBe(2);
}

describe("a resumed document", () => {
  it("sends the shell first and the hole after it", async () => {
    await assertResumedInOrder(true);
  });

  it("sends the shell first and the hole after it, for a document uf wraps", async () => {
    await assertResumedInOrder(false);
  });

  it("streams the host's chunks in the same order", async () => {
    const built = await prerenderShell(<Page name={never} document={true} />, {
      shell,
      onError: () => {},
      settle,
    });
    const body = resumeDocument(<Page name={Promise.resolve("bo")} document={true} />, built, {
      onError: () => {},
      payload: payloadOf('0:"rows"\n'),
    });
    const chunks = await chunksOf(body.stream());
    expect(chunks[0]).toBe(built.html);
    expect(chunks.join("")).toContain("signed in as bo");
  });
});

describe("a payload with the rows a read left behind", () => {
  const left = (digest: string): boolean => digest.startsWith("uf:postponed:");

  it("loses exactly those rows", () => {
    const rows =
      '0:["$","main",null,{"children":"$L1"}]\n' +
      '1:E{"digest":"uf:postponed:1"}\n' +
      '2:E{"digest":"uf:3"}\n' +
      '3:"kept"\n';
    const { payload, removed } = withoutErrorRows(encode(rows), left);
    expect(removed).toBe(1);
    expect(decode(payload)).toBe(
      '0:["$","main",null,{"children":"$L1"}]\n2:E{"digest":"uf:3"}\n3:"kept"\n',
    );
  });

  it("steps over a sized row whose bytes hold what looks like a row", () => {
    const text = 'a\n4:E{"digest":"uf:postponed:9"}\nb';
    const rows =
      `5:T${new TextEncoder().encode(text).byteLength.toString(16)},${text}` +
      '6:E{"digest":"uf:postponed:1"}\n' +
      '7:"after"\n';
    const { payload, removed } = withoutErrorRows(encode(rows), left);
    expect(removed).toBe(1);
    expect(decode(payload)).toBe(
      `5:T${new TextEncoder().encode(text).byteLength.toString(16)},${text}7:"after"\n`,
    );
  });

  it("gives the payload back unchanged when there is nothing to take out", () => {
    const bytes = encode('0:"nothing to see"\n');
    expect(withoutErrorRows(bytes, left).payload).toBe(bytes);
  });
});
