// @flow
//
// What `uf dev` says when a hydration fails.
//
// React's message is the same every time and names no node, so the thing worth
// testing is not that an error was reported — React does that — but that uf
// found *which* node differed, said what the two sides had there, and named the
// component. Every test below is one of the shapes a real mismatch takes, and
// each asserts the three facts a reader needs rather than the wording around
// them.
//
// The analysis is deliberately reachable without a hydration: `hydrationReport`
// takes two trees and a document and returns the report, so a test builds the
// server's markup as a string and the client's as a real DOM and asks what
// differs. That is the same call `hydrationErrorHandler` makes, with the same
// arguments, so what is tested here is what runs in a browser.
//
// The module is internal to `@uniflowed/router` and is reached by path, the way
// `error-boundary.test.js` reaches `packages/vite/internal/events.js`. It has no
// package export and should not: nothing outside the client entry has any
// business installing an overlay.

import { createRequire } from "node:module";

import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, render } from "@uniflowed/react-testing";

import { bodyOf, elementIn } from "./dom.js";
import { DIAGNOSTIC_ENDPOINT } from "../../packages/router/internal/diagnostics.js";
import {
  SERVER_MARKUP_LIMIT,
  captureServerMarkup,
  componentsOf,
  formatHydrationReport,
  hydrationErrorHandler,
  hydrationReport,
  isHydrationMessage,
  showHydrationReport,
} from "../../packages/router/internal/hydration.js";

/**
 * React's two renderers, required rather than imported.
 *
 * `react-dom/client` reads `document` while it is being evaluated, so it cannot
 * be a static import in a file that installs the document itself. The same
 * reason `tests/library/rsc-split.test.js` reaches for its two entry points
 * through a dynamic import.
 */
const load = createRequire(import.meta.url);

/** React 19's wording, which is what the handler matches on. */
const MISMATCH =
  "Hydration failed because the server rendered HTML didn't match the client. " +
  "As a result this tree will be regenerated on the client.";

/**
 * A document, and a container holding the client's version of a tree.
 *
 * `render` is what installs the DOM in this process, so every test goes through
 * it once before touching `globalThis.document`. The container is filled by
 * assignment rather than by rendering components, because what is under test is
 * the comparison of two trees and not React's ability to build one.
 */
function trees(serverMarkup: string, clientMarkup: string) {
  render(<div />);
  const document = globalThis.document;
  const container = document.createElement("div");
  container.innerHTML = clientMarkup;
  bodyOf().appendChild(container);
  appended.push(container);
  return { document, container, serverMarkup };
}

/**
 * Everything this file has put in the document, so it can take it out again.
 *
 * The document belongs to the process, not to the file: `uf test` runs several
 * files in one worker and they share the DOM this suite installs. A container
 * left behind here is a `<main>` or an `<img>` in the next file's document, and
 * a `getByRole` there finds two of something and fails on a page it never
 * rendered — which is what happened to `web.test.js` when this file grew and
 * the scheduler moved the two into the same worker.
 */
const appended: Array<Element> = [];

afterEach(() => {
  while (appended.length > 0) {
    appended.pop()?.remove();
  }
  // The panel too: `showHydrationReport` mounts it on the body rather than in a
  // container, so it outlives everything above.
  //
  // `document` is read through the global rather than through `bodyOf`, because
  // this runs after *every* test in the file including the ones that never
  // rendered — and in a worker where no file has installed a DOM yet there is
  // no document at all. `bodyOf` throws there, which would fail the first test
  // in the file for something it did not do.
  const body = globalThis.document?.body;
  if (body == null) {
    return;
  }
  for (const host of Array.from(body.querySelectorAll("#uf-hydration-overlay"))) {
    host.remove();
  }
});

function reportFor(serverMarkup: string, clientMarkup: string, message: string = MISMATCH) {
  const built = trees(serverMarkup, clientMarkup);
  return hydrationReport({
    message,
    serverMarkup: built.serverMarkup,
    container: built.container,
    document: built.document,
    componentStack:
      "\n    at Article (http://localhost/src/Article.js:12:3)\n    at Layout\n    at App",
  });
}

