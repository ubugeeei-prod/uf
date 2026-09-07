// @flow
//
// `@uniflowed/hooks/render`: the two things a render must decide once.
//
// A page that is prerendered is rendered twice — once on a server, once in the
// browser that hydrates it — and React compares the two. Anything the second
// render works out for itself differs from the first, and the two that differ
// in practice are the clock and the random number generator. `new Date()` is a
// different instant on the two machines and `Math.random()` is a different
// number by construction, so a countdown, a greeting that depends on the hour,
// a shuffled list of featured articles and a randomly chosen placeholder are
// each a hydration mismatch that the application did nothing to deserve.
//
// The usual advice is to render nothing until an effect has run. That works,
// and it costs the page: the value is invisible to a crawler and to a reader
// with no JavaScript, and it arrives one frame late and moves the layout when
// it does.
//
// This module is the other answer. The render decides both values once, on
// whichever side goes first, and the other side reads what was decided instead
// of deciding again. Both renders then produce the same markup, because they
// are working from the same two numbers.
//
// # Why a provider, and not module state
//
// A clock installed in `@uniflowed/core/clock` is the process's. That is right
// for a test and for a runtime, and wrong for a server: a server renders
// several requests at once, and two responses that shared one "rendered at"
// would each be stamped with whenever the other one started. React's context is
// per-render by construction, which is the granularity this actually needs, so
// the values travel through the tree rather than beside it.
//
// # How the value crosses the network
//
// `RenderProvider` writes what it decided into the markup, as an inert
// `<script type="application/json">`, and reads it back on the client before
// the first render — the same carrier and the same escaping
// `@uniflowed/router` uses for loader data, because it is the same problem and
// a second mechanism would be a second thing to get wrong. `dangerouslySetInnerHTML`
// rather than a text child, because the HTML parser treats a `<script>` body as
// raw text and does not decode entities: React's escaping of `&` would survive
// into `JSON.parse` and fail there.
//
// A page nobody prerendered has no script to read, decides both values from the
// host, and is correct for the reason that there is nothing to disagree with.
//
// # What belongs in this module
//
// A value that a render has to fix rather than derive. Two so far, and they are
// the two React itself has an answer for exactly one of: `useId` solves ids by
// deriving them from the position in the tree, which works for an id and for
// nothing that has to be shuffled or counted from.
//
// Not here: a hook that reads a clock to schedule work. `useInterval`,
// `useTimeout` and `useNow` are `timing.js`'s, and they read the *current* time
// — this module is about the one instant that must not move.

import * as React from "@uniflowed/react";
import { currentClock } from "@uniflowed/core/clock";
import type { Random } from "@uniflowed/core/random";
import { hostSeed, seededRandom, shuffled } from "@uniflowed/core/random";
import type { Instant } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";

/** What a render fixes, and what travels to the client. */
export type RenderEnvelope = {
  /** The instant the render was anchored to, in epoch milliseconds. */
  readonly at: number,
  /** The IANA zone the server was in. Not the reader's — see `Time`. */
  readonly timeZone: string,
  /** The seed both sides replay the same numbers from. */
  readonly seed: string,
};

/** The element the envelope is written into, and read back out of. */
export const RENDER_ID: string = "__uf_render";

const RenderContext: React.Context<RenderEnvelope | null> = React.createContext(null);

/**
 * The envelope the server left in the document, if there is one.
 *
 * Read from the DOM rather than from a global an inline script assigned,
 * because an inline script that runs is a script a content-security policy has
 * to allow, and this value is worth no relaxation of one.
 */
function embedded(): RenderEnvelope | null {
  const document = globalThis.document;
  if (document == null) {
    return null;
  }
  const element = document.getElementById(RENDER_ID);
  if (element == null) {
    return null;
  }
  try {
    const found = JSON.parse(element.textContent ?? "null");
    if (found == null || typeof found.at !== "number" || typeof found.seed !== "string") {
      return null;
    }
    return { at: found.at, timeZone: String(found.timeZone), seed: found.seed };
  } catch {
    // A truncated document — a stream that was cut off — leaves half a JSON
    // object here. Deciding fresh values is wrong on that page in exactly the
    // way this module exists to prevent, and it is still better than a render
    // that throws: the page renders, and hydration reports what it always
    // would have.
    return null;
  }
}

/** Decide what this render is anchored to, from props, from the markup, or from the host. */
function envelope(given: { at?: number, timeZone?: string, seed?: string }): RenderEnvelope {
  const found = embedded();
  const clock = currentClock();
  return {
    at: given.at ?? found?.at ?? clock.now(),
    timeZone: given.timeZone ?? found?.timeZone ?? clock.timeZone(),
    seed: given.seed ?? found?.seed ?? hostSeed(),
  };
}

/**
 * `envelope` as the text of a `<script type="application/json">`.
 *
 * `<` is escaped so that a zone name or a seed holding `</script>` cannot end
 * the element early, and the two line separators are escaped because they are
 * newlines to a JavaScript parser and are not to `JSON.stringify`. The same
 * three replacements `@uniflowed/router` makes, deliberately duplicated rather
 * than shared: this package does not depend on the router, and three lines are
 * not worth an import that would drag one in.
 */
