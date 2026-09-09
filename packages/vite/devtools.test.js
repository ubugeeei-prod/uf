// @flow
//
// React DevTools, and the check that the hook survived.
//
// DevTools attaches to a page by being there first: while `react-dom` is
// evaluated it looks for `__REACT_DEVTOOLS_GLOBAL_HOOK__` and registers itself
// with whatever it finds, once and never again. `uf dev` had a hook for that
// line to find, and had it by accident — the Fast Refresh preamble installs one
// as a side effect, in a runtime vendored from `@vitejs/plugin-react`, in a file
// that says nothing about DevTools. "It worked when I tried it" is how that
// regresses silently, which is what ubugeeei-prod/uf#503 asks for a test about.
//
// So there are three questions here and each is one somebody could break
// without noticing:
//
//   * does the script uf injects install a hook a renderer will register with,
//     and does it leave the extension's alone when there is one;
//   * does `uf dev` inject it at all, before every module, and does a build
//     inject nothing;
//   * do the other two conditions DevTools needs still hold — one copy of
//     `react-dom`, and the development build of it.
//
// The script is run rather than read: `new Function` evaluates the exact text
// the browser is sent, against a window this file makes up, so what is asserted
// is behaviour and not a substring. `crates/uf_cli/tests/vite.rs` owns the half
// this cannot see, which is that nothing `uf build` wrote mentions the hook.
//
// `packages/vite/internal/` by path, not a package export, for the reason
// `dev-channel.test.js` gives about `internal/diagnostics.js`: this is the dev
// server's own wiring rather than an interface anything outside
// `@uniflowed/vite` is invited to use.

import { describe, expect, it } from "@uniflowed/test";

import uniflowed from "./index.js";
import { DEVTOOLS_HOOK, devtoolsPreamble } from "./internal/devtools.js";
// The DOM is installed before the refresh runtime is imported, and never after:
// that module assigns `window.__registerBeforePerformReactRefresh` while it is
// being evaluated, so on a process with no window it throws at import rather
// than at use. The same reason `rsc-split.test.js` reaches for
// `@uniflowed/router/client` through a dynamic import.
import { installDom } from "../../packages/react-testing/internal/dom.js";
import {
  DEVTOOLS_HOOK as ROUTER_DEVTOOLS_HOOK,
  devtoolsProblem,
  reportDevtools,
} from "../../packages/router/internal/devtools.js";

/** A window with nothing on it, which is a browser with no extension. */
function emptyWindow(): $FlowFixMe {
  return {};
}

/**
 * Run the injected script against `win`, the way the document does.
 *
 * `security/no-eval` is the right rule and this is the case it is not about.
 * The rule exists because compiling a string into code is how untrusted input
 * becomes execution; the string here is `devtoolsPreamble()`'s own return
 * value, generated a line earlier in this process from a template literal with
 * one interpolation, and it never leaves this file. And the alternative is
 * worse than the risk it avoids: what has to be asserted is that the exact text
 * a browser is sent installs a hook a renderer will register with, so a test
 * that re-implemented the script — or matched substrings of it — would go on
 * passing after the script it is about stopped working. Running it is the whole
 * of the coverage.
 */
function runPreamble(win: $FlowFixMe): void {
  // uf-lint-disable-next-line security/no-eval
  new Function("window", devtoolsPreamble())(win);
}

/** The vendored Fast Refresh runtime, once there is a window to import it. */
async function refreshRuntime(): Promise<$FlowFixMe> {
  installDom();
  return import("./internal/refresh-runtime.js");
}

/** The uf plugin, configured the way the driver configures it. */
function plugin(config?: $FlowFixMe): $FlowFixMe {
  const plugins: $FlowFixMe = uniflowed({ root: process.cwd(), config: config ?? {} });
  return plugins[0];
}

/** Put the plugin in the state one command leaves it in. */
function forCommand(flow: $FlowFixMe, command: "serve" | "build"): $FlowFixMe {
  return flow.config(
    { root: process.cwd() },
    { mode: command === "build" ? "production" : "development", command },
  );
}