describe("recognising the errors that are about hydration", () => {
  /**
   * An overlay that opened for every recoverable error is an overlay people
   * turn off. React sends a Suspense boundary that recovered on the client
   * through the same callback, and that is not a bug anybody has to see.
   */
  it("takes React's mismatch wording and leaves its other recoveries alone", () => {
    expect(isHydrationMessage(MISMATCH)).toBe(true);
    expect(isHydrationMessage("In HTML, <div> cannot be a descendant of <p>.")).toBe(true);
    expect(isHydrationMessage("Text content did not match. Server: a Client: b")).toBe(true);
    expect(
      isHydrationMessage(
        "The server could not finish this Suspense boundary, likely due to an error",
      ),
    ).toBe(false);
  });
});

describe("finding the node that differed", () => {
  /**
   * The whole point of keeping the server's markup.
   *
   * React says a tree did not match. This says which text, in which element, at
   * which path — the three things a reader would otherwise find by hand.
   */
  it("reports the text, the path to it and both sides of it", () => {
    const report = reportFor(
      "<main><h1>Posts</h1><p>3 minutes ago</p></main>",
      "<main><h1>Posts</h1><p>5 minutes ago</p></main>",
    );
    const difference = report.difference;
    expect(difference?.kind).toBe("text");
    expect(difference?.server).toBe("3 minutes ago");
    expect(difference?.client).toBe("5 minutes ago");
    expect(difference?.path).toBe("main:nth-child(1) > p:nth-child(2) > text()[0]");
  });

  /**
   * The first difference in document order, not the first one a walk happens to
   * reach. A depth-first walk that pushed children in the wrong order would
   * blame the last paragraph on the page for a mismatch in the first.
   */
  it("blames the earliest difference when there is more than one", () => {
    const report = reportFor(
      "<ul><li>one</li><li>two</li><li>three</li></ul>",
      "<ul><li>ONE</li><li>two</li><li>THREE</li></ul>",
    );
    expect(report.difference?.server).toBe("one");
  });

  /** An id is what a reader recognises, so the path uses it over a position. */
  it("names a node by its id where it has one", () => {
    const report = reportFor(
      '<section id="cart"><span>0</span></section>',
      '<section id="cart"><span>2</span></section>',
    );
    expect(report.difference?.path).toBe("section#cart > span:nth-child(1) > text()[0]");
  });

  /**
   * A node on one side and nothing on the other is the commonest mismatch there
   * is — it is what every `typeof window !== "undefined"` branch produces — and
   * the report has to be able to say "the server sent nothing here".
   */
  it("reports a node the client rendered and the server did not", () => {
    const report = reportFor(
      "<div><p>hello</p></div>",
      "<div><p>hello</p><aside>1024px</aside></div>",
    );
    const difference = report.difference;
    expect(difference?.kind).toBe("extra");
    expect(difference?.server).toBe(null);
    expect(difference?.client).toBe("<aside>1024px</aside>");
  });

  it("reports a node the server sent and the client did not", () => {
    const report = reportFor(
      "<div><p>hello</p><aside>server</aside></div>",
      "<div><p>hello</p></div>",
    );
    const difference = report.difference;
    expect(difference?.kind).toBe("missing");
    expect(difference?.server).toBe("<aside>server</aside>");
    expect(difference?.client).toBe(null);
  });

  /**
   * An attribute is the quiet one: the text matches, the page looks right, and
   * a `className` computed from `window.innerWidth` is what actually differed.
   * Attributes are compared in sorted order so two runs blame the same one.
   */
  it("names the attribute when the elements are otherwise the same", () => {
    const report = reportFor(
      '<div><img alt="" src="/a.png" width="10"/></div>',
      '<div><img alt="" src="/b.png" width="10"/></div>',
    );
    const difference = report.difference;
    expect(difference?.kind).toBe("attribute");
    expect(difference?.attribute).toBe("src");
    expect(difference?.server).toBe("/a.png");
    expect(difference?.client).toBe("/b.png");
  });

  it("reports different elements at the same position as a tag difference", () => {
    const report = reportFor("<div><span>x</span></div>", "<div><b>x</b></div>");
    expect(report.difference?.kind).toBe("tag");
  });

  /**
   * React repairs as it goes, so a mismatch reported late enough leaves two
   * trees that already agree. Showing them side by side and saying nothing
   * would read as a broken tool; the note is what makes it read as a fact.
   */
  it("says the trees agree rather than showing two identical ones", () => {
    const report = reportFor("<p>same</p>", "<p>same</p>");
    expect(report.difference).toBe(null);
    expect(report.note).toContain("already repaired");
  });
});

