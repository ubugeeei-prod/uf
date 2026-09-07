// @flow
//
// `@uniflowed/web/vitals`: the five numbers a page is judged on, and where
// they go.
//
// LCP, CLS, INP, TTFB and FCP are the measurements a real page is held to, and
// none of them can be taken from outside: they are facts about one reader's
// browser on one reader's connection, and a synthetic run in a data centre is
// a different page. The browser already computes all five and hands them over
// through `PerformanceObserver`. This module reads them, decides when each one
// is final, and hands each to a callback the application supplied.
//
// # Nothing leaves the machine
//
// `collectVitals` takes a reporter and calls it. It opens no connection, and
// importing this module does nothing at all — there is no top-level side
// effect for a bundler to keep or a page to pay for. Sending the numbers
// somewhere is `vitalsBeacon`, which the application has to construct, and
// which posts to a path on the page's own origin. There is no uf endpoint, no
// third party, and no default destination, because a toolchain that phones
// home by default is a toolchain nobody can audit once.
//
// # Reading the browser rather than shipping a copy of a library
//
// `web-vitals` is very good and this is not a fork of it. It is a direct read
// of the same entry types, which is a different thing to maintain: what it
// costs is the handful of corrections that make a raw entry stream into a
// number anybody quotes — CLS is a windowed maximum rather than a sum, INP is
// a high percentile of interactions rather than the slowest one — and those
// are implemented here rather than approximated. Where uf still differs from
// `web-vitals` it is written down in `docs/app/guide/vitals`, because a
// performance number whose definition is unstated is a number two people will
// read differently.
//
// # The one check that decides whether any of this runs
//
// `PerformanceObserver.supportedEntryTypes`, per metric, and never
// `typeof PerformanceObserver`. Node has had a `PerformanceObserver` for years
// and it supports `mark`, `measure`, `resource` and a set of things about
// sockets — none of the five entry types here. So the obvious guard passes on
// a server and then observes nothing, which is the worst of both: code that
// runs where it cannot work and reports numbers nobody asked for.
//
// Asking the constructor which types it serves answers three questions with
// one expression. A server has no browser entry types. A browser too old for
// `layout-shift` has no `layout-shift`. And a test host that borrowed Node's
// constructor is, correctly, both.
//
// # A metric the browser cannot measure is absent, not zero
//
// This is the rule the rest of the module is arranged around. A browser
// without `largest-contentful-paint` does not get an LCP of 0; it gets no LCP.
// Zero is the best possible score, and a collector that reports it for
// "unknown" produces a dashboard whose worst pages look like its best ones,
// which is a mistake that survives for months because it looks like good news.
//
// The distinction has a second edge worth being precise about: CLS *is*
// reported as zero when the browser supports `layout-shift` and no layout
// shifted, because that is a measurement rather than an absence. INP is not
// reported at all when nobody interacted, because there is no interaction to
// have been slow.
//
// # What belongs in this module
//
// A measurement the browser takes of the whole page, whose value is not known
// until the page has been used, and which is judged against a published
// threshold. That is what makes these five one subject rather than five.
//
// Not here: a timing an application takes of itself. `performance.mark` and
// `performance.measure` are the browser's API for that, they need no help, and
// wrapping them would be a second name for something that already has one.
// Not here either: the elements that cause these numbers. An image that
// reserves its space and a font that does not swap late are `media.js`'s, and
// that is the file to change when a number here is bad.

import * as React from "@uniflowed/react";

/** The metrics this module reports. */
export type VitalName = "TTFB" | "FCP" | "LCP" | "CLS" | "INP";

/** Where a value falls against the thresholds the metric is published with. */
export type Rating = "good" | "needs-improvement" | "poor";

/**
 * How the page was reached.
 *
 * Carried on every metric because it changes what the metric means: a reload
 * has a warm cache, a back-forward restore has a warm everything, and a
 * prerender was fetched before anybody asked for it. Averaging the four
 * together is how a fast site measures slow.
 */
