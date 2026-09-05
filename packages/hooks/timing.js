// @flow
//
// `@uniflowed/hooks/timing`: timers that stop when the component does.
//
// Every one of these exists because the hand-written version leaks: a
// `setInterval` in a `useEffect` whose dependency array includes the callback
// is torn down and restarted on every render, and one without the callback in
// the array calls a stale closure forever. `useStableCallback` removes the
// choice — the timer is set once and always calls the current body.
//
// # What belongs in this module
//
// A hook whose subject is *when* something runs: on a schedule, after a wait,
// no more often than some rate, on the next frame, once the reader has stopped
// touching anything. Every one of them owns a handle that has to be cleared,
// and the cleanup is the reason the hook exists rather than a detail of it.
//
// `useNow` and `useTimeAgo` belong here for the same reason, which is easy to
// miss: what is difficult about "3 minutes ago" is not the words, it is
// deciding how often the words have to be worked out again. A label a minute
// old must be redrawn every second and one a week old must not be redrawn at
// all, and getting that wrong is either a wrong label or a component that
// re-renders sixty times a second forever.
//
// Not here: `useMount` and `useUnmount`, which are about the component's life
// rather than a clock, and live in `lifecycle.js`; and rendering a time, which
// is `@uniflowed/web`'s `Time`. The split between that component and
// `useTimeAgo` is the split between markup and schedule — `Time` decides what
// a `<time>` element contains and how it survives hydration, and works out its
// relative text exactly once; `useTimeAgo` is for a label that has to stay
// true while the reader looks at it. This package does not depend on
// `@uniflowed/web` to get there, because a hook library that pulled in a
// component library would be the wrong direction for the one arrow between
// them.

import { useEffect, useMemo, useRef, useState } from "@uniflowed/react";

import { browserWindow } from "./browser.js";
import { useMounted, useStableCallback } from "./lifecycle.js";

/**
 * Call `body` every `millis`, or not at all when `millis` is null.
 *
 * Null rather than a separate `enabled` flag because "no interval" and "an
 * interval of nothing" are the same thing, and one argument cannot disagree
 * with itself.
 */
export hook useInterval(body: () => mixed, millis: number | null): void {
  const stable = useStableCallback(body);
  useEffect(() => {
    if (millis == null) {
      return;
    }
    const id = setInterval(stable, millis);
    return () => clearInterval(id);
  }, [stable, millis]);
}

/** Call `body` once after `millis`, or not at all when `millis` is null. */
export hook useTimeout(body: () => mixed, millis: number | null): void {
  const stable = useStableCallback(body);
  useEffect(() => {
    if (millis == null) {
      return;
    }
    const id = setTimeout(stable, millis);
    return () => clearTimeout(id);
  }, [stable, millis]);
}

/**
 * `value`, but only after it has stopped changing for `millis`.
 *
 * The classic use is a search box: the query updates on every keystroke and
 * the request should not.
 */
export hook useDebouncedValue<T>(value: T, millis: number): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const id = setTimeout(() => setSettled(value), millis);
    return () => clearTimeout(id);
  }, [value, millis]);

  return settled;
}

/**
 * A callback that runs at most once per `millis`.
 *
 * Leading edge: the first call goes through immediately and later ones inside
 * the window are dropped, which is what a scroll or resize handler wants —
 * the trailing-edge version would make the first paint late.
 */
export hook useThrottledCallback<TArgs extends $ReadOnlyArray<mixed>>(
  body: (...args: TArgs) => mixed,
  millis: number,
): (...args: TArgs) => void {
  // Written out rather than inferred: Flow cannot instantiate one function's
  // rest-parameter type variable from another's, so the type arguments are
  // given here and at every other call in this file.
  const stable = useStableCallback<TArgs, mixed>(body);
  const last = useRef(0);

  return useStableCallback<TArgs, void>((...args: TArgs) => {
    const now = Date.now();
    if (now - last.current >= millis) {
      last.current = now;
      stable(...args);
    }
  });
}

/**
 * A callback that runs `millis` after the last time it was asked to.
 *
 * Trailing edge, and it cancels itself at unmount — the version people write
 * calls `setState` on a component that is gone.
 */
export hook useDebouncedCallback<TArgs extends $ReadOnlyArray<mixed>>(
  body: (...args: TArgs) => mixed,
  millis: number,
): (...args: TArgs) => void {
  const stable = useStableCallback<TArgs, mixed>(body);
  const timer = useRef<TimeoutID | null>(null);

  useEffect(
    () => () => {
      if (timer.current != null) {
        clearTimeout(timer.current);
      }
    },
    [],
  );

  return useStableCallback<TArgs, void>((...args: TArgs) => {
    if (timer.current != null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      stable(...args);
    }, millis);
  });
}

