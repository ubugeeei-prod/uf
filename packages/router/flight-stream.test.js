// @flow
//
// A Flight payload, written into the document it was rendered from.
//
// The HTML renderer writes React's payload into the document as it arrives, in
// `<script type="application/json" data-uf-flight>` chunks a browser reads back
// while the document is still streaming (ubugeeei-prod/uf#519). Two halves are
// uf's own and are tested here without a Flight renderer, because neither
// depends on what the bytes say: how a chunk of bytes becomes an element and
// back, and *where* in the document the elements go. What React writes into
// them, and that a suspending page's rows arrive after its shell, is
// `crates/uf_cli/tests/vite.rs`'s `flight_order`, against a real build.

import * as React from "react";
import { describe, expect, it } from "@uniflowed/test";

import { createChunkEncoder, flightChunkBytes } from "./internal/flight-chunks.js";
import {
  type DocumentShell,
  prerenderDocument,
  renderDocument,
  renderWithReadableStream,
} from "./internal/stream.js";

const shell: DocumentShell = {
  head: "",
  open: '<!doctype html>\n<html lang="en"><head><meta charset="utf-8">',
  body: '</head><body><div id="uf-root">',
  close: "</div></body></html>\n",
};

const END_MARKER = '<script type="application/json" data-uf-flight>null</script>';

/**
 * What may follow the end marker: the shell's close, and nothing else.
 *
 * `</body></html>` is always held back until the payload has ended. Whether the
 * root container's `</div>` goes out before the last chunks or after them
 * depends on where React's own chunks happened to split, and both places are
 * ones React's hydration steps over an element it did not render in: a child of
 * the root container, or a child of `<body>`. What can never happen is a chunk
 * after `</html>`, which the parser would move.
 */
const ENDING = /^(?:<\/div>)?<\/body><\/html>\n$/;

/** Everything after the end marker. */
const tail = (html: string): string => html.slice(html.lastIndexOf(END_MARKER) + END_MARKER.length);

/** Every chunk element in `html`, as the JSON text each one holds. */
function chunkTexts(html: string): Array<string> {
  const texts = [];
  const pattern = /<script type="application\/json" data-uf-flight>([\s\S]*?)<\/script>/g;
  let match = pattern.exec(html);
  while (match != null) {
    texts.push(match[1]);
    match = pattern.exec(html);
  }
  return texts;
}

/** The bytes a run of chunk elements carries, up to the end marker. */
function payloadOf(html: string): Uint8Array {
  const parts: Array<Uint8Array> = [];
  for (const text of chunkTexts(html)) {
    const bytes = flightChunkBytes(text);
    if (bytes == null) break;
    parts.push(bytes);
  }
  const total = parts.reduce((sum, part) => sum + part.byteLength, 0);
  const joined = new Uint8Array(total);
  let at = 0;
  for (const part of parts) {
    joined.set(part, at);
    at += part.byteLength;
  }
  return joined;
}

const encode = (text: string): Uint8Array => new TextEncoder().encode(text);
const decode = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);

/** A payload stream the test writes into by hand. */
function handPayload(): {|
  +stream: ReadableStream<Uint8Array>,
  +write: (text: string) => void,
  +end: () => void,
|} {
  let controller = null;
  const stream = new ReadableStream({
    start(opened) {
      controller = opened;
    },
  });
  return {
    stream,
    write: (text) => controller?.enqueue(encode(text)),
    end: () => controller?.close(),
  };
}

describe("a chunk of payload bytes, as an element", () => {
  it("carries a character split across two chunks to the second one", () => {
    const encoder = createChunkEncoder();
    const bytes = encode("hé!");
    // `é` is two bytes, and the first chunk ends between them.
    const first = encoder.encode(bytes.slice(0, 2));
    const second = encoder.encode(bytes.slice(2));
    expect(chunkTexts(first).map((text) => JSON.parse(text))).toEqual(["h"]);
    expect(chunkTexts(second).map((text) => JSON.parse(text))).toEqual(["é!"]);
    expect(decode(payloadOf(`${first}${second}${encoder.end()}`))).toBe("hé!");
  });

  it("writes bytes that are not text as base64, and gives back exactly those bytes", () => {
    const encoder = createChunkEncoder();
    const bytes = new Uint8Array([0xff, 0xfe, 0x00, 0x41]);
    const element = encoder.encode(bytes);
    expect(JSON.parse(chunkTexts(element)[0])).toEqual({ bytes: expect.any(String) });
    expect([...payloadOf(`${element}${encoder.end()}`)]).toEqual([...bytes]);
  });

  it("cannot be closed early by a payload that holds a closing script tag", () => {
    const encoder = createChunkEncoder();
    const hostile = '</script><script>alert("the payload")</script>';
    const element = encoder.encode(encode(hostile));
    // One element, and the only `</script>` in it is its own.
    expect(element.split("</script>").length).toBe(2);
    expect(decode(payloadOf(`${element}${encoder.end()}`))).toBe(hostile);
  });

  it("ends with the marker, which reads as no bytes at all", () => {
    const encoder = createChunkEncoder();
    expect(encoder.end()).toBe(END_MARKER);
    expect(flightChunkBytes("null")).toBe(null);
  });
});