describe("the hook uf installs", () => {
  it("gives a renderer something to register with", () => {
    // What `react-dom` does on the line this exists for: read the global, and
    // if it is there, hand it a renderer and keep the id it gets back. A hook
    // missing any of these is a hook React declines to use.
    const win = emptyWindow();
    runPreamble(win);

    const hook = win[DEVTOOLS_HOOK];
    expect(hook).not.toBe(undefined);
    expect(hook.supportsFiber).toBe(true);
    expect(typeof hook.inject).toBe("function");
    expect(typeof hook.onCommitFiberRoot).toBe("function");
    expect(typeof hook.onCommitFiberUnmount).toBe("function");
    expect(typeof hook.onScheduleFiberRoot).toBe("function");
    expect(hook.inject({})).toBe(0);
    expect(hook.inject({})).toBe(1);
  });

  it("leaves the extension's hook exactly where it was", () => {
    // The one way this file could break the thing it exists to support. The
    // extension installs its hook at `document_start`, before any script in the
    // document, and it is the end of the connection to the panel — replacing it
    // with a stub is a page DevTools can see and never hear from.
    const win = emptyWindow();
    const extension = { renderers: new Map(), supportsFiber: true, inject: () => 7 };
    win[DEVTOOLS_HOOK] = extension;

    runPreamble(win);

    expect(win[DEVTOOLS_HOOK]).toBe(extension);
    expect(win[DEVTOOLS_HOOK].inject({})).toBe(7);
  });

  it("is a hook Fast Refresh decorates rather than complains about", async () => {
    // The two preambles meet on this object: uf's installs it and the refresh
    // runtime's `injectIntoGlobalHook` wraps `inject`, `onCommitFiberRoot` and
    // `onScheduleFiberRoot` so that a refresh can find the renderer again. A
    // hook of the wrong shape makes it log "something has shimmed the React
    // DevTools global hook" and turn Fast Refresh off, which is the failure
    // this ordering has to not cause.
    const win = emptyWindow();
    runPreamble(win);
    const installed = win[DEVTOOLS_HOOK];
    const before = installed.onCommitFiberRoot;

    const { injectIntoGlobalHook } = await refreshRuntime();
    injectIntoGlobalHook(win);

    expect(win[DEVTOOLS_HOOK]).toBe(installed);
    expect(installed.isDisabled).toBe(undefined);
    expect(installed.onCommitFiberRoot).not.toBe(before);
    // Still a working `inject` after the decoration: the id the renderer keeps
    // comes back through the wrapper.
    expect(installed.inject({})).toBe(0);
  });
});

describe("what a document is given", () => {
  it("injects the hook, as a classic script, above the module preamble", () => {
    // A module script is deferred: it runs once the document has been parsed.
    // That is still before the client entry today, and "before, as long as
    // nobody adds a script above it" is the accident #503 is about. A classic
    // inline script runs while the parser is on it, so no module and no
    // `<script src>` a project's own Vite plugin injects can get between this
    // and the renderer — which is why the absence of `type` is asserted.
    // The accessibility audit is off, so what this counts is the DevTools
    // document and nothing else. `uf dev` also injects the axe runtime when
    // the project has the engine — this repository does — and that tag goes
    // into the *body*, where it cannot come between the hook and the renderer;
    // `a11y-audit.test.js` owns the assertion that it is there, is last, and
    // leaves these two exactly as they are. Turning it off here rather than
    // widening the count keeps this file able to fail when a third tag appears
    // in the head, which is the failure ubugeeei-prod/uf#503 is about.
    const flow = plugin({ accessibility: { devAudit: false } });
    forCommand(flow, "serve");

    const tags = flow.transformIndexHtml();

    expect(tags.length).toBe(2);
    expect(tags[0].tag).toBe("script");
    expect(tags[0].attrs).toBe(undefined);
    expect(tags[0].injectTo).toBe("head-prepend");
    expect(tags[0].children).toContain(DEVTOOLS_HOOK);
    // The Fast Refresh preamble is second, and it is a module.
    expect(tags[1].attrs?.type).toBe("module");
  });

  it("injects nothing into a build", () => {
    // The half a reader can check on the artefact rather than by reading, and
    // the half `a_build_ships_no_devtools_hook` in `crates/uf_cli/tests/vite.rs`
    // checks on a real `dist/`.
    const flow = plugin();
    forCommand(flow, "build");

    expect(flow.transformIndexHtml()).toEqual([]);
  });
});

