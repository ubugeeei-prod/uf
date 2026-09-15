// @flow
//
// The router runtime: the browser's binding.
//
// A client module, and the directive is load-bearing rather than descriptive:
// in the module graph React Server Components render in, every export of this
// file is a client reference — `Link` renders as markup on the server and runs
// in the browser — and `../server-components.js` is what that graph gets for
// the hooks instead. Everywhere else the directive changes nothing.
//
// A route table is data — the virtual module `virtual:uf/routes` that
// `@uniflowed/vite` generates from the `app/` directory — and this module is
// what turns it into a running application in a page: the provider that holds
// the current route, the hooks that read it, navigation, view transitions and
// `Link`. What a URL resolves to is `./resolve.js`, the tree a resolved route
// renders is `./compose.js`, and the metadata elements are `./head.js`; the
// three are split out because none of them may reach a hook, a context or a
// class component, which is what lets a server graph resolved under React's
// `react-server` condition import them (ubugeeei-prod/uf#519).

"use client";

import * as React from "react";
import {
  Suspense,
  createContext,
  startTransition,
  use,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from "react";
// The one thing in this module that only a browser can do, and the reason it
// is imported here rather than from `../client.js`: a view transition needs
// the DOM updated inside the callback it was handed, and `startTransition`
// schedules. "View transitions", below, is the argument. Importing `react-dom`
// costs the server bundle nothing it did not already have — `internal/stream.js`
// imports `react-dom/server` — and this entry touches no document while it is
// being evaluated.
import { flushSync } from "react-dom";

// The two things a render has to fix — its instant and its random seed — and
// the provider that fixes them. Imported here rather than left to the
// application, because a hydration guarantee nobody wires is not a guarantee:
// see [`routerView`] and ubugeeei-prod/uf#559.
import { RenderProvider } from "@uniflowed/hooks/render";

// The id of the script the loader data is embedded in. It moved out of the
// head and into the tree with ubugeeei-prod/uf#373 — see [`payloadElements`]
// — so the module that renders it is this one rather than `../server.js`.
import { DATA_ID } from "./document.js";

// The payload the loader's answer is written as, and the rows it defers. Row 0
// is the element `DATA_ID` names and is byte-identical to what this file wrote
// inline before the payload existed whenever nothing is deferred; a promise
// anywhere in the data turns into a reference and a row of its own. See
// `./payload.js` for the format and ubugeeei-prod/uf#519 for the half of it
// that is still an element payload rather than a data one.
import {
  type PayloadRowMessage,
  PayloadRowError,
  encodePayload,
  encodeRowValue,
  payloadJson,
} from "./payload.js";

// The development-only half of [`RouteView`]: the marks that say which DOM
// subtree each boundary owns, and the report that reads them. Every reference
// to it is inside a `BOUNDARY_MARKS` branch, which is why a static import is
// safe here where `../client.js` needs a dynamic one — a component cannot be
// awaited in the middle of a render, and `false` folds the references away
// before the bundler is asked to keep the module. See [`BOUNDARY_MARKS`].
import { BoundaryReporter } from "./boundaries.js";
import { routeBoundaries } from "./boundary-data.js";
import { composeRoute, pageComponent } from "./compose.js";
import { type FetchedFlight, type FlightRoot, type RouteState, routeState } from "./flight.js";
import { Head } from "./head.js";
import { addressOf, applicationPathOf, canonicalAddress } from "./base-path.js";
import {
  clearNavigationCache,
  flightNavigations,
  keepsNavigations,
  navigationKey,
  routeNavigations,
} from "./navigation-cache.js";
import { hasClientPage, matchRoute, nearestBoundary } from "./routing.js";
import type { RouteParams, SearchParams } from "./routing.js";
import {
  beneath,
  interceptingRoutes,
  loadOnce,
  resolveInterception,
  resolveMatch,
} from "./resolve.js";
import type { Metadata, ResolvedRoute, RouteTable } from "./resolve.js";

export type { RouteError, RouteParamSpec, RouteParams, SearchParams } from "./routing.js";

export {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  UnauthorizedError,
  buildRoute,
  forbidden,
  hasClientPage,
  matchRoute,
  notFound,
  parseSearch,
  permanentRedirect,
  redirect,
  routeErrorStatus,
  splitUrl,
  unauthorized,
} from "./routing.js";

export type {
  ErrorBoundary,
  ErrorModule,
  Interception,
  JsonLd,
  LayoutModule,
  LoaderArgs,
  LoadingModule,
  LoadingRecord,
  Metadata,
  MetadataArgs,
  NotFoundBoundary,
  PageModule,
  ResolveOptions,
  ResolvedRoute,
  ResolvedSlot,
  Robots,
  RouteMatch,
  RouteRecord,
  RouteTable,
  SlotRecord,
  SlotRouteRecord,
  TemplateModule,
  TemplateRecord,
  TwitterCard,
} from "./resolve.js";

export { resolveFailure, resolveMatch } from "./resolve.js";

// `app.router.basePath` and `trailingSlash`, installed by the entry that starts
// the application; see `./base-path.js`.
export type { RoutingSettings, TrailingSlash } from "./base-path.js";
export { basePath, installRouting } from "./base-path.js";

// `app.rendering.staleTime`, installed by the same entry; see
// `./navigation-cache.js`.
export { installStaleTime } from "./navigation-cache.js";

// ---------------------------------------------------------------------------
// View transitions
// ---------------------------------------------------------------------------
//
// A client navigation replaces the tree and the browser paints the new one,
// which is a cut. `document.startViewTransition` is the platform's answer, and
// it is opt-in per navigation rather than per site — so somebody has to call
// it, and the somebody is whatever replaced the tree. That is this module.
// Leaving it to the application would mean every application reimplementing
// the same four decisions below, and getting the last one wrong.
//
// # Why `flushSync` rather than `startTransition`
//
// The browser captures the old frame, calls the callback, and waits on the
// promise the callback returns before capturing the new one. So the callback
// has to leave the DOM updated, and `startTransition` deliberately does not:
// it schedules, and returns having changed nothing.
//
// The alternative was to hand the browser a promise resolved from a layout
// effect after the commit, which keeps the render concurrent and can hang: a
// running view transition blocks input until its callback settles, so a commit
// React decides not to make — an interrupted transition, an unmounted provider
// — is a frozen page with no way back. `flushSync` cannot hang.
//
// The cost is real and worth stating rather than discovering. Inside a
// transition the commit is synchronous, so a route that suspends *while
// rendering* shows its `$loading.js` fallback instead of leaving the
// previous page up until it resolves. Its modules and its loader are already
// finished by this point — `resolveMatch` awaited both — so what is left is a
// component suspending on something else, and it degrades to the fallback the
// project wrote for exactly that.
//
// # Why not React's `<ViewTransition>`
//
// It is not in a stable React. This package's peer range is `react >= 19`, and
// reaching for a component that exists only in an experimental build would
// turn an animation into a reason a project cannot use the router at all.
// `startViewTransition` is the same feature one layer down, and it is in the
// browser rather than in a dependency.
//
// # What must not change
//
// A browser without `startViewTransition` navigates exactly as it did before
// any of this. A reader who asked for less motion gets the cut they asked for,
// without the application having to remember to ask on their behalf. And the
// server renders nothing about it: a transition is a client-only concern, and
// the moment one reaches the markup it is a hydration difference instead.

/**
 * The attribute a running transition's name reaches CSS through.
 *
 * On the document element, because that is where the `::view-transition`
 * pseudo-elements hang and therefore the only element a selector can reach
 * them from.
 */
const VIEW_TRANSITION_ATTRIBUTE = "data-uf-view-transition";

/**
 * The part of a running transition this module reads.
 *
 * One property, because one is what a navigation needs: `finished` settles
 * when the animation is over, which is when the document may stop saying which
 * transition is running. `ready` and `updateCallbackDone` are for an
 * application animating something itself, and a router holding them would be
 * claiming to know what they were for.
 */
type ViewTransition = { readonly finished: Promise<mixed>, ... };

/**
 * The document, under the one description this module has of it.
 *
 * Flow's library definitions have no `startViewTransition` — the API is newer
 * than they are — and reading it off `any` would leave the one call that
 * performs a navigation unchecked, where a wrong type is a broken navigation
 * rather than a broken animation. Optional, because "this browser may not have
 * it" is the entire point.
 *
 * An `interface` rather than an object type, because a `Document` is a class
 * instance and class instances are not subtypes of object types. `documentElement`
 * is nullable for the same reason it is in Flow's own libdef: a document parsed
 * from nothing has no root element.
 */
interface ViewTransitionDocument {
  readonly startViewTransition?: (update: () => mixed) => ViewTransition;
  readonly documentElement: HTMLElement | null;
}

/**
 * Whether the reader has asked for less motion.
 *
 * Asked at the moment of the navigation rather than subscribed to, because it
 * is not a rendered value: nothing re-renders when the preference changes, and
 * the only question is what to do with the click that just happened.
 * `usePrefersReducedMotion` in `@uniflowed/hooks` is the rendered form of the
 * same query and answers a different question.
 *
 * `matchMedia` is optional here because a document installed by a test runner
 * may not have one, and a media query that cannot be asked is not a reason to
 * fail a navigation.
 */
function prefersReducedMotion(): boolean {
  const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
  return query != null && query.matches === true;
}

/**
 * Apply `update`, inside a view transition where there is one to be had.
 *
 * Two ways out and they are one decision: with no `startViewTransition`, or
 * with a reader who asked for less motion, this is the `startTransition` the
 * router did before any of this existed — same commit, same concurrency, no
 * animation.
 *
 * `name` is the route's, and it reaches CSS as an attribute for as long as the
 * transition runs. The other spelling is the `types` option, which is the
 * platform's own vocabulary for the same idea and is *newer than
 * `startViewTransition` itself* — so passing the options object to a browser
 * that has only the callback form is a `TypeError` thrown out of the call that
 * performs the navigation. Naming a transition would then need a second and
 * finer feature detection than the one for having transitions at all, and the
 * cost of getting that one wrong is the navigation rather than the animation.
 * One attribute needs no detection and is removed again when the transition
 * ends.
 */
function withViewTransition(name: ?string, update: () => void): void {
  const owner: ViewTransitionDocument = document;
  const start = owner.startViewTransition?.bind(owner);
  if (start == null || prefersReducedMotion()) {
    startTransition(update);
    return;
  }

  const root = owner.documentElement;
  if (name != null && root != null) {
    root.setAttribute(VIEW_TRANSITION_ATTRIBUTE, name);
  }
  const ended = () => {
    if (name != null && root != null) {
      root.removeAttribute(VIEW_TRANSITION_ATTRIBUTE);
    }
  };
  // Both settlements do the same thing, and the rejection is not a failure:
  // `finished` rejects when the transition is skipped — a second navigation
  // before this one finished, a tab that went to the background — and a
  // skipped transition has still ended. Handling it is also what keeps a
  // routine interruption from being reported as an unhandled rejection.
  start(() => {
    flushSync(update);
  }).finished.then(ended, ended);
}

// ---------------------------------------------------------------------------
// The React binding
// ---------------------------------------------------------------------------

/** How a navigation is performed. */
export type NavigateOptions = {|
  readonly replace?: boolean,
  readonly scroll?: boolean,
  /**
   * Whether this navigation may animate. Defaults to `true`, which is what
   * every navigation does.
   *
   * `false` is how a caller says this one is a change of state rather than a
   * change of place — a tab within a page, a filter written into the query
   * string — and should be a cut. `true` does not *force* one: a browser
   * without `startViewTransition` and a reader who asked for less motion still
   * get the cut, because an application able to override the second would
   * eventually override it.
   */
  readonly transition?: boolean,
|};

/** What `useRouter()` returns. */
export type Router = {|
  readonly push: (to: string, options?: NavigateOptions) => Promise<void>,
  readonly replace: (to: string) => Promise<void>,
  readonly prefetch: (to: string) => Promise<void>,
  readonly refresh: () => Promise<void>,
  readonly back: () => void,
  readonly forward: () => void,
|};

/** What `useRoute()` returns. */
export type RouteInfo = {|
  readonly path: string,
  readonly pathname: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
  readonly pending: boolean,
|};

/**
 * What this application does when a visitor follows a link.
 *
 * `app.rendering.navigation` in `uf.config.js`, and the same two words: the
 * client router takes the link over, or the browser does.
 */
export type Navigation = "client" | "document";

/**
 * What the router holds, and what every hook and `RouteView` read.
 *
 * Two halves, because a route arrives two ways. `route` is what a hook reads —
 * the path, the parameters, the loader's answer — and it is the same shape
 * whichever way the route was rendered. `view` is what `RouteView` renders:
 * the tree a server composed for React Server Components, or a route resolved
 * from its modules, which the browser composes itself.
 */
type RouterState = {|
  readonly route: RouteState,
  readonly view: RouteViewState,
  readonly router: Router,
  readonly pending: boolean,
  readonly navigation: Navigation,
|};

/** What `RouteView` renders: a server's tree, or a route to compose. */
type RouteViewState =
  | {| readonly kind: "flight", readonly tree: React.Node |}
  | {| readonly kind: "modules", readonly resolved: ResolvedRoute |};

const RouterContext: React.Context<?RouterState> = createContext(null);

/** The route table the application was started with. */
let installedTable: ?RouteTable = null;

/**
 * How the application navigates, installed by the entry that started it.
 *
 * Module state beside `installedTable`, and for the same reason: the entry is
 * the only thing that knows, and every component that needs the answer is
 * somewhere under a `RouterProvider` it did not construct. `routerView` builds
 * that provider from two props the server handed it, and threading a third one
 * from the entry through the application root would have made every
 * hand-written `<App>` in a test a place the default lives.
 *
 * `"client"` until something says otherwise, which is what every uf
 * application did before `app.rendering.navigation` existed and what a test
 * that renders `routerView` directly still gets.
 */
let installedNavigation: Navigation = "client";

/**
 * Say how this application navigates. Called once, by the client entry.
 *
 * `@uniflowed/vite` generates the call into `virtual:uf/client` from
 * `app.rendering.navigation`; nothing else should call it, and calling it after
 * the first render is a change no rendered `Link` will notice.
 */
export function installNavigation(navigation: Navigation): void {
  installedNavigation = navigation;
}

/** How this application navigates. */
export function navigationMode(): Navigation {
  return installedNavigation;
}

/**
 * How a page that React Server Components rendered fetches the next route's
 * payload. `hydrateFlight` in `../rsc-client.js` installs it.
 *
 * Handed in rather than imported, because this module is in every
 * application's bundle: one rendered from its modules, a single-page one, and
 * the server's. The fetch reads its answer with React's Flight client,
 * `react-server-dom-parcel`, which only an application that renders Server
 * Components installs, and which needs React 19.3 while the rest of the router
 * runs on 19.2.3 (ubugeeei-prod/uf#992). A bundler resolves every import it is
 * shown, whether or not anything calls it, so an import here would put that
 * package in every one of those bundles, or fail the build where it is absent.
 */
let installedFlightFetch: ((url: string) => Promise<FetchedFlight>) | null = null;

/** Hand the router the payload fetch. Called once, by `hydrateFlight`, before the first render. */
export function installFlightFetch(fetcher: (url: string) => Promise<FetchedFlight>): void {
  installedFlightFetch = fetcher;
}

/** The next route's payload, through the fetch `hydrateFlight` installed. */
function fetchFlight(url: string): Promise<FetchedFlight> {
  if (installedFlightFetch == null) {
    return Promise.reject(
      new Error(
        "@uniflowed/router: a page rendered from a Flight payload navigated before anything " +
          "installed the payload fetch. `hydrateFlight` from `@uniflowed/router/rsc/client` " +
          "installs it before it hydrates, so an entry that hydrates a payload has to call that.",
      ),
    );
  }
  return installedFlightFetch(url);
}

/** Register the generated route table. Called once by the client and server entries. */
export function installRoutes(table: RouteTable): void {
  installedTable = table;
}

/** The registered table, or a clear error when the entry forgot to install it. */
export function routeTable(): RouteTable {
  if (installedTable == null) {
    throw new Error(
      "@uniflowed/router: no route table is installed; start the app through `uf dev` or `uf build`",
    );
  }
  return installedTable;
}

/**
 * Props the app root receives from the client and server entries.
 *
 * One of `flight` and `initial`. A document React Server Components rendered
 * hands the root its payload, on the server and again in the browser, so both
 * sides render the same tree from the same bytes. A single-page application —
 * and a project that turned `app.rsc` off — hands it a route resolved from its
 * modules instead. See ubugeeei-prod/uf#519.
 */
export type AppProps = {|
  readonly url: string,
  readonly initial?: ResolvedRoute,
  readonly flight?: Promise<FlightRoot>,
|};

/**
 * Whether there is a document to navigate.
 *
 * Asked every time rather than answered once at module scope, and the
 * difference is not a style preference. The answer is a constant inside a
 * browser bundle and inside a server process; it is *not* a constant inside a
 * test runner, where a DOM is installed on the first render and one worker
 * serves many files out of one module registry. Latched, the first file in a
 * worker to import this module decided for every file after it whether a
 * `Link` navigates or silently does nothing — and a server-rendering test
 * imports it before any document exists. See ubugeeei-prod/uf#445.
 *
 * The cost is a `typeof` per navigation, which is a navigation.
 */
function isBrowser(): boolean {
  return typeof window !== "undefined" && typeof document !== "undefined";
}

/**
 * Provides the current route to the tree and performs navigation.
 *
 * On the server the route is fixed for the request. In the browser the
 * provider listens to history and to `Link` clicks; a navigation fetches the
 * next route's payload — or, for a route resolved from its modules, loads its
 * chunks and runs its loader — *before* committing, inside a transition, so the
 * previous page stays interactive meanwhile.
 *
 * Which of the two it does is decided by what it was started with: a Flight
 * payload is [`FlightRouter`], and a resolved route is [`ModuleRouter`].
 *
 * # Unless the application asked the browser to do it
 *
 * Under `app.rendering.navigation: "document"` every one of those sentences
 * stops being true, and the provider is still here: the tree below it still
 * reads `useRoute`, still renders `<RouteView>`, and still hydrates whatever
 * `"use client"` boundary made the document interactive. What it does not do is
 * take the link over. `navigate` hands the URL to the browser, no `popstate`
 * listener is installed, and `prefetch` — which exists to load the chunks of a
 * route this page will render — has no page to load them for.
 *
 * That is one branch rather than a second provider because the two differ in
 * what happens on a click and in nothing else. A second implementation would
 * have had to keep `resolved`, `pending`, the context and every hook that
 * reads it in step with this one, which is four things to keep in step for one
 * that actually differs.
 */
export component RouterProvider(
  url: string,
  initial?: ResolvedRoute,
  flight?: Promise<FlightRoot>,
  children: React.Node,
) {
  if (flight != null) {
    return <FlightRouter flight={flight}>{children}</FlightRouter>;
  }
  if (initial == null) {
    throw new Error(
      "@uniflowed/router: RouterProvider was given neither a Flight payload nor a resolved route " +
        "to start from. `virtual:uf/client` and `virtual:uf/server` hand it one of the two.",
    );
  }
  return (
    <ModuleRouter url={url} initial={initial}>
      {children}
    </ModuleRouter>
  );
}

/**
 * The key an intercepted navigation writes into its history entry.
 *
 * One string in `history.state` rather than the resolved route, because the
 * browser structured-clones the state and keeps it across a reload: it can hold
 * a URL and nothing with a module in it. A URL is also all the entry needs —
 * where the navigation came from, resolved again when that page is not the one
 * on screen, and the entry's own URL for what intercepted it.
 */
const INTERCEPTED_FROM = "uf:intercepted-from";

/**
 * The state a history entry for `resolved` is written with.
 *
 * `null` for a navigation nothing intercepted, which is what every entry this
 * router wrote was before interception existed.
 */
function historyStateFor(resolved: ResolvedRoute): mixed {
  const interception = resolved.interception;
  if (interception == null) {
    return null;
  }
  return { [INTERCEPTED_FROM]: interception.base.pathname + interception.base.search };
}

/** Where the history entry holding `state` was intercepted from, if it was. */
function interceptedFrom(state: mixed): ?string {
  if (state == null || typeof state !== "object" || Array.isArray(state)) {
    return null;
  }
  const from = state[INTERCEPTED_FROM];
  return typeof from === "string" ? from : null;
}

/**
 * `state` without the interception in it.
 *
 * What is left is handed back rather than cleared, because an entry's state is
 * not only this router's to write: another library may have put something
 * beside it.
 */
function withoutInterception(state: mixed): mixed {
  if (state == null || typeof state !== "object" || Array.isArray(state)) {
    return state;
  }
  const rest: { [string]: mixed } = {};
  for (const key of Object.keys(state)) {
    if (key !== INTERCEPTED_FROM) {
      rest[key] = state[key];
    }
  }
  return Object.keys(rest).length === 0 ? null : rest;
}

/**
 * The provider for a route resolved from its modules: a single-page
 * application, and a project that turned `app.rsc` off.
 *
 * It is also the provider that intercepts. Whether a navigation is intercepted
 * is a question about the slots on screen, and only a router holding a route
 * resolved from its modules has them to ask; a payload holds a rendered tree.
 * See [`resolveInterception`].
 */
component ModuleRouter(url: string, initial: ResolvedRoute, children: React.Node) {
  const [resolved, setResolved] = useState<ResolvedRoute>(initial);
  const [pending, setPending] = useState<boolean>(false);
  // Read once per render rather than per navigation: it is installed by the
  // entry before the first render and never changes after it, and a `Link`
  // that asked at click time would be asking a question whose answer decided
  // what it rendered.
  const navigation = navigationMode();
  // The route on screen, for the code that runs after a render has finished.
  //
  // State is what renders, and a closure only sees the state of the render that
  // made it: the `popstate` listener below is installed once and would go on
  // reading the first route forever, and a navigation awaits between reading
  // what is on screen and replacing it. Interception is what needs the answer —
  // whether a navigation is intercepted is a question about the page it starts
  // on — and `show` writes both in the same breath, so the two cannot disagree
  // about what was last committed.
  const shown = React.useRef<ResolvedRoute>(initial);
  const show = (next: ResolvedRoute) => {
    shown.current = next;
    setResolved(next);
  };

  const navigate = async (to: string, options?: NavigateOptions): Promise<void> => {
    if (!isBrowser()) {
      return;
    }
    const target = new URL(addressOf(to), window.location.href);
    // The application path the route table is asked about, and the address the
    // history entry keeps: one URL, with and without `app.router.basePath`.
    const applicationPath = applicationPathOf(target.pathname);
    const next = (applicationPath ?? target.pathname) + target.search;
    const address = target.pathname + target.search;
    // The browser's job in this application. `assign` and `replace` rather
    // than the history API, because the point is a document request: the
    // history entry, the scroll position, the `Referer` and the unload
    // handlers are then the browser's, done the way they are done for a link
    // in a page with no JavaScript on it at all.
    if (navigation === "document") {
      if (options?.replace === true) {
        window.location.replace(target.href);
      } else {
        window.location.assign(target.href);
      }
      return;
    }
    // Interception first, because it is a question about the page this
    // navigation starts on rather than about the one it reaches. A slot on
    // screen that intercepts the URL renders a page of its own, so whether the
    // URL's ordinary page is in this bundle — the paragraph below — is not a
    // question this navigation has to ask.
    // An address outside the base path is not this application's to render.
    if (applicationPath == null) {
      window.location.assign(target.href);
      return;
    }
    const origin = beneath(shown.current);
    const intercepting = interceptingRoutes(origin.slots, applicationPath).length > 0;
    // The half of the split that is not about bytes. A route whose page is not
    // in this bundle is not a route this router can render, and pretending
    // otherwise is the silent break: the navigation would resolve to nothing
    // and the visitor would be left on the page they clicked from. The browser
    // has the document, so the browser does the navigation — which is what a
    // link does when there is no JavaScript at all, and what the anchor
    // `Link` renders would have done on its own.
    if (!intercepting) {
      const matched = matchRoute(routeTable().routes, applicationPath);
      if (matched != null && !hasClientPage(matched.route)) {
        window.location.assign(target.href);
        return;
      }
    }
    setPending(true);
    try {
      // An interception depends on the page it starts from, so it is resolved
      // every time; any other navigation reads what this page kept while it is
      // fresh. See `./navigation-cache.js`.
      const key = navigationKey(target.pathname, target.search);
      const nextResolved =
        (intercepting ? await resolveInterception(routeTable(), origin, next) : null) ??
        (await (routeNavigations.read(key) ?? keepRoute(key, resolveMatch(routeTable(), next))));
      // An intercepted entry remembers where it was intercepted from, so back
      // and forward can put the page underneath under it again. Every other
      // entry is written the way it always was.
      const state = historyStateFor(nextResolved);
      if (options?.replace === true) {
        window.history.replaceState(state, "", address + target.hash);
      } else {
        window.history.pushState(state, "", address + target.hash);
      }
      const commit = () => {
        show(nextResolved);
        setPending(false);
      };
      if (options?.transition === false) {
        startTransition(commit);
      } else {
        withViewTransition(nextResolved.viewTransition, commit);
      }
      // An intercepted navigation leaves the page underneath where the reader
      // left it — the modal opens over the post they clicked, not over the top
      // of the feed — so it moves the window only for a caller who asks with
      // `scroll: true`. Every other navigation scrolls unless asked not to.
      const scroll =
        nextResolved.interception == null ? options?.scroll !== false : options?.scroll === true;
      if (scroll) {
        if (target.hash !== "") {
          const element = document.getElementById(target.hash.slice(1));
          if (element != null) {
            element.scrollIntoView();
            return;
          }
        }
        window.scrollTo(0, 0);
      }
    } catch (error) {
      setPending(false);
      throw error;
    }
  };

  useEffect(() => {
    if (!isBrowser()) {
      return undefined;
    }
    // An entry that says it was intercepted, under a provider that has only
    // just mounted, is an entry the browser reloaded or restored — and the
    // document on screen is what a request for its URL returned, which is the
    // ordinary page. Clearing the mark makes the entry say what the reader is
    // looking at, so coming back to it later renders this page again rather
    // than a modal over a page they never saw one on.
    const restored = window.history.state;
    if (interceptedFrom(restored) != null) {
      window.history.replaceState(withoutInterception(restored), "", window.location.href);
    }
    // Nothing pushed a history entry, so there is nothing to pop back into: a
    // document-navigating application left this page when the link was
    // followed, and the back button asks the browser for the previous document
    // rather than asking this listener to rebuild it. Installing one anyway
    // would put a `resolveMatch` on the back button of a page that is about to
    // be replaced by the one the browser already has.
    if (navigation === "document") {
      return undefined;
    }
    const arrive = (nextResolved: ResolvedRoute) => {
      // The back button is a navigation, and a navigation that animates in
      // one direction and cuts in the other would read as a bug in the
      // animation rather than as a decision.
      withViewTransition(nextResolved.viewTransition, () => {
        show(nextResolved);
      });
    };
    const onPopState = () => {
      const next =
        (applicationPathOf(window.location.pathname) ?? window.location.pathname) +
        window.location.search;
      // Back or forward into an entry an interception wrote: the page it was
      // intercepted from, with the interception over it again. That page is
      // resolved afresh only when it is not already the one underneath, so
      // back from the second photo to the first leaves the feed exactly where
      // it is.
      const from = interceptedFrom(window.history.state);
      if (from != null) {
        const underneath = beneath(shown.current);
        const origin =
          underneath.pathname + underneath.search === from
            ? Promise.resolve(underneath)
            : resolveMatch(routeTable(), from);
        origin
          .then((page) => resolveInterception(routeTable(), page, next))
          // Nothing on that page intercepts the entry's URL any more — a
          // module that will not load, a table a development server rebuilt —
          // so the entry is what its URL names.
          .then((intercepted) => intercepted ?? resolveMatch(routeTable(), next))
          .then(arrive);
        return;
      }
      // Back into a route this bundle has no page for. The history entry is
      // already the browser's — it moved before this listener ran — so the
      // document that belongs to it is what has to be fetched.
      const matched = matchRoute(
        routeTable().routes,
        applicationPathOf(window.location.pathname) ?? window.location.pathname,
      );
      if (matched != null && !hasClientPage(matched.route)) {
        window.location.reload();
        return;
      }
      const key = navigationKey(window.location.pathname, window.location.search);
      (routeNavigations.read(key) ?? keepRoute(key, resolveMatch(routeTable(), next))).then(arrive);
    };
    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, []);

  const router: Router = {
    push: (to, options) => navigate(to, options),
    replace: (to) => navigate(to, { replace: true }),
    prefetch: async (to) => {
      // A prefetch loads the modules the *next render* will need, and under
      // document navigation there is no next render in this page: the browser
      // fetches a document and throws this one away. Loading the chunks would
      // be bytes spent on a page that is leaving, so this declines rather than
      // warming a cache nothing reads.
      if (!isBrowser() || navigation === "document") {
        return;
      }
      const target = new URL(addressOf(to), window.location.href);
      const applicationPath = applicationPathOf(target.pathname);
      if (applicationPath == null) {
        return;
      }
      // What the next render will need is decided the way the navigation will
      // decide it: a URL a slot on screen intercepts renders that slot's page,
      // so that is the module worth having, and the page the URL names is not.
      const intercepting = interceptingRoutes(beneath(shown.current).slots, applicationPath);
      if (intercepting.length > 0) {
        await Promise.all(
          intercepting.flatMap((route) => [
            loadOnce(route.page),
            ...route.layouts.map((layout) => loadOnce(layout)),
          ]),
        );
        return;
      }
      const matched = matchRoute(routeTable().routes, applicationPath);
      const load = matched?.route.page;
      if (matched == null || load == null) {
        return;
      }
      // With `app.rendering.staleTime` set, the whole route: its loader runs
      // now, and the click, a later visit and the back button read what it
      // answered while it is fresh. Otherwise only the modules it will need.
      if (keepsNavigations()) {
        const key = navigationKey(target.pathname, target.search);
        await (
          routeNavigations.read(key) ??
          keepRoute(key, resolveMatch(routeTable(), applicationPath + target.search))
        );
        return;
      }
      await Promise.all([
        loadOnce(load),
        ...matched.route.layouts.map((layout) => loadOnce(layout)),
      ]);
    },
    refresh: async () => {
      if (!isBrowser()) {
        return;
      }
      // The same URL, rendered again — which under document navigation is what
      // the browser calls a reload. Resolving it in the page instead would
      // re-run the loader and commit a tree whose links this application has
      // already said it does not drive.
      if (navigation === "document") {
        window.location.reload();
        return;
      }
      // Everything a navigation kept is older than what this asks for, so none
      // of it is shown again; see `./navigation-cache.js`.
      clearNavigationCache();
      // A refresh of an intercepted page refreshes both of its halves: the
      // page underneath, resolved again for its own URL, and the interception
      // resolved again over it. Resolving only the address bar's URL would
      // close the modal, which is a navigation nobody asked for.
      const interception = shown.current.interception;
      const nextResolved =
        interception == null
          ? await resolveMatch(
              routeTable(),
              (applicationPathOf(window.location.pathname) ?? window.location.pathname) +
                window.location.search,
            )
          : ((await resolveInterception(
              routeTable(),
              await resolveMatch(
                routeTable(),
                interception.base.pathname + interception.base.search,
              ),
              interception.pathname + interception.search,
            )) ?? (await resolveMatch(routeTable(), interception.pathname + interception.search)));
      // No view transition, and it is the one place that is right: a refresh
      // is the same URL resolved again, so a transition would animate a page
      // into itself — a cross-fade between two frames of the same thing,
      // which is a flicker with a name.
      startTransition(() => {
        show(nextResolved);
      });
    },
    back: () => {
      if (isBrowser()) {
        window.history.back();
      }
    },
    forward: () => {
      if (isBrowser()) {
        window.history.forward();
      }
    },
  };

  const value: RouterState = {
    route: routeState(resolved),
    view: { kind: "modules", resolved },
    router,
    pending,
    navigation,
  };
  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

/**
 * The provider for a route React Server Components rendered.
 *
 * What it holds is the payload rather than a resolved route: `use` reads its
 * root — the route a hook reads and the tree `RouteView` renders — and a
 * navigation fetches the next route's payload and swaps the promise. The
 * browser resolves nothing and imports no page, layout or loader; the server
 * did all three, and a component that needs the browser arrived as a client
 * reference inside the tree.
 *
 * A navigation reads the next payload's root before it commits, for the reason
 * [`ModuleRouter`] resolves the next route before it commits: the page on
 * screen stays interactive while the next one is on its way, and a commit
 * inside a view transition is synchronous, so a root that had not arrived would
 * show nothing rather than the page being left. What may still suspend after
 * the commit is a `$loading.js` boundary inside the new tree, which is what that
 * file is for.
 */
component FlightRouter(flight: Promise<FlightRoot>, children: React.Node) {
  const [current, setCurrent] = useState<Promise<FlightRoot>>(flight);
  const [pending, setPending] = useState<boolean>(false);
  const root = use(current);
  // Read once per render, for the reason `ModuleRouter` reads it once.
  const navigation = navigationMode();

  const navigate = async (to: string, options?: NavigateOptions): Promise<void> => {
    if (!isBrowser()) {
      return;
    }
    // A payload URL is an address, so it keeps the base path; the server takes
    // it off.
    const target = new URL(addressOf(to), window.location.href);
    const next = target.pathname + target.search;
    // The browser's job in this application; `ModuleRouter` has the argument.
    if (navigation === "document") {
      if (options?.replace === true) {
        window.location.replace(target.href);
      } else {
        window.location.assign(target.href);
      }
      return;
    }
    setPending(true);
    try {
      // The route this page already has while it is fresh, then a prefetch
      // still in hand, then the network. See `./navigation-cache.js`.
      const key = navigationKey(target.pathname, target.search);
      const fetched = await (
        flightNavigations.read(key) ??
        takePrefetched(next) ??
        keepFlight(key, fetchFlight(next))
      );
      // Not a payload: a redirect off this origin, or a host that has no payload
      // for this URL. The browser loads it as a document, which is what the
      // anchor would have done.
      if (fetched.kind === "document") {
        window.location.assign(fetched.url);
        return;
      }
      const payload = fetched.root;
      const nextRoot = await payload;
      // The URL the payload came from, which is a redirect's target when the
      // route redirected: the history entry is where the visitor ended up.
      // In the trailing-slash policy's spelling: a payload URL names its
      // document without the slash, and the history entry should be the
      // address the server answers without a redirect.
      const arrived = new URL(fetched.url, window.location.href);
      const landed = canonicalAddress(arrived.pathname) + arrived.search + target.hash;
      if (options?.replace === true) {
        window.history.replaceState(null, "", landed);
      } else {
        window.history.pushState(null, "", landed);
      }
      const commit = () => {
        setCurrent(payload);
        setPending(false);
      };
      if (options?.transition === false) {
        startTransition(commit);
      } else {
        withViewTransition(nextRoot.route.viewTransition, commit);
      }
      if (options?.scroll !== false) {
        if (target.hash !== "") {
          const element = document.getElementById(target.hash.slice(1));
          if (element != null) {
            element.scrollIntoView();
            return;
          }
        }
        window.scrollTo(0, 0);
      }
    } catch (error) {
      setPending(false);
      throw error;
    }
  };

  useEffect(() => {
    if (!isBrowser()) {
      return undefined;
    }
    // No history entry was pushed, so there is nothing to pop back into; see
    // `ModuleRouter`.
    if (navigation === "document") {
      return undefined;
    }
    const onPopState = () => {
      const next = window.location.pathname + window.location.search;
      // The history entry already moved; a payload that cannot be had for it is
      // a document to load, and a reload is the browser's way to load it. While
      // this page keeps the route fresh, what it kept is what comes back.
      const key = navigationKey(window.location.pathname, window.location.search);
      (flightNavigations.read(key) ?? keepFlight(key, fetchFlight(next))).then(
        (fetched) => {
          if (fetched.kind === "document") {
            window.location.reload();
            return;
          }
          const payload = fetched.root;
          payload.then(
            (nextRoot) => {
              withViewTransition(nextRoot.route.viewTransition, () => {
                setCurrent(payload);
              });
            },
            () => {
              window.location.reload();
            },
          );
        },
        () => {
          window.location.reload();
        },
      );
    };
    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, []);

  const router: Router = {
    push: (to, options) => navigate(to, options),
    replace: (to) => navigate(to, { replace: true }),
    prefetch: async (to) => {
      // Under document navigation there is no next render in this page to
      // fetch a payload for; see `ModuleRouter`'s prefetch.
      if (!isBrowser() || navigation === "document") {
        return;
      }
      const target = new URL(addressOf(to), window.location.href);
      if (target.origin !== window.location.origin) {
        return;
      }
      const next = target.pathname + target.search;
      // Kept for every navigation to it while it is fresh, when a project set
      // `app.rendering.staleTime`; otherwise held for the one click after it.
      if (keepsNavigations()) {
        const key = navigationKey(target.pathname, target.search);
        await (flightNavigations.read(key) ?? keepFlight(key, fetchFlight(next)));
        return;
      }
      await prefetchFlight(next);
    },
    refresh: async () => {
      if (!isBrowser()) {
        return;
      }
      if (navigation === "document") {
        window.location.reload();
        return;
      }
      // Everything a navigation kept is older than what this asks for, so none
      // of it is shown again; see `./navigation-cache.js`.
      clearNavigationCache();
      const fetched = await keepFlight(
        navigationKey(window.location.pathname, window.location.search),
        fetchFlight(window.location.pathname + window.location.search),
      );
      if (fetched.kind === "document") {
        window.location.reload();
        return;
      }
      const payload = fetched.root;
      await payload;
      // No view transition: a refresh is the same URL rendered again. See
      // `ModuleRouter`'s refresh.
      startTransition(() => {
        setCurrent(payload);
      });
    },
    back: () => {
      if (isBrowser()) {
        window.history.back();
      }
    },
    forward: () => {
      if (isBrowser()) {
        window.history.forward();
      }
    },
  };

  const value: RouterState = {
    route: root.route,
    view: { kind: "flight", tree: root.tree },
    router,
    pending,
    navigation,
  };
  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

/**
 * Payloads a `Link` fetched on intent, kept for the navigation that follows it.
 *
 * Bounded and short-lived, because a payload is the rendering of a route at one
 * moment: one old enough to disagree with the server is one a navigation should
 * not show, and a page with a hundred links hovered over must not hold a hundred
 * renderings. Taken rather than read, so a prefetched payload serves exactly one
 * navigation and the next visit to the same URL asks again.
 */
const PREFETCH_LIMIT = 32;
const PREFETCH_LIFETIME_MS = 30000;
const prefetchedFlights: Map<
  string,
  {| readonly fetched: Promise<FetchedFlight>, readonly at: number |},
> = new Map();

/**
 * `fetched`, kept under `key` for as long as `app.rendering.staleTime` says,
 * and forgotten again if it turns out not to be a route to show twice: a
 * request that failed, an answer that was a document rather than a payload, or
 * a payload React could not read.
 */
function keepFlight(key: string, fetched: Promise<FetchedFlight>): Promise<FetchedFlight> {
  if (!keepsNavigations()) {
    return fetched;
  }
  flightNavigations.store(key, fetched);
  const forget = () => {
    flightNavigations.forget(key, fetched);
  };
  void fetched.then((answer) => {
    if (answer.kind === "document") {
      forget();
      return;
    }
    // A route that answered with its error or not-found boundary is shown this
    // once: asked again, it may have recovered.
    void answer.root.then((root) => {
      if (root.route.status !== 200) {
        forget();
      }
    }, forget);
  }, forget);
  return fetched;
}

/**
 * `resolved`, kept under `key` for as long as `app.rendering.staleTime` says,
 * and forgotten again if it did not resolve to a page: a loader that threw or
 * redirected, or a route that answered with its error or not-found boundary.
 */
function keepRoute(key: string, resolved: Promise<ResolvedRoute>): Promise<ResolvedRoute> {
  if (!keepsNavigations()) {
    return resolved;
  }
  routeNavigations.store(key, resolved);
  const forget = () => {
    routeNavigations.forget(key, resolved);
  };
  void resolved.then((route) => {
    if (route.status !== 200) {
      forget();
    }
  }, forget);
  return resolved;
}

function prefetchFlight(url: string): Promise<FetchedFlight> {
  const existing = prefetchedFlights.get(url);
  if (existing != null && Date.now() - existing.at < PREFETCH_LIFETIME_MS) {
    return existing.fetched;
  }
  if (existing == null && prefetchedFlights.size >= PREFETCH_LIMIT) {
    const oldest = prefetchedFlights.keys().next();
    if (oldest.done !== true) {
      prefetchedFlights.delete(oldest.value);
    }
  }
  const fetched = fetchFlight(url);
  prefetchedFlights.set(url, { fetched, at: Date.now() });
  fetched.catch(() => {
    prefetchedFlights.delete(url);
  });
  return fetched;
}

function takePrefetched(url: string): Promise<FetchedFlight> | null {
  const entry = prefetchedFlights.get(url);
  prefetchedFlights.delete(url);
  if (entry == null || Date.now() - entry.at >= PREFETCH_LIFETIME_MS) {
    return null;
  }
  return entry.fetched;
}

export hook useRouterState(): RouterState {
  const state = useContext(RouterContext);
  if (state == null) {
    throw new Error(
      "@uniflowed/router: this hook must be used inside the app started by `routerView`",
    );
  }
  return state;
}

/** The current route. */
export hook useRoute(): RouteInfo {
  const { route, pending } = useRouterState();
  return {
    path: route.path,
    pathname: route.pathname,
    params: route.params,
    searchParams: route.searchParams,
    data: useResolvedData(route),
    pending,
  };
}

/**
 * The loader's answer, waiting for it if the router deferred it.
 *
 * Both hooks that expose the data go through here, and both therefore suspend
 * when the answer is not in yet. That is the conservative choice rather than
 * the clever one: the alternative is handing back `undefined` for a value that
 * is on its way, which is a page reading a field that is about to exist and
 * finding nothing there, with nothing anywhere to say why.
 *
 * Suspending costs a caller *above* the innermost `<Suspense>` — a layout, a
 * masthead — the streaming it would otherwise have got, because React holds the
 * shell for a component that suspends with no boundary above it. That is
 * exactly what such a route did before the loader could be deferred at all, so
 * it is a benefit not taken rather than a regression, and it is visible: the
 * fallback does not appear.
 */
hook useResolvedData(route: RouteState): mixed {
  const loader = route.deferred;
  return loader == null ? route.data : use(loader);
}

/** Navigation. */
export hook useRouter(): Router {
  return useRouterState().router;
}

/**
 * The current page's loader data.
 *
 * `mixed`, so the page that reads it says what it is and the checker watches
 * it do so. This was `useLoaderData<T>(): T`, which looks like inference and
 * is a cast a caller writes at a distance: `useLoaderData<Post>()` asserted
 * that a loader three files away returned a `Post` and nothing anywhere
 * checked it, so a loader that changed shape produced a `Post`-shaped
 * `undefined` at the first property read rather than an error where the shape
 * was decided.
 *
 * Narrowing is a line at the top of the page — `if (typeof data !== "object"
 * || data == null) { … }`, or the page's own validator schema, which is what
 * `@uniflowed/validator` is for at exactly this boundary.
 *
 * The type that would need no narrowing is a *generated* one: the route table
 * already produces `RoutePath` and `RouteParams` from the `app/` directory
 * (`crates/uf_router/src/lib.rs`), and a loader's return type belongs in the
 * same file, keyed by route. Until it is there, this says what is true.
 */
export hook useLoaderData(): mixed {
  return useResolvedData(useRouterState().route);
}

/**
 * Whether this bundle marks the boundaries it renders.
 *
 * `import.meta.hot` is the same gate `../client.js` uses for the hydration
 * report and the DevTools check, chosen there for the reason it is chosen here:
 * Vite defines it while serving and replaces it with `undefined` in a build, so
 * every branch below is statically dead in a production bundle and the module
 * behind it — `@uniflowed/router` is `sideEffects: false` — is dropped rather
 * than shipped unused. Node leaves it undefined, so a host that imports this
 * file without a bundler gets the production path, and so does the test suite.
 *
 * It is a module constant rather than a per-render question because the branch
 * has to be foldable, and it may answer differently in the browser and on the
 * server without costing anything: a mark renders nothing until it has mounted,
 * so neither the server's markup nor the tree React hydrates against it can
 * contain one. See `./boundaries.js`, which has the argument.
 */
const BOUNDARY_MARKS: boolean = import.meta.hot != null;

/**
 * Renders the matched page inside its layouts, innermost last, with the
 * document metadata as hoistable head elements.
 *
 * The walk itself is `composeRoute` in `./compose.js`, which has the whole
 * argument for where each boundary goes. What is left here is the half that
 * reads the router: which route, the page element that carries the loader's
 * answer and the payload the browser hydrates it from, and — under `uf dev` —
 * the boundary marks and the report that watches them. See ubugeeei-prod/uf#520.
 */
export component RouteView() {
  const { view } = useRouterState();
  // A tree a server composed for React Server Components is the whole of it:
  // the boundaries, the fallbacks, the marks and the head were placed by
  // `composeRoute` on the server, before any of it was written into the payload.
  if (view.kind === "flight") {
    return view.tree;
  }
  const resolved = view.resolved;
  const loader = resolved.deferred;
  // The route's boundaries, named once and read by both the marks the
  // composition places and the report that watches them. `installedTable`
  // rather than [`routeTable`], which throws: a test may render this view
  // without an entry having installed a table, and an error boundary named by
  // its depth alone is worth less than one named by its file rather than wrong.
  const marks = BOUNDARY_MARKS
    ? routeBoundaries(
        resolved,
        nearestBoundary(installedTable?.errors ?? [], resolved.pathname)?.file,
      )
    : null;
  const page =
    loader == null ? <RenderedPage data={resolved.data} /> : <AwaitedPage loader={loader} />;
  return (
    <>
      {composeRoute(resolved, { page, marks })}
      {/* After the tree rather than before it, so its effect runs once every
          mark below has had its own — which is the commit the marks are in. */}
      {BOUNDARY_MARKS && marks != null ? (
        <BoundaryReporter path={resolved.path} boundaries={marks} />
      ) : null}
    </>
  );
}

/**
 * The page, with the loader's answer and the copy of it the browser hydrates
 * from.
 *
 * The two are rendered together because they are one fact told twice, and
 * anything that could put them out of step is a page whose first client render
 * disagrees with the document it was sent. Being one component is what keeps
 * the script in the same position in the tree on both sides — inside the
 * innermost `<Suspense>` when the route deferred its loader on the server, and
 * exactly there again on the client, where the data is already in hand and
 * nothing suspends at all.
 *
 * "The loader's answer" is now a payload rather than a value, so what
 * [`payloadElements`] renders is that script plus one boundary per value the
 * answer deferred. The same argument covers all of them: the browser's copy of
 * this component renders the same rows in the same places, from the values it
 * read out of those very elements.
 */
component RenderedPage(data: mixed) {
  const { view } = useRouterState();
  // Only ever rendered by `RouteView` for a route resolved from its modules.
  if (view.kind !== "modules") {
    return null;
  }
  const resolved = view.resolved;
  const Page = pageComponent(resolved.page);
  return (
    <>
      <Page params={resolved.params} searchParams={resolved.searchParams} data={data} />
      {payloadElements(data)}
    </>
  );
}

/**
 * The same page, once the loader the router deferred has answered.
 *
 * A component of its own rather than a `use` guarded by an `if` inside
 * [`RenderedPage`], so the call is unconditional where it is written: this one
 * is rendered only when there is a promise, and `RouteView` chooses between
 * them. `use` may legally be called conditionally, and code that reads as
 * though it may not is worth avoiding anyway.
 */
component AwaitedPage(loader: Promise<mixed>) {
  return <RenderedPage data={use(loader)} />;
}

/**
 * The loader's answer, embedded for the browser to hydrate from.
 *
 * In the tree rather than in the head, which is the third of the three options
 * ubugeeei-prod/uf#373 weighed and the only one that survives a deferred
 * loader. `server.js` wrote this into the head from the resolved route, and a
 * deferred answer does not exist when the head goes out — losing it would mean
 * every deferred route's loader running a second time in the browser, on the
 * way in, for data the document already contained.
 *
 * The two rejected options are worth naming. Writing it at the end of the body
 * from outside React would have worked — uf's client entry is a module script,
 * so it runs after parsing either way — but it would be markup inside the
 * hydration root that React did not render, which is the definition of a
 * mismatch. Emitting it through `bootstrapScriptContent` as a global is
 * React's own documented pattern and costs the one property this element has
 * that matters: `application/json` is data a browser does not execute, and a
 * script that is executed is a script a content security policy has to allow.
 *
 * `<` is escaped inside the JSON so a string holding `</script>` cannot end the
 * element early, and U+2028 and U+2029 because a JSON document is not
 * JavaScript source but is sometimes read as if it were — the escape moved to
 * `./payload.js` when the model stopped being the only thing written that way.
 * `dangerouslySetInnerHTML` rather than a text child because React escapes a
 * text child and `&quot;` is not JSON any more. `security/no-dangerously-set-
 * inner-html` is about markup that came from somewhere and has to be sanitized
 * before a browser parses it as HTML; this is `JSON.stringify`'s output with
 * `<` escaped, in an element the browser never parses as HTML and never runs.
 * `docs/app/$layout.js` carries the same suppression for the same reason.
 *
 * # And the rows the model deferred
 *
 * A promise anywhere in the loader's answer used to be `JSON.stringify`'d to
 * `{}`. It is now a `"$P<n>"` reference in the element above and a `<script
 * data-uf-row="n">` of its own, inside a `<Suspense fallback={null}>` — which
 * is what makes React stream it at the moment the promise settles rather than
 * holding the document for it. Each row is its own boundary, so two deferred
 * values arrive in the order they resolved in and not in the order they were
 * written. `./payload.js` is the format; `./payload-rows.js` is the browser
 * reading them back.
 *
 * The boundaries sit after the page rather than before it, where the data
 * element already was. A page that suspends with no `$loading.js` above it
 * holds the whole shell — that is React's rule and uf does not work around it
 * — so the position buys nothing either way, and "the scripts are where the
 * script was" is worth more than a rearrangement that is not.
 *
 * # Why the rows are inside an element
 *
 * Because a `<Suspense>` that is a direct child of the *render root* stops the
 * shell being flushed at all. React's renderer can only write a segment once
 * the segment is complete, and the root segment holds an unresolved boundary
 * open: measured against React 19.2.8, a tree of `[<div>, <Suspense>]` writes
 * its first byte when the boundary resolves, and the same tree with the
 * boundary inside any host element writes it immediately. Every component
 * between the root and here — `RenderProvider`, `RouterProvider`, `RouteView`,
 * `RouteErrorBoundary` — renders no element of its own, so without this
 * `<span>` the rows would be exactly that first shape and a payload would have
 * streamed nothing.
 *
 * `hidden` because it holds no content a reader is meant to see: `<script
 * type="application/json">` renders nothing either way, and the attribute is
 * what says so to anything that inspects the document. One element for all the
 * rows rather than one each — the boundaries inside it still resolve
 * independently, since each is its own.
 *
 * The same rule catches a route whose `$loading.js` sits above no layout, so
 * `RouteView` wraps that specific root shape in `RootStreamFrame`.
 */
function payloadElements(data: mixed): React.Node {
  if (data === undefined) {
    return null;
  }
  const { model, rows } = encodePayload(data, "the route's loader data");
  // Before anything renders, so a promise that has already rejected is one
  // somebody is listening to. `settledRow` is memoized, so the components
  // below get these same promises rather than a second set.
  for (const row of rows) {
    settledRow(row.value);
  }
  const html = { __html: payloadJson(model) };
  // uf-lint-disable-next-line security/no-dangerously-set-inner-html
  const row0 = <script id={DATA_ID} type="application/json" dangerouslySetInnerHTML={html} />;
  return (
    <>
      {row0}
      {rows.length === 0 ? null : (
        <span hidden>
          {rows.map((row) => (
            <Suspense key={row.id} fallback={null}>
              <PayloadRow id={row.id} value={row.value} />
            </Suspense>
          ))}
        </span>
      )}
    </>
  );
}

/**
 * One deferred value, written when it settles.
 *
 * Rendered on both sides, which is the thing to keep in mind about it. On the
 * server `value` is the loader's own promise; in the browser it is the promise
 * `./payload-rows.js` created for this row and resolved out of this very
 * element. Both then write the element from the settled result through the
 * same [`payloadJson`], so the bytes agree and hydration has nothing to
 * report. `encodeRowValue` is what re-applies the reference escape to a value
 * the browser has already had it removed from.
 *
 * It never rejects. `use` on a rejected promise throws, and a throw here would
 * put the *row's* boundary into the error boundary above it — which is the
 * page, for a value the page may not even be reading. The failure travels as a
 * row instead, and the page's own `use` of the same promise is what reaches
 * the page's boundary, exactly as it would have without a payload.
 */
component PayloadRow(id: number, value: Promise<mixed>) {
  const message = use(settledRow(value));
  const html = { __html: payloadJson(message) };
  return (
    // uf-lint-disable-next-line security/no-dangerously-set-inner-html
    <script type="application/json" data-uf-row={String(id)} dangerouslySetInnerHTML={html} />
  );
}

/**
 * The message a row will carry, as a promise that always fulfils.
 *
 * Keyed by the promise rather than recomputed, because `use` wants the same
 * promise every render and a render is repeated: React renders a component
 * again after it suspends, and Strict Mode renders it twice more. A `WeakMap`
 * so a route that has navigated away takes its rows with it.
 */
const settledRows: WeakMap<Promise<mixed>, Promise<PayloadRowMessage>> = new WeakMap();

function settledRow(value: Promise<mixed>): Promise<PayloadRowMessage> {
  const existing = settledRows.get(value);
  if (existing != null) {
    return existing;
  }
  const settled = value.then(
    (resolved) => ({ value: encodeRowValue(resolved, "a deferred value") }),
    (error) => ({ error: rowFailure(error) }),
  );
  settledRows.set(value, settled);
  return settled;
}

/**
 * What a row says when the value failed.
 *
 * A fixed sentence in a build, and the error's own words where
 * `import.meta.hot` says a developer is reading them — the same gate
 * [`BOUNDARY_MARKS`] uses, and the same argument: a message that came out of a
 * loader can name a table, a query or a file path, and a browser is not where
 * any of those belong.
 *
 * A `PayloadRowError` short-circuits both, and has to. That error is what the
 * browser's reader rejects with, carrying the row's own text, so echoing it is
 * what makes the element the browser renders equal the one the server sent
 * whichever of the two builds was the development one.
 */
const ROW_FAILURE = "@uniflowed/router: a deferred value failed on the server.";

function rowFailure(error: mixed): string {
  if (error instanceof PayloadRowError) {
    return error.wire;
  }
  if (!BOUNDARY_MARKS) {
    return ROW_FAILURE;
  }
  return error instanceof Error ? `${ROW_FAILURE} ${error.message}` : ROW_FAILURE;
}

/**
 * Head elements a component contributes while it is rendering.
 *
 * `metadata` and `generateMetadata` are how a *route* says what it is, and
 * both are resolved before anything renders — which is what makes them work
 * for a crawler that runs no JavaScript. They are also declarations by the
 * route module, and part of what a page has to say is decided further in: a
 * paginated list knows its `prev` and `next` in the component that draws the
 * pager, and a breadcrumb knows the trail it has just walked.
 *
 * So this returns elements rather than writing to the head. Writing would have
 * to happen in an effect, an effect does not run on a server, and the result
 * would be a page whose tags are right in a browser and missing from the
 * crawler — `packages/web/head.js` is that escape hatch and says so at the top
 * of the file. Rendering is what puts a tag in a server-rendered head, so the
 * caller renders what comes back:
 *
 *     export component Pager(page: number, of: number) {
 *       const seo = useSeo({
 *         pagination: {
 *           prev: page > 1 ? `/posts?page=${page - 1}` : undefined,
 *           next: page < of ? `/posts?page=${page + 1}` : undefined,
 *         },
 *       });
 *       return <nav className="pager">{seo}…</nav>;
 *     }
 *
 * The argument is a `Metadata` — the same type a route exports — because there
 * is one vocabulary for what a page says about itself, and a second one would
 * be a second place for it to be wrong. What this adds over rendering the tags
 * by hand is the thing a component three levels down cannot know:
 * `metadataBase`, which the root layout declared, and against which the
 * relative URLs written here are resolved.
 */
export hook useSeo(seo: Metadata): React.Node {
  const { route } = useRouterState();
  const base = seo.metadataBase ?? route.metadata.metadataBase;
  return <Head metadata={base == null ? seo : { ...seo, metadataBase: base }} />;
}

/** When a `Link` loads the route it points at. */
export type LinkPrefetch = "off" | "intent" | "render";

/**
 * A client-side navigation.
 *
 * Renders a real anchor, so the link works before hydration and for a right
 * click, and takes over only a plain left click. `prefetch="intent"` (the
 * default) loads the destination's chunks on hover or focus, and
 * `transition={false}` makes this one navigation a cut — most navigations are
 * a link, so the opt-out in [`NavigateOptions`] has to be reachable from one.
 *
 * # Under `app.rendering.navigation: "document"` it is only the anchor
 *
 * No click handler of uf's, no `preventDefault`, no prefetch listeners: the
 * element the browser gets is the one it would have got from `<a href>` in the
 * source. That is the whole of what changing the mode does to a component,
 * which is the point — a project moving between the two rewrites its
 * `uf.config.js` and none of its pages, and a component library built on
 * `Link` works in both without knowing which it is in.
 *
 * It matters that the handler is *absent* rather than a handler that calls
 * `location.assign`. The two look the same for a left click and are not the
 * same link: `preventDefault` and a scripted navigation lose `download`, lose
 * a `target`, and change what the browser does with a middle click and with a
 * gesture uf has not heard of. An ordinary link is not an approximation of an
 * ordinary link.
 */
export component Link(
  to: string,
  prefetch?: LinkPrefetch = "intent",
  replace?: boolean = false,
  transition?: boolean = true,
  children?: React.Node,
  className?: string,
  onClick?: (event: SyntheticMouseEvent<HTMLAnchorElement>) => mixed,
  ...rest: { readonly [string]: mixed }
) {
  const { router, navigation } = useRouterState();
  const prefetched = React.useRef(false);
  const drives = navigation === "client";

  const doPrefetch = () => {
    if (!drives || prefetch === "off" || prefetched.current || isExternal(to)) {
      return;
    }
    prefetched.current = true;
    router.prefetch(to).catch(() => {});
  };

  useEffect(() => {
    if (prefetch === "render") {
      doPrefetch();
    }
  });

  const handleClick = (event: SyntheticMouseEvent<HTMLAnchorElement>) => {
    if (onClick != null) {
      onClick(event);
    }
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey ||
      isExternal(to)
    ) {
      return;
    }
    event.preventDefault();
    router.push(to, { replace, transition }).catch((error) => {
      // A failed navigation falls back to the browser doing it.
      console.error(error);
      window.location.assign(addressOf(to));
    });
  };

  // The caller's own `onClick` still runs under document navigation — it is
  // theirs, and an application that closes a menu when a link is clicked is
  // not asking uf to take the navigation over — so it is passed through rather
  // than dropped with the rest of the behaviour.
  return (
    <a
      {...rest}
      // `to` is an application path; the anchor is the address, with the base
      // path in front and the trailing-slash policy's spelling, so a link that
      // works before hydration and for a right click goes where this one does.
      href={addressOf(to)}
      className={className}
      onClick={drives ? handleClick : onClick}
      onMouseEnter={drives && prefetch === "intent" ? doPrefetch : undefined}
      onFocus={drives && prefetch === "intent" ? doPrefetch : undefined}
    >
      {children}
    </a>
  );
}

function isExternal(to: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(to) || to.startsWith("//");
}

/**
 * The application root `app.js` exports: `export default routerView("./app")`.
 *
 * The argument documents where the routes live; the table itself is generated
 * from that directory at build time and installed by the entry that starts
 * the app, so the component only has to render it.
 *
 * # Why the render anchor is here
 *
 * `RenderProvider` fixes the render's instant, time zone and random seed once,
 * writes them into the markup and reads them back on the client, which is what
 * makes `useRenderedAt` and `useRandom` agree across hydration. An application
 * that did not render one got no error — it got the old behaviour, which is a
 * silent hydration mismatch in every page with a clock or a shuffle on it. A
 * guarantee that depends on remembering to opt in is not one, so the router
 * provides it and an application that wants different values *replaces* it by
 * rendering its own inside this one. See ubugeeei-prod/uf#559.
 *
 * Above `RouterProvider` rather than below it, because the route's own
 * modules — layouts as much as pages — are things that read a clock, and a
 * masthead showing the time is the first component anybody writes that does.
 *
 * It is safe above a root layout that renders `<html>` only because the
 * envelope's carrier is a `<meta>`: React hoists one into the head of a
 * document it rendered, and to the front of a tree that is not one, where uf's
 * shell lifts it into the head it wrote itself. `packages/hooks/render.js` has
 * the argument, and it is the reason the carrier is no longer a `<script>`.
 */
export function routerView(root: string): React.ComponentType<AppProps> {
  void root;
  component App(url: string, initial?: ResolvedRoute, flight?: Promise<FlightRoot>) {
    return (
      <RenderProvider>
        <RouterProvider url={url} initial={initial} flight={flight}>
          <RouteView />
        </RouterProvider>
      </RenderProvider>
    );
  }
  return App;
}

/**
 * Whether the app is being rendered on the server.
 *
 * Read through `useSyncExternalStore` so a component that branches on it
 * hydrates consistently: the server snapshot is `true`, the client one `false`.
 */
export hook useIsServer(): boolean {
  return useSyncExternalStore(
    () => () => {},
    () => false,
    () => true,
  );
}