export type NavigationType = "navigate" | "reload" | "back-forward" | "prerender" | "unknown";

/** One measurement, final. */
export type Vital = {
  readonly name: VitalName,
  /** Milliseconds, except `CLS`, which is a unitless layout-shift score. */
  readonly value: number,
  readonly rating: Rating,
  readonly navigationType: NavigationType,
};

/** What a collector hands each finished measurement to. */
export type VitalsReporter = (vital: Vital) => void;

/**
 * The body posted to [`VITALS_ENDPOINT`].
 *
 * A receiver gets more than one of these per page load — see `vitalsBeacon` —
 * so the arrays are additive rather than a complete picture, and `url`
 * identifies the page rather than the report.
 */
export type VitalsReport = {
  readonly url: string,
  readonly vitals: $ReadOnlyArray<Vital>,
};

/** How to collect. */
export type CollectOptions = {
  readonly report: VitalsReporter,
};

/**
 * The path `vitalsBeacon` posts to unless the project names another.
 *
 * Under `/__uf/` beside the update stream `@uniflowed/hmr` opens, for the same
 * reason: it is a namespace the dev server owns and an application cannot
 * collide with — a directory in `app/` whose name begins with `_` is not a
 * route, so this path is unreachable by anything a project writes.
 *
 * Which is also why it is only half of the answer. It is where a page being
 * developed reports to, and in production the project passes a path of its own
 * to `vitalsBeacon` and serves it from an ordinary route handler. The contract
 * that makes both work is a path and a [`VitalsReport`] body, rather than an
 * integration with anybody.
 */
export const VITALS_ENDPOINT: string = "/__uf/vitals";

/**
 * The published boundaries, in the metric's own units.
 *
 * At or below `good` is good; above `poor` is poor; between them is the middle
 * band. Written here rather than left to the reporter because a number without
 * its threshold is not yet an answer, and because two reporters that disagreed
 * about where "good" ends would be a bug nobody could see.
 */
const THRESHOLDS: { readonly [VitalName]: { readonly good: number, readonly poor: number } } = {
  TTFB: { good: 800, poor: 1800 },
  FCP: { good: 1800, poor: 3000 },
  LCP: { good: 2500, poor: 4000 },
  CLS: { good: 0.1, poor: 0.25 },
  INP: { good: 200, poor: 500 },
};

/**
 * How long a run of layout shifts stays one run.
 *
 * A gap of a second ends the run and a run may not exceed five seconds. Both
 * numbers are the metric's definition rather than a tuning choice: CLS is the
 * worst such window, not the total, so that a long-lived page is not punished
 * for the sum of a hundred separate small shifts.
 */
const SHIFT_GAP: number = 1000;
const SHIFT_SPAN: number = 5000;

/**
 * The shortest interaction worth an entry.
 *
 * The Event Timing API will report every event if asked, which on a page being
 * scrolled is thousands of them. Forty milliseconds is below the threshold any
 * reader perceives and is the same figure `web-vitals` uses, so the two agree
 * about which interactions exist before they disagree about anything else.
 */
const EVENT_THRESHOLD: number = 40;

/**
 * How the slowest interaction is discounted.
 *
 * INP is not the slowest interaction: on a page with hundreds of them, the
 * slowest is a coincidence rather than a description. The metric keeps the ten
 * longest and steps one place down the list for every fifty interactions, so a
 * page has to be reliably slow rather than once unlucky. Below fifty
 * interactions this is exactly the maximum, which is why a short session and a
 * long one can be compared at all.
 */
const INTERACTIONS_KEPT: number = 10;
const INTERACTIONS_PER_STEP: number = 50;

/** The part of a performance entry this module reads. */
type TimingEntry = {
  readonly entryType: string,
  readonly name: string,
  readonly startTime: number,
  readonly duration: number,
  /** `layout-shift`: how much moved. */
  readonly value?: number,
  /** `layout-shift`: whether a reader had just done something. */
  readonly hadRecentInput?: boolean,
  /** `event`: which interaction this event belongs to, or 0 for none. */
  readonly interactionId?: number,
  ...
};

