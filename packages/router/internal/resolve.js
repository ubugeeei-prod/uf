// @flow
//
// Internal to `@uniflowed/router`: what a URL resolves to, with no rendering in it.
//
// The route table is data and a resolved route is data — the modules a match
// loaded, the loader's answer, the merged metadata and the boundaries around the
// page — and nothing in this file puts either on a screen. That is what makes it
// the half of the router a React Server Component graph can import: that graph
// resolves `react` to the build with no `useState`, no `createContext` and no
// `Component` in it (ubugeeei-prod/uf#519), so the module that resolves a route
// may not reach any of them, even at import time.
//
// It was the top half of `./runtime.js`, which a comment on #519 measured as
// "everything above the error boundary class". Composition is `./compose.js`,
// the head is `./head.js`, the error views are `./error-view.js`, and the
// browser's binding — the provider, the hooks, navigation and `Link` — is
// what is left in `./runtime.js`.
//
// The two things here that are components are the framework's own not-found
// page, which is markup, and the placeholder an error route carries as its
// page, which `./error-view.js` owns because it has to reach the router to
// offer a retry.

import * as React from "react";

import { ResolvedErrorPage } from "./error-view.js";
import {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  UnauthorizedError,
  matchIn,
  matchRoute,
  nearestBoundary,
  parseSearch,
  routeErrorStatus,
  splitUrl,
} from "./routing.js";
import type {
  ErrorBoundary as RoutingErrorBoundary,
  LoadingRecord as RoutingLoadingRecord,
  NotFoundBoundary as RoutingNotFoundBoundary,
  RouteError,
  RouteMatch as RoutingRouteMatch,
  RouteParams,
  RouteRecord as RoutingRouteRecord,
  RouteTable as RoutingRouteTable,
  SearchParams,
  SlotRecord as RoutingSlotRecord,
  SlotRouteRecord as RoutingSlotRouteRecord,
  TemplateRecord as RoutingTemplateRecord,
} from "./routing.js";

/**
 * A component found in a route module.
 *
 * `React.ComponentType<empty>` is "some React component", and it is a claim
 * rather than a shrug. `ComponentType` is contravariant in its props — Flow's
 * library definition writes it `component(...P)` with `in P` — so `empty` is
 * the *top* of the component types: every component is one, and nothing may be
 * passed to one until a caller has said which props it is passing. That is
 * exactly what is known here. The router finds these by dynamic import, and
 * nobody has told it what a page's props are.
 *
 * It cannot be `React.ComponentType<PageRenderProps>`, the props the router
 * actually passes, because Flow's `component` syntax gives a component *exact*
 * props and a page is free to want none of them. This repository's own pages
 * and layouts are `component NotFound()` and
 * `component Layout(children: React.Node)`, and against the props the router
 * hands them that reads:
 *
 *     error[incompatible-type]: property `data`, property `params`, and
 *     property `searchParams` are extra in `PageRenderProps` but missing in
 *     `props of component NotFound`. Exact objects do not accept extra props.
 *
 * React passing a component a prop it did not declare is allowed and always
 * has been. `renderable` is the one line that says so.
 */
export type RouteComponent = React.ComponentType<empty>;

/**
 * The props `RouteView` gives the page it renders.
 *
 * The same three as the public `PageProps` in `../index.js`, at the arguments
 * the runtime instantiates it with: the runtime knows the parameters as
 * strings and the loader's data as `mixed`, and a page narrows both by
 * annotating its own props.
 */
export type PageRenderProps = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
|};

/**
 * The props `RouteView` gives each layout, outermost first.
 *
 * Inexact, and that is the parallel routes reaching the type: a layout on a
 * segment that declares `@team` is handed a `team` prop beside `children`, and
 * the names are the project's rather than this file's. Every extra prop is a
 * `React.Node` — a rendered slot, or `null` when the URL addressed neither the
 * slot's routes nor a `$default.js`.
 *
 * The exactness is not lost so much as moved: what a layout may be *given* is
 * open, and what it *declares* is still its own exact props type, which is
 * where a typo in a slot name shows up.
 */
export type LayoutRenderProps = {
  readonly params: RouteParams,
  readonly children: React.Node,
  ...
};

/** What a page module may export. The component is `default` or `Page`. */
export type PageModule = {
  readonly default?: RouteComponent,
  readonly Page?: RouteComponent,
  readonly loader?: (args: LoaderArgs) => mixed | Promise<mixed>,
  readonly metadata?: Metadata,
  readonly generateMetadata?: (args: MetadataArgs) => Metadata | Promise<Metadata>,
  readonly generateStaticParams?: () =>
    | $ReadOnlyArray<RouteParams>
    | Promise<$ReadOnlyArray<RouteParams>>,
  readonly frontmatter?: { readonly title?: string, readonly description?: string, ... },
  /**
   * What a stylesheet calls the transition this route arrives under.
   *
   * Not a switch. Every client navigation opts into a view transition where
   * the browser has one, and this is how one arrival is told from another —
   * the name reaches CSS as an attribute on the document element for as long
   * as the transition runs:
   *
   *     html[data-uf-view-transition="manual"]::view-transition-old(root) { … }
   *
   * A layout may declare one too, and then it covers every route under it; the
   * page's own wins. That is the rule `metadata` already follows, and there is
   * no reason for a second one — "the nearest declaration" is how everything
   * else in a route module is resolved.
   */
  readonly viewTransition?: string,
  ...
};

/** What a layout module may export. The component is `default` or `Layout`. */
export type LayoutModule = {
  readonly default?: RouteComponent,
  readonly Layout?: RouteComponent,
  readonly metadata?: Metadata,
  /** A transition name for every route under this layout; see [`PageModule`]. */
  readonly viewTransition?: string,
  ...
};

/**
 * What a template module may export. The component is `default` or `Template`.
 *
 * A layout's shape without its `metadata`, and the omission is the type saying
 * what a template is for. A layout persists across navigation, so a title it
 * declares is a claim about a section of the site; a template is thrown away
 * and built again on every navigation, so a title on one would be a claim
 * about nothing. Titles come from the page and the layouts above it.
 */
export type TemplateModule = {
  readonly default?: RouteComponent,
  readonly Template?: RouteComponent,
  ...
};

/**
 * What an error module may export. The component is `default` or `Error`.
 *
 * `Error` shadows the global inside the file that writes it, which is the
 * cost of naming the export after what it is; a file that needs the
 * constructor still has `globalThis.Error`. The alternative was a name the
 * convention would have to explain — `ErrorPage`, `Boundary` — for a file
 * whose whole job is already in its name.
 */
export type ErrorModule = {
  readonly default?: RouteComponent,
  readonly Error?: RouteComponent,
  readonly metadata?: Metadata,
  ...
};

/**
 * What a loading module may export. The component is `default` or `Loading`.
 *
 * No `metadata`, and that is the type saying something true rather than an
 * omission. A fallback renders while the route is still resolving, and the
 * route's metadata was decided before the first byte — a title on a file that
 * renders after the head has gone could never be used. `packages/web/head.js`
 * documents the same constraint from the other side.
 */
export type LoadingModule = {
  readonly default?: RouteComponent,
  readonly Loading?: RouteComponent,
  ...
};