describe("where a streamed document puts its payload", () => {
  it("writes nothing before the head, and closes the document after the end marker", async () => {
    const payload = handPayload();
    // Written before React has rendered a byte: it still has to wait for the
    // document to open, or the parser would put it before `<html>`.
    payload.write('0:{"tree":"early"}\n');
    const body = await renderDocument(<p>hello</p>, {
      shell,
      onError: () => {},
      payload: payload.stream,
    });
    const text = body.text();
    payload.write('1:{"tree":"late"}\n');
    payload.end();
    const html = await text;

    expect(html.startsWith("<!doctype html>")).toBe(true);
    expect(html.indexOf("data-uf-flight")).toBeGreaterThan(html.indexOf("<body>"));
    expect(html.indexOf(END_MARKER)).toBeGreaterThan(html.indexOf("hello"));
    expect(tail(html)).toMatch(ENDING);
    expect(decode(payloadOf(html))).toBe('0:{"tree":"early"}\n1:{"tree":"late"}\n');
  });

  it("keeps Flight rows when React removes a streamed Suspense fallback", async () => {
    const payload = handPayload();
    const row = '0:{"tree":"keep this after the fallback is removed"}\n';
    payload.write(row);
    payload.end();
    // These are Fizz's boundaries, split where a large fallback ends one of
    // React's output buffers. A chunk between tags is still inside a fallback.
    const chunks = [
      '<html><head></head><body><main><!--$?--><template id="B:0"></template><section>',
      "<p>discard this fallback</p></section><!--/$",
      '--></main><div hidden id="S:0"><p>ready</p></div></body></html>',
    ];
    let chunkIndex = 0;
    const body = await renderWithReadableStream(
      async () => ({
        getReader: () => ({
          read: async () => {
            const chunk = chunks[chunkIndex++];
            return chunk == null ? { done: true } : { done: false, value: encode(chunk) };
          },
          releaseLock: () => {},
        }),
      }),
      <p />,
      { shell, onError: () => {}, payload: payload.stream },
    );
    const html = await body.text();
    const revealed = html.replace(/<!--\$\?-->[\s\S]*?<!--\/\$-->/, "");
    expect(revealed).not.toContain("discard this fallback");
    expect(decode(payloadOf(revealed))).toBe(row);
  });

  it("holds `</html>` until the payload has ended, however long that is", async () => {
    const payload = handPayload();
    const body = await renderDocument(<p>hello</p>, {
      shell,
      onError: () => {},
      payload: payload.stream,
    });
    let finished = false;
    const text = body.text().then((html) => {
      finished = true;
      return html;
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    // The HTML is done and the payload is not, so neither is the document.
    expect(finished).toBe(false);

    payload.write('0:"the last row"\n');
    payload.end();
    const html = await text;
    expect(html.indexOf("the last row")).toBeLessThan(html.indexOf(END_MARKER));
    expect(tail(html)).toMatch(ENDING);
  });

  it("puts the same payload in the same place in a prerendered document", async () => {
    const payload = handPayload();
    payload.write('0:["a","whole","payload"]\n');
    payload.end();
    const html = await prerenderDocument(<p>hello</p>, {
      shell,
      onError: () => {},
      payload: payload.stream,
    });
    expect(html.indexOf("data-uf-flight")).toBeGreaterThan(html.indexOf("<body>"));
    expect(tail(html)).toMatch(ENDING);
    expect(decode(payloadOf(html))).toBe('0:["a","whole","payload"]\n');
  });

  // React hands its HTML on in fixed-size pieces, so a long page is split in the
  // middle of tags, attribute values and character references. A payload
  // element written after one of those pieces lands inside the markup: the
  // browser reads it as part of an attribute or a run of text, never as an
  // element, and React's client closes the payload with its rows missing.
  it("writes the payload only between elements, wherever React's pieces split", async () => {
    for (const shape of ["streamed", "prerendered"]) {
      const payload = handPayload();
      payload.write('0:["rows","the","page","needs"]\n');
      const options = { shell, onError: () => {}, payload: payload.stream };
      let html = "";
      if (shape === "streamed") {
        const text = (await renderDocument(<LongPage />, options)).text();
        payload.end();
        html = await text;
      } else {
        payload.end();
        html = await prerenderDocument(<LongPage />, options);
      }

      // Long enough to be many of React's pieces, or this checks nothing.
      expect(html.length).toBeGreaterThan(16000);
      const inside = beforeEachElement(html).filter((before) => !before.endsWith(">"));
      expect({ shape, inside }).toEqual({ shape, inside: [] });
      expect(decode(payloadOf(html))).toBe('0:["rows","the","page","needs"]\n');
      expect(tail(html)).toMatch(ENDING);
    }
  });

  // Between elements is where the parser keeps a payload element; React is
  // stricter. Hydrating, it steps over an element it did not render only among
  // the children of its root and of `<body>`. One level further in, the element
  // is a child the tree has no fiber for: the hydration fails at the frame, and
  // React discards the server's markup and renders the whole document again.
  // A page whose shell is one server-rendered frame is what finds this — React
  // ends a piece in front of every long string, so inside the frame and on a
  // `>`, which is everything the rule above asks for.
  it("writes the payload only among the children of React's root", async () => {
    for (const [shape, Page, deepest] of [
      ["wrapped", Frame, 1],
      ["document", FramedDocument, 0],
    ]) {
      const payload = handPayload();
      payload.write('0:["rows","written","before","the","frame"]\n');
      const text = (
        await renderDocument(<Page />, { shell, onError: () => {}, payload: payload.stream })
      ).text();
      payload.end();
      const html = await text;

      // Some piece has to end inside the frame, or this checks nothing.
      expect(html).toContain(`<p>${LONG_TEXT}</p>`);
      const depths = depthAtEachElement(html);
      expect(depths.length).toBeGreaterThan(0);
      expect({ shape, inside: depths.filter((depth) => depth > deepest) }).toEqual({
        shape,
        inside: [],
      });
      expect(decode(payloadOf(html))).toBe('0:["rows","written","before","the","frame"]\n');
    }
  });

  it("waits for a frame to close when a piece ends between two of its children", async () => {
    const payload = handPayload();
    const row = '0:{"tree":"the whole payload, ready before the second piece"}\n';
    payload.write(row);
    payload.end();
    // The document a signed-out `/signup` was: a boundary that finished inside
    // the shell, and a piece that ends where the sidebar does.
    const chunks = [
      '<html><head></head><body><div class="app-shell"><aside><!--$--><a href="/login">Sign in</a><!--/$--></aside>',
      '<div class="workspace"><main>page</main></div></div>',
      "</body></html>",
    ];
    let chunkIndex = 0;
    const body = await renderWithReadableStream(
      async () => ({
        getReader: () => ({
          read: async () => {
            const chunk = chunks[chunkIndex++];
            return chunk == null ? { done: true } : { done: false, value: encode(chunk) };
          },
          releaseLock: () => {},
        }),
      }),
      <p />,
      { shell, onError: () => {}, payload: payload.stream },
    );
    const html = await body.text();
    expect(depthAtEachElement(html)).toEqual([0, 0]);
    expect(html.indexOf("data-uf-flight")).toBeGreaterThan(html.indexOf("</main></div></div>"));
    expect(decode(payloadOf(html))).toBe(row);
  });
});

/** Long enough that React writes it on its own, ending the piece before it. */
const LONG_TEXT = "a frame rendered on the server ".repeat(160);

/** One element around the whole page, the way an application's shell is. */
component Frame() {
  return (
    <div className="frame">
      <aside>navigation</aside>
      {Array.from({ length: 6 }, (_, index) => (
        <p key={index}>{LONG_TEXT}</p>
      ))}
      <main>content</main>
    </div>
  );
}

/** The same frame in a document React writes, where the root is `<body>`. */
component FramedDocument() {
  return (
    <html lang="en">
      <head>
        <title>Framed</title>
      </head>
      <body>
        <Frame />
      </body>
    </html>
  );
}

/** How many elements are open inside `<body>` where each payload element is. */
function depthAtEachElement(html: string): Array<number> {
  const depths = [];
  const pattern =
    /<script type="application\/json" data-uf-flight>[\s\S]*?<\/script>|<!--[\s\S]*?-->|<(\/?)[a-z][^>]*?(\/?)>/gi;
  pattern.lastIndex = html.indexOf(">", html.search(/<body[\s>]/)) + 1;
  let depth = 0;
  let match = pattern.exec(html);
  while (match != null) {
    if (match[0].includes("data-uf-flight")) depths.push(depth);
    else if (match[1] === "/") depth -= 1;
    else if (match[1] === "" && match[2] === "") depth += 1;
    match = pattern.exec(html);
  }
  return depths;
}

/** Two hundred and forty links, each long enough to be split by React's buffer. */
component LongPage() {
  return (
    <ul>
      {Array.from({ length: 240 }, (_, index) => (
        <li key={index}>
          <a href={`/a/page/whose/path/is/long/enough/to/cross/a/piece/${index}`}>
            Page {index} &amp; the rest
          </a>
        </li>
      ))}
    </ul>
  );
}

/** The markup just before each payload element, a few characters of it. */
function beforeEachElement(html: string): Array<string> {
  const found = [];
  const pattern = /<script type="application\/json" data-uf-flight>/g;
  let match = pattern.exec(html);
  while (match != null) {
    found.push(html.slice(Math.max(0, match.index - 32), match.index));
    match = pattern.exec(html);
  }
  return found;
}