describe("the two conditions that are not this file's", () => {
  it("keeps one copy of the renderer", () => {
    // Two copies of `react-dom` register two renderers with the hook, and
    // DevTools shows the tree of whichever one it heard from — the shape of the
    // "multiple renderers concurrently rendering" report. `dedupe` is what
    // stops a project with a transitive React from getting a second one, and it
    // is one line away from being deleted by somebody tidying a config.
    const config = forCommand(plugin(), "serve");

    expect(config.resolve.dedupe).toEqual(["react", "react-dom"]);
  });

  it("pre-bundles the development JSX runtime", () => {
    // Against React's production build DevTools says so and shows a tree with
    // no props, no hooks and no source. `jsx-dev-runtime` is what `uf
    // transform` emits calls to when it is asked for a development build, so a
    // dependency optimiser that had never heard of it is a dev server that
    // reloads the page the first time a component renders.
    const config = forCommand(plugin(), "serve");

    expect(config.optimizeDeps.include).toContain("react/jsx-dev-runtime");
    expect(config.optimizeDeps.include).toContain("react-dom/client");
  });
});

describe("the check the page makes on itself", () => {
  // `@uniflowed/vite` says the hook will be there; this is what asks a running
  // page whether it is. The two are different claims — the preamble goes into a
  // document a project's own plugins also write to, and what has to be true is
  // an ordering rather than a line of configuration — which is why #503 asks
  // for a check and not only an injector.

  it("spells the global the way the injector does", () => {
    // Two constants, because `@uniflowed/vite` is loaded by Vite before any
    // Flow transform exists and `@uniflowed/router` is Flow, so neither can
    // import the other's. The whole value of the name is that it matches
    // React's exactly, so the agreement is asserted rather than assumed — the
    // same bargain `dev-channel.test.js` makes about the endpoint paths.
    expect(ROUTER_DEVTOOLS_HOOK).toBe(DEVTOOLS_HOOK);
  });

  it("says nothing about a page with one renderer registered", () => {
    // The ordinary development page, and the case that has to stay silent: a
    // check that reported on every load would be a warning people learn to
    // scroll past.
    const win = emptyWindow();
    runPreamble(win);
    const hook = win[DEVTOOLS_HOOK];
    hook.renderers.set(hook.inject({}), {});

    expect(devtoolsProblem(win)).toBe(null);
  });

  it("says nothing about a hook whose renderers it does not recognise", () => {
    // A hook uf did not install and DevTools did not either — a browser
    // extension of some other kind, or a version of the panel whose bookkeeping
    // is not a `Map`. Reporting on it would be this module failing to recognise
    // something and calling it the page's problem.
    const win = emptyWindow();
    win[DEVTOOLS_HOOK] = { renderers: { size: 4 }, supportsFiber: true };

    expect(devtoolsProblem(win)).toBe(null);
  });

  it("reports a page whose hook never arrived", () => {
    // What a document looks like when something got between the preamble and
    // `react-dom`, or when the preamble never reached it: React registered with
    // nothing and the panel says the page is not using React.
    const problem = devtoolsProblem(emptyWindow());

    expect(problem?.message).toContain(DEVTOOLS_HOOK);
    expect(problem?.message).toContain("cannot attach");
    // The message says what to do about it, because the reader is looking at a
    // terminal and not at this file.
    expect(problem?.detail.join(" ")).toContain("classic script");
  });

  it("reports a page carrying two renderers", () => {
    // Two copies of `react-dom`, which is the shape of the "multiple renderers
    // concurrently rendering the same context provider" report: the panel shows
    // one tree and the rest of the page is missing from it with nothing on
    // screen saying why.
    const win = emptyWindow();
    runPreamble(win);
    const hook = win[DEVTOOLS_HOOK];
    hook.renderers.set(hook.inject({}), {});
    hook.renderers.set(hook.inject({}), {});

    const problem = devtoolsProblem(win);

    expect(problem?.message).toContain("2 renderers");
    expect(problem?.detail.join(" ")).toContain("dedupe");
  });

  it("is not a page's problem when the hook throws at it", () => {
    // The whole of this module's contract with the page it runs on: it is
    // called from the line after a successful hydration, so anything it cannot
    // read is something it declines to report rather than something it raises
    // over a page that is otherwise working.
    const win = emptyWindow();
    win[DEVTOOLS_HOOK] = {
      // uf-lint-disable-next-line flow/unsafe-getters-setters
      get renderers(): mixed {
        throw new Error("no");
      },
    };

    expect(() => devtoolsProblem(win)).toThrow();
    expect(() => reportDevtools(win)).not.toThrow();
  });
});