/**
 * How a Twitter card is laid out, which is the whole of what `card` may be.
 *
 * A union rather than a string: every one of the four is spelled exactly this
 * way and a fifth value is silently ignored by the crawler, so a typo in it
 * costs a card and produces no error anywhere.
 */
export type TwitterCard = "summary" | "summary_large_image" | "app" | "player";

/**
 * What a crawler may do with a page.
 *
 * Four fields rather than the whole `robots` vocabulary, and the omissions are
 * the argument. `index` and `follow` are the two directives a page has an
 * opinion about; `maxSnippet` and `maxImagePreview` are the two that change
 * what a result *looks* like and have no other spelling. `nosnippet` is not
 * here because `maxSnippet: 0` is the same instruction, and a type with two
 * ways to say one thing is a type somebody will eventually ask which of them
 * wins.
 *
 * Every field is optional and every one is only emitted when it is declared,
 * because "index, follow" is what a document with no `robots` meta already
 * says — the tag exists to say something else.
 */
export type Robots = {
  readonly index?: boolean,
  readonly follow?: boolean,
  /** The longest snippet a result may quote; `0` is none, `-1` is no limit. */
  readonly maxSnippet?: number,
  readonly maxImagePreview?: "none" | "standard" | "large",
};

/**
 * One JSON-LD object, as a page hands it over.
 *
 * `mixed` values rather than a schema.org type, because there is no useful
 * middle: the vocabulary is hundreds of types deep, it grows without asking
 * anyone, and a partial transcription of it would reject correct documents far
 * more often than it caught wrong ones. What this type does claim is the part
 * uf is answerable for — that the thing is an object, and therefore that it
 * serialises into one `<script>`.
 */
export type JsonLd = { readonly [string]: mixed };

/** Document metadata a page or layout declares. */
export type Metadata = {
  readonly title?: string,
  readonly description?: string,
  /**
   * The absolute URL every other URL here is resolved against.
   *
   * Open Graph and Twitter both require absolute image URLs, and a route
   * module has no way to know the host it will be served from — so without
   * this, `openGraph.images: ["/og.png"]` ships exactly as written and is not
   * a valid `og:image`. Declare it once on the root layout and every
   * descendant inherits it through the same merge as everything else.
   *
   * Resolution is the URL standard's, so `"/og.png"` is resolved against the
   * *origin* and `"og.png"` against the base's own path — not against the
   * page's URL, which `Head` does not know.
   */
  readonly metadataBase?: string,
  /**
   * This page's canonical URL, for `<link rel="canonical">` and `og:url`.
   *
   * Relative to `metadataBase` when it is not absolute. A page
   * reachable at more than one path — a query a filter added, a duplicate
   * under a second section — is one page, and this is how it says so.
   */
  readonly canonical?: string,
  /**
   * What a crawler may do with this page. See [`Robots`].
   *
   * The one field here that is usually declared on a *layout*: a staging
   * section, a preview tree or an account area is `index: false` for
   * everything under it, and saying so once is the only version of that which
   * stays true when a page is added.
   */
  readonly robots?: Robots,
  /**
   * The other addresses this same page is published at.
   *
   * `languages` maps a BCP 47 tag to that translation's URL and becomes one
   * `<link rel="alternate" hreflang>` each. The set has to be reciprocal —
   * every page in it lists every other one *and itself*, which is what makes a
   * search engine read them as translations rather than as duplicates — so it
   * is usually the same map on every page of the set, declared on the layout
   * they share. `"x-default"` is a tag like any other here, and names what a
   * reader whose language is not in the set should be given.
   *
   * Nested under `alternates` rather than sitting at the top level as
   * `languages`, because `alternate` is the link relation and a language is
   * only one kind of alternate; the outer name is a fact about the wire rather
   * than a shape invented here.
   */
  readonly alternates?: {
    readonly languages?: { readonly [string]: string },
  },
  /**
   * The pages either side of this one in a sequence.
   *
   * `<link rel="prev">` and `<link rel="next">`, resolved against
   * `metadataBase` like every other URL here. A page four of a list, and a
   * chapter in the middle of a manual, are the same statement: this document
   * is one of a series and here is where the series continues.
   *
   * `canonical` still belongs to the page itself. Pointing every page of a
   * paginated list at page one is the mistake this pair exists to make
   * unnecessary — it tells a search engine that pages two onwards are
   * duplicates of page one, and everything only reachable from them stops
   * being reachable at all.
   */
  readonly pagination?: {
    readonly prev?: string,
    readonly next?: string,
  },
  /**
   * Structured data, as JSON-LD.
   *
   * One `<script type="application/ld+json">` per entry. Unlike everything
   * else here it *accumulates* down the tree rather than being replaced by the
   * nearest declaration: an `Organization` on the root layout and an `Article`
   * on the page are two statements about one page, not two answers to one
   * question, and replacing would mean a page that describes itself silently
   * deletes the site's description of itself.
   *
   * The scripts are rendered with the rest of the route rather than hoisted
   * into `<head>`, because React hoists `<title>`, `<meta>` and `<link>` and
   * not a script it has to keep the body of. JSON-LD is read from anywhere in
   * the document, so this costs nothing; it is worth knowing when reading the
   * markup.
   */
  readonly jsonLd?: $ReadOnlyArray<JsonLd>,
  readonly openGraph?: {
    /**
     * The title a share card shows.
     *
     * Falls back to `title`, because a page that has said what it is called
     * has said what its card is called — and a site made to write it twice
     * writes it twice once and then lets them drift.
     */
    readonly title?: string,
    /** The description a share card shows. Falls back to `description`. */
    readonly description?: string,
    /**
     * The Open Graph object type. `website` unless a page says otherwise.
     *
     * Defaulted rather than omitted because `og:type` is one of the four
     * properties Open Graph requires, and a document without it is not an
     * Open Graph document at all — so leaving it to every project to remember
     * is leaving most of them without one.
     */
    readonly type?: string,
    /**
     * The name of the site the page belongs to, which a card prints above the
     * title. Declared once on the root layout.
     */
    readonly siteName?: string,
    readonly images?: $ReadOnlyArray<string>,
    /**
     * What the card's image shows, for a reader who cannot see it.
     *
     * One description rather than one per image: a card shows one image, and
     * the array exists so a site can offer a crawler a choice of sizes rather
     * than so it can show several.
     */
    readonly imageAlt?: string,
  },
  readonly twitter?: {
    readonly card?: TwitterCard,
    readonly site?: string,
    readonly creator?: string,
    /** Falls back to `openGraph.title`, and then to `title`. */
    readonly title?: string,
    /** Falls back to `openGraph.description`, and then to `description`. */
    readonly description?: string,
    readonly images?: $ReadOnlyArray<string>,
    /** Falls back to `openGraph.imageAlt`. */
    readonly imageAlt?: string,
  },
};

/** Arguments a loader receives. */
export type LoaderArgs = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly pathname: string,
|};

/** Arguments `generateMetadata` receives. */
export type MetadataArgs = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
|};

export type RouteRecord = RoutingRouteRecord<
  PageModule,
  LayoutModule,
  TemplateModule,
  LoadingModule,
  ErrorModule,