describe("saying which of the usual causes it was", () => {
  /**
   * The classifier's one rule, and the reason it is a digit scan rather than a
   * regular expression: both strings came out of the application's own page,
   * and text uf did not write does not go through a backtracking engine.
   */
  it("calls the same sentence with different numbers in it variable input", () => {
    expect(reportFor("<p>42 items</p>", "<p>43 items</p>").cause).toBe("variable-input");
    expect(reportFor("<p>1/2/2026</p>", "<p>02/01/2026</p>").cause).toBe("variable-input");
    expect(reportFor("<p>0.8102</p>", "<p>0.4471</p>").cause).toBe("variable-input");
  });

  /**
   * Two different words are not a clock. Calling them one would send the reader
   * to look for a `Date.now()` that is not there, which is worse than the
   * honest "unknown" the report gives instead.
   */
  it("declines to call two different words a clock", () => {
    const report = reportFor("<p>Sign in</p>", "<p>Sign out</p>");
    expect(report.cause).toBe("unknown");
    expect(report.remedy).toBe("");
  });

  it("calls a node that is only on one side a browser-only reading", () => {
    expect(reportFor("<div></div>", "<div><b>dark</b></div>").cause).toBe("browser-only");
    expect(reportFor("<p></p>", "<p>1200px</p>").cause).toBe("browser-only");
  });

  /**
   * An attribute is classified by the same two questions as text — is one side
   * missing, and do the digits move — and the answers point at different
   * mistakes: a `width` computed from `innerWidth` against a `src` chosen by a
   * coin flip. The three cases are asserted together because the classifier
   * reaches them through one arm each, and a fourth kind arriving in
   * `DifferenceKind` must not quietly land in any of them.
   */
  it("reads an attribute by whether it is one-sided and whether its digits move", () => {
    expect(
      reportFor(
        '<div><img alt="" src="/a.png" width="10"/></div>',
        '<div><img alt="" src="/a.png" width="12"/></div>',
      ).cause,
    ).toBe("variable-input");
    expect(
      reportFor('<div><img alt=""/></div>', '<div><img alt="" data-width="1200"/></div>').cause,
    ).toBe("browser-only");
    expect(
      reportFor('<div><img alt="" src="/a.png"/></div>', '<div><img alt="" src="/b.png"/></div>')
        .cause,
    ).toBe("unknown");
  });

  /**
   * Two different nodes in the same place — a `<span>` where the server sent a
   * `<b>`, an element where it sent text — say that the trees diverged, and
   * nothing about why. Naming a cause there would be a guess, and a guess sends
   * the reader looking for a clock that was never in the file.
   */
  it("gives no cause for two trees that hold different nodes", () => {
    expect(reportFor("<div><span>x</span></div>", "<div><b>x</b></div>").cause).toBe("unknown");
    expect(reportFor("<div>x</div>", "<div><b>x</b></div>").cause).toBe("unknown");
  });

  /**
   * Nesting is the one cause the trees cannot show, because the parser has
   * already moved the node by the time either tree exists. React's own error is
   * the signal, and it is read before anything else.
   */
  it("takes React's word for it when the markup could not nest", () => {
    const report = reportFor(
      "<div><p>one</p></div>",
      "<div><p>two</p></div>",
      "In HTML, <div> cannot be a descendant of <p>.",
    );
    expect(report.cause).toBe("invalid-nesting");
  });
});

