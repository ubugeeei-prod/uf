// @flow
//
// The payload, and the rows that arrive after the document did.
//
// `streaming.test.js` covers the half of ubugeeei-prod/uf#519 that was already
// true: a page that suspends sends its layouts and its fallback first, and the
// content follows. What it could not cover is the other half — the loader's
// *data* was one `JSON.stringify` of one finished value, so a page whose data
// was one slow thing and four fast ones said nothing about the four until the
// one arrived, and a promise anywhere in it became `{}` without a word.
//
// So this file asserts four claims, in the order they have to be true in:
//
//   1. the format round-trips, and refuses what it says it refuses;
//   2. a browser can read rows out of a document that is still arriving;
//   3. the bytes arrive in the right order — the model before any row, and the
//      row that resolved first before the row that was written first;
//   4. the browser applies them, and each boundary resolves on its own.
//
// # Why (3) is asserted on chunks, and without a clock
//
// "The model went out before the value did" is a claim about order, and a
// document that is correct at the end and streamed nothing would pass every
// assertion made on the finished string. So the assertions are made on the
// chunks: the first one is read while neither promise has settled and nothing
// is scheduled to settle them, and only then is one of them resolved.
//
// `streaming.test.js` makes the same claim against a deadline — "the shell was
// out before 120 ms" — which is the more direct spelling and the one that fails
// on a machine running eight test files at once. Reading the shell first and
// resolving afterwards says the same thing and cannot be outrun: a renderer
// that waited for the data would still be waiting at the assertion, with no
// timer coming to rescue it.

import * as React from "react";
import { Suspense, use } from "@uniflowed/react";
import { act } from "@uniflowed/react-testing";
import { routerView } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { describe, expect, it } from "@uniflowed/test";

// Not package exports: the payload is internal to `@uniflowed/router` on
// purpose — it is a wire format between two halves of one build, not something
// an application writes — so the format is driven directly, the way
// `streaming.test.js` drives `renderWithReadableStream`.
import {
  MAX_PAYLOAD_DEPTH,
  PAYLOAD_ROW_ATTRIBUTE,
  PayloadValueError,
  decodePayload,
  encodePayload,
  encodeRowValue,
  parseRowMessage,
  payloadJson,
} from "../../packages/router/internal/payload.js";
import { createPayloadReader } from "../../packages/router/internal/payload-rows.js";

// `@uniflowed/router/client` statically imports `react-dom/client`, which reads
// `document` while it is being evaluated — so the DOM has to exist before the
// *import* and not merely before the first render. `streaming.test.js` and
// `rsc-split.test.js` reach for the same two for the same reason.
import { installDom } from "../../packages/react-testing/internal/dom.js";

async function clientModule() {
  installDom();
  return import("@uniflowed/router/client");
}

const assets = { scripts: [], styles: [], preloads: [] };

/** A promise with its settle functions in hand. */
function deferred<T>(): {|
  readonly promise: Promise<T>,
  readonly resolve: (value: T) => void,
  readonly reject: (error: mixed) => void,
|} {
  let settle: (value: T) => void = () => {};
  let fail: (error: mixed) => void = () => {};
  const promise = new Promise<T>((resolve, reject) => {
    settle = resolve;
    fail = reject;
  });
  return { promise, resolve: settle, reject: fail };
}

/**
 * A render's chunks, read one at a time.
 *
 * `first` rather than a whole document, because the claim these tests make is
 * about what had happened when the shell went out — and the honest way to
 * assert it is to read the shell while nothing has resolved yet and *then*
 * resolve something. A wall-clock deadline would say the same thing on an idle
 * machine and something else entirely on a machine running eight of these at
 * once; this says it either way.
 */
function chunksOf(result: { readonly stream: () => ReadableStream, ... }): {|
  readonly next: () => Promise<string>,
  readonly rest: () => Promise<Array<string>>,
|} {
  const decoder = new TextDecoder();
  const reader = result.stream().getReader();
  return {
    async next(): Promise<string> {
      const { done, value } = await reader.read();
      return done === true ? "" : decoder.decode(value, { stream: true });
    },
    async rest(): Promise<Array<string>> {
      const out = [];
      while (true) {
        const { done, value } = await reader.read();
        if (done === true) {
          return out;
        }
        out.push(decoder.decode(value, { stream: true }));
      }
    },
  };
}

// ---------------------------------------------------------------------------
// 1. The format
// ---------------------------------------------------------------------------

