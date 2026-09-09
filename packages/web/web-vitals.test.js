// @flow
//
// The five measurements a page is judged on.
//
// Almost every case here is about a number that would be *wrong* rather than
// missing, which is the failure mode this whole module is arranged against: a
// CLS that is a sum instead of a windowed maximum, an INP that is the unluckiest
// interaction instead of a percentile, an LCP measured from the moment a
// prerender started rather than from the moment the reader arrived. Each of
// those produces a plausible number, so none of them is caught by looking at a
// dashboard — only by a test that knows what the right answer is.
//
// The other half is about absence. A browser that cannot measure something has
// to report nothing rather than zero, and zero is the best possible score for
// three of these five, so getting that wrong makes the worst pages look like
// the best ones.
//
// # How the browser is faked
//
// `uf test` runs on Node, whose real `PerformanceObserver` supports `mark`,
// `measure`, `resource` and a set of things about sockets — none of the entry
// types here. That is not an obstacle, it is one of the cases: it is exactly
// what an old browser looks like, and the "reports nothing" tests use it
// directly with nothing installed. Everything else installs an observer that
// serves precisely the entry types the case names, so a test that thinks it is
// measuring CLS cannot accidentally be served an LCP.

import { StrictMode } from "react";

import { afterEach, describe, expect, it, render } from "@uniflowed/testing";
import { VITALS_ENDPOINT, collectVitals, useVitals, vitalsBeacon } from "@uniflowed/web";
import type { Vital } from "@uniflowed/web";

/** Undone after each case, in reverse, whether or not the case threw. */
const undo: Array<() => void> = [];

afterEach(() => {
  for (let index = undo.length - 1; index >= 0; index -= 1) {
    undo[index]();
  }
  undo.length = 0;
});

/**
 * Replace one property and remember how to put it back.
 *
 * `Object.defineProperty` rather than assignment because most of what these
 * tests replace — `performance`, `visibilityState`, `sendBeacon` — is an
 * accessor on a prototype, and an assignment to one of those either throws or
 * silently does nothing.
 */
function replace(target: $FlowFixMe, name: string, value: mixed): void {
  const previous = Object.getOwnPropertyDescriptor(target, name);
  Object.defineProperty(target, name, { value, configurable: true, writable: true });
  undo.push(() => {
    if (previous == null) {
      delete target[name];
    } else {
      Object.defineProperty(target, name, previous);
    }
  });
}

/**
 * The window the collector will find.
 *
 * Rendering is what installs a document on a test process that has none, and
 * the collector looks for `globalThis.window` — so a case that only wants a
 * window still has to have rendered something first.
 */
function theWindow(): $FlowFixMe {
  render(<span />);
  return globalThis.window;
}

/** One observer the code under test asked for. */
type Live = {
  readonly type: string,
  readonly options: $FlowFixMe,
  readonly callback: ($FlowFixMe) => void,
};

/** What a faked browser lets a case do. */
type FakeBrowser = {
  /** Hand entries to whichever observers asked for this type. */
  readonly deliver: (type: string, entries: $ReadOnlyArray<mixed>) => void,
  /** The options one observe call was made with. */
  readonly observeOf: (type: string) => $FlowFixMe,
  /** How many observers have been constructed. */
  readonly constructed: () => number,
  /** The live navigation entry, which a case may fill in as a browser would. */
  readonly navigation: $FlowFixMe,
  /** Put the page away, the way a tab being closed does. */
  readonly hide: () => void,
  /** Put the page away, the way switching to another tab does. */
  readonly conceal: () => void,
};

/** How a case describes the browser it wants. */
type FakeOptions = {
  readonly supports?: $ReadOnlyArray<string>,
  /** A type the browser lists and then refuses to observe. */
  readonly refuses?: string,
  readonly interactionCount?: number,
  readonly navigationType?: string,
  readonly responseStart?: number,
  readonly activationStart?: number,
};