>;

export type SlotRecord = RoutingSlotRecord<
  PageModule,
  LayoutModule,
  TemplateModule,
  LoadingModule,
  ErrorModule,
>;

export type SlotRouteRecord = RoutingSlotRouteRecord<
  PageModule,
  LayoutModule,
  TemplateModule,
  LoadingModule,
  ErrorModule,
>;

export type TemplateRecord = RoutingTemplateRecord<TemplateModule>;

export type LoadingRecord = RoutingLoadingRecord<LoadingModule>;

export type NotFoundBoundary = RoutingNotFoundBoundary<PageModule, LayoutModule>;

export type ErrorBoundary = RoutingErrorBoundary<ErrorModule, LayoutModule>;

export type RouteTable = RoutingRouteTable<
  PageModule,
  LayoutModule,
  TemplateModule,
  LoadingModule,
  ErrorModule,
>;

export type RouteMatch = RoutingRouteMatch<RouteRecord>;

export type ResolvedTemplate = {|
  readonly above: number,
  readonly module: TemplateModule,
|};

type ResolvedSlotErrorBoundary = {|
  readonly above: number,
  readonly module: ?ErrorModule,
|};

type SlotErrorBoundaryLoader = {|
  readonly above: number,
  readonly module: () => Promise<ErrorModule>,
|};

/**
 * A match whose modules are loaded and whose loader has run or is running — or,
 * when `error` is set, the error page that stands in for it.
 */
export type ResolvedRoute = {|
  readonly pathname: string,
  readonly search: string,
  readonly path: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly page: PageModule,
  readonly layouts: $ReadOnlyArray<LayoutModule>,
  /** What the loader returned, once it has. `undefined` while `deferred` is set. */
  readonly data: mixed,
  /**
   * The loader still running, when the router handed the page its promise
   * rather than its value. `null` on every other path, which is most of them.
   *
   * Two fields rather than a `data` that is sometimes a promise, because a
   * loader is free to return something with a `then` on it and no duck test
   * could tell that apart from a deferral. This one is the router's own answer
   * to a question the router asked, so it says so.
   *
   * Set only by a streaming render of a route that declares a
   * `$loading.js` and generates no metadata from its data — the two
   * conditions under which deferring buys anything and costs nothing that was
   * not already spent. [`resolveRoute`] is where that is decided and argued.
   */
  readonly deferred: ?Promise<mixed>,
  readonly metadata: Metadata,
  /**
   * What a stylesheet calls the transition this route arrives under, or `null`
   * when neither the page nor a layout above it named one.
   *
   * Resolved with the route rather than looked up at the moment of the
   * navigation, because by then the answer is a property of the destination's
   * modules and those are exactly what has just been loaded. A server render
   * carries it and never reads it; see "View transitions".
   */
  readonly viewTransition: ?string,
  readonly status: 200 | 401 | 403 | 404 | 500,
  /**
   * Set when this resolution *is* the error page: the loader threw, or the
   * server render did and the renderer resolved again. `null` on the ordinary
   * path.
   */
  readonly error: ?RouteError,
  /**
   * The boundary that would catch a throw while rendering this route.
   *
   * Always present, because every route has an answer for a throw: `module`
   * is `null` when the project declares no `$error.js` above the path, and
   * the framework's own error page renders instead. `above` is how many of
   * `layouts` are outside the boundary — the ones that stay mounted, which is
   * what "the rest of the document is still interactive" means.
   */
  readonly errorBoundary: {|
    readonly module: ?ErrorModule,
    readonly above: number,
  |},
  /**
   * The loading boundaries around this route, root first, already imported.
   *
   * Imported rather than lazy: React decides to render a fallback
   * synchronously, during the render that suspended, so a module that is still
   * being fetched is a module that is not there at the only moment it is
   * wanted. Empty for a route with no `$loading.js` above it, which is the
   * ordinary case and renders exactly the tree it did before.
   */
  readonly loading: $ReadOnlyArray<{| readonly above: number, readonly module: LoadingModule |}>,
  /**
   * The templates around this route, root first, already imported.
   *
   * Empty for a route with no `$template.js` above it, which is the
   * ordinary case and renders exactly the tree it did before templates
   * existed. Empty too on a resolution that *is* a boundary — a not-found or
   * an error page — for the reason its `loading` is: those are matched rather
   * than walked to, and templates are accumulated on the walk down to a route
   * the URL never reached.
   */
  readonly templates: $ReadOnlyArray<ResolvedTemplate>,
  /**
   * The slots this route renders, outermost first, already imported.
   *
   * Empty for a route with no slot above it, and empty on a resolution that
   * *is* a boundary — a not-found or an error page — for the reason its
   * `templates` is: a boundary is matched rather than walked to, and a slot
   * belongs to the segment the walk went through.
   */
  readonly slots: $ReadOnlyArray<ResolvedSlot>,
  /**
   * Set when a client navigation was intercepted: this is then the route the
   * navigation came from, still on screen, with the intercepting route in one
   * of its slots.
   *
   * `null` or absent on every other resolution — which includes every one a
   * server makes, because a document request is never intercepted. See
   * [`resolveInterception`].
   */
  readonly interception?: ?Interception,
|};

/**
 * What an intercepted navigation went to, and what it went there from.
 *
 * A route is resolved *for* a URL, and an intercepted navigation is the one
 * case where the URL in the address bar is not the URL the page on screen was
 * resolved for. The `ResolvedRoute` that carries this is the page the
 * navigation started on — its `pathname`, `params` and `data` are that page's,
 * because that page is what `children` still renders and what `useRoute()`
 * still describes — and this is the other half: where the address bar went.
 *
 * `base` is the same page as it was before anything intercepted it. Kept rather
 * than re-derived, because a second interception from inside the first — the
 * next photo, from a photo already open in the modal — has to start from the
 * page underneath rather than from a page that already has a modal in it, and
 * going back from the second to the first has to find that page where it left
 * it.
 */
export type Interception = {|
  /** The URL the navigation went to: the one in the address bar. */
  readonly pathname: string,
  readonly search: string,
  /** The route the navigation came from, as it was before it was intercepted. */
  readonly base: ResolvedRoute,
|};

/**
 * One slot, matched against the URL and imported.
 *
 * `page` is `null` for a slot the URL addressed and that declares no
 * `$default.js`, and the layout receives `null` rather than nothing at all:
 * a layout that declares a slot always gets that prop, so a project can write
 * `{team ?? <Empty />}` and mean it.
 *
 * `params` are the slot's own. A slot matches the same URL by its own patterns,
 * so `@team/[member]` captures `member` while the page beside it captures
 * nothing — which is the point of matching twice rather than sharing one match.
 */