describe("the payload format", () => {
  it("hands back the value it was given when nothing is deferred", () => {
    // Identity, not equality. A payload that rebuilt every object would mean
    // every page paid a copy of its data for a feature it is not using, and
    // that "nothing changed here" was a hope rather than a property.
    const data = { user: { name: "ada" }, tags: ["a", "b"] };
    const encoded = encodePayload(data, "data");
    expect(encoded.model).toBe(data);
    expect(encoded.rows.length).toBe(0);
    expect(decodePayload(data, () => null, "data")).toBe(data);
  });

  it("replaces a promise with a reference and keeps it as a row", () => {
    const comments = Promise.resolve([{ body: "hi" }]);
    const encoded = encodePayload({ user: "ada", comments }, "data");
    expect(encoded.model).toEqual({ user: "ada", comments: "$P1" });
    expect(encoded.rows.length).toBe(1);
    expect(encoded.rows[0].id).toBe(1);
    expect(encoded.rows[0].value).toBe(comments);
  });

  it("numbers rows by where they sit rather than by when they were made", () => {
    // The id has to be a function of the model and of nothing else: the
    // browser re-encodes the same model to render the same row elements, and
    // it has no idea in which order the loader created these.
    const second = Promise.resolve("second");
    const first = Promise.resolve("first");
    const encoded = encodePayload({ a: first, nested: { list: [1, second] } }, "data");
    expect(encoded.model).toEqual({ a: "$P1", nested: { list: [1, "$P2"] } });
    expect(encoded.rows.map((row) => row.value)).toEqual([first, second]);
  });

  it("escapes a string that would read as a reference, and gives it back", () => {
    const encoded = encodePayload({ price: "$100", tag: "$P1" }, "data");
    expect(encoded.model).toEqual({ price: "$$100", tag: "$$P1" });
    expect(decodePayload(encoded.model, () => "never", "data")).toEqual({
      price: "$100",
      tag: "$P1",
    });
  });

  it("round-trips a deferred model through JSON and back", () => {
    const encoded = encodePayload({ now: "$here", later: Promise.resolve(1) }, "data");
    const overTheWire = JSON.parse(payloadJson(encoded.model));
    const rebuilt = decodePayload(overTheWire, (id) => `row ${String(id)}`, "data");
    expect(rebuilt).toEqual({ now: "$here", later: "row 1" });
  });

  it("refuses a reference tag it has no meaning for", () => {
    // The whole reason `$` is a closed namespace. A decoder that passed `$X1`
    // through would accept it silently on the day `$X` means a client module.
    expect(() => decodePayload({ a: "$X1" }, () => null, "data")).toThrow(PayloadValueError);
    expect(() => decodePayload({ a: "$P0" }, () => null, "data")).toThrow(PayloadValueError);
    expect(() => decodePayload({ a: "$P1e3" }, () => null, "data")).toThrow(PayloadValueError);
    expect(() => decodePayload({ a: "$P9999" }, () => null, "data")).toThrow(PayloadValueError);
  });

  it("names where in the loader's answer the trouble was", () => {
    let caught: mixed = null;
    try {
      decodePayload({ page: { items: [{ kind: "$nope" }] } }, () => null, "data");
    } catch (error) {
      caught = error;
    }
    expect(caught instanceof PayloadValueError).toBe(true);
    expect(caught instanceof PayloadValueError ? caught.path : "").toBe("data.page.items[0].kind");
  });

  it("refuses a row that is itself deferred", () => {
    // Ids come from one walk of the model, which is the only structure both
    // sides hold before anything resolves. A row that could add more would be
    // a numbering that depends on what finished first.
    expect(() => encodeRowValue({ more: Promise.resolve(1) }, "row 1")).toThrow(PayloadValueError);
    expect(encodeRowValue({ ok: "$x" }, "row 1")).toEqual({ ok: "$$x" });
  });

  it("stops rather than following a cycle", () => {
    const loop: { self?: mixed, ... } = {};
    loop.self = loop;
    expect(() => encodePayload({ loop, later: Promise.resolve(1) }, "data")).toThrow(
      PayloadValueError,
    );
    expect(MAX_PAYLOAD_DEPTH > 24).toBe(true);
  });

  it("leaves everything JSON.stringify already decided about alone", () => {
    // The model is not the action wire. A `Date`, a `Map` and an `undefined`
    // property crossed as `JSON.stringify` renders them before the payload
    // existed, and a payload with a promise in it must not change that.
    const when = new Date(0);
    const encoded = encodePayload(
      { when, set: new Map([["a", 1]]), missing: undefined, later: Promise.resolve(1) },
      "data",
    );
    expect(payloadJson(encoded.model)).toBe(
      `{"when":"1970-01-01T00:00:00.000Z","set":{},"later":"$P1"}`,
    );
  });

  it("writes a model no `</script>` can end early", () => {
    const encoded = encodePayload({ html: "</script><b>", later: Promise.resolve(1) }, "data");
    const json = payloadJson(encoded.model);
    expect(json).not.toContain("</script>");
    expect(JSON.parse(json).html).toBe("</script><b>");
  });

  it("does not let `__proto__` become a prototype on the way back", () => {
    // The hazard the rebuild introduces and `JSON.parse` does not: assignment
    // calls the setter on `Object.prototype`, `defineProperty` does not.
    const rebuilt = decodePayload(
      JSON.parse(`{"__proto__":{"polluted":true},"tag":"$$x"}`),
      () => null,
      "data",
    );
    expect(({}: $FlowFixMe).polluted).toBe(undefined);
    expect(Object.getPrototypeOf(rebuilt)).toBe(Object.getPrototypeOf({}));
    expect(Object.prototype.hasOwnProperty.call(rebuilt, "__proto__")).toBe(true);
  });

  it("reads a row message, and refuses one that says two things or none", () => {
    expect(parseRowMessage(`{"value":{"ok":true}}`, 1)).toEqual({ value: { ok: true } });
    expect(parseRowMessage(`{"error":"boom"}`, 1)).toEqual({ error: "boom" });
    expect(parseRowMessage(`{"value":"$$x"}`, 1)).toEqual({ value: "$x" });
    expect(() => parseRowMessage(`{}`, 1)).toThrow(PayloadValueError);
    expect(() => parseRowMessage(`{"value":1,"error":"b"}`, 1)).toThrow(PayloadValueError);
    expect(() => parseRowMessage(`{"error":7}`, 1)).toThrow(PayloadValueError);
    expect(() => parseRowMessage(`not json`, 1)).toThrow(PayloadValueError);
    expect(() => parseRowMessage(`[1]`, 1)).toThrow(PayloadValueError);
    // A row may not name another row.
    expect(() => parseRowMessage(`{"value":"$P2"}`, 1)).toThrow(PayloadValueError);
  });
});