function encode(value: RenderEnvelope): string {
  return JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

/**
 * Fix this render's instant, zone and seed, and hand them to the tree.
 *
 * Render it once, above everything that reads a clock — a root layout is where
 * it belongs. Every argument is optional and the defaults are the whole point:
 * a server decides, the markup carries what it decided, and the browser reads it
 * back before its first render, so neither side has to be told which one it is.
 *
 * `at` and `seed` are there for the two cases that are not that. A test passes
 * them to get a page that renders the same bytes every time; an application
 * whose instant comes from somewhere better — a request header, a loader —
 * passes that instead.
 *
 * `security/no-dangerously-set-inner-html` is suppressed on the one line that
 * needs it, and the argument is narrow enough to state exactly. The rule is
 * about markup that came from somewhere — a comment, a profile, a response —
 * and its escape hatch is a `@uniflowed/markdown` sanitizer, which is the right
 * answer for markup and no answer at all for JSON. What is written here is
 * three fields this module produced: two of them are typed `number` and
 * `string` and all three go through `JSON.stringify` and then `encode`, which
 * removes the only three characters that can end a `<script>` early or split a
 * line inside one. There is also no other spelling — the HTML parser reads a
 * `<script>` body as raw text and does not decode entities, so React's escaping
 * of a text child would survive into `JSON.parse` and fail there.
 */
export component RenderProvider(
  at?: number,
  timeZone?: string,
  seed?: string,
  children: React.Node,
) {
  // The initializer runs once per mount, on both sides, which is what makes
  // this a fixed value rather than a clock: a re-render for any other reason
  // must not move the instant the page has already been drawn with.
  const [value] = React.useState(() => envelope({ at, timeZone, seed }));

  return (
    <RenderContext.Provider value={value}>
      <script
        id={RENDER_ID}
        type="application/json"
        // uf-lint-disable-next-line security/no-dangerously-set-inner-html
        dangerouslySetInnerHTML={{ __html: encode(value) }}
      />
      {children}
    </RenderContext.Provider>
  );
}

/**
 * What this render was anchored to, or `null` outside a `RenderProvider`.
 *
 * Null rather than a fabricated envelope, because "nobody fixed these values"
 * is a fact the hooks in `timing.js` act on: without a provider they read the
 * clock, which is the behaviour they have always had.
 */
export hook useRenderEnvelope(): RenderEnvelope | null {
  return React.useContext(RenderContext);
}

/**
 * The instant this page was rendered at, as a `Temporal.Instant`.
 *
 * Constant for the life of the render, on both sides, which is what makes it
 * safe to put in the markup. It is not "now" and does not become "now": a page
 * left open for an hour still reports the instant it was rendered at, and a
 * label that has to stay true while the reader looks at it is `useTimeAgo`.
 *
 * Falls back to the clock outside a provider, which is right for a page that is
 * only ever rendered once — and is a hydration mismatch on one that is
 * prerendered, which is what the provider is for.
 */
export hook useRenderedAt(): Instant {
  const found = useRenderEnvelope();
  const at = found?.at;
  // The number, not the instant, is what the memoization keys on: `Instant` is
  // a new object every render, so a scope that depended on one would rebuild
  // this on every render rather than on every change of clock.
  const clockAt = at ?? currentClock().now();
  return Temporal.Instant.fromEpochMilliseconds(clockAt);
}

/**
 * The zone the render was made in.
 *
 * The server's, not the reader's, and the distinction is the second half of the
 * hydration problem rather than a detail: markup formatted in the reader's zone
 * cannot match markup formatted in the server's, so a component renders this one
 * and localises after hydration. `@uniflowed/web`'s `Time` is that component.
 */
export hook useRenderTimeZone(): string {
  const found = useRenderEnvelope();
  return found?.timeZone ?? currentClock().timeZone();
}

/**
 * A stream of random numbers that both renders produce identically.
 *
 * `label` names the stream, and naming it is what makes it independent of every
 * other one: two components that ask for `"featured"` and `"sidebar"` get the
 * same numbers whatever order they render in, and whatever suspends between
 * them. Sharing one stream would make each component's numbers depend on how
 * many the components above it happened to draw — stable in a synchronous
 * render, and not stable once a boundary resolves at a different moment on the
 * two sides.
 *
 * The stream is stateful, so a component that draws from it during render draws
 * different numbers on a re-render. Draw into a `const` keyed by what the
 * numbers are for — which the React Compiler memoizes — or in an event, and
 * never twice in the body of a component React may render twice.
 *
 * Outside a provider the seed is a constant rather than the host's, which looks
 * like the wrong default and is the right one: two renders with no envelope
 * between them still have to agree, and a constant is the only seed both of them
 * can arrive at. What it costs is that every such page shuffles the same way,
 * which is a reason to render a provider rather than a reason to be
 * unpredictable here.
 */
export hook useRandom(label: string): Random {
  const found = useRenderEnvelope();
  const seed = found?.seed;
  return seededRandom(seed ?? "uf").fork(label);
}

/**
 * `items`, shuffled the same way on both sides of a hydration.
 *
 * The shuffle is memoized over the seed, the label and the items — by the React
 * Compiler, which is where uf's memoization comes from — so it is one shuffle
 * rather than one per render. That matters for more than speed: a fresh draw on
 * every render would reorder the list under the reader every time anything else
 * on the page changed.
 */
export hook useShuffled<T>(items: $ReadOnlyArray<T>, label: string): Array<T> {
  const random = useRandom(label);
  return shuffled(items, random);
}