export type ResolvedSlot = {|
  readonly name: string,
  readonly above: number,
  readonly page: ?PageModule,
  readonly params: RouteParams,
  readonly layouts: $ReadOnlyArray<LayoutModule>,
  readonly loading: $ReadOnlyArray<{| readonly above: number, readonly module: LoadingModule |}>,
  readonly templates: $ReadOnlyArray<ResolvedTemplate>,
  readonly errorBoundary: ?ResolvedSlotErrorBoundary,
  readonly slots: $ReadOnlyArray<ResolvedSlot>,
  /**
   * The table record this slot was resolved from.
   *
   * Carried so a client navigation can ask the slots *on screen* whether they
   * intercept where it is going. Interception is a question about the page a
   * navigation starts on, and this is that page's own answer rather than a
   * second match of the table that could arrive at different slots. Absent on
   * a slot written by hand, which then intercepts nothing.
   */
  readonly record?: SlotRecord,
  /**
   * Set when this slot renders an intercepting route, or sits inside one: the
   * URL that was intercepted.
   *
   * The slot's keys and its page's `searchParams` come from here rather than
   * from the route on screen, because the route on screen is the page the
   * navigation started on. Keying the modal's templates on that page's
   * pathname would leave the second photo mounted as the first one.
   */
  readonly intercepted?: ?InterceptedUrl,
|};

/** The URL an intercepting route was matched against. */
type InterceptedUrl = {|
  readonly pathname: string,
  readonly searchParams: SearchParams,
|};

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

const moduleCache: Map<() => Promise<mixed>, Promise<mixed>> = new Map();

export function loadOnce<T>(load: () => Promise<T>): Promise<T> {
  let pending = moduleCache.get(load);
  if (pending == null) {
    pending = load();
    moduleCache.set(load, pending);
  }
  // $FlowFixMe[incompatible-return] the cache is keyed by the loader, whose result type it stores.
  return pending;
}

/**
 * Load a match's modules and run its loader.
 *
 * `data` is what the loader returned; on the client after hydration it is the
 * value the server embedded, so the loader does not run twice for the first
 * page.
 *
 * # This resolves or redirects; it does not reject
 *
 * Everything a route can go wrong with is a route to render: no match and
 * `notFound()` are the not-found boundary, a loader that threw and
 * `forbidden()`/`unauthorized()` are the error boundary. Only `redirect()`
 * comes back out, because a redirect is a response rather than a page and the
 * caller is what has one to send.
 *
 * That guarantee is the point rather than a convenience. `hydrate` awaits this
 * before `hydrateRoot`, so a rejection there is not an error page — it is no
 * `hydrateRoot` call at all, and the document the server sent stays on screen
 * with nothing attached to it.
 *
 * # `onMatch`
 *
 * Called with the route pattern the moment the URL matches one, before any
 * module is imported and before the loader runs. It exists because the server
 * has something to do with that fact and does it too late otherwise: the route
 * a request turned out to be is what its log line carries, and a loader is
 * inside this call, so a server that recorded the route after this resolved
 * would have every line a loader wrote saying it belonged to no route.
 *
 * A callback rather than a return value because both callers already have one
 * — the pattern is on the `ResolvedRoute` this hands back — and only one of
 * them needs it *early*. The browser passes nothing and pays nothing.
 */
export async function resolveMatch(
  table: RouteTable,
  url: string,
  options?: ResolveOptions,
): Promise<ResolvedRoute> {
  try {
    return await resolveRoute(table, url, options);
  } catch (error) {
    if (error instanceof RedirectError) {
      throw error;
    }
    return resolveFailure(table, url, error);
  }
}

/** What a caller may tell [`resolveMatch`] about the resolution it wants. */
export type ResolveOptions = {|
  /** The loader's answer, already in hand — the value the server embedded. */
  readonly data?: mixed,
  /** Do not run the loader at all; `data` is the answer. */
  readonly skipLoader?: boolean,
  /**
   * Whether the caller can render a route whose loader has not answered yet.
   *
   * Only a streaming server render can, and that is the whole of why this is
   * a caller's choice rather than the router's. `createRenderer`'s `render`
   * sends a `<Suspense>` fallback now and the content when it arrives, so
   * deferring is what turns a slow loader from a delay before the first byte
   * into a fallback the reader is already looking at.
   *
   * Nothing else is in that position, and each for its own reason. `prerender`
   * writes a file, which has no first paint to improve and no reader to show a
   * fallback to. `hydrate` has the server's answer already. A client
   * navigation has a page on screen that stays interactive while the next one
   * resolves, which is the browser's version of the same idea and does not
   * need this one.
   *
   * It costs the loader its say in the response: a status is decided when the
   * shell goes out, so a deferred `notFound()` reaches the error boundary
   * rather than the 404 page, and the document is a 200. That is inherent to
   * streaming rather than a shortcut — the bytes have gone — and it is the
   * reason this is off unless a caller asks.
   */
  readonly defer?: boolean,
  /** The route pattern, the moment the URL matches one; see [`resolveMatch`]. */
  readonly onMatch?: (pattern: string) => void,
|};

async function resolveRoute(
  table: RouteTable,
  url: string,
  options?: ResolveOptions,
): Promise<ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const searchParams = parseSearch(search);
  const matched = matchRoute(table.routes, pathname);

  if (matched != null) {
    options?.onMatch?.(matched.route.path);
  }

  if (matched == null) {
    return resolveNotFound(table, pathname, search, searchParams);
  }

  const load = matched.route.page;
  if (load == null) {
    // Reachable only by asking this table to render a route it was built
    // without. `hydrate` and every navigation check `hasClientPage` first and
    // hand the URL to the browser instead, so arriving here means a caller
    // went around them — and the honest answer is to say so rather than to
    // render an empty page.
    throw new Error(
      `@uniflowed/router: ${matched.route.path} has no page in this route table; it ships no ` +
        "client JavaScript, so the browser navigates to it rather than rendering it",
    );
  }
  const [page, ...layouts] = await Promise.all([
    loadOnce(load),
    ...matched.route.layouts.map((layout) => loadOnce(layout)),
  ]);
  // Started here and awaited at the end: the boundary's module does not depend
  // on the loader, so importing it alongside costs a navigation nothing. It
  // never rejects, so an early throw below leaves no unhandled rejection.
  const boundary = resolveErrorBoundary(table, pathname, matched.route.layouts.length);
  // Started alongside for the same reason, and awaited at the end: a fallback
  // depends on nothing the loader produces.
  const loading = resolveLoading(matched.route, matched.route.layouts.length);
  const templates = resolveTemplates(matched.route, matched.route.layouts.length);
  // Started alongside and awaited at the end, for the reason the boundaries
  // are: a slot is matched against the URL and depends on nothing the loader
  // produces, so the second match and its imports overlap the first page's
  // loader rather than following it.
  const slots = resolveSlots(
    matched.route.slots ?? [],
    pathname,
    matched.route.layouts.length,
    matched.params,
  );

  // The loader, run here and awaited below — or not awaited at all.
  //
  // A page that suspends while *rendering* has always streamed; a page waiting
  // on its loader could not, because this function awaited the loader before it
  // returned and by the time React saw the tree the data was already in hand.
  // The fallback beside such a page showed for zero milliseconds, which made
  // `$loading.js` useful for the one case a page usually is not slow for.
  //
  // Two things stand in the way of simply not awaiting, and both are about the
  // document rather than about the route. Metadata goes in the head and the
  // head is written before the body, so a title computed from the data
  // genuinely cannot be deferred — that is a rule worth stating rather than a
  // limitation to hide, and it is the `generateMetadata` half of the condition
  // below. The other is that a route with no `<Suspense>` above it has nothing
  // to defer *into*: React holds the whole shell for a page that suspends with
  // no boundary, which is the same wait by another name, with an unresolved
  // promise flowing through the tree for nothing. So the loader is deferred
  // exactly when there is a boundary to defer it into.
  //
  // See ubugeeei-prod/uf#373, and `ResolveOptions.defer` for who asks.
  let data: mixed = options?.data;
  let deferred: ?Promise<mixed> = null;
  if (options?.skipLoader !== true && typeof page.loader === "function") {
    const running = page.loader({ params: matched.params, searchParams, pathname });
    const canDefer =
      options?.defer === true &&
      (matched.route.loading ?? []).length > 0 &&
      typeof page.generateMetadata !== "function";
    if (canDefer) {
      // `Promise.resolve`, because a loader may return a plain value and `use`
      // wants a promise either way. A loader that answered without waiting
      // costs one microtask and renders in the same pass.
      deferred = Promise.resolve(running);
    } else {
      data = await running;
    }
  }

  const metadata = await resolveMetadata(page, layouts, {
    params: matched.params,
    searchParams,
    data,
  });
  return {
    pathname,
    search,
    path: matched.route.path,
    params: matched.params,
    searchParams,
    page,
    layouts,
    data,
    deferred,
    metadata,
    viewTransition: resolveViewTransition(page, layouts),
    status: 200,
    error: null,
    errorBoundary: await boundary,
    loading: await loading,
    templates: await templates,
    slots: await slots,
  };
}