describe("naming the component", () => {
  it("takes the names out of React's stack and drops the bundler's urls", () => {
    const names = componentsOf(
      "\n    at Article (http://localhost/src/Article.js:12:3)\n    at Layout (http://localhost/src/Layout.js:4:1)\n    at App",
    );
    expect(names).toEqual(["Article", "Layout", "App"]);
  });

  /**
   * React's stack interleaves the elements with the components, so the
   * innermost frame of a text mismatch is always the `<p>` the text is in —
   * which the path already names. Keeping it would answer a question nobody
   * asked and bury the one they did.
   */
  it("drops the host elements and keeps the components", () => {
    expect(componentsOf("\n    at p\n    at main\n    at Posted\n    at App")).toEqual([
      "Posted",
      "App",
    ]);
  });

  it("reports no components rather than failing when React sent no stack", () => {
    expect(componentsOf(null)).toEqual([]);
  });
});

describe("the snapshot of the server's markup", () => {
  it("keeps the children of the element React hydrates", () => {
    render(<div />);
    const container = globalThis.document.createElement("div");
    container.innerHTML = "<p>server</p>";
    expect(captureServerMarkup(container)).toBe("<p>server</p>");
  });

  /**
   * Every hydration pays for this, on every page, whether anything goes wrong
   * or not. A document over the ceiling is not snapshotted, the report says so,
   * and the page is not made to carry a second copy of itself for a diagnostic
   * nobody may ever read.
   */
  it("keeps nothing at all from a document over the ceiling", () => {
    render(<div />);
    const container = globalThis.document.createElement("div");
    container.innerHTML = `<p>${"x".repeat(SERVER_MARKUP_LIMIT)}</p>`;
    expect(captureServerMarkup(container)).toBe(null);
  });

  /**
   * An application whose root layout renders `<html>` hydrates the document
   * itself, and a `Document` has no `innerHTML` to take a copy of. Both halves
   * of that path — the whole element on the way out, a real HTML parse on the
   * way back — are only exercised here, and getting either wrong would leave
   * every such application with no diagnostic at all.
   */
  it("compares a whole document for an application that renders html itself", () => {
    render(<div />);
    const client = new DOMParser().parseFromString(
      "<html><head></head><body><p>client</p></body></html>",
      "text/html",
    );
    expect(captureServerMarkup(client)).toContain("<p>client</p>");

    const report = hydrationReport({
      message: MISMATCH,
      serverMarkup: "<html><head></head><body><p>server</p></body></html>",
      container: client,
      document: globalThis.document,
      componentStack: null,
    });
    expect(report.difference?.server).toBe("server");
    expect(report.difference?.client).toBe("client");
  });

  it("says why there is no comparison when there was no snapshot", () => {
    render(<div />);
    const report = hydrationReport({
      message: MISMATCH,
      serverMarkup: null,
      container: globalThis.document.createElement("div"),
      document: globalThis.document,
      componentStack: null,
    });
    expect(report.difference).toBe(null);
    expect(report.note).toContain("No copy of the server's markup");
  });
});

describe("the report as text", () => {
  it("leads with the two values and the path, and puts React's sentence last", () => {
    const text = formatHydrationReport(
      reportFor("<main><p>3 minutes ago</p></main>", "<main><p>5 minutes ago</p></main>"),
    );
    expect(text).toContain("Hydration mismatch in <Article>");
    expect(text).toContain("server   3 minutes ago");
    expect(text).toContain("client   5 minutes ago");
    expect(text).toContain("Rendered by: Article < Layout < App");
    expect(text).toContain(`React said: ${MISMATCH}`);
  });
});