/** Install a browser that serves exactly the entry types a case names. */
function fakeBrowser(options: FakeOptions): FakeBrowser {
  const win = theWindow();
  const supported = options.supports ?? [];
  const live: Array<Live> = [];
  let built = 0;

  class Observer {
    static supportedEntryTypes: $ReadOnlyArray<string> = supported;
    callback: ($FlowFixMe) => void;

    constructor(callback: ($FlowFixMe) => void) {
      built += 1;
      this.callback = callback;
    }

    observe(init: $FlowFixMe): void {
      if (init.type === options.refuses) {
        throw new Error(`this document will not serve ${String(init.type)} entries`);
      }
      live.push({ type: init.type, options: init, callback: this.callback });
    }

    disconnect(): void {
      for (let index = live.length - 1; index >= 0; index -= 1) {
        if (live[index].callback === this.callback) {
          live.splice(index, 1);
        }
      }
    }
  }

  const navigation: $FlowFixMe = {
    type: options.navigationType ?? "navigate",
    responseStart: options.responseStart ?? 0,
    activationStart: options.activationStart ?? 0,
  };

  replace(win, "PerformanceObserver", Observer);
  replace(win, "performance", {
    getEntriesByType: (type: string) => (type === "navigation" ? [navigation] : []),
    interactionCount: options.interactionCount,
  });

  return {
    deliver(type, entries) {
      for (const observer of [...live]) {
        if (observer.type === type) {
          observer.callback({ getEntries: () => entries });
        }
      }
    },
    observeOf(type) {
      const found = live.find((observer) => observer.type === type);
      if (found == null) {
        throw new Error(`nothing observed ${type}`);
      }
      return found.options;
    },
    constructed: () => built,
    navigation,
    hide() {
      win.dispatchEvent(new globalThis.Event("pagehide"));
    },
    conceal() {
      replace(win.document, "visibilityState", "hidden");
      win.document.dispatchEvent(new globalThis.Event("visibilitychange"));
    },
  };
}

/** A reporter that keeps what it was handed. */
function recorder(): { readonly seen: Array<Vital>, readonly report: (Vital) => void } {
  const seen: Array<Vital> = [];
  return { seen, report: (vital: Vital) => void seen.push(vital) };
}

/** The value reported for one metric, or `null` if it was never reported. */
function valueOf(seen: $ReadOnlyArray<Vital>, name: string): number | null {
  const found = seen.find((vital) => vital.name === name);
  return found == null ? null : found.value;
}

/** One `paint` entry. */
function paint(name: string, startTime: number): mixed {
  return { entryType: "paint", name, startTime, duration: 0 };
}

/** One largest-contentful-paint candidate. */
function candidate(startTime: number): mixed {
  return { entryType: "largest-contentful-paint", name: "", startTime, duration: 0 };
}

/** One layout shift. */
function shift(startTime: number, value: number, hadRecentInput?: boolean): mixed {
  return {
    entryType: "layout-shift",
    name: "",
    startTime,
    duration: 0,
    value,
    hadRecentInput: hadRecentInput === true,
  };
}

/** One event belonging to an interaction. */
function interaction(interactionId: number, duration: number, name?: string): mixed {
  return {
    entryType: "event",
    name: name ?? "pointerdown",
    startTime: 0,
    duration,
    interactionId,
  };
}