/**
 * The route's slots, matched against the URL and imported.
 *
 * The second matching pass parallel routes are, and it is a pass rather than a
 * branch of the first: a slot has its own patterns over the same path, so
 * `/dashboard/members` can be `[member]` to one slot, a static segment to
 * another and nothing at all to a third, at once.
 *
 * A slot that will not load renders nothing rather than taking the page with
 * it, which is the judgement `resolveTemplates` and `resolveLoading` already
 * make: a slot is a second thing beside the page, and a broken second thing
 * must not become a broken route. The entry stays in the list with `page:
 * null`, so the layout still receives the prop it declares.
 */
async function resolveSlots(
  records: $ReadOnlyArray<SlotRecord>,
  pathname: string,
  layoutCount: number,
  fallbackParams: RouteParams,
  intercepted?: ?InterceptedUrl,
): Promise<$ReadOnlyArray<ResolvedSlot>> {
  if (records.length === 0) {
    return [];
  }
  return Promise.all(
    records.map((record) =>
      resolveSlot(record, pathname, layoutCount, fallbackParams, intercepted),
    ),
  );
}

async function resolveSlot(
  record: SlotRecord,
  pathname: string,
  layoutCount: number,
  fallbackParams: RouteParams,
  intercepted?: ?InterceptedUrl,
): Promise<ResolvedSlot> {
  // Clamped exactly as a template's `above` is, and for the same reason: a
  // hand-written table, or a `(group)` between the layout and the route, can
  // leave a route with fewer layouts than the slot was declared above.
  const above = Math.min(record.above, layoutCount);
  const empty: ResolvedSlot = {
    name: record.name,
    above,
    page: null,
    params: fallbackParams,
    layouts: [],
    loading: [],
    templates: [],
    errorBoundary: null,
    slots: [],
    record,
    intercepted,
  };

  const matched = matchIn(record.routes, pathname);
  if (matched == null) {
    // The URL says nothing about this slot. `$default.js` is what it says
    // instead, and a slot that declares none renders nothing at all.
    const load = record.defaultPage;
    if (load == null) {
      return empty;
    }
    const module = await loadOrNull(load);
    if (module == null) {
      return empty;
    }
    return {
      ...empty,
      page: withoutLoader(module, record.defaultFile ?? record.name),
      errorBoundary: await resolveSlotErrorBoundary(record.defaultErrorBoundary ?? null, 0),
    };
  }

  return (await resolveSlotRoute(record, matched, above, pathname, intercepted)) ?? empty;
}

/**
 * One route inside a slot, imported: what the slot renders for a match.
 *
 * Shared by the two ways a slot comes to render a route — one of its own
 * `routes`, matched against the URL, and one of its `intercepts`, matched
 * against where a client navigation is going — so an intercepting page is
 * composed exactly the way every other slot page is: inside its own layouts,
 * fallbacks, templates and error boundary, with the slots those layouts
 * declare.
 *
 * `null` when the page or a layout will not import, and what that means is the
 * caller's to say. For a match it is an empty slot, for the reason
 * [`resolveSlots`] gives; for an interception it is no interception, and the
 * navigation goes where the URL says instead.
 */
async function resolveSlotRoute(
  record: SlotRecord,
  matched: RoutingRouteMatch<SlotRouteRecord>,
  above: number,
  pathname: string,
  intercepted: ?InterceptedUrl,
): Promise<?ResolvedSlot> {
  const route = matched.route;
  // Started together and awaited apart, so the two `await`s are not a
  // waterfall and each keeps the type its loader had.
  const pending = loadOrNull(route.page);
  const pendingLayouts = Promise.all(route.layouts.map((layout) => loadOrNull(layout)));
  const page = await pending;
  const layouts = await pendingLayouts;
  if (page == null) {
    return null;
  }
  const loaded = layouts.filter(Boolean);
  if (loaded.length !== layouts.length) {
    return null;
  }
  const loading = await resolveLoadingRecords(route.loading ?? [], loaded.length);
  const templates = await resolveTemplateRecords(route.templates ?? [], loaded.length);
  const errorBoundary = await resolveSlotErrorBoundary(route.errorBoundary ?? null, loaded.length);
  return {
    name: record.name,
    above,
    page: withoutLoader(page, route.file),
    params: matched.params,
    layouts: loaded,
    loading,
    templates,
    errorBoundary,
    // The slot's own layouts are what a nested slot is measured against, so
    // the count handed down is this slot's rather than the route's. A slot
    // nested inside an interception is matched against the intercepted URL,
    // and keyed on it, for the same reason the interception is.
    slots: await resolveSlots(route.slots, pathname, loaded.length, matched.params, intercepted),
    record,
    intercepted,
  };
}

// ---------------------------------------------------------------------------
// Interception
// ---------------------------------------------------------------------------

/**
 * The page underneath: `resolved` itself, or what it was before an interception
 * put something in one of its slots.
 *
 * Every question about where a navigation *starts* is asked of this rather than
 * of the route on screen, because an interception is not a place a navigation
 * can start from. The next photo, opened from inside the modal, is intercepted
 * from the feed.
 */
export function beneath(resolved: ResolvedRoute): ResolvedRoute {
  return resolved.interception?.base ?? resolved;
}

/**
 * The intercepting routes the slots on screen have for `pathname`.
 *
 * Only the slots on screen, and that is the whole of what "a navigation from
 * inside `/feed`" means. A page under `app/feed/$layout.js` renders the layout
 * that declares `@modal`, so the slot is in its tree and so are the slot's
 * `intercepts`. A page outside that segment has no such slot in its tree —
 * which is why a link to `/feed/photo/1` from `/about` is an ordinary
 * navigation to the photo page, not an interception with nowhere to render.
 *
 * A slot that intercepts `pathname` is not looked inside: what it holds is
 * about to be replaced, nested slots and all.
 */