/** The part of a navigation entry this module reads. */
type NavigationEntry = {
  readonly type?: string,
  /** Milliseconds from the start of the document to the first response byte. */
  readonly responseStart?: number,
  /** Non-zero only on a prerendered page: when the reader actually arrived. */
  readonly activationStart?: number,
  ...
};

/** What a `PerformanceObserver` callback is handed. */
type EntryList = { readonly getEntries: () => $ReadOnlyArray<TimingEntry>, ... };

/** The part of a live observer this module uses. */
type ObserverHandle = {
  readonly observe: (options: { readonly [string]: mixed }) => mixed,
  readonly disconnect: () => mixed,
  ...
};

/**
 * Anything this module attaches a listener to.
 *
 * The window and the document both, because the two events that mean "this
 * page is over" are not dispatched on the same object.
 */
type Listens = {
  readonly addEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
  readonly removeEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
  ...
};

/**
 * The part of a `window` this module reads.
 *
 * The same idea as `@uniflowed/hooks`' `BrowserWindow`, and deliberately a
 * second and much smaller copy: importing that package here would put every
 * hook in it into the graph of any page that measures itself. Naming what is
 * touched is what lets everything below `browserWindow()` be checked.
 *
 * `PerformanceObserver` is typed as the object it is read as rather than as a
 * constructor, because Flow has no way to write "a property that is a class"
 * for something arriving off an untyped global. `observeEntries` is where that
 * is resolved, once.
 */
type BrowserWindow = {
  readonly document: {
    readonly visibilityState: string,
    readonly addEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
    readonly removeEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
    ...
  },
  readonly location?: ?{ readonly href: string, ... },
  readonly performance?: ?{
    readonly getEntriesByType?: (type: string) => $ReadOnlyArray<NavigationEntry>,
    /** Chromium only: every interaction, including the ones too fast to observe. */
    readonly interactionCount?: number,
    ...
  },
  readonly navigator?: ?{
    readonly sendBeacon?: (url: string, body: string) => boolean,
    ...
  },
  readonly fetch?: ?(url: string, init: { readonly [string]: mixed }) => Promise<mixed>,
  readonly PerformanceObserver?: ?{
    readonly supportedEntryTypes?: $ReadOnlyArray<string>,
    ...
  },
  readonly addEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
  readonly removeEventListener: (type: string, listener: () => mixed, options?: mixed) => mixed,
  ...
};

/** A stop function for a collector that never started. */
function noop(): void {}

/**
 * The window this module measures, or `null` where there is no browser.
 *
 * In a browser `globalThis` *is* the window. It is not anywhere a document has
 * been installed onto another host's global, which is every uf test process,
 * so asking the document's own window is what makes both cases work. The
 * single `?? globalThis` is this module's only unchecked step: `window` is
 * `any` in Flow's library definition, and this is where that stops.
 */
function browserWindow(): BrowserWindow | null {
  if (typeof globalThis.document === "undefined") {
    return null;
  }
  return globalThis.window ?? globalThis;
}

/** Whether the browser will actually serve entries of this type. */
function supports(win: BrowserWindow, type: string): boolean {
  const types = win.PerformanceObserver?.supportedEntryTypes;
  return types != null && types.includes(type);
}

/**
 * Start one observer, or return `null` where the browser cannot serve it.
 *
 * `buffered: true` on every one of them is what makes a late start correct:
 * the browser replays the entries it recorded before this call, so a collector
 * mounted by a React root that has already painted still sees its own first
 * paint. Without it, every metric would depend on how early the application
 * happened to run this, which is a measurement of the bundler.
 *
 * The `new` here is the module's one cast. Flow cannot express "an object
 * property that is a constructor" for a value read off an untyped global, and
 * doing it in one function rather than at each call site keeps everything the
 * observers hand back checked.
 */
