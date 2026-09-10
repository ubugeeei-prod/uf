// @flow
//
// The accessibility audit `uf dev` runs against the page on screen.
//
// The half of ubugeeei-prod/uf#511 that runs in a browser. It cannot be driven
// through a browser here, so it is driven where it lives: `start()` is an
// ordinary module that reads a `document`, watches it with a `MutationObserver`
// and posts to an endpoint, and `@uniflowed/react-testing` installs a document
// that has all three. `packages/vite/dev-channel.test.js`'s neighbours are
// where the channel's own half is checked.
//
// The two halves this file *does* check are the ones a mistake would be silent
// in: the module the plugin generates is the module the browser has to be able
// to run, and the report it sends has to be a shape the channel's normaliser
// keeps — that normaliser is an allowlist, so an invented field is dropped
// without a word.

import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { cleanup, render } from "@uniflowed/react-testing";

// Before the runtime is imported, and that order is the whole of it: axe-core
// reads `window` and `document` when it is *evaluated*, not when it is called,
// and answers "Required window or document globals not defined" for ever after
// if they were not there. In a browser they always are. Here the document is
// installed by `@uniflowed/react-testing`, so the engine has to come after it —
// which is why the import below is dynamic and this one reaches past the
// package's own exports, the way `packages/router/streaming.test.js` does.
import { installDom } from "../../packages/react-testing/internal/dom.js";
import { bodyOf } from "../../tests/library/dom.js";

import {
  AUDIT_PUBLIC_PATH,
  AUDIT_RESOLVED_ID,
  SETTLE_MS,
  auditAvailable,
  auditRuntimeSource,
  auditTag,
} from "./internal/a11y.js";
import { DIAGNOSTIC_ENDPOINT } from "./internal/diagnostics.js";
import uniflowed from "./index.js";
import { DEVTOOLS_HOOK } from "./internal/devtools.js";

installDom();

const { start } = await import("./internal/a11y-runtime.js");

/** Where a report is posted in this file; never actually fetched. */
const ENDPOINT = "/__uf/diagnostic";

/**
 * The rules a bare test document breaks by existing.
 *
 * The audit reads the whole document, because that is what a reader is looking
 * at. A document a test rendered into has no `lang`, no `<title>`, no `<h1>`
 * and no landmark, so `html-has-lang` and its neighbours fire whatever the
 * component under test does — which would make every assertion below "at least
 * one violation" rather than "this violation". Turned off here so the rest of
 * the file is about the markup it rendered; a real page keeps them, and they
 * are exactly the sort of thing an audit of a real page should say.
 */
const DOCUMENT_RULES = [
  "document-title",
  "html-has-lang",
  "landmark-one-main",
  "page-has-heading-one",
  "region",
];

/** The bodies `start` posted, in order. */
type Sent = Array<mixed>;

// uf lint's static rules are right about this markup and it is wrong on
// purpose: the audit needs a rendered tree with something in it to find, and a
// component that renders correct markup would prove nothing. The block is
// closed immediately after it.
//
// uf-lint-disable a11y/alt-text
component Inaccessible() {
  return (
    <main>
      <img src="/chart.png" />
    </main>
  );
}
// uf-lint-enable a11y/alt-text

/** Collect what the audit posts, and put `fetch` back afterwards. */
function collect(): Sent {
  const sent: Sent = [];
  uft.stubGlobal("fetch", (url: mixed, init: mixed) => {
    const body = (init as $FlowFixMe)?.body;
    sent.push({ url, body: typeof body === "string" ? JSON.parse(body) : null });
    return Promise.resolve({ status: 204 });
  });
  return sent;
}

describe("the module the dev server generates", () => {
  it("is the runtime with a call to it, carrying the project's rule set", () => {
    const source = auditRuntimeSource({
      tags: ["wcag2a"],
      disabledRules: ["color-contrast"],
      minImpact: "serious",
    });

    expect(source).toContain("export function start(");
    // The endpoint is the one the middleware answers on, taken from the module
    // that defines it rather than written out a second time.
    expect(source).toContain(JSON.stringify(DIAGNOSTIC_ENDPOINT));
    expect(source).toContain(`"settleMs":${String(SETTLE_MS)}`);
    expect(source).toContain('"tags":["wcag2a"]');
    expect(source).toContain('"disabledRules":["color-contrast"]');
    expect(source).toContain('"minImpact":"serious"');
  });

  it("says nothing about rules a project did not name", () => {
    const source = auditRuntimeSource(undefined);
    expect(source).toContain('"tags":[]');
    expect(source).toContain('"minImpact":null');
  });

  it("is injected only when the project has the engine", () => {
    // This repository does, which is what makes the audit exercisable at all.
    expect(auditAvailable(process.cwd())).toBe(true);
    // A directory with no `node_modules` above it has no engine and gets no
    // tag — not a tag pointing at a module that cannot resolve its import.
    expect(auditAvailable("/")).toBe(false);
    expect(auditTag("/", false)).toBe(null);

    const tag = auditTag("/", true);
    expect(tag?.attrs?.src).toBe(AUDIT_PUBLIC_PATH);
    // In the head, because `uf dev` streams the body and a body injection can
    // split a React chunk mid-attribute before the rest of the body arrives.
    expect(tag?.injectTo).toBe("head");
    // Vite's convention for a resolved virtual id.
    expect(AUDIT_RESOLVED_ID.startsWith("\0")).toBe(true);
  });
});