export function interceptingRoutes(
  slots: $ReadOnlyArray<ResolvedSlot>,
  pathname: string,
): $ReadOnlyArray<SlotRouteRecord> {
  const found: Array<SlotRouteRecord> = [];
  for (const slot of slots) {
    const matched = matchIn(slot.record?.intercepts ?? [], pathname);
    if (matched != null) {
      found.push(matched.route);
    } else {
      found.push(...interceptingRoutes(slot.slots, pathname));
    }
  }
  return found;
}

/**
 * `base`, with every slot on it that intercepts `url` rendering what it
 * intercepts — or `null` when none of them does.
 *
 * # What stays, and what does not
 *
 * Everything that is not an intercepting slot stays exactly as it was, and that
 * is the feature rather than a shortcut. `children` goes on rendering the page
 * the reader navigated *from* — its data, its scroll position, whatever state
 * its components are holding — and every other slot keeps what it was showing.
 * An intercepted navigation changes the address bar and the slots that
 * intercept it, and nothing else. Matching the rest against the new URL would
 * be an ordinary navigation with a modal on top of it: the page underneath
 * swapped for the page the URL names, which is precisely what interception
 * exists not to do.
 *
 * Every slot that intercepts the URL renders it, not only the first, because
 * slots are independent of each other: two named places may each have
 * something to show for one URL, the way two slots each match one URL by their
 * own routes.
 *
 * # Never for a document
 *
 * A document request for an intercepted URL resolves the ordinary page,
 * because a request carries where it is going and not what was on screen when
 * it was made — which is what a reload, a shared link and a crawler all are.
 * A Flight payload request may carry the page the browser is navigating from,
 * and the React Server Components renderer calls this for that request only.
 *
 * # When the interception cannot render
 *
 * A slot whose intercepting page will not import keeps what it had, and when no
 * slot could render the interception this answers `null`: the navigation goes
 * ahead as an ordinary one, and the reader gets the page the URL names — what a
 * reload would have given them — rather than a click that did nothing. An
 * intercepting page that exports a `loader`, which no slot page may, is the
 * error boundary for the URL, the way any other slot page's is.
 */
export async function resolveInterception(
  table: RouteTable,
  base: ResolvedRoute,
  url: string,
): Promise<?ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const intercepted: InterceptedUrl = { pathname, searchParams: parseSearch(search) };
  // The first slot that renders the interception, for the transition's name.
  let first: ?ResolvedSlot = null;
  const visit = (slots: $ReadOnlyArray<ResolvedSlot>): Promise<$ReadOnlyArray<ResolvedSlot>> =>
    Promise.all(
      slots.map(async (slot): Promise<ResolvedSlot> => {
        const record = slot.record;
        const matched = record == null ? null : matchIn(record.intercepts ?? [], pathname);
        if (record == null || matched == null) {
          return slot.slots.length === 0 ? slot : { ...slot, slots: await visit(slot.slots) };
        }
        const rendered = await resolveSlotRoute(record, matched, slot.above, pathname, intercepted);
        if (rendered == null) {
          return slot;
        }
        first = first ?? rendered;
        return rendered;
      }),
    );

  let slots: $ReadOnlyArray<ResolvedSlot>;
  try {
    slots = await visit(base.slots);
  } catch (error) {
    return resolveFailure(table, url, error);
  }
  if (first == null) {
    return null;
  }
  return {
    ...base,
    slots,
    // The intercepting page's own name, where it or a layout inside the slot
    // declares one, so a stylesheet can tell a modal opening from a page
    // arriving. The page underneath has not moved, so its name would say
    // nothing about this arrival.
    viewTransition: resolveViewTransition(first.page ?? {}, first.layouts),
    interception: { pathname, search, base },
  };
}

async function resolveSlotErrorBoundary(
  boundary: ?SlotErrorBoundaryLoader,
  layoutCount: number,
): Promise<?ResolvedSlotErrorBoundary> {
  if (boundary == null) {
    return null;
  }
  const above = Math.min(boundary.above, layoutCount);
  try {
    return { module: await loadOnce(boundary.module), above };
  } catch {
    // Keep the declared depth even when the custom file fails to import. The
    // framework fallback still contains the slot instead of escalating the
    // page beside it.
    return { module: null, above };
  }
}

/**
 * A module, or `null` when it would not import.
 *
 * The judgement [`resolveTemplates`] and [`resolveLoading`] already make, at
 * the granularity a slot needs it: a slot is a second thing beside the page, so
 * a slot whose module is missing renders nothing rather than taking the route
 * down with it — and the import error surfaces where it belongs, the next time
 * the module is asked for.
 */
async function loadOrNull<TModule>(load: () => Promise<TModule>): Promise<?TModule> {
  try {
    return await loadOnce(load);
  } catch {
    return null;
  }
}

/**
 * The same page module, having said out loud that a slot's loader does not run.
 *
 * A slot page is a component. It is *not* handed data, and this throws rather
 * than passing `undefined` to a page that asked for some, because a slot whose
 * loader is quietly skipped is exactly the failure ubugeeei-prod/uf#267 is
 * about — a file written to a convention, and nothing that reads it.
 *
 * Why not run it. A page's loader answer is embedded in the document for the
 * browser to hydrate from, once, under one id; a slot's would have nowhere to
 * go, so it would run on the server and again in the browser on the way in.
 * That is not merely two fetches: a loader that reads `cookies()` succeeds on
 * the server and throws in the browser, and the slot would render on one side
 * and not the other — a hydration mismatch produced by the router. So the rule
 * is the narrow one, and lifting it means embedding per-slot data, which is
 * named in the issue as what is left.
 */
function withoutLoader(module: PageModule, file: string): PageModule {
  if (typeof module.loader === "function") {
    throw new Error(
      `@uniflowed/router: ${file} is inside a \`@slot\` and exports a \`loader\`, which the ` +
        "router does not run — a slot's data has nowhere to be embedded for hydration, so it " +
        "would be fetched again in the browser and a server-only loader would render one tree " +
        "on the server and another in the page. Fetch inside the component, or move the data to " +
        "the page the URL names. https://github.com/ubugeeei-prod/uf/issues/267",
    );
  }
  return module;
}

/**
 * The route's templates, imported.
 *
 * A template that will not load is dropped, the way a fallback is: it is a
 * wrapper around the page, not the page, so a broken wrapper must not become a
 * broken route. The tree renders without it — the page keeps the layout it was
 * inside, and loses only the remount — and the import error surfaces where it
 * belongs, when the module is next asked for.
 */
async function resolveTemplates(
  route: RouteRecord,
  layoutCount: number,
): Promise<$ReadOnlyArray<ResolvedTemplate>> {
  return resolveTemplateRecords(route.templates ?? [], layoutCount);
}