/**
 * Run `body` before every frame the browser paints, while `active`.
 *
 * `delta` is the milliseconds since the previous frame and is zero on the
 * first, which is what an animation integrates against: a frame that took 32ms
 * because the tab was busy has to move twice as far as one that took 16ms, and
 * a hand-written loop that assumes sixty a second runs at half speed on a
 * hundred-and-twenty-hertz display.
 *
 * Nothing runs before hydration: there is no frame to paint during a prerender,
 * and the effect that would ask for one does not run there.
 */
export hook useAnimationFrame(
  body: (frame: {| readonly delta: number, readonly time: number |}) => mixed,
  active: boolean = true,
): void {
  const stable = useStableCallback(body);

  useEffect(() => {
    const win = browserWindow();
    if (!active || win == null || win.requestAnimationFrame == null) {
      return;
    }
    let handle: AnimationFrameID | null = null;
    let previous: number | null = null;

    const step = (time: number) => {
      const delta = previous == null ? 0 : time - previous;
      previous = time;
      // Asked for before the body runs, so a body that throws does not stop
      // the loop silently — it stops it loudly, on the next frame, having
      // already reported the throw to the browser.
      handle = win.requestAnimationFrame?.(step) ?? null;
      stable({ delta, time });
    };

    handle = win.requestAnimationFrame(step);
    return () => {
      if (handle != null) {
        win.cancelAnimationFrame?.(handle);
      }
    };
  }, [active, stable]);
}

/** What counts as the reader still being there. */
const ACTIVITY: $ReadOnlyArray<string> = [
  "pointermove",
  "pointerdown",
  "keydown",
  "wheel",
  "touchstart",
  "scroll",
  "visibilitychange",
];

/**
 * Whether the reader has stopped doing anything for `millis`.
 *
 * `false` on a server and on the first client render, which is the answer that
 * cannot be wrong: nobody is idle before the page exists, and starting at
 * `true` would flash whatever the page shows an idle reader.
 *
 * The listeners are passive and on the window rather than on any element, so
 * this costs nothing on a touch screen and sees activity anywhere on the page.
 */
export hook useIdle(
  millis: number = 60_000,
  options?: {| readonly events?: $ReadOnlyArray<string> |},
): boolean {
  const [idle, setIdle] = useState(false);
  const events = options?.events ?? ACTIVITY;
  // Compared by contents: an array written inline in the call is a new array
  // every render, and depending on its identity would re-listen every render.
  const key = events.join(",");

  useEffect(() => {
    const win = browserWindow();
    if (win == null) {
      return;
    }
    const names = key.split(",");
    let timer: TimeoutID | null = null;

    const wake = () => {
      setIdle(false);
      if (timer != null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => setIdle(true), millis);
    };

    wake();
    for (const name of names) {
      win.addEventListener(name, wake, { passive: true });
    }
    return () => {
      if (timer != null) {
        clearTimeout(timer);
      }
      for (const name of names) {
        win.removeEventListener(name, wake);
      }
    };
  }, [millis, key]);

  return idle;
}

/**
 * The current time, re-read every `millis`.
 *
 * The clock is read in the initial state rather than in an effect, so a
 * client-only page has the right time on its first paint instead of a frame of
 * something else. That read is the one impure thing in this package, and it is
 * bounded: it happens once, the value is never re-read during a render, and a
 * render React throws away is replaced by another whose clock is just as valid.
 *
 * A prerendered page that puts this on screen needs `serverValue`, because the
 * server's clock and the reader's are not the same number and React compares
 * the text. Given one, the first render on both sides is that value and the
 * real time arrives with the first effect. `useTimeAgo` has already made this
 * choice; prefer it for a label.
 */
export hook useNow(millis: number | null = 1000, serverValue: Date | null = null): Date {
  // The instant rather than the object: a caller writing `new Date(...)` in
  // the call passes a different object every render, and a dependency on it
  // would re-run the effect forever.
  const since = serverValue == null ? null : serverValue.getTime();
  const [now, setNow] = useState<Date>(() => (since == null ? new Date() : new Date(since)));

  useEffect(() => {
    if (since != null) {
      setNow(new Date());
    }
  }, [since]);

  useInterval(() => setNow(new Date()), millis);
  return now;
}