function observeEntries(
  win: BrowserWindow,
  type: string,
  onEntries: (entries: $ReadOnlyArray<TimingEntry>) => void,
  extra?: { readonly [string]: mixed },
): ObserverHandle | null {
  const Observer = win.PerformanceObserver;
  if (Observer == null || !supports(win, type)) {
    return null;
  }
  const observer: ObserverHandle = new (Observer as $FlowFixMe)((list: EntryList) => {
    onEntries(list.getEntries());
  });
  try {
    observer.observe({ ...extra, type, buffered: true });
  } catch {
    // `supportedEntryTypes` said yes and `observe` disagreed, which happens
    // for a type the browser knows the name of and will not serve in this
    // context. Reporting nothing is the same answer as not supporting it, and
    // an exception thrown out of a collector would take the application's
    // startup with it.
    return null;
  }
  return observer;
}

/**
 * The document's navigation entry, or `null`.
 *
 * Read through `getEntriesByType` rather than observed, because the entry
 * exists from the moment the document does and an observer would put it behind
 * a callback that has to be ordered against the others. It is also a *live*
 * object: the browser fills its fields in as the navigation proceeds, so
 * holding it and reading a field later is not the same as reading that field
 * now. Both `responseStart` and `activationStart` depend on that.
 */
function navigationEntry(win: BrowserWindow): NavigationEntry | null {
  const entries = win.performance?.getEntriesByType?.("navigation");
  return entries == null || entries.length === 0 ? null : entries[0];
}

/** What the navigation entry calls it, in this module's spelling. */
function navigationTypeOf(entry: NavigationEntry | null): NavigationType {
  switch (entry?.type) {
    case "navigate":
      return "navigate";
    case "reload":
      return "reload";
    case "back_forward":
      return "back-forward";
    case "prerender":
      return "prerender";
    default:
      return "unknown";
  }
}

/** Where `value` falls against the metric's published thresholds. */
function rate(name: VitalName, value: number): Rating {
  const threshold = THRESHOLDS[name];
  if (value <= threshold.good) {
    return "good";
  }
  if (value <= threshold.poor) {
    return "needs-improvement";
  }
  return "poor";
}

/** Longest first. */
function longestFirst(a: number, b: number): number {
  return b - a;
}

/**
 * Measure this page and report each metric once, as it becomes final.
 *
 * Returns a function that stops collecting. Calling it does not report
 * anything: stopping is not the page ending, and a collector that flushed on
 * teardown would report a half-measured page every time a React root remounted.
 *
 * That is also what makes this safe under `StrictMode`, where an effect is set
 * up, torn down and set up again. The first collector is stopped before any
 * observer callback can run — they are delivered in a later task — so it
 * reports nothing at all and the second collector, replaying the same buffered
 * entries, reports each metric exactly once.
 *
 * On a server this observes nothing and returns immediately, so a component
 * that renders in both places can call it unconditionally.
 */