async function resolveTemplateRecords(
  records: $ReadOnlyArray<TemplateRecord>,
  layoutCount: number,
): Promise<$ReadOnlyArray<ResolvedTemplate>> {
  if (records.length === 0) {
    return [];
  }
  const loaded = await Promise.all(
    records.map(async (record) => {
      try {
        return {
          // Clamped exactly as the error and loading boundaries' are: a
          // `(group)` directory can leave a route with fewer layouts than the
          // template declared above it.
          above: Math.min(record.above, layoutCount),
          module: await loadOnce(record.module),
        };
      } catch {
        return null;
      }
    }),
  );
  return loaded.filter(Boolean);
}

/**
 * The route's loading boundaries, imported.
 *
 * A boundary whose module will not load is dropped rather than thrown for, and
 * this is the same judgement `resolveErrorBoundary` makes one function above: a
 * fallback is what the router shows while it does not yet have the page, so a
 * broken fallback must not become a broken page. The route renders without that
 * boundary — the next one out, or the shell, waits for it instead — and the
 * import error surfaces where it belongs, when the module is next asked for.
 */
async function resolveLoading(
  route: RouteRecord,
  layoutCount: number,
): Promise<$ReadOnlyArray<{| readonly above: number, readonly module: LoadingModule |}>> {
  return resolveLoadingRecords(route.loading ?? [], layoutCount);
}

async function resolveLoadingRecords(
  records: $ReadOnlyArray<LoadingRecord>,
  layoutCount: number,
): Promise<$ReadOnlyArray<{| readonly above: number, readonly module: LoadingModule |}>> {
  if (records.length === 0) {
    return [];
  }
  const loaded = await Promise.all(
    records.map(async (record) => {
      try {
        return {
          // Clamped exactly as the error boundary's is, and for the same
          // reason: a `(group)` directory can leave a route with fewer layouts
          // than the boundary that covers it.
          above: Math.min(record.above, layoutCount),
          module: await loadOnce(record.module),
        };
      } catch {
        return null;
      }
    }),
  );
  return loaded.filter(Boolean);
}

/**
 * The route to render after something threw.
 *
 * Two callers, one behaviour: [`resolveMatch`] when a loader or a module
 * import threw, and `createRenderer` when the *render* did — React's error
 * boundaries do not run in `renderToString`, so the server has to catch it
 * itself and resolve again.
 */
export async function resolveFailure(
  table: RouteTable,
  url: string,
  error: mixed,
): Promise<ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const searchParams = parseSearch(search);
  if (error instanceof NotFoundError) {
    try {
      return await resolveNotFound(table, pathname, search, searchParams);
    } catch (failure) {
      // The not-found page itself would not load. Falling through to the error
      // boundary rather than rethrowing is what keeps the promise above: the
      // page a project wrote to explain a 404 is not more load-bearing than
      // the document staying on screen.
      return resolveError(table, pathname, search, searchParams, routeErrorFor(failure));
    }
  }
  return resolveError(table, pathname, search, searchParams, routeErrorFor(error));
}

/** What a thrown value means to the router. */
export function routeErrorFor(error: mixed): RouteError {
  if (error instanceof UnauthorizedError) {
    return { kind: "unauthorized" };
  }
  if (error instanceof ForbiddenError) {
    return { kind: "forbidden" };
  }
  return { kind: "thrown", error };
}

/**
 * The error boundary a route renders inside, loaded with the route rather than
 * when it is needed.
 *
 * React decides to show a boundary's fallback synchronously, during the render
 * that threw. A module that still has to be imported is a module that is not
 * there at the only moment it can be used, so this is one more dynamic import
 * per navigation and not a lazy one.
 *
 * `above` is the boundary's own layout count, clamped to the route's. The
 * first attempt compared the two layout arrays for a shared prefix, which is
 * more precise when a `(group)` directory puts a boundary beside a route
 * rather than above it — and it worked by *reference identity* of the loader
 * functions, which holds only because `routesModuleSource` deduplicates them
 * by file. A rule that depends on an invisible property of the generated
 * module is a rule that reads as zero the moment a table is built any other
 * way, and it did: it put the boundary outside the layouts it was written
 * inside. Nesting a boundary per group needs parallel-route trees (#267);
 * until then this is the honest approximation, and it is stated rather than
 * inferred.
 */
async function resolveErrorBoundary(
  table: RouteTable,
  pathname: string,
  layoutCount: number,
): Promise<{| readonly module: ?ErrorModule, readonly above: number |}> {
  const boundary = nearestBoundary(table.errors, pathname);
  if (boundary == null) {
    return { module: null, above: 0 };
  }
  // Clamped, because a route group can leave a route with fewer layouts than
  // the boundary covering it, and an `above` past the end would compose the
  // layouts out of nothing.
  const above = Math.min(boundary.layouts.length, layoutCount);
  const load = boundary.module;
  // The synthesised root record, which names layouts and no module: the
  // framework's page renders, and `above` still says where — inside the site's
  // own layouts rather than outside everything. See [`NotFoundBoundary`]`.page`.
  if (load == null) {
    return { module: null, above };
  }
  try {
    return { module: await loadOnce(load), above };
  } catch {
    // A boundary whose module will not load cannot be the answer to a throw,
    // and this is why the field is nullable: containment must not itself
    // depend on an import working. The depth is kept, because the layouts the
    // boundary named are still there and the framework's page is better inside
    // them than outside them.
    return { module: null, above };
  }
}

/**
 * The error page for `pathname`, inside the layouts above the boundary that
 * answers it.
 *
 * The layouts are the boundary's, for the same reason [`resolveNotFound`]
 * gives: they are what stays mounted around the error, and the layouts below
 * the boundary belong to the subtree that just stopped.
 */
async function resolveError(
  table: RouteTable,
  pathname: string,
  search: string,
  searchParams: SearchParams,
  routeError: RouteError,
): Promise<ResolvedRoute> {
  const boundary = nearestBoundary(table.errors, pathname);
  let module: ?ErrorModule = null;
  let layouts: $ReadOnlyArray<LayoutModule> = [];
  if (boundary != null) {
    const load = boundary.module;
    try {
      // The layouts whether or not there is a module, because the synthesised
      // root record has layouts and no module and its whole purpose is that
      // the framework's error page renders inside them: a site whose root
      // layout owns the masthead and the stylesheet answered a 500 with
      // neither. See ubugeeei-prod/uf#351.
      layouts = await Promise.all(boundary.layouts.map((layout) => loadOnce(layout)));
      module = load == null ? null : await loadOnce(load);
    } catch {
      // See `resolveErrorBoundary`: the framework's own page answers instead.
      module = null;
      layouts = [];
    }
  }

  const declared = await resolveMetadata(
    module?.metadata != null ? { metadata: module.metadata } : {},
    layouts,
    { params: {}, searchParams, data: undefined },
  );
  return {
    pathname,
    search,
    path: "*",
    params: {},
    searchParams,
    page: { default: ResolvedErrorPage },
    layouts,
    data: undefined,
    deferred: null,
    metadata: declared.title != null ? declared : { ...declared, title: errorTitle(routeError) },
    // The boundary's own layouts may name one; the page cannot, because the
    // page here is this module's. An error arriving under the section's
    // transition is the same answer as a page arriving under it.
    viewTransition: resolveViewTransition({}, layouts),
    status: routeErrorStatus(routeError),
    error: routeError,
    // All of the boundary's layouts are above it, and no inner boundary is
    // inserted around a page that already is one; see `RouteView`.
    errorBoundary: { module, above: layouts.length },
    // An error page has nothing left to wait for: it renders the value it was
    // resolved with. A fallback around it would be a boundary that can never
    // show, which is worse than none.
    loading: [],
    templates: [],
    // And slots for the third time: a slot belongs to the segment the walk went
    // through, and an error page is matched rather than walked to. A layout
    // that declares one is still mounted above the boundary, holding the slot
    // it was rendered with — the boundary replaces what is under it.
    slots: [],
  };
}