/** uf's Flow plugin, in the state `serve` or `build` leaves it in. */
function flowPlugin(config: $FlowFixMe, command: "serve" | "build"): $FlowFixMe {
  const flow: $FlowFixMe = uniflowed({ root: process.cwd(), config })[0];
  flow.config(
    { root: process.cwd() },
    { mode: command === "build" ? "production" : "development", command },
  );
  return flow;
}

describe("the wiring into a development document", () => {
  it("adds the audit to the document DevTools already owns, and adds it last", () => {
    // The half `packages/vite/devtools.test.js` deliberately does not check.
    // That file turns this feature off so its own count is about DevTools;
    // this is the assertion that the two coexist — and it is the one that
    // would have caught the collision, because "the document has two tags" and
    // "the document has three" cannot both be somebody else's problem.
    const flow = flowPlugin({}, "serve");

    const tags = flow.transformIndexHtml();

    expect(tags.length).toBe(3);
    // Untouched, and in the order #503 requires: the classic hook script
    // first, the Fast Refresh module second, both at the top of the head.
    expect(tags[0].attrs?.["data-uf-dev-head-preamble"]).toBe("react-devtools");
    expect(tags[0].injectTo).toBe("head-prepend");
    expect(tags[0].children).toContain(DEVTOOLS_HOOK);
    expect(tags[1].injectTo).toBe("head-prepend");
    expect(tags[1].attrs?.type).toBe("module");
    expect(tags[1].attrs?.["data-uf-dev-head-preamble"]).toBe("react-refresh");
    // And the audit after both, still in the head: the runtime waits for a
    // settled DOM, but the injected tag itself stays out of streamed body
    // chunks so it cannot corrupt markup.
    expect(tags[2].injectTo).toBe("head");
    expect(tags[2].attrs?.src).toBe(AUDIT_PUBLIC_PATH);
  });

  it("adds nothing when the project turned the dev audit off", () => {
    const flow = flowPlugin({ accessibility: { devAudit: false } }, "serve");

    const tags = flow.transformIndexHtml();

    expect(tags.length).toBe(2);
    expect(tags.some((tag) => tag.attrs?.src === AUDIT_PUBLIC_PATH)).toBe(false);
  });

  it("adds nothing to a build, which has no audit to run", () => {
    expect(flowPlugin({}, "build").transformIndexHtml()).toEqual([]);
  });

  it("serves the runtime the tag asks for", () => {
    // The other half of the wiring: the tag names a URL, and the plugin has to
    // resolve that URL to the virtual module and load the generated source for
    // it. A tag whose module 404s is a red console on every page.
    const flow = flowPlugin({ accessibility: { axe: { minImpact: "serious" } } }, "serve");

    expect(flow.resolveId(AUDIT_PUBLIC_PATH)).toBe(AUDIT_RESOLVED_ID);
    const source = flow.load(AUDIT_RESOLVED_ID);
    expect(source).toContain("export function start(");
    // The project's own rule set, not the default: what `load` is handed is
    // the `accessibility.axe` block the plugin was built with.
    expect(source).toContain('"minImpact":"serious"');
  });

  it("pre-bundles the engine rather than meeting it after a page has loaded", () => {
    // axe-core is CommonJS and is reached only from a virtual module, so Vite
    // would discover it during the first render, optimise it, and reload the
    // page it had just served. Naming it up front is what stops `uf dev` from
    // reloading itself once per session.
    const config = uniflowed({ root: process.cwd(), config: {} })[0].config(
      { root: process.cwd() },
      { mode: "development", command: "serve" },
    );
    expect(config.optimizeDeps.include).toContain("axe-core");

    const off = uniflowed({
      root: process.cwd(),
      config: { accessibility: { devAudit: false } },
    })[0].config({ root: process.cwd() }, { mode: "development", command: "serve" });
    expect(off.optimizeDeps.include).not.toContain("axe-core");
  });
});

