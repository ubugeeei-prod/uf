// @flow
//
// `@uniflowed/hooks`, in a process that has no DOM at all.
//
// This is a separate file from `hooks.test.js` and the separation is the
// point, not a filing preference. `@uniflowed/react-testing` installs a
// document onto the global the first time anything renders, and it installs it
// once per process and never removes it — so a "there is no `window`" test
// living beside a `render` would be testing a process that has a `window`, and
// would pass for a package that read `document` at module scope.
//
// Nothing here imports `@uniflowed/react-testing`. But a worker runs one file
// after another, so a file that did may well have run first in this process —
// and then the document is already there. This file takes it away for its own
// length and puts back exactly what it found, so what it asserts is true
// however the runner scheduled it. Putting it back matters as much as taking
// it away: `installDom` installs once per process and returns early ever
// after, so a file that ran later would find no document and no way to get
// one.
//
// What is being checked is the claim `index.js` makes: uf prerenders every
// static route, so every hook in this package runs once where there is no
// browser. A hook that touched `window` at module scope or during its first
// render would throw here.

import { createRequire } from "node:module";

import * as React from "@uniflowed/react";
import { afterAll, beforeAll, describe, expect, it } from "@uniflowed/test";
import {
  RenderProvider,
  browserWindow,
  useAnimationFrame,
  useAsync,
  useBroadcast,
  useClickOutside,
  useClipboard,
  useCounter,
  useCycle,
  useDebouncedCallback,
  useDebouncedValue,
  useDocumentVisible,
  useElementRef,
  useElementSize,
  useElementState,
  useEventListener,
  useEventSource,
  useFocusWithin,
  useGeolocation,
  useHash,
  useHover,
  useIdle,
  useIntersecting,
  useInterval,
  useIsomorphicLayoutEffect,
  useKeyCombo,
  useKeyHeld,
  useList,
  useLongPress,
  useMediaQuery,
  useMount,
  useMounted,
  useMutationObserver,
  useNetwork,
  useNow,
  useOnline,
  usePermission,
  usePreferredColorScheme,
  usePrefersReducedMotion,
  usePrevious,
  useRandom,
  useRenderTimeZone,
  useRenderedAt,
  useRerender,
  useScroll,
  useScrollLock,
  useSet,
  useShuffled,
  useStableCallback,
  useStorage,
  useSupported,
  useThrottledCallback,
  useTimeAgo,
  useTimeout,
  useToggle,
  useUndoable,
  useUnmount,
  useWindowScroll,
  useWindowSize,
} from "@uniflowed/hooks";

// Loaded the way `@uniflowed/react-testing` loads `react-dom/client`: through a
// synchronous require, so importing this module costs nothing until a test
// renders. `react-dom/server` reads no DOM, which is the whole reason it is
// the renderer this file uses.
const server = createRequire(import.meta.url)("react-dom/server");

const markupOf = (element: React.Node): string => String(server.renderToStaticMarkup(element));

/**
 * The globals a document installs, taken away and put back.
 *
 * `delete` rather than assigning `undefined`, because `typeof` is what the
 * hooks ask and `undefined` assigned to a defined property still answers
 * `"object"` for `document` on some hosts. What was there is restored exactly,
 * so a file that runs after this one in the same worker finds what it left.
 */
const DOM_GLOBALS = ["document", "window", "navigator", "localStorage", "sessionStorage"];
const removed: { [string]: mixed } = {};

beforeAll(() => {
  for (const name of DOM_GLOBALS) {
    if (name in globalThis) {
      removed[name] = globalThis[name];
      delete globalThis[name];
    }
  }
});

afterAll(() => {
  for (const name of Object.keys(removed)) {
    Object.defineProperty(globalThis, name, {
      value: removed[name],
      writable: true,
      configurable: true,
      enumerable: true,
    });
  }
});

describe("the process these tests run in", () => {
  it("has no document, so a hook that needed one would throw here", () => {
    expect(typeof globalThis.document).toBe("undefined");
    expect(typeof globalThis.window).toBe("undefined");
  });

  it("reports no window from browserWindow, which is what every hook asks first", () => {
    expect(browserWindow()).toBe(null);
  });
});