/**
 * The not-found page for `pathname`, inside the layouts above the boundary
 * that answers it.
 *
 * The layouts are the *boundary's*, not the ones the URL had already matched.
 * Taking the matched route's layouts was the other candidate and it is wrong
 * in both directions: for an unmatched URL there is no matched route to take
 * them from, and for `notFound()` thrown from a page they would keep the
 * layouts *below* the boundary — so `app/guide/[slug]/$layout.js` would
 * wrap a 404 that `app/guide/$not-found.js` answered, which is the layout
 * of the page that just said it does not exist.
 *
 * # The record with no page
 *
 * A project that declares no `$not-found.js` anywhere still has a record —
 * the one the build synthesises for the router root — and it names the root's
 * layouts and no module. Before that record existed this function answered
 * with `layouts: []`, so a site whose root layout owns the masthead, the
 * stylesheet and often `<html>` itself answered an unmatched URL with a white
 * page carrying `404` and no way to leave it. That was not the nearest-ancestor
 * rule failing; it was the fallback having no record to take layouts from, and
 * giving it one is the whole of ubugeeei-prod/uf#351.
 *
 * The framework's page then merges its title over the layouts' metadata like
 * any page would, so a `metadataBase` or an `og:site_name` declared on the root
 * layout still applies to the 404.
 */
async function resolveNotFound(
  table: RouteTable,
  pathname: string,
  search: string,
  searchParams: SearchParams,
): Promise<ResolvedRoute> {
  const record = nearestBoundary(table.notFound, pathname);
  const load = record?.page;
  const [page, ...layouts] = await Promise.all([
    load == null
      ? Promise.resolve<PageModule>({ default: DefaultNotFound, metadata: { title: "Not found" } })
      : loadOnce(load),
    ...(record?.layouts ?? []).map((layout) => loadOnce(layout)),
  ]);
  const metadata = await resolveMetadata(page, layouts, {
    params: {},
    searchParams,
    data: undefined,
  });
  return {
    pathname,
    search,
    path: "*",
    params: {},
    searchParams,
    page,
    layouts,
    data: undefined,
    deferred: null,
    metadata,
    viewTransition: resolveViewTransition(page, layouts),
    status: 404,
    error: null,
    // A not-found page is a page: one that throws is contained like any other.
    errorBoundary: await resolveErrorBoundary(table, pathname, layouts.length),
    // A not-found boundary is matched, not nested: `nearestBoundary` picked one
    // record and the loading files are a property of the route that was walked
    // to, which this URL never reached. Nothing to wait for, so no boundary.
    loading: [],
    // Templates are accumulated on that same walk, and for the same reason.
    templates: [],
    // Slots too: a URL that matched no route addressed no slot either.
    slots: [],
  };
}

/**
 * The route's metadata: each declaration merged over the ones outside it.
 *
 * Per key, so a page that declares only `canonical` keeps the title its layout
 * set — with one exception, and it is deliberate. `jsonLd` is gathered along
 * the way instead of merged, because a nearer declaration of it is an addition
 * rather than a correction; [`Metadata`] has the argument.
 */
async function resolveMetadata(
  page: PageModule,
  layouts: $ReadOnlyArray<LayoutModule>,
  args: MetadataArgs,
): Promise<Metadata> {
  let merged: Metadata = {};
  let structured: $ReadOnlyArray<JsonLd> = [];
  const take = (declared: Metadata) => {
    if (declared.jsonLd != null) {
      structured = [...structured, ...declared.jsonLd];
    }
    merged = { ...merged, ...declared };
  };

  for (const layout of layouts) {
    if (layout.metadata != null) {
      take(layout.metadata);
    }
  }
  if (page.frontmatter != null) {
    const { title, description } = page.frontmatter;
    merged = {
      ...merged,
      ...(title != null ? { title } : {}),
      ...(description != null ? { description } : {}),
    };
  }
  if (page.metadata != null) {
    take(page.metadata);
  }
  if (typeof page.generateMetadata === "function") {
    take(await page.generateMetadata(args));
  }
  return structured.length === 0 ? merged : { ...merged, jsonLd: structured };
}

/** A module that may name the transition its route arrives under. */
type Transitioning = { readonly viewTransition?: string, ... };

/**
 * What a stylesheet calls this route's arrival: the nearest declaration wins.
 *
 * The same walk `resolveMetadata` does one function above, and stated as its
 * own function rather than folded into that one because the two answer
 * different questions and only one of them is a document. Layouts are root
 * first, so overwriting as it descends leaves the innermost, and the page has
 * the last word.
 *
 * The parameters say what is read rather than naming `PageModule` and
 * `LayoutModule`, which is the shape `nearestBoundary` already takes for the
 * same reason: this reads one optional field, so requiring the whole of either
 * type would be a claim it does not need and cannot use.
 */
function resolveViewTransition(
  page: Transitioning,
  layouts: $ReadOnlyArray<Transitioning>,
): ?string {
  let name: ?string = null;
  for (const layout of layouts) {
    if (layout.viewTransition != null) {
      name = layout.viewTransition;
    }
  }
  return page.viewTransition ?? name;
}

component DefaultNotFound() {
  return (
    <main>
      <title>Not found</title>
      <h1>404</h1>
      <p>This page does not exist.</p>
    </main>
  );
}

/** The document title an error page gets when nothing declared one. */
export function errorTitle(error: RouteError): string {
  return match (error) {
    {kind: "unauthorized"} => "Sign in required",
    {kind: "forbidden"} => "Not allowed",
    {kind: "thrown"} => "Something went wrong",
  };
}

/**
 * A route module's component, as the router is about to render it.
 *
 * # The one cast in this file, and why it is here rather than in six places
 *
 * A `RouteComponent` is a component about whose props nothing was claimed, and
 * `RouteView` is about to pass it three. React allows that — a component
 * receives the props its parent wrote and ignores the ones it did not declare
 * — but Flow cannot be told it: a page's props are exact, so no props type but
 * that page's own is assignable, and the router does not know which page it
 * has. `React.ComponentType<any>` on the module types was this same
 * unsoundness spread over six declarations, where it also stopped anyone from
 * checking that `RouteView` passes the props a page is documented to receive.
 * Here it is one line, and everything on either side of it is checked: what a
 * module may export, and what a page is handed. Suppressed by name so that
 * `check:lib` can gate CI without this file being the thing that stops it; the
 * directive names the rule, and this is the argument for escaping it.
 */
export function renderable<TProps extends { ... }>(
  component: RouteComponent,
): React.ComponentType<TProps> {
  // uf-lint-disable-next-line flow/unclear-type
  return component as any;
}
