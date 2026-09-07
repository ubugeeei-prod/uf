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
// `RenderProvider` writes what it decided into the markup, as a `<meta>`, and
// reads it back on the client before the first render.
//
// A `<meta>` rather than the `<script type="application/json">` this used to
// be, and the reason is where the two end up. React hoists a `<title>`, a
// `<meta>` and a `<link>` and does not hoist a script: in a document React
// rendered, the meta goes into `<head>`; in a tree that is not a document — a
// `@uniflowed/router` application whose root layout renders content rather
// than `<html>` — React writes it at the front, which is the run uf's shell
// lifts into the head it wrote itself. A script had neither behaviour, so a
// provider rendered above a root layout that owns `<html>` emitted it *before*
// the document, and the router could not provide one without deciding where in
// the tree the application's `<html>` was. That is ubugeeei-prod/uf#559: a
// guarantee that depends on the application remembering to opt in is not one,
// and the carrier was the thing standing in the way of it being automatic.
//
// It also removes an escaping problem rather than solving one. A `<script>`
// body is raw text to the HTML parser, so the encoder had to remove `<` and
// the two line separators itself and the element needed
// `dangerouslySetInnerHTML`; an attribute value is escaped by React and decoded
// by the parser, so what `JSON.parse` gets back is what `JSON.stringify`
// produced, with nothing in between to get wrong.
//
// A page nobody prerendered has no meta to read, decides both values from the
// host, and is correct for the reason that there is nothing to disagree with.
//
// # Nesting replaces; it does not add
//
// The router renders one of these above every application, so an application
// that renders its own is nested inside that one. A nested provider inherits
// the envelope above it and overrides only the fields it was given, and it
// writes no second carrier — which is what makes "a project that wants a
// different clock or seed *replaces* it" true rather than aspirational. Two
// carriers would be two answers to one question, and the client reads the
// first.
//
// # What it costs, which is worth saying out loud
//
// Every uf document now carries an envelope, so two renders of one route are no
// longer the same bytes: the instant moved and the seed is a fresh one. That is
// the same price an application that followed the old advice and wrapped its own
// tree already paid — what changed is that every application pays it — and it is
// the price of the guarantee rather than an oversight. A render that has to be
// reproducible fixes `at` and `seed` itself, which is what those two props are
// for; `gives the same document however the host takes it` in
// `tests/library/streaming.test.js` is a test that compares two renders and says
// so.
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

/**
 * The `<meta>` name the envelope is written under, and read back out of.
 *
 * A name rather than an id because that is what a `<meta>` is addressed by:
 * `document.querySelector('meta[name=…]')` is the read, and React's own
 * hoisting treats the `name`/`content` pair as the element's identity.
 */
export const RENDER_META: string = "uf:render";

/**
 * What the render above this one was anchored to, if anything was.
 *
 * Null outside a provider, which is the same question two callers ask of it:
 * [`useRenderEnvelope`] asks whether the values were fixed at all, and
 * [`RenderProvider`] asks whether it is the outermost one and therefore the
 * one that writes the carrier. "Is there one above me" is exactly the question
 * a context answers, and during a render there is no other way to ask it.
 */
const RenderContext: React.Context<RenderEnvelope | null> = React.createContext(null);

/**
 * The envelope the server left in the document, if there is one.
 *
 * Read from the DOM rather than from a global an inline script assigned,
 * because an inline script that runs is a script a content-security policy has
 * to allow, and this value is worth no relaxation of one.
 *
 * `querySelector` takes the first match rather than requiring the only one: a
 * document that somehow carried two would still have an answer, and the first
 * is the one every other reader of a duplicated `<meta>` takes.
 */