describe("a real hydration, failing", () => {
  /**
   * The whole chain, with React doing the reporting.
   *
   * Every other test in this file hands the analysis two trees it built itself.
   * This one renders a component on the server, hydrates the same component
   * with a different value, and lets React decide there was a mismatch — so
   * what is checked is that `onRecoverableError` fires where `client.js`
   * installs it, that the snapshot taken before `hydrateRoot` survives React
   * repairing the tree, and that the two values reach the panel. A change to
   * React's recovery timing that left the handler holding a repaired tree would
   * fail here and nowhere else.
   */
  it("shows both sides of the value React repaired", () => {
    render(<div />);
    const document = globalThis.document;
    const server = load("react-dom/server");
    const { hydrateRoot } = load("react-dom/client");

    component Posted(at: string) {
      return (
        <main>
          <h1>Posts</h1>
          <p>{at}</p>
        </main>
      );
    }

    const container = document.createElement("div");
    container.innerHTML = String(server.renderToString(<Posted at="3 minutes ago" />));
    bodyOf().appendChild(container);

    // Taken here, which is the moment `client.js` takes it: after the parser
    // and before React.
    const markup = captureServerMarkup(container);
    const handler = hydrationErrorHandler(container, markup, document);

    let root = null;
    act(() => {
      root = hydrateRoot(container, <Posted at="5 minutes ago" />, {
        onRecoverableError: handler,
      });
    });

    const host = elementIn(bodyOf(), "#uf-hydration-overlay");
    const shown = host.shadowRoot?.textContent ?? "";
    // The component, not the `<p>` React's innermost frame names: the element
    // is already in the path a line above, and the question is whose component
    // rendered it.
    expect(shown).toContain("Hydration mismatch in <Posted>");
    expect(shown).toContain("3 minutes ago");
    expect(shown).toContain("5 minutes ago");
    expect(shown).toContain("different every time it is read");

    host.remove();
    act(() => {
      root?.unmount();
    });
    container.remove();
  });
});

describe("reaching the terminal", () => {
  /**
   * The half of the report that is not in the browser.
   *
   * A hydration mismatch used to exist in a panel and in the console, and
   * nowhere else. Both are in a window that may not be in front, read by
   * somebody who knows to look — while every other uf diagnostic arrives in the
   * terminal the developer already has open. `uf dev` serves
   * `DIAGNOSTIC_ENDPOINT` and renders what arrives with the severity, the page
   * and the same words; this is the call that gets it there. See
   * ubugeeei-prod/uf#583.
   *
   * The headline and the detail are asserted together with the overlay's text,
   * because the point is that they are the *same* report: one formatter, three
   * destinations, and no chance of the terminal and the panel disagreeing about
   * what differed.
   */
  it("posts the report the panel shows to the dev server", () => {
    render(<div />);
    const document = globalThis.document;
    const container = document.createElement("div");
    container.innerHTML = "<p>4 items</p>";
    bodyOf().appendChild(container);
    appended.push(container);

    const posted: Array<{ readonly target: string, readonly body: mixed }> = [];
    const restore = stubFetch((target, init) => {
      posted.push({ target, body: init.body });
      return Promise.resolve(null);
    });
    try {
      const handler = hydrationErrorHandler(container, "<p>3 items</p>", document);
      handler(new Error(MISMATCH), {
        componentStack: "\n    at Article (http://localhost/src/Article.js:12:3)\n    at App",
      });
    } finally {
      restore();
    }

    const shown = elementIn(bodyOf(), "#uf-hydration-overlay").shadowRoot?.textContent ?? "";

    expect(posted.length).toBe(1);
    expect(posted[0].target).toBe(DIAGNOSTIC_ENDPOINT);
    const sent = JSON.parse(String(posted[0].body));
    // Loud, because a mismatch is wrong rather than merely worth knowing.
    expect(sent.severity).toBe("error");
    expect(sent.message).toBe("Hydration mismatch in <Article>");
    // No file and no line: a mismatch is a fact about a DOM node, and a code
    // frame drawn around an invented position would send the reader to a line
    // that is not the answer.
    expect(sent.file).toBe(undefined);
    expect(sent.line).toBe(undefined);
    const detail = sent.detail.join("\n");
    for (const fact of ["3 items", "4 items", "different every time it is read"]) {
      expect(detail).toContain(fact);
      expect(shown).toContain(fact);
    }
  });

  /**
   * A production page has no `/__uf/` anything, and a browser that is not
   * running under `uf dev` is the ordinary case for this module's one caller
   * being wrong about its gate. Losing the report is the right outcome; turning
   * it into an unhandled rejection in somebody's error reporter is not.
   */
  it("does not fail the page when nothing answers", () => {
    render(<div />);
    const document = globalThis.document;
    const container = document.createElement("div");
    container.innerHTML = "<p>b</p>";
    bodyOf().appendChild(container);
    appended.push(container);

    const restore = stubFetch(() => Promise.reject(new Error("404")));
    try {
      const handler = hydrationErrorHandler(container, "<p>a</p>", document);
      expect(() => {
        handler(new Error(MISMATCH), { componentStack: "\n    at App" });
      }).not.toThrow();
    } finally {
      restore();
    }
  });
});