/**
 * `Intl.RelativeTimeFormat`, which Flow's own library definition does not have.
 *
 * The vendored `intl.js` declares `Collator`, `DateTimeFormat`, `Locale`,
 * `NumberFormat`, `PluralRules` and `Segmenter` and stops there, so
 * `Intl.RelativeTimeFormat` is a missing property and `Intl$RelativeTimeFormatUnit`
 * is an unresolvable name. Declaring the shape here is how this file names a
 * global its checker has not caught up with — narrow, exactly as wide as what
 * is called, and optional so that a runtime without the constructor is a
 * branch rather than a crash.
 */
declare class RelativeTimeFormat {
  constructor(locale?: string, options?: { numeric?: "always" | "auto", ... }): void;
  format(value: number, unit: RelativeUnit): string;
}

declare var Intl: {
  RelativeTimeFormat?: Class<RelativeTimeFormat>,
  ...
};

/** The units `ago` is willing to describe a gap in. */
type RelativeUnit = "year" | "month" | "day" | "hour" | "minute" | "second";

/** Milliseconds in each unit, largest first. */
const UNITS: $ReadOnlyArray<[RelativeUnit, number]> = [
  ["year", 31_536_000_000],
  ["month", 2_592_000_000],
  ["day", 86_400_000],
  ["hour", 3_600_000],
  ["minute", 60_000],
  ["second", 1_000],
];

/** How often a label this far from now has to be worked out again. */
function cadence(distance: number): number {
  if (distance < 60_000) {
    return 1_000;
  }
  if (distance < 3_600_000) {
    return 30_000;
  }
  return 60_000;
}

/**
 * "3 minutes ago", or "in 3 minutes".
 *
 * Falls back to the instant itself where the browser has no
 * `Intl.RelativeTimeFormat`, because a wrong-language string invented here
 * would be worse than the unambiguous one.
 */
function ago(at: Date, from: Date, locale: string | void): string {
  const Formatter = Intl.RelativeTimeFormat;
  if (Formatter == null) {
    return at.toISOString();
  }
  const difference = at.getTime() - from.getTime();
  const formatter = new Formatter(locale, { numeric: "auto" });
  for (const [unit, span] of UNITS) {
    if (Math.abs(difference) >= span) {
      return formatter.format(Math.round(difference / span), unit);
    }
  }
  // Under a second in either direction is "now", not "in 0 seconds".
  return formatter.format(0, "second");
}

/**
 * "3 minutes ago", kept true while the reader looks at it.
 *
 * Before hydration and on the first client render this is `serverValue`,
 * defaulting to the instant's UTC ISO string — the same choice
 * `@uniflowed/web`'s `Time` makes, and for the same reason: the relative form
 * depends on a clock and a locale that the server does not have, so rendering
 * it on both sides would be a hydration mismatch by construction. The text is
 * in the markup for a crawler and for a reader with no JavaScript, and becomes
 * relative once the page is alive.
 *
 * The update rate follows the distance rather than being fixed: a label from
 * this minute is redrawn every second, one from this hour every thirty, and an
 * older one every minute. That is why this is not "call `useNow` and format
 * it" — a fixed one-second clock re-renders a week-old timestamp 604,800 times
 * to no effect.
 */
export hook useTimeAgo(
  value: Date | string | number,
  options?: {|
    readonly serverValue?: string,
    /** Override the schedule. `null` works it out once and leaves it. */
    readonly interval?: number | null,
    readonly locale?: string,
  |},
): string {
  const serverValue = options?.serverValue;
  const override = options?.interval;
  const locale = options?.locale;

  const instant = value instanceof Date ? value.getTime() : new Date(value).getTime();
  const at = useMemo(() => new Date(instant), [instant]);

  const mounted = useMounted();
  // The schedule itself is the state, not the gap it was chosen from. Holding
  // the gap would mean a render every time the clock moved *and* a second one
  // to record the new gap; holding the schedule means `setSchedule` is handed
  // the same number on all but the few ticks that cross a threshold, and React
  // bails out of those renders entirely.
  const [schedule, setSchedule] = useState(1_000);
  const tick = override === undefined ? schedule : override;
  const now = useNow(mounted ? tick : null);

  const wanted = cadence(Math.abs(now.getTime() - instant));
  useEffect(() => {
    setSchedule(wanted);
  }, [wanted]);

  if (!mounted) {
    return serverValue ?? at.toISOString();
  }
  return ago(at, now, locale);
}