function embedded(): RenderEnvelope | null {
  const document = globalThis.document;
  if (document == null) {
    return null;
  }
  const element = document.querySelector(`meta[name="${RENDER_META}"]`);
  if (element == null) {
    return null;
  }
  try {
    const found = JSON.parse(element.getAttribute("content") ?? "null");
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

/**
 * Decide what this render is anchored to.
 *
 * What the caller gave, then what a provider above already fixed, then what
 * the markup carries, then the host — in that order, per field. `inherited`
 * comes before `embedded()` because a provider that overrode `at` for its
 * subtree must not have `timeZone` read back out of the document and quietly
 * paired with somebody else's instant.
 */
function envelope(
  given: { at?: number, timeZone?: string, seed?: string },
  inherited: RenderEnvelope | null,
): RenderEnvelope {
  const found = inherited ?? embedded();
  const clock = currentClock();
  return {
    at: given.at ?? found?.at ?? clock.now(),
    timeZone: given.timeZone ?? found?.timeZone ?? clock.timeZone(),
    seed: given.seed ?? found?.seed ?? hostSeed(),
  };
}

/**
 * Fix this render's instant, zone and seed, and hand them to the tree.
 *
 * A `@uniflowed/router` application already has one: `routerView` renders this
 * above everything, so `useRenderedAt` and `useRandom` agree across hydration
 * without the application saying anything. Rendering one by hand is for the
 * cases that need different values, and it is a *replacement* rather than an
 * addition — a nested provider inherits the envelope above it, overrides only
 * the fields it was given, and writes no second carrier. See
 * ubugeeei-prod/uf#559.
 *
 * Every argument is optional and the defaults are the whole point: a server
 * decides, the markup carries what it decided, and the browser reads it back
 * before its first render, so neither side has to be told which one it is.
 *
 * `at` and `seed` are there for the two cases that are not that. A test passes
 * them to get a page that renders the same bytes every time; an application
 * whose instant comes from somewhere better — a request header, a loader —
 * passes that instead. Both have to be values the *browser* arrives at too:
 * they are not carried, because what is carried is the outermost envelope, and
 * a value only one side can compute is the mismatch this module exists to
 * remove.
 *
 * The carrier is a `<meta>`, which is what lets this be rendered above a root
 * layout that owns `<html>`: React hoists it into the head of the document
 * either way. The header of this file has the whole argument.
 */
export component RenderProvider(
  at?: number,
  timeZone?: string,
  seed?: string,
  children: React.Node,
) {
  const enclosing = React.useContext(RenderContext);
  // The initializer runs once per mount, on both sides, which is what makes
  // this a fixed value rather than a clock: a re-render for any other reason
  // must not move the instant the page has already been drawn with.
  const [decided] = React.useState(() => envelope({ at, timeZone, seed }, enclosing));
  // The outermost provider writes the carrier and a nested one does not, so a
  // document holds one envelope however many providers a tree has. Not state,
  // because whether there is a provider above this one is a fact about the
  // shape of the tree: a subtree cannot gain or lose an enclosing provider
  // without being remounted, so this cannot change under a re-render and ask
  // React to add or remove an element the server's markup already settled.
  const outermost = enclosing == null;

  return (
    <RenderContext.Provider value={decided}>
      {outermost ? <meta name={RENDER_META} content={JSON.stringify(decided)} /> : null}
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
  // The number, not the instant, in the dependency: `Instant` is a new object
  // every render and depending on it would rebuild this on every one.
  const clockAt = at ?? currentClock().now();
  return React.useMemo(() => Temporal.Instant.fromEpochMilliseconds(clockAt), [clockAt]);
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
 * different numbers on a re-render. Draw in a `useMemo` keyed by what the
 * numbers are for, or in an event, and never in the body of a component React
 * may render twice.
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
  return React.useMemo(() => seededRandom(seed ?? "uf").fork(label), [seed, label]);
}

/**
 * `items`, shuffled the same way on both sides of a hydration.
 *
 * The shuffle is a `useMemo` over the seed, the label and the items, so it is
 * one shuffle rather than one per render — which matters for more than speed:
 * a fresh draw on every render would reorder the list under the reader every
 * time anything else on the page changed.
 */
export hook useShuffled<T>(items: $ReadOnlyArray<T>, label: string): Array<T> {
  const random = useRandom(label);
  return React.useMemo(() => shuffled(items, random), [items, random]);
}