describe("the environment hooks, prerendered", () => {
  it("uses the stated server value rather than guessing", () => {
    component Probe() {
      const wide = useMediaQuery("(min-width: 48rem)", true);
      const narrow = useMediaQuery("(min-width: 48rem)");
      const scheme = usePreferredColorScheme("dark");
      const still = usePrefersReducedMotion(true);
      const online = useOnline();
      const visible = useDocumentVisible();
      return (
        <ul>
          <li>{String(wide)}</li>
          <li>{String(narrow)}</li>
          <li>{scheme}</li>
          <li>{String(still)}</li>
          <li>{String(online)}</li>
          <li>{String(visible)}</li>
        </ul>
      );
    }
    // Two calls with the same query and different server values give different
    // answers, which is the proof that the value is stated rather than probed.
    expect(markupOf(<Probe />)).toBe(
      "<ul><li>true</li><li>false</li><li>dark</li><li>true</li><li>true</li><li>true</li></ul>",
    );
  });

  it("reports the stated size and scroll offset", () => {
    component Probe() {
      const size = useWindowSize({ width: 1280, height: 800 });
      const offset = useWindowScroll();
      return <output>{`${size.width}x${size.height}@${offset.x},${offset.y}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>1280x800@0,0</output>");
  });

  it("says nothing is supported, which is what hydration has to agree with", () => {
    component Probe() {
      // The probe would answer `true` in this process — `Date` is always
      // there — and the server snapshot is what wins, because the client's
      // hydrating render has to produce the same markup.
      const supported = useSupported(() => true);
      const clipboard = useClipboard();
      const channel = useBroadcast("uf-ssr", () => {});
      const network = useNetwork();
      const where = useGeolocation();
      const idle = useGeolocation({ enabled: false });
      const permission = usePermission("geolocation");
      return (
        <output>
          {[
            String(supported),
            String(clipboard.supported),
            String(channel.supported),
            String(network.measured),
            String(network.online),
            where.status,
            idle.status,
            permission,
          ].join(" ")}
        </output>
      );
    }
    // `measured: null` and `status: "unsupported"` are the shapes that replaced
    // a `supported` boolean beside fields that meant nothing without it: a
    // prerender cannot measure a connection or find anybody, and the type is
    // where it says so.
    expect(markupOf(<Probe />)).toBe(
      "<output>false false false null true unsupported idle unknown</output>",
    );
  });

  /**
   * The fragment is the one part of the URL a server has never seen.
   *
   * A browser strips everything after `#` before the request goes out, so there
   * is no value for a prerender to know and no caller who could supply a better
   * one — which is why `useHash` takes no server value where `useMediaQuery`
   * and `useOnline` do. A version of this hook that read `location.hash`
   * directly would throw here rather than render.
   */
  it("renders an empty fragment, because a request never carried one", () => {
    component Probe() {
      const [fragment, write] = useHash();
      const [, again] = useHash();
      // One identity for the writer, from two calls in one render: it is a
      // module-level function rather than a closure, so nothing has to memoize
      // it and a caller can put it in a dependency array.
      return <output>{`[${fragment}] ${String(write === again)}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>[] true</output>");
  });

  it("opens no event stream, and says nothing has been attempted", () => {
    // `idle` rather than `connecting`, because they are different facts and
    // only one of them is true here: a prerender neither connects nor is about
    // to. A hook that reported `connecting` would put a spinner in every
    // prerendered document that uses one.
    component Probe() {
      const stream = useEventSource("/api/feed");
      return (
        <output>{[stream.status, String(stream.supported), String(stream.last)].join(" ")}</output>
      );
    }
    expect(markupOf(<Probe />)).toBe("<output>idle false null</output>");
  });

  it("leaves the page alone when a dialog asks for the scroll to be locked", () => {
    component Probe() {
      useScrollLock(true);
      return <p>dialog</p>;
    }
    // The lock is a layout effect, and effects do not run during a prerender.
    // The assertion is that rendering one produces plain markup and does not
    // reach for a `document.body` that is not there.
    expect(markupOf(<Probe />)).toBe("<p>dialog</p>");
  });
});

describe("the element hooks, prerendered", () => {
  it("renders every one of them from a ref that is null", () => {
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      const [node, attach] = useElementState<HTMLSpanElement>();
      const size = useElementSize(ref);
      const offset = useScroll(ref);
      const hovered = useHover(ref);
      const focused = useFocusWithin(ref);
      const seen = useIntersecting(ref);
      useClickOutside(ref, () => {});
      useLongPress(ref, () => {});
      useMutationObserver(ref, () => {});
      useEventListener(ref, "click", () => {});
      return (
        <div ref={ref}>
          <span ref={attach}>{String(node)}</span>
          <output>
            {[
              `${size.width}x${size.height}`,
              `${offset.x},${offset.y}`,
              String(hovered),
              String(focused),
              String(seen),
            ].join(" ")}
          </output>
        </div>
      );
    }
    expect(markupOf(<Probe />)).toBe(
      "<div><span>null</span><output>0x0 0,0 false false false</output></div>",
    );
  });

  it("binds no shortcut where there is no document", () => {
    component Probe() {
      useKeyCombo("mod+k", () => {});
      const held = useKeyHeld("shift");
      return <output>{String(held)}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>false</output>");
  });
});

describe("state and time, prerendered", () => {
  it("gives storage the stated initial value", () => {
    component Probe() {
      const [name] = useStorage("uf-ssr-name", "unset");
      const [count] = useStorage("uf-ssr-count", 7, { session: true });
      return <output>{`${name} ${String(count)}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>unset 7</output>");
  });

  it("renders the state shapes at their starting values", () => {
    component Probe() {
      const { on } = useToggle();
      const { count } = useCounter(3, { min: 0, max: 5 });
      const list = useList<string>(["a", "b"]);
      const set = useSet<string>(["x"]);
      const cycle = useCycle(["one", "two"]);
      const empty = useCycle<string>([]);
      const history = useUndoable("start");
      const before = usePrevious(count);
      const mounted = useMounted();
      return (
        <output>
          {[
            String(on),
            String(count),
            list.items.join("/"),
            String(set.items.size),
            String(cycle.value),
            String(empty.value),
            history.value,
            String(history.canUndo),
            String(before),
            String(mounted),
          ].join(" ")}
        </output>
      );
    }
    // `useMounted` is `false` here and `false` in the client's first render,
    // which is the whole reason it exists.
    expect(markupOf(<Probe />)).toBe(
      "<output>false 3 a/b 1 one null start false undefined false</output>",
    );
  });

  it("shows the stable text for a relative time rather than a clock", () => {
    const at = new Date("2026-01-02T03:04:05.000Z");
    component Probe() {
      const stated = useTimeAgo(at);
      const chosen = useTimeAgo(at, { serverValue: "a while back" });
      const now = useNow(1000, new Date(0));
      return <output>{`${stated} | ${chosen} | ${now.toISOString()}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe(
      "<output>2026-01-02T03:04:05.000Z | a while back | 1970-01-01T00:00:00.000Z</output>",
    );
  });

  it("starts no timers and reports nobody idle", () => {
    let ran = 0;
    component Probe() {
      useInterval(() => {
        ran += 1;
      }, 1);
      const idle = useIdle(1);
      const settled = useDebouncedValue("typed", 1);
      const throttled = useThrottledCallback(() => {}, 1);
      useAnimationFrame(() => {
        ran += 1;
      });
      return <output>{`${String(idle)} ${settled} ${typeof throttled}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>false typed function</output>");
    expect(ran).toBe(0);
  });

  /**
   * The last seven names in the package, so the claim covers all of them.
   *
   * `index.js` says this file "renders the whole surface in a process that has
   * no DOM at all", and for a while that was true of every hook whose subject
   * was the browser and of none of the seven whose subject is the component.
   * A `useMount` body that ran during a prerender would double every effect a
   * page has, on the server, where nothing can undo it.
   */
  it("runs none of the lifecycle bodies and schedules nothing", () => {
    let ran = 0;
    component Probe() {
      useIsomorphicLayoutEffect(() => {
        ran += 1;
      }, []);
      useMount(() => {
        ran += 1;
      });
      useUnmount(() => {
        ran += 1;
      });
      useTimeout(() => {
        ran += 1;
      }, 1);
      const debounced = useDebouncedCallback(() => {
        ran += 1;
      }, 1);
      // Called during the render on purpose: the ref behind a stable callback is
      // seeded with the body and only *updated* in an insertion effect, so a
      // prerender that called one would otherwise reach a `null.current`.
      const stable = useStableCallback(() => "stable");
      const rerender = useRerender();
      return <output>{`${typeof debounced} ${stable()} ${typeof rerender}`}</output>;
    }
    expect(markupOf(<Probe />)).toBe("<output>function stable function</output>");
    expect(ran).toBe(0);
  });

  it("reports a promise as pending and never starts it", () => {
    let started = 0;
    component Probe() {
      const { value, error, pending } = useAsync(async () => {
        started += 1;
        return "loaded";
      }, []);
      return <output>{`${String(value)} ${String(error)} ${String(pending)}`}</output>;
    }
    // A page that needs data in its HTML uses a route loader; this hook is an
    // effect, and an effect does not run during a prerender.
    expect(markupOf(<Probe />)).toBe("<output>null null true</output>");
    expect(started).toBe(0);
  });
});

describe("the render anchor, prerendered", () => {
  // This is the side of the handoff that goes first, and the only side that has
  // no document to read from. `RenderProvider` has to notice that, decide the
  // values itself, and write them where the other side will look — which is
  // exactly what cannot be checked in a process that has a document, because
  // there the read would succeed for the wrong reason.
  const ANCHOR = Date.UTC(2026, 8, 4, 6, 0, 0);

  it("writes what it decided into the markup, with nowhere to read it from", () => {
    const markup = markupOf(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="fixedseed">
        <p>page</p>
      </RenderProvider>,
    );

    // A `<meta>` rather than the `<script>` this used to be, and React hoists
    // it: in a document React rendered it lands in `<head>`, and in a tree
    // that is not one it lands at the front, which is the run `@uniflowed/
    // router`'s shell lifts into the head it wrote itself. That is what lets
    // the router render this above a root layout that owns `<html>`, which is
    // what ubugeeei-prod/uf#559 needed.
    expect(markup).toBe(
      '<meta name="uf:render" ' +
        'content="{&quot;at&quot;:1788501600000,&quot;timeZone&quot;:&quot;Asia/Tokyo&quot;,' +
        '&quot;seed&quot;:&quot;fixedseed&quot;}"/><p>page</p>',
    );
  });

  it("hands the tree the instant, the zone and a stream, and no clock", () => {
    component Probe() {
      const at = useRenderedAt();
      const zone = useRenderTimeZone();
      const drawn = useRandom("featured").next();
      const order = useShuffled(["a", "b", "c"], "featured").join("");
      return <output>{`${at.toString()} ${zone} ${drawn.toFixed(6)} ${order}`}</output>;
    }
    const page = (
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="fixedseed">
        <Probe />
      </RenderProvider>
    );

    // Byte-identical twice, which is the claim: nothing in that tree read a
    // clock or a random number generator, so a second render — the browser's —
    // has nothing it could produce differently.
    const first = markupOf(page);
    expect(first).toBe(markupOf(page));
    expect(first).toContain("2026-09-04T06:00:00Z Asia/Tokyo");
  });

  it("escapes a seed that would otherwise close the carrier", () => {
    // A seed is uf's own eight base-36 characters, so this cannot happen today.
    // It is checked because the carrier holds a value, and a value that can end
    // the element it is written in is the oldest injection there is. The guard
    // moved when the carrier did: an attribute is escaped by React and decoded
    // by the parser, so `JSON.stringify` is the whole of the encoding and the
    // element cannot be closed from inside it.
    const markup = markupOf(
      <RenderProvider at={ANCHOR} timeZone="UTC" seed={'"><script>alert(1)</script>'}>
        <p>page</p>
      </RenderProvider>,
    );

    expect(markup).not.toContain("<script>alert(1)");
    expect(markup).toContain("&lt;script&gt;alert(1)");
  });

  it("is one carrier and one envelope however many providers are nested", () => {
    // The rule ubugeeei-prod/uf#559 turns on: the router renders one of these
    // above every application, so an application that renders its own is
    // nested inside that one. Adding a second envelope would put two answers
    // in the document and the client would read the first — so a nested
    // provider replaces, and writes nothing.
    const markup = markupOf(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="outer">
        <RenderProvider seed="inner">
          <p>page</p>
        </RenderProvider>
      </RenderProvider>,
    );

    expect(markup.split("uf:render").length - 1).toBe(1);
    expect(markup).toContain("&quot;seed&quot;:&quot;outer&quot;");
    expect(markup).not.toContain("&quot;seed&quot;:&quot;inner&quot;");
  });

  it("gives a nested provider the fields it overrode and inherits the rest", () => {
    component Probe() {
      const at = useRenderedAt();
      const zone = useRenderTimeZone();
      const drawn = useRandom("featured").next();
      return <output>{`${at.toString()} ${zone} ${drawn.toFixed(6)}`}</output>;
    }

    const outer = markupOf(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="outer">
        <Probe />
      </RenderProvider>,
    );
    const inner = markupOf(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="outer">
        <RenderProvider seed="inner">
          <Probe />
        </RenderProvider>
      </RenderProvider>,
    );

    // The instant and the zone come down from above; only the seed changed.
    expect(inner).toContain("2026-09-04T06:00:00Z Asia/Tokyo");
    expect(inner).not.toBe(outer);
  });
});