/** Let a queued microtask run. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("what the browser cannot measure", () => {
  it("a browser without the entry type reports nothing rather than zero", () => {
    // Zero is the best possible score for CLS, LCP and INP, so a collector
    // that reports it for "this browser has no idea" makes a site's worst
    // pages indistinguishable from its best — and it looks like good news, so
    // nobody goes looking. Node's own `PerformanceObserver` supports none of
    // these types, which is exactly the shape of the browser this is about,
    // so nothing is installed here.
    theWindow();
    const { seen, report } = recorder();

    const stop = collectVitals({ report });
    globalThis.window.dispatchEvent(new globalThis.Event("pagehide"));
    stop();

    expect(seen).toEqual([]);
  });

  it("reports a zero cls when the browser measured no shift at all", () => {
    // The other side of the same rule, and the reason it cannot be "skip
    // anything that is zero": a page whose layout never moved has a CLS, it
    // is zero, and it is the answer somebody worked for.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.hide();

    expect(valueOf(seen, "CLS")).toBe(0);
  });

  it("reports no inp for a page nobody interacted with", () => {
    // Unlike CLS there is nothing to have been slow, so an INP of zero would
    // be an invented measurement rather than an observed one.
    const browser = fakeBrowser({ supports: ["event"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(null);
  });

  it("reports nothing for a type the browser lists and then refuses to observe", () => {
    // `supportedEntryTypes` names what the implementation knows about, not
    // what this document will serve, and `observe` is where the difference
    // arrives — as an exception, thrown out of whatever called the collector.
    const browser = fakeBrowser({ supports: ["layout-shift"], refuses: "layout-shift" });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.hide();

    expect(seen).toEqual([]);
  });

  it("collects nothing on a server and still hands back a stop function", () => {
    // Every static route is prerendered, so this runs with no document on
    // every one of them. Returning a no-op rather than throwing is what lets
    // `useVitals` sit in a layout that renders in both places.
    theWindow();
    replace(globalThis, "document", undefined);
    const { seen, report } = recorder();

    const stop = collectVitals({ report });
    stop();

    expect(seen).toEqual([]);
  });
});

describe("layout shift", () => {
  it("reports the worst run of shifts rather than their total", () => {
    // CLS is a windowed maximum. Summing instead is the single most tempting
    // mistake in this file: it agrees with the real metric for every page
    // that shifts once, which is every page anybody writes a test for by
    // hand, and then reports a long-lived page as catastrophic because it
    // added up an hour of small independent corrections.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.05), shift(500, 0.05)]);
    browser.deliver("layout-shift", [shift(9000, 0.03)]);
    browser.hide();

    expect(valueOf(seen, "CLS")).toBeCloseTo(0.1, 5);
  });

  it("starts a new run when a second passes with nothing moving", () => {
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.2), shift(1500, 0.3)]);
    browser.hide();

    expect(valueOf(seen, "CLS")).toBeCloseTo(0.3, 5);
  });

  it("ends a run after five seconds even with no gap in it", () => {
    // Without the span, a page that shifts every 900ms forever is one
    // unbounded window and its score grows for as long as the tab is open.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [
      shift(0, 0.02),
      shift(900, 0.02),
      shift(1800, 0.02),
      shift(2700, 0.02),
      shift(3600, 0.02),
      shift(4500, 0.02),
      shift(5400, 0.02),
    ]);
    browser.hide();

    expect(valueOf(seen, "CLS")).toBeCloseTo(0.12, 5);
  });

  it("ignores a shift the reader asked for", () => {
    // Opening a menu moves the page and is not a defect. Only the browser
    // knows which shifts followed an interaction, which is why this is a flag
    // to respect rather than a heuristic to write.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.4, true), shift(100, 0.01)]);
    browser.hide();

    expect(valueOf(seen, "CLS")).toBeCloseTo(0.01, 5);
  });
});

describe("largest contentful paint", () => {
  it("takes the last candidate the browser nominated", () => {
    // The browser nominates a bigger element each time it finds one, so the
    // first entry is a hero image being replaced by the text under it, not
    // the answer.
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("largest-contentful-paint", [candidate(300)]);
    browser.deliver("largest-contentful-paint", [candidate(1200)]);
    browser.hide();

    expect(valueOf(seen, "LCP")).toBe(1200);
  });

  it("reports the paint as soon as the reader interacts", () => {
    // The browser stops nominating candidates at the first interaction, so
    // the value is final then. Waiting for the tab to close would mean a
    // developer watching their own page never sees the number for the change
    // they just made, which is the whole reason to measure locally.
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("largest-contentful-paint", [candidate(800)]);
    globalThis.window.dispatchEvent(new globalThis.Event("click"));

    expect(valueOf(seen, "LCP")).toBe(800);
  });

  it("reports no paint at all when the browser nominated nothing", () => {
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.hide();

    expect(valueOf(seen, "LCP")).toBe(null);
  });
});

describe("interaction to next paint", () => {
  it("takes the longest event of one interaction rather than their sum", () => {
    // One tap is a pointerdown, a pointerup and a click. Adding them reports
    // three times the latency the reader actually waited, and it does it
    // consistently, so the number looks like a real regression.
    const browser = fakeBrowser({ supports: ["event"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("event", [
      interaction(1, 48, "pointerdown"),
      interaction(1, 120, "pointerup"),
      interaction(1, 90, "click"),
    ]);
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(120);
  });

  it("ignores an event that belongs to no interaction", () => {
    // Scrolling produces event entries with an interactionId of zero, and a
    // page being scrolled produces a great many of them.
    const browser = fakeBrowser({ supports: ["event"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("event", [interaction(0, 900), interaction(7, 60)]);
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(60);
  });

  it("reports the slowest interaction on a page that had only a few", () => {
    const browser = fakeBrowser({ supports: ["event"], interactionCount: 4 });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("event", [
      interaction(1, 100),
      interaction(2, 500),
      interaction(3, 200),
      interaction(4, 300),
    ]);
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(500);
  });

  it("steps down from the slowest once a page has had fifty interactions", () => {
    // On a long session the slowest interaction is a coincidence — a garbage
    // collection, a background tab waking up — and reporting it makes a
    // responsive application look broken in proportion to how much it is
    // used, which is the worst possible direction for the error to point.
    const browser = fakeBrowser({ supports: ["event"], interactionCount: 150 });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("event", [
      interaction(1, 100),
      interaction(2, 200),
      interaction(3, 300),
      interaction(4, 400),
      interaction(5, 500),
    ]);
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(200);
  });

  it("counts the interactions it saw where the browser will not say", () => {
    // `interactionCount` is Chromium's, and it includes the interactions that
    // were too fast to be worth an entry. Falling back to the observed count
    // biases the percentile towards the slowest interaction, which is the
    // honest direction to be wrong in: it never flatters the page.
    const browser = fakeBrowser({ supports: ["event"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("event", [interaction(1, 90), interaction(2, 210)]);
    browser.hide();

    expect(valueOf(seen, "INP")).toBe(210);
  });

  it("asks the browser to skip the events too fast to matter", () => {
    // Without a threshold the observer is handed every event on the page.
    const browser = fakeBrowser({ supports: ["event"] });

    collectVitals({ report: () => {} });

    expect(browser.observeOf("event").durationThreshold).toBe(40);
  });
});

describe("time to first byte", () => {
  it("reads the navigation entry the browser filled in after it was handed over", () => {
    // The entry is delivered once and is live: the browser writes its timings
    // into the same object as the navigation proceeds. A collector that read
    // it only at startup would report nothing on a browser that fills it in
    // late, so the value is taken again when the page is put away.
    const browser = fakeBrowser({ supports: [], responseStart: 0 });
    const { seen, report } = recorder();

    collectVitals({ report });
    expect(valueOf(seen, "TTFB")).toBe(null);

    browser.navigation.responseStart = 240;
    browser.hide();

    expect(valueOf(seen, "TTFB")).toBe(240);
  });

  it("reports for a page nobody ever leaves", async () => {
    // The counterpart: a tab left open forever never fires either of the
    // events that finalise the other metrics, and TTFB is known as soon as the
    // navigation entry has it.
    //
    // `await settle()` and not a synchronous read, because the first attempt is
    // a microtask rather than a line inside `collectVitals` — a collector that
    // reported anything before it could be stopped reported it twice under
    // Strict Mode. What is asserted is unchanged: nothing puts this page away
    // and it still gets its TTFB.
    fakeBrowser({ supports: [], responseStart: 91 });
    const { seen, report } = recorder();

    collectVitals({ report });
    await settle();

    expect(valueOf(seen, "TTFB")).toBe(91);
  });
});

describe("what a number means", () => {
  it("subtracts the prerender from every time it reports", () => {
    // A prerendered page starts its clock before anybody asked for it. Left
    // in, that time makes a page that was ready before the reader clicked
    // look like the slowest page on the site.
    const browser = fakeBrowser({
      supports: ["paint", "largest-contentful-paint"],
      navigationType: "prerender",
      responseStart: 3050,
      activationStart: 3000,
    });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("paint", [paint("first-contentful-paint", 3100)]);
    browser.deliver("largest-contentful-paint", [candidate(3400)]);
    browser.hide();

    expect(valueOf(seen, "FCP")).toBe(100);
    expect(valueOf(seen, "LCP")).toBe(400);
    expect(valueOf(seen, "TTFB")).toBe(50);
  });

  it("reads the activation at the moment it reports, not when the entry arrived", () => {
    // `activationStart` is zero until the prerendered page is activated, and
    // an entry can easily arrive before that. Correcting when the entry lands
    // would use the zero and report the prerender's clock.
    const browser = fakeBrowser({
      supports: ["largest-contentful-paint"],
      navigationType: "prerender",
    });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("largest-contentful-paint", [candidate(2500)]);
    browser.navigation.activationStart = 2000;
    browser.hide();

    expect(valueOf(seen, "LCP")).toBe(500);
  });

  it("never reports a negative time", () => {
    const browser = fakeBrowser({ supports: ["paint"], activationStart: 900 });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("paint", [paint("first-contentful-paint", 100)]);

    expect(valueOf(seen, "FCP")).toBe(0);
  });

  it("says how the page was reached, so a reload is not averaged with a cold start", () => {
    // A reload has a warm cache and a back-forward restore has a warm
    // everything. Pooling the four is how a slow site measures fast.
    const browser = fakeBrowser({ supports: ["paint"], navigationType: "back_forward" });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("paint", [paint("first-contentful-paint", 12)]);

    expect(seen[0].navigationType).toBe("back-forward");
  });

  it("rates a value against the threshold its metric is published with", () => {
    // The rating is on the metric rather than left to the reporter because
    // two reporters that disagreed about where good ends would be a
    // discrepancy nobody could see.
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("largest-contentful-paint", [candidate(2500)]);
    globalThis.window.dispatchEvent(new globalThis.Event("click"));

    expect(seen[0].rating).toBe("good");
  });

  it("rates a value on the wrong side of the second threshold as poor", () => {
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("largest-contentful-paint", [candidate(4001)]);
    globalThis.window.dispatchEvent(new globalThis.Event("click"));

    expect(seen[0].rating).toBe("poor");
  });
});

describe("the collector's lifetime", () => {
  it("asks for buffered entries, so a late start still sees the first paint", () => {
    // The collector is normally started by a component, which commits well
    // after the page painted. Without the replay every metric would really be
    // a measurement of how early the application happened to run this.
    const browser = fakeBrowser({ supports: ["paint"] });

    collectVitals({ report: () => {} });

    expect(browser.observeOf("paint").buffered).toBe(true);
  });

  it("reports each metric once however many times the page is put away", () => {
    // A tab can be hidden, shown and hidden again all day, and both of the
    // events that finalise a metric can fire for one departure.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.2)]);
    browser.hide();
    browser.hide();
    browser.conceal();

    expect(seen.length).toBe(1);
  });

  it("finalises when the tab is hidden, not only when the page unloads", () => {
    // A tab switched away may never unload at all — the reader closes the
    // browser a day later, and by then nothing is running.
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.3)]);
    browser.conceal();

    expect(valueOf(seen, "CLS")).toBeCloseTo(0.3, 5);
  });

  it("does not finalise while the tab is still on screen", () => {
    const browser = fakeBrowser({ supports: ["layout-shift"] });
    const { seen, report } = recorder();

    collectVitals({ report });
    browser.deliver("layout-shift", [shift(0, 0.3)]);
    globalThis.window.document.dispatchEvent(new globalThis.Event("visibilitychange"));

    expect(seen).toEqual([]);
  });

  it("reports nothing at all once it has been stopped", () => {
    // This is what makes the whole thing safe under StrictMode, where an
    // effect is set up, torn down and set up again. Observer callbacks are
    // delivered in a later task, so the first collector is already stopped
    // when its buffered entries would arrive; without this guard every metric
    // of a development build would be reported twice.
    const browser = fakeBrowser({ supports: ["paint"] });
    const { seen, report } = recorder();

    const stop = collectVitals({ report });
    stop();
    browser.deliver("paint", [paint("first-contentful-paint", 400)]);
    browser.hide();

    expect(seen).toEqual([]);
  });

  it("stops listening to the page when it is stopped", () => {
    // A collector whose listeners outlive it keeps the reporter — and
    // whatever the reporter closed over — alive for the life of the document.
    const browser = fakeBrowser({ supports: ["layout-shift"] });

    const stop = collectVitals({ report: () => {} });
    stop();

    expect(() => browser.observeOf("layout-shift")).toThrow();
  });
});

describe("useVitals", () => {
  it("measures from a component that mounted long after the page painted", () => {
    const browser = fakeBrowser({ supports: ["paint"] });
    const { seen, report } = recorder();

    component Root() {
      useVitals(report);
      return <span />;
    }
    render(<Root />);
    browser.deliver("paint", [paint("first-contentful-paint", 640)]);

    expect(valueOf(seen, "FCP")).toBe(640);
  });

  it("does not restart the collector when the page renders again", () => {
    // The natural way to call this is with an inline arrow, which is a new
    // function every render. A collector rebuilt on each one would replay the
    // buffered entries and report every metric again.
    const browser = fakeBrowser({ supports: ["paint"] });
    const { seen, report } = recorder();

    component Root(n: number) {
      useVitals((vital) => report(vital));
      return <span>{n}</span>;
    }
    const view = render(<Root n={1} />);
    const before = browser.constructed();
    view.rerender(<Root n={2} />);
    browser.deliver("paint", [paint("first-contentful-paint", 100)]);

    expect(browser.constructed()).toBe(before);
    expect(seen.length).toBe(1);
  });

  it("reports each metric once under Strict Mode", async () => {
    // The case ubugeeei-prod/uf#516 created. `uf dev` now hydrates inside
    // `<StrictMode>`, so every effect is set up, torn down and set up again —
    // and `collectVitals` is called twice against the same page, with the
    // second collector replaying the same buffered entries.
    //
    // Everything the observers deliver was already safe, because their
    // callbacks arrive in a later task and the first collector is stopped by
    // then. `tryTtfb` was not: it read the navigation entry synchronously
    // inside `collectVitals`, before the collector could be stopped, so the
    // first collector reported TTFB and the second reported it again. With
    // #602's `/__uf/vitals` that is two web-vitals reports in the terminal for
    // one page load, on every project, from the moment Strict Mode went on.
    const browser = fakeBrowser({ supports: ["paint"], responseStart: 91 });
    const { seen, report } = recorder();

    component Root() {
      useVitals(report);
      return <span />;
    }
    render(
      <StrictMode>
        <Root />
      </StrictMode>,
    );
    await settle();
    browser.deliver("paint", [paint("first-contentful-paint", 640)]);

    expect(seen.map((vital) => vital.name)).toEqual(["TTFB", "FCP"]);
  });

  it("uses the reporter the latest render supplied", () => {
    // The ref exists so the collector survives a re-render, and a ref that
    // was never updated would keep calling the closure from the first one.
    const browser = fakeBrowser({ supports: ["paint"] });
    const first = recorder();
    const second = recorder();

    component Root(to: (Vital) => void) {
      useVitals(to);
      return <span />;
    }
    const view = render(<Root to={first.report} />);
    view.rerender(<Root to={second.report} />);
    browser.deliver("paint", [paint("first-contentful-paint", 55)]);

    expect(first.seen).toEqual([]);
    expect(valueOf(second.seen, "FCP")).toBe(55);
  });
});

describe("vitalsBeacon", () => {
  /** Install a `sendBeacon` that records rather than sends. */
  function watchBeacon(accept: boolean): Array<{ url: string, body: string }> {
    const sent: Array<{ url: string, body: string }> = [];
    replace(globalThis.window.navigator, "sendBeacon", (url: string, body: string) => {
      sent.push({ url, body });
      return accept;
    });
    return sent;
  }

  it("sends nothing until a metric is final", async () => {
    // Importing this module and constructing a reporter must not itself be a
    // request. Nothing leaves the machine that the page did not measure.
    fakeBrowser({ supports: ["paint"] });
    const sent = watchBeacon(true);

    vitalsBeacon();
    await settle();

    expect(sent).toEqual([]);
  });

  it("posts the metrics that became final together as one report", async () => {
    const browser = fakeBrowser({ supports: ["layout-shift", "largest-contentful-paint"] });
    const sent = watchBeacon(true);

    collectVitals({ report: vitalsBeacon() });
    browser.deliver("largest-contentful-paint", [candidate(700)]);
    browser.deliver("layout-shift", [shift(0, 0.02)]);
    browser.hide();
    await settle();

    expect(sent.length).toBe(1);
    const report = JSON.parse(sent[0].body);
    expect(report.vitals.map((vital: $FlowFixMe) => vital.name)).toEqual(["LCP", "CLS"]);
  });

  it("names the page rather than the report", async () => {
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const sent = watchBeacon(true);

    collectVitals({ report: vitalsBeacon() });
    browser.deliver("largest-contentful-paint", [candidate(700)]);
    browser.hide();
    await settle();

    expect(JSON.parse(sent[0].body).url).toBe(globalThis.window.location.href);
  });

  it("posts to a path on the page's own origin by default", async () => {
    // A path rather than a URL, so this cannot be pointed at somebody else's
    // server by accident, and the destination is whatever the project chose
    // to serve — never uf, and never a third party.
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const sent = watchBeacon(true);

    collectVitals({ report: vitalsBeacon() });
    browser.deliver("largest-contentful-paint", [candidate(1)]);
    browser.hide();
    await settle();

    expect(VITALS_ENDPOINT.startsWith("/")).toBe(true);
    expect(sent[0].url).toBe(VITALS_ENDPOINT);
  });

  it("sends where the project asked instead", async () => {
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    const sent = watchBeacon(true);

    collectVitals({ report: vitalsBeacon("/telemetry") });
    browser.deliver("largest-contentful-paint", [candidate(1)]);
    browser.hide();
    await settle();

    expect(sent[0].url).toBe("/telemetry");
  });

  it("falls back to a keepalive post when the beacon queue refuses the report", async () => {
    // `sendBeacon` answers false when the payload will not fit in the queue,
    // and a report that was dropped after looking sent is worse than one that
    // was never sent: the gap in the data looks like a page nobody visited.
    const browser = fakeBrowser({ supports: ["largest-contentful-paint"] });
    watchBeacon(false);
    const posted: Array<$FlowFixMe> = [];
    replace(globalThis.window, "fetch", (url: string, init: $FlowFixMe) => {
      posted.push({ url, init });
      return Promise.resolve();
    });

    collectVitals({ report: vitalsBeacon() });
    browser.deliver("largest-contentful-paint", [candidate(1)]);
    browser.hide();
    await settle();

    expect(posted.length).toBe(1);
    expect(posted[0].init.keepalive).toBe(true);
    expect(posted[0].init.method).toBe("POST");
  });

  it("sends nothing from a server render", async () => {
    // A prerender has a reporter and no network to put it on. Throwing here
    // would fail the build of every static route in the project.
    theWindow();
    const sent = watchBeacon(true);
    replace(globalThis, "document", undefined);

    const send = vitalsBeacon();
    send({ name: "LCP", value: 1, rating: "good", navigationType: "navigate" });
    await settle();

    expect(sent).toEqual([]);
  });
});