export function collectVitals(options: CollectOptions): () => void {
  const win = browserWindow();
  if (win == null) {
    return noop;
  }

  const report = options.report;
  const teardown: Array<() => void> = [];
  const reported: Set<VitalName> = new Set();
  const navigation = navigationEntry(win);
  const navigationType = navigationTypeOf(navigation);
  let stopped = false;

  /**
   * A prerendered page starts its clock before anybody asked for it, so every
   * timestamp on it is that far ahead of what the reader experienced. Applied
   * when the metric is reported rather than when the entry arrives, because
   * `activationStart` is zero until the page is activated and the entry may
   * well have arrived before that.
   */
  const sinceArrival = (time: number): number =>
    Math.max(time - (navigation?.activationStart ?? 0), 0);

  const emit = (name: VitalName, value: number): void => {
    if (stopped || reported.has(name)) {
      return;
    }
    reported.add(name);
    report({ name, value, rating: rate(name, value), navigationType });
  };

  // Attempted twice: once now, and once when the page is put away. The
  // navigation entry exists from the start and its timings can still be zero
  // when it is first read, so a collector that only looked now would report
  // nothing on a browser that fills the entry in later — and one that only
  // looked at the end would report nothing for a page nobody ever leaves.
  const tryTtfb = (): void => {
    const responseStart = navigation?.responseStart ?? 0;
    if (responseStart > 0) {
      emit("TTFB", sinceArrival(responseStart));
    }
  };

  const paint = observeEntries(win, "paint", (entries) => {
    for (const entry of entries) {
      if (entry.name === "first-contentful-paint") {
        emit("FCP", sinceArrival(entry.startTime));
      }
    }
  });

  // The raw timestamp rather than the corrected one, for the same reason
  // `sinceArrival` is applied late.
  let lcpAt = -1;
  const lcp = observeEntries(win, "largest-contentful-paint", (entries) => {
    for (const entry of entries) {
      lcpAt = entry.startTime;
    }
  });
  const finishLcp = (): void => {
    if (lcpAt >= 0) {
      emit("LCP", sinceArrival(lcpAt));
    }
  };

  let shifted = 0;
  let worstRun = 0;
  let runStart = 0;
  let runLast = 0;
  const cls = observeEntries(win, "layout-shift", (entries) => {
    for (const entry of entries) {
      // A shift the reader caused by clicking something is not a shift the
      // reader suffered, and the browser is the only thing that knows which
      // is which.
      if (entry.hadRecentInput === true) {
        continue;
      }
      const value = entry.value ?? 0;
      const continues =
        shifted !== 0 &&
        entry.startTime - runLast < SHIFT_GAP &&
        entry.startTime - runStart < SHIFT_SPAN;
      if (continues) {
        shifted += value;
      } else {
        shifted = value;
        runStart = entry.startTime;
      }
      runLast = entry.startTime;
      if (shifted > worstRun) {
        worstRun = shifted;
      }
    }
  });

  // Keyed by interaction rather than by event: one tap is a `pointerdown`, a
  // `pointerup` and a `click`, and the interaction's latency is the longest of
  // them rather than their sum.
  const interactions: Map<number, number> = new Map();
  const inp = observeEntries(
    win,
    "event",
    (entries) => {
      for (const entry of entries) {
        const id = entry.interactionId ?? 0;
        if (id === 0) {
          continue;
        }
        if (entry.duration > (interactions.get(id) ?? 0)) {
          interactions.set(id, entry.duration);
        }
      }
    },
    { durationThreshold: EVENT_THRESHOLD },
  );
  const finishInp = (): void => {
    if (interactions.size === 0) {
      return;
    }
    const longest = [...interactions.values()].sort(longestFirst).slice(0, INTERACTIONS_KEPT);
    // `interactionCount` includes the interactions that were too fast to be
    // observed at all, which is what makes this a percentile of a page rather
    // than of a sample. Where the browser does not have it, the observed
    // count is the best available and biases the answer towards the slowest
    // interaction — which is the honest direction to be wrong in.
    const count = win.performance?.interactionCount ?? interactions.size;
    const step = Math.floor(count / INTERACTIONS_PER_STEP);
    emit("INP", longest[Math.min(longest.length - 1, step)]);
  };

  const finish = (): void => {
    tryTtfb();
    finishLcp();
    // Zero rather than nothing, and only when the browser served the entry
    // type: a page whose layout never moved has a CLS, and it is zero.
    if (cls != null) {
      emit("CLS", worstRun);
    }
    finishInp();
  };

  const onVisibilityChange = (): void => {
    if (win.document.visibilityState === "hidden") {
      finish();
    }
  };

  const listen = (target: Listens, type: string, listener: () => mixed, options?: mixed): void => {
    target.addEventListener(type, listener, options);
    teardown.push(() => {
      target.removeEventListener(type, listener, options);
    });
  };

  // Both, because they are not the same event in every browser: a tab switched
  // away fires `visibilitychange` and never unloads, and a page replaced by a
  // navigation has historically been the one that only fires `pagehide`.
  listen(win.document, "visibilitychange", onVisibilityChange);
  listen(win, "pagehide", finish);

  // The browser stops nominating largest-contentful-paint candidates once the
  // reader has interacted, so the value is final at that moment rather than at
  // the end of the page. Reporting it then is what makes LCP visible to
  // somebody watching their own page instead of only to whoever reads the logs.
  listen(win, "keydown", finishLcp, { once: true, capture: true });
  listen(win, "click", finishLcp, { once: true, capture: true });

  for (const observer of [paint, lcp, cls, inp]) {
    if (observer != null) {
      teardown.push(() => {
        observer.disconnect();
      });
    }
  }

  tryTtfb();

  return () => {
    if (stopped) {
      return;
    }
    stopped = true;
    for (const off of teardown) {
      off();
    }
  };
}