// ---------------------------------------------------------------------------
// 2. Reading rows out of a document that is still arriving
// ---------------------------------------------------------------------------

/** A document with a growing list of row scripts, and nothing else in it. */
function rowDocument(): {|
  readonly document: $FlowFixMe,
  readonly write: (id: number, text: string) => void,
  readonly observe: (callback: () => void) => () => void,
  readonly watchers: () => number,
|} {
  const elements: Array<{|
    readonly getAttribute: (name: string) => string | null,
    readonly textContent: string,
  |}> = [];
  let callbacks: Array<() => void> = [];
  return {
    document: {
      querySelectorAll: () => elements,
      documentElement: null,
    },
    write(id: number, text: string) {
      elements.push({
        getAttribute: (name) => (name === PAYLOAD_ROW_ATTRIBUTE ? String(id) : null),
        textContent: text,
      });
      for (const callback of [...callbacks]) {
        callback();
      }
    },
    observe(callback: () => void) {
      callbacks.push(callback);
      return () => {
        callbacks = callbacks.filter((entry) => entry !== callback);
      };
    },
    watchers: () => callbacks.length,
  };
}

describe("reading rows out of a document", () => {
  it("applies the rows that are already there, and then the ones that follow", async () => {
    const doc = rowDocument();
    const reader = createPayloadReader(doc.document, doc.observe);
    const first = reader.resolve(1);
    const second = reader.resolve(2);
    doc.write(2, `{"value":"second"}`);
    reader.watch();

    // The one that was in the document before the watch started, and the one
    // that arrived after it: the same promise either way.
    expect(await second).toBe("second");
    expect(doc.watchers()).toBe(1);
    doc.write(1, `{"value":"first"}`);
    expect(await first).toBe("first");
    // Every row the model named has arrived, so the watch is over.
    expect(doc.watchers()).toBe(0);
  });

  it("rejects the row the server said failed, and nothing else", async () => {
    const doc = rowDocument();
    const reader = createPayloadReader(doc.document, doc.observe);
    const failed = reader.resolve(1);
    const fine = reader.resolve(2);
    reader.watch();
    doc.write(1, `{"error":"@uniflowed/router: a deferred value failed on the server."}`);
    doc.write(2, `{"value":7}`);

    await expect(failed).rejects.toThrow("failed on the server");
    expect(await fine).toBe(7);
  });

  it("rejects a row it cannot read rather than the page", async () => {
    const doc = rowDocument();
    const reader = createPayloadReader(doc.document, doc.observe);
    const row = reader.resolve(1);
    reader.watch();
    doc.write(1, `{"value":`);
    await expect(row).rejects.toThrow("row 1 is not JSON");
  });

  it("ignores a row nothing referred to, and a second copy of one", async () => {
    const doc = rowDocument();
    const reader = createPayloadReader(doc.document, doc.observe);
    const row = reader.resolve(1);
    reader.watch();
    doc.write(4, `{"value":"nobody asked"}`);
    doc.write(1, `{"value":"first"}`);
    doc.write(1, `{"value":"second"}`);
    expect(await row).toBe("first");
  });

  it("never watches a document with nothing deferred in it", () => {
    const doc = rowDocument();
    createPayloadReader(doc.document, doc.observe).watch();
    expect(doc.watchers()).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// 3. The order the bytes arrive in
// ---------------------------------------------------------------------------

/**
 * A route whose loader answers with one value now and two later.
 *
 * The page renders a `<Suspense>` of its own around each deferred value, which
 * is what "a boundary that resolves on its own" means here: neither wraps the
 * other, and neither is the route's `$loading.js`.
 */
function deferringTable(slow: Promise<string>, quick: Promise<string>) {
  component Deferred(value: Promise<string>) {
    return <p>{use(value)}</p>;
  }
  component DataPage(data: mixed) {
    const answer: $FlowFixMe = data;
    return (
      <div>
        <p>{String(answer.now)}</p>
        <Suspense fallback={<p>waiting for the slow one</p>}>
          <Deferred value={answer.slow} />
        </Suspense>
        <Suspense fallback={<p>waiting for the quick one</p>}>
          <Deferred value={answer.quick} />
        </Suspense>
      </div>
    );
  }
  return {
    routes: [
      {
        path: "/deferred",
        params: [],
        mdx: false,
        file: "app/deferred/$page.js",
        page: () =>
          Promise.resolve({
            default: DataPage,
            loader: () => ({ now: "the model is here", slow, quick }),
          }),
        layouts: [],
        loading: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("streaming a payload", () => {
  it("sends the model before either deferred value has resolved", async () => {
    const slow = deferred<string>();
    const quick = deferred<string>();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...deferringTable(slow.promise, quick.promise),
    });

    const result = await renderer.render("/deferred", assets);
    // Read the shell with neither promise settled and nothing scheduled to
    // settle them. A renderer that waited for the data would still be waiting
    // here, and no timer would rescue it.
    const shell = await chunksOf(result).next();

    // The model, with a reference standing where each promise was. This is the
    // assertion the old `JSON.stringify` could not have passed: it wrote `{}`
    // for a promise, and it wrote nothing at all until both had settled.
    expect(shell).toContain('"now":"the model is here"');
    expect(shell).toContain('"slow":"$P1"');
    expect(shell).toContain('"quick":"$P2"');
    expect(shell).toContain("waiting for the slow one");
    expect(shell).toContain("waiting for the quick one");
    expect(shell).not.toContain("the quick one is here");
    expect(shell).not.toContain(PAYLOAD_ROW_ATTRIBUTE);

    // And the render is still live rather than finished: settling both is what
    // ends it, which is the other half of "the shell went first".
    quick.resolve("the quick one is here");
    slow.resolve("the slow one is here");
  });

  it("writes the row that resolved first first, whatever order they were written in", async () => {
    // Row 1 is the slow one because it is earlier in the model; row 2 is the
    // quick one. If the rows arrived in id order the payload would be a list
    // that streams and a document that waits, which is the failure this issue
    // is about.
    const slow = deferred<string>();
    const quick = deferred<string>();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...deferringTable(slow.promise, quick.promise),
    });

    const result = await renderer.render("/deferred", assets);
    const stream = chunksOf(result);
    const shell = await stream.next();
    // The quick one first, and the slow one only once the quick one's row has
    // had every chance to be written: the order of the two settlements is the
    // input, and the order of the two rows is what is being measured.
    quick.resolve("the quick one is here");
    setTimeout(() => slow.resolve("the slow one is here"), 40);
    const chunks = [shell, ...(await stream.rest())];

    const at = (needle: string): number => {
      const index = chunks.findIndex((chunk) => chunk.includes(needle));
      expect(index).not.toBe(-1);
      return index;
    };
    const document = chunks.join("");
    expect(document).toContain(`${PAYLOAD_ROW_ATTRIBUTE}="1"`);
    expect(document).toContain(`${PAYLOAD_ROW_ATTRIBUTE}="2"`);

    // Row 2 — the quick one — before row 1, in the bytes.
    expect(at(`{"value":"the quick one is here"}`)).toBeLessThan(
      at(`{"value":"the slow one is here"}`),
    );
    // And the page's own boundary for it resolved on its own, in the same
    // chunk range rather than at the end: the quick value's markup is out
    // before the slow one's row is.
    expect(at("the quick one is here")).toBeLessThan(at("the slow one is here"));
    // Neither of them is in the shell.
    expect(at("the quick one is here") > 0).toBe(true);
    // And the model is still exactly one element with one id.
    expect(document.split(`${PAYLOAD_ROW_ATTRIBUTE}="2"`).length).toBe(2);
  });

  it("says a row failed instead of leaving its boundary open", async () => {
    const slow = deferred<string>();
    const quick = deferred<string>();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...deferringTable(slow.promise, quick.promise),
      // A rejected deferred value reaches the page's error boundary, which is
      // the framework's own here; the assertion is about the row.
    });
    const result = await renderer.render("/deferred", assets);
    const stream = chunksOf(result);
    const shell = await stream.next();
    quick.resolve("the quick one is here");
    slow.reject(new Error("the loader could not answer"));
    const document = shell + (await stream.rest()).join("");

    const rows = [
      ...document.matchAll(/<script type="application\/json" data-uf-row="(\d)">([^<]*)</g),
    ];
    const row = new Map(rows.map((match) => [match[1], match[2]]));
    expect(row.get("1")).toBe(
      `{"error":"@uniflowed/router: a deferred value failed on the server."}`,
    );
    // The row says the value failed and does not say what the loader said. The
    // words are the server's and stay there — React puts them in the page's own
    // boundary in a development render, which is React's decision and not this
    // element's.
    expect(row.get("1") ?? "").not.toContain("the loader could not answer");
    // And the other value is unaffected: one failure is one row.
    expect(row.get("2")).toBe(`{"value":"the quick one is here"}`);
  });

  it("writes the same document it always did when nothing is deferred", async () => {
    // The property that makes this format safe to have landed: a page with
    // ordinary data produces the bytes it produced before the payload existed.
    component Page(data: mixed) {
      return <p>{String(data)}</p>;
    }
    const table = {
      routes: [
        {
          path: "/plain",
          params: [],
          mdx: false,
          file: "app/plain/$page.js",
          page: () => Promise.resolve({ default: Page, loader: () => "the page is here" }),
          layouts: [],
          loading: [],
        },
      ],
      notFound: [],
      errors: [],
    };
    const { html } = await createRenderer({ App: routerView("./app"), ...table }).prerender(
      "/plain",
      assets,
    );
    expect(html).toContain('<script id="__uf_data" type="application/json">"the page is here"');
    expect(html).not.toContain(PAYLOAD_ROW_ATTRIBUTE);
  });
});

// ---------------------------------------------------------------------------
// 4. The browser applying it
// ---------------------------------------------------------------------------

describe("the browser applying a payload", () => {
  it("hydrates from the rows the document carried, without running the loader again", async () => {
    const seen = { loads: 0 };
    component Deferred(value: Promise<string>) {
      return <p>{use(value)}</p>;
    }
    component DataPage(data: mixed) {
      const answer: $FlowFixMe = data;
      return (
        <div>
          <p>{String(answer.now)}</p>
          <Suspense fallback={<p>waiting</p>}>
            <Deferred value={answer.later} />
          </Suspense>
        </div>
      );
    }
    // One table for both renders: `loadOnce` caches a module by the identity of
    // the function that loads it, and hydration is React comparing two renders
    // of the same components. `streaming.test.js` says the same at length.
    const table = {
      routes: [
        {
          path: "/deferred",
          params: [],
          mdx: false,
          file: "app/deferred/$page.js",
          page: () =>
            Promise.resolve({
              default: DataPage,
              loader: () => {
                seen.loads += 1;
                return { now: "the model is here", later: Promise.resolve("the row is here") };
              },
            }),
          layouts: [],
          loading: [],
        },
      ],
      notFound: [],
      errors: [],
    };

    const { html } = await createRenderer({ App: routerView("./app"), ...table }).prerender(
      "/deferred",
      assets,
    );
    expect(seen.loads).toBe(1);
    expect(html).toContain('"later":"$P1"');
    expect(html).toContain(`${PAYLOAD_ROW_ATTRIBUTE}="1"`);

    installDom();
    const parsed = new globalThis.DOMParser().parseFromString(html, "text/html");
    const rendered = parsed.getElementById("uf-root");
    const root = globalThis.document.createElement("div");
    root.id = "uf-root";
    root.innerHTML = rendered?.innerHTML ?? "";
    globalThis.document.body.replaceChildren(root);
    globalThis.window.history.pushState(null, "", "/deferred");

    const { hydrate } = await clientModule();
    await act(async () => {
      await hydrate({ App: routerView("./app"), ...table });
    });

    // Still one: the browser read the model and the row rather than fetching
    // either of them again.
    expect(seen.loads).toBe(1);
    const text = globalThis.document.getElementById("uf-root")?.textContent ?? "";
    expect(text).toContain("the model is here");
    expect(text).toContain("the row is here");
    expect(text).not.toContain("waiting");
  });

  it("resolves a boundary from a row that arrives after hydration", async () => {
    // The claim the whole format exists for, on the reading side: the document
    // that reached the browser did not have the value in it, the page hydrated
    // with a fallback on screen, and the row that landed afterwards is what
    // filled it in — with no second request and no second render of the page.
    const late = deferred<string>();
    const seen = { loads: 0 };
    component Deferred(value: Promise<string>) {
      return <p>{use(value)}</p>;
    }
    component DataPage(data: mixed) {
      const answer: $FlowFixMe = data;
      return (
        <div>
          <p>{String(answer.now)}</p>
          <Suspense fallback={<p>waiting for the row</p>}>
            <Deferred value={answer.later} />
          </Suspense>
        </div>
      );
    }
    const table = {
      routes: [
        {
          path: "/late",
          params: [],
          mdx: false,
          file: "app/late/$page.js",
          page: () =>
            Promise.resolve({
              default: DataPage,
              loader: () => {
                seen.loads += 1;
                return { now: "the model is here", later: late.promise };
              },
            }),
          layouts: [],
          loading: [],
        },
      ],
      notFound: [],
      errors: [],
    };

    // The shell, as the browser would have received it: everything up to the
    // point the row would have been streamed into, and not the row.
    const renderer = createRenderer({ App: routerView("./app"), ...table });
    const result = await renderer.render("/late", assets);
    const stream = chunksOf(result);
    const shell = await stream.next();
    expect(shell).toContain("waiting for the row");
    expect(shell).not.toContain("the row is here");
    // The rest of the render, which this test deliberately never delivers to
    // the browser: what it is asserting is that the row can arrive on its own.
    late.resolve("the row is here");
    await stream.rest();

    installDom();
    const parsed = new globalThis.DOMParser().parseFromString(shell, "text/html");
    const rendered = parsed.getElementById("uf-root");
    const root = globalThis.document.createElement("div");
    root.id = "uf-root";
    root.innerHTML = rendered?.innerHTML ?? "";
    globalThis.document.body.replaceChildren(root);
    globalThis.window.history.pushState(null, "", "/late");

    const { hydrate } = await clientModule();
    await act(async () => {
      await hydrate({ App: routerView("./app"), ...table });
    });
    expect(seen.loads).toBe(1);
    expect(globalThis.document.getElementById("uf-root")?.textContent ?? "").toContain(
      "waiting for the row",
    );

    // The row, arriving the way the rest of the stream would have delivered it:
    // an element appended to the document while React is already attached.
    await act(async () => {
      const row = globalThis.document.createElement("script");
      row.setAttribute("type", "application/json");
      row.setAttribute(PAYLOAD_ROW_ATTRIBUTE, "1");
      row.textContent = `{"value":"the row is here"}`;
      globalThis.document.body.append(row);
      await Promise.resolve();
      await Promise.resolve();
    });

    const text = globalThis.document.getElementById("uf-root")?.textContent ?? "";
    expect(text).toContain("the row is here");
    expect(text).not.toContain("waiting for the row");
    // And the loader still ran exactly once, on the server.
    expect(seen.loads).toBe(1);
  });
});