/**
 * Put `fetch` on the window the reporter reads, and give back the undo.
 *
 * `Object.defineProperty` and the previous descriptor rather than assignment
 * and `delete`: the DOM this suite installs may define `fetch` on its window as
 * an accessor or not define it at all, and both have to be put back exactly as
 * they were or the next file in the run inherits this one's stub.
 */
function stubFetch(post: (target: string, init: { readonly body: mixed, ... }) => Promise<mixed>) {
  const target: $FlowFixMe = globalThis.window ?? globalThis;
  const previous = Object.getOwnPropertyDescriptor(target, "fetch");
  Object.defineProperty(target, "fetch", { value: post, configurable: true, writable: true });
  return () => {
    if (previous == null) {
      delete target.fetch;
    } else {
      Object.defineProperty(target, "fetch", previous);
    }
  };
}

describe("the overlay", () => {
  /**
   * The one thing this overlay must never do.
   *
   * It exists to show markup, and the markup it shows is the page's own — which
   * on a page whose mismatch is in somebody's comment is attacker-authored. A
   * panel that wrote it as HTML would run the page's scripts a second time and
   * be an injection hole in the tool built to find bugs. Every value goes in
   * through `textContent`, and this asserts the consequence: the tag is on
   * screen as characters, and no element was created from it.
   */
  it("writes the page's markup as text and never as markup", () => {
    render(<div />);
    const document = globalThis.document;
    const report = reportFor(
      "<div><p>hello</p></div>",
      '<div><p>hello</p><img src="x" onerror="window.__ufOverlayEscaped = true"/></div>',
    );
    showHydrationReport(report, document);

    const host = elementIn(bodyOf(), "#uf-hydration-overlay");
    const root = host.shadowRoot;
    expect(root).not.toBe(null);
    const shown = root?.textContent ?? "";
    expect(shown).toContain("window.__ufOverlayEscaped");
    // The characters are on screen; the element they spell is not in the panel.
    expect(root?.querySelectorAll("img").length).toBe(0);
    expect(globalThis.window.__ufOverlayEscaped).toBe(undefined);
    host.remove();
  });

  /**
   * A page can report six mismatches in one hydration. Six stacked panels hide
   * the page and each other; the console still has all six.
   */
  it("draws one panel however many times it is asked", () => {
    render(<div />);
    const document = globalThis.document;
    const report = reportFor("<p>a</p>", "<p>b</p>");
    showHydrationReport(report, document);
    showHydrationReport(report, document);
    expect(bodyOf().querySelectorAll("#uf-hydration-overlay").length).toBe(1);
    elementIn(bodyOf(), "#uf-hydration-overlay").remove();
  });

  it("shows the path, both sides and the component it came from", () => {
    render(<div />);
    const document = globalThis.document;
    showHydrationReport(reportFor("<p>3 items</p>", "<p>4 items</p>"), document);

    const host = elementIn(bodyOf(), "#uf-hydration-overlay");
    const shown = host.shadowRoot?.textContent ?? "";
    expect(shown).toContain("Hydration mismatch in <Article>");
    expect(shown).toContain("3 items");
    expect(shown).toContain("4 items");
    expect(shown).toContain("Article < Layout < App");
    host.remove();
  });
});