/**
 * Measure this page for as long as the component is mounted.
 *
 * Belongs in a root layout, which is late — the page has usually painted by
 * the time React commits. That is correct rather than tolerated: every
 * observer asks for buffered entries, so the browser replays what it recorded
 * before this ran, and the first paint of a server-rendered page is measured
 * by a component that did not exist when it happened.
 *
 * `report` is read through a ref so that an inline arrow does not tear the
 * collector down and build a new one on every render. Restarting is not
 * harmless: the replayed entries would be reported a second time.
 */
export function useVitals(report: VitalsReporter): void {
  const latest = React.useRef<VitalsReporter>(report);

  React.useEffect(() => {
    latest.current = report;
  }, [report]);

  React.useEffect(() => {
    return collectVitals({
      report: (vital) => {
        latest.current(vital);
      },
    });
  }, []);
}

/**
 * A reporter that posts to `endpoint` on the page's own origin.
 *
 * Defaults to [`VITALS_ENDPOINT`]. Nothing is sent until a metric is final, no
 * connection is opened before that, and the destination is a path rather than
 * a URL, so this cannot be pointed at somebody else's server by accident.
 *
 * # Why more than one request
 *
 * Metrics become final at different moments — TTFB and FCP early, LCP, CLS and
 * INP when the page is put away — and the sends are coalesced across a
 * microtask rather than held until the end. So a receiver normally sees two
 * reports and must treat them as additive.
 *
 * Holding everything until the page is hidden would have been one request, and
 * it would have made the whole thing depend on which listener the browser
 * called first: the collector's, which produces the last three metrics, or
 * this one's, which sends them. That ordering is not something either side can
 * state, so it is not something the design relies on. It also costs nothing to
 * give up — a receiver that prints a number as soon as it is known is more
 * useful than one that waits for a tab to close.
 */
export function vitalsBeacon(endpoint?: string): VitalsReporter {
  const win = browserWindow();
  if (win == null) {
    return noop;
  }

  const target = endpoint ?? VITALS_ENDPOINT;
  const pending: Array<Vital> = [];
  let scheduled = false;

  const flush = (): void => {
    scheduled = false;
    if (pending.length === 0) {
      return;
    }
    const body = JSON.stringify({ url: win.location?.href ?? "", vitals: pending });
    pending.length = 0;

    const navigator = win.navigator;
    const sendBeacon = navigator?.sendBeacon;
    // `sendBeacon` is the only transport a browser promises to finish after
    // the page is gone, so it is tried first and its return value is read: it
    // answers `false` when the payload will not fit in the queue, and a
    // dropped report that looked sent is worse than one that was never sent.
    if (navigator != null && sendBeacon != null && sendBeacon.call(navigator, target, body)) {
      return;
    }
    const post = win.fetch;
    if (post != null) {
      // A page being put away can lose the network mid-request, and a rejected
      // promise nobody is holding becomes an unhandled rejection — which
      // arrives in whatever error reporter the application installed, from a
      // line about telemetry. Losing the report is the right outcome here;
      // reporting the loss as an application error is not.
      post(target, {
        method: "POST",
        body,
        keepalive: true,
        headers: { "content-type": "application/json" },
      }).catch(noop);
    }
  };

  return (vital: Vital): void => {
    pending.push(vital);
    if (scheduled) {
      return;
    }
    scheduled = true;
    queueMicrotask(flush);
  };
}