describe("the audit itself", () => {
  afterEach(() => {
    uft.unstubAllGlobals();
    // What this file rendered goes with it. `render` clears its own containers
    // on the way *in*, so a file that never calls this leaves its last one for
    // whichever file the scheduler puts next on this worker — which is
    // ubugeeei-prod/uf#607, and is not a thing to add another instance of.
    cleanup();
  });

  it("reports what axe found, in a shape the channel accepts", async () => {
    const sent = collect();
    render(<Inaccessible />);

    const stop = start({ endpoint: ENDPOINT, settleMs: 1, axe: {} });
    await uft.waitFor(() => {
      expect(sent.length).toBe(1);
    });
    stop();

    const body: $FlowFixMe = (sent[0] as $FlowFixMe).body;
    expect(body.severity).toBe("warn");
    expect(body.message).toContain("accessibility");
    const detail: $ReadOnlyArray<string> = body.detail;
    expect(detail.join("\n")).toContain("image-alt");
    // Deque's page for the rule, which is the part that turns an id into
    // something a person can act on without a search engine.
    expect(detail.join("\n")).toContain("dequeuniversity.com");

    // And every field is one the middleware's normaliser keeps. That normaliser
    // is an allowlist — a field invented here would be dropped without a word,
    // and a severity it made up would arrive as `error` — so this asserts the
    // shape rather than a round trip through a fake request object.
    expect(["error", "warn", "info"]).toContain(body.severity);
    expect(typeof body.message).toBe("string");
    expect(detail.every((line: string) => typeof line === "string")).toBe(true);
    expect(typeof body.url).toBe("string");
  });

  it("says nothing twice about a page that has not changed", async () => {
    const sent = collect();
    render(<Inaccessible />);

    const stop = start({ endpoint: ENDPOINT, settleMs: 1, axe: {} });
    await uft.waitFor(() => {
      expect(sent.length).toBe(1);
    });
    // A re-render is hundreds of mutations and every one of them schedules an
    // audit. What must not follow is hundreds of identical blocks in the
    // terminal, so a report that says what the last one said is not sent.
    bodyOf().setAttribute("data-rerendered", "1");
    await uft.waitUntil(() => sent.length > 1, { timeout: 120 }).catch(() => {});
    stop();
    expect(sent.length).toBe(1);
  });

  it("audits the tree a mutation left behind, even one that arrived mid-run", async () => {
    // The engine reads the tree as it was when the run started, so a mutation
    // that lands while it is working is about a page the answer in flight says
    // nothing about. The timer that would have covered it has already cleared
    // itself by the time `audit` discovers the engine is busy — so returning
    // there dropped the mutation for good: nothing was left scheduled, and the
    // DOM it settled into never got audited at all.
    //
    // The engine is replaced outright rather than held open around a real run.
    // What is under test is the scheduling, `axe` is a singleton whose lock is
    // process-wide, and a case that parks the real engine for the length of a
    // real audit makes every case after it wait on this one. A stand-in that
    // answers "nothing wrong" costs nothing and pins the same thing.
    const axe = (await import("axe-core")).default;
    const real = axe.run;
    let started = 0;
    let finished = 0;
    let release = () => {};
    const held = new Promise((resolve) => {
      release = resolve;
    });
    axe.run = async () => {
      started += 1;
      // Only the first run is held; the rest answer at once.
      if (started === 1) await held;
      finished += 1;
      return { violations: [] };
    };

    try {
      const stop = start({ endpoint: ENDPOINT, settleMs: 1, axe: {} });

      // Wait until the engine is inside the first run and holding.
      await uft.waitFor(() => {
        expect(started).toBe(1);
      });

      // Now move the DOM. The timer this schedules fires while the run is
      // still held, which is the case being pinned.
      bodyOf().setAttribute("data-changed", "1");
      await uft.waitUntil(() => started > 1, { timeout: 60 }).catch(() => {});
      expect(started).toBe(1);

      release();
      // The run that was owed to the mutation, which used to never come.
      await uft.waitFor(() => {
        expect(started).toBeGreaterThan(1);
      });
      stop();
      // Drained before the case ends: a run still in flight holds the engine's
      // process-wide lock, and the next case's first audit would come back
      // "Axe is already running" — a failure with nothing to do with the case
      // it lands in.
      await uft.waitFor(() => {
        expect(finished).toBe(started);
      });
    } finally {
      axe.run = real;
    }
  });

  it("says nothing at all about a page with nothing wrong with it", async () => {
    const sent = collect();
    render(
      <main>
        <h1>Fine</h1>
      </main>,
    );

    const stop = start({
      endpoint: ENDPOINT,
      settleMs: 1,
      axe: { disabledRules: DOCUMENT_RULES },
    });
    await uft.waitUntil(() => sent.length > 0, { timeout: 120 }).catch(() => {});
    stop();
    expect(sent.length).toBe(0);
  });

  it("honours the rule set it was given", async () => {
    const sent = collect();
    render(<Inaccessible />);

    const stop = start({
      endpoint: ENDPOINT,
      settleMs: 1,
      axe: { disabledRules: [...DOCUMENT_RULES, "image-alt"] },
    });
    await uft.waitUntil(() => sent.length > 0, { timeout: 120 }).catch(() => {});
    stop();
    expect(sent.length).toBe(0);
  });

  it("drops a finding below the floor the project set", async () => {
    const sent = collect();
    render(<Inaccessible />);

    // `image-alt` is `critical`, and `critical` is the top of the scale, so a
    // floor cannot silence it. `region` and its neighbours are `moderate` and
    // below, so a `critical` floor is what keeps this about one rule.
    const stop = start({
      endpoint: ENDPOINT,
      settleMs: 1,
      axe: { minImpact: "critical" },
    });
    await uft.waitFor(() => {
      expect(sent.length).toBe(1);
    });
    stop();
    const detail: $ReadOnlyArray<string> = (sent[0] as $FlowFixMe).body.detail;
    expect(detail.join("\n")).toContain("image-alt");
    expect(detail.join("\n")).not.toContain("html-has-lang");
  });
});
