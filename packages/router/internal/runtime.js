// @flow
//
// The router runtime: matching, loading, navigation, and the React binding.
//
// A route table is data — the virtual module `virtual:uf/routes` that
// `@uniflowed/vite` generates from the `app/` directory — and this module is
// everything that turns it into a running application. The same code runs on
// the server (`./server.js` renders one URL) and in the browser (`./client.js`
// hydrates it and then navigates), so a page's loader, layouts and metadata
// resolve identically in both places.

import * as React from "react";
import {
  Suspense,
  createContext,
  startTransition,
  use,
  useCallback,
  useContext,
  useEffect,
  useMemo,
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

// The id of the script the loader data is embedded in. It moved out of the
// head and into the tree with ubugeeei-prod/uf#373 — see [`loaderDataScript`]
// — so the module that renders it is this one rather than `../server.js`.
import { DATA_ID } from "./document.js";

/** One parameter a route path captures. */
export type RouteParamSpec = {| readonly name: string, readonly catchAll: boolean |};

/** The parameters captured from a URL. A catch-all captures the rest as a list. */
export type RouteParams = { readonly [string]: string | $ReadOnlyArray<string> };

/** The query string, as a read-only map. */
export type SearchParams = { readonly [string]: string };

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
type RouteComponent = React.ComponentType<empty>;

/**
 * The props `RouteView` gives the page it renders.
 *
 * The same three as the public `PageProps` in `../index.js`, at the arguments
 * the runtime instantiates it with: the runtime knows the parameters as
 * strings and the loader's data as `mixed`, and a page narrows both by
 * annotating its own props.
 */
type PageRenderProps = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
|};

/** The props `RouteView` gives each layout, outermost first. */
type LayoutRenderProps = {|
  readonly params: RouteParams,
  readonly children: React.Node,
|};

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

/** One entry of the generated route table. */
export type RouteRecord = {|
  readonly path: string,
  readonly params: $ReadOnlyArray<RouteParamSpec>,
  readonly mdx: boolean,
  readonly file: string,
  /**
   * The page module — absent when this table cannot render the route.
   *
   * The server's table always has one: the server renders every route. The
   * browser's may not. `@uniflowed/vite` leaves the page out of the client
   * route table when uf's server-component analysis finds no `"use client"`
   * boundary reachable from the page, its layouts or its fallbacks, and with
   * the `import()` gone so is the whole subtree it reached — which is the
   * point of leaving it out.
   *
   * The route stays in the table because the router still has to *match* the
   * URL. Matching is what tells a `Link` that the destination is a document
   * the browser must fetch rather than a page this bundle can render; a route
   * missing from the table entirely would be a 404 instead. See
   * [`hasClientPage`], which is the question every caller asks.
   */
  readonly page?: () => Promise<PageModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
  /**
   * The `<Suspense>` boundaries this route renders inside, root first.
   *
   * Optional because a table written before `_uf.loading.js` existed — a
   * hand-written one in a test, a server bundle built by an older `uf` —
   * is still a table this router can render, and a route with no boundary is
   * exactly what it had before.
   */
  readonly loading?: $ReadOnlyArray<LoadingRecord>,
  /**
   * The `_uf.template.js` wrappers this route renders inside, root first.
   *
   * Optional for the reason `loading` is: a table written before templates
   * existed is still a table this router can render, and a route with no
   * template renders exactly the tree it did before.
   */
  readonly templates?: $ReadOnlyArray<TemplateRecord>,
|};

/**
 * One `_uf.template.js`, as the route table carries it.
 *
 * The same shape as [`LoadingRecord`] and the same `above`, because it answers
 * the same question — where in the stack of layouts this thing sits — and
 * there is no second vocabulary for it.
 */
export type TemplateRecord = {|
  readonly above: number,
  readonly module: () => Promise<TemplateModule>,
|};

/**
 * One `_uf.loading.js`, as the route table carries it.
 *
 * `above` is how many of the route's `layouts` are outside the boundary, which
 * is the same number `ResolvedRoute["errorBoundary"].above` means and is
 * spelled the same way on purpose: both answer "where in the stack of layouts
 * does this thing sit", and there is no second vocabulary for it.
 */
export type LoadingRecord = {|
  readonly above: number,
  readonly module: () => Promise<LoadingModule>,
|};

/**
 * One not-found boundary: the page for a path under `path` that matched
 * nothing.
 *
 * `_uf.not-found.js` is a segment file, so `path` is the route path of the
 * directory that declares it and `layouts` are the layouts in scope *there* —
 * which is what the boundary renders inside. A project with one at the router
 * root has one of these; a project whose manual answers its own 404 has two.
 */
export type NotFoundBoundary = {|
  readonly path: string,
  readonly mdx: boolean,
  readonly file: string,
  /**
   * The page this boundary renders — `null` for the one the build synthesises
   * at the router root when a project declares no `_uf.not-found.js` there.
   *
   * A project that had declared none used to get `layouts: []` along with the
   * framework's page: not the nearest-ancestor rule failing, but the fallback
   * having no record to take layouts from. So the root's layouts are a record
   * like any other, with the framework's component in place of a module to
   * import — which is a nullable field rather than a second kind of answer, and
   * is why an unmatched URL still arrives inside the site's own masthead. See
   * ubugeeei-prod/uf#351.
   */
  readonly page: ?() => Promise<PageModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/**
 * One error boundary: what renders in place of the subtree under `path` when
 * something in it throws.
 *
 * The same nearest-ancestor shape as [`NotFoundBoundary`], and `layouts` means
 * the same thing — the layouts in scope where the file is, which stay mounted
 * around the error and are why the rest of the document is still there.
 */
export type ErrorBoundary = {|
  readonly path: string,
  readonly file: string,
  /** `null` for the synthesised root record; see [`NotFoundBoundary`]`.page`. */
  readonly module: ?() => Promise<ErrorModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/**
 * A route table plus the boundaries declared under it.
 *
 * `errors` is the error boundaries a project declared, not failures that
 * happened.
 */
export type RouteTable = {|
  readonly routes: $ReadOnlyArray<RouteRecord>,
  readonly notFound: $ReadOnlyArray<NotFoundBoundary>,
  readonly errors: $ReadOnlyArray<ErrorBoundary>,
|};

/** A URL matched against the table. */
export type RouteMatch = {|
  readonly route: RouteRecord,
  readonly params: RouteParams,
|};

/**
 * Why the router is rendering an error boundary instead of a page.
 *
 * One union rather than one file convention per status. `forbidden()` and
 * `unauthorized()` are not different *kinds* of file to write; they are
 * different sentences an error page says, and `match` over this is where a
 * page says all three and the checker confirms it covered them. Deciding it
 * the other way — `_uf.forbidden.js` and `_uf.unauthorized.js` beside
 * `_uf.error.js`, which is what Next.js does — is three files per segment to
 * express one thing, and nothing would check that any of them handled the
 * case it was named for.
 *
 * The thrown value is carried but deliberately not rendered by the default
 * boundary: a server exception's message is written for the person who
 * deployed the application, not for whoever asks for the page.
 */
export type RouteError =
  | {| readonly kind: "thrown", readonly error: mixed |}
  | {| readonly kind: "unauthorized" |}
  | {| readonly kind: "forbidden" |};

/** The status a `RouteError` answers with. */
export function routeErrorStatus(error: RouteError): 401 | 403 | 500 {
  return match (error) {
    {kind: "unauthorized"} => 401,
    {kind: "forbidden"} => 403,
    {kind: "thrown"} => 500,
  };
}

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
   * `_uf.loading.js` and generates no metadata from its data — the two
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
   * is `null` when the project declares no `_uf.error.js` above the path, and
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
   * wanted. Empty for a route with no `_uf.loading.js` above it, which is the
   * ordinary case and renders exactly the tree it did before.
   */
  readonly loading: $ReadOnlyArray<{| readonly above: number, readonly module: LoadingModule |}>,
  /**
   * The templates around this route, root first, already imported.
   *
   * Empty for a route with no `_uf.template.js` above it, which is the
   * ordinary case and renders exactly the tree it did before templates
   * existed. Empty too on a resolution that *is* a boundary — a not-found or
   * an error page — for the reason its `loading` is: those are matched rather
   * than walked to, and templates are accumulated on the walk down to a route
   * the URL never reached.
   */
  readonly templates: $ReadOnlyArray<{|
    readonly above: number,
    readonly module: TemplateModule,
  |}>,
|};

/** Thrown by `notFound()`; the renderer answers with the not-found page. */
export class NotFoundError extends Error {
  constructor() {
    super("not found");
    this.name = "NotFoundError";
  }
}

/** Thrown by `unauthorized()`; the renderer answers with the error boundary. */
export class UnauthorizedError extends Error {
  constructor() {
    super("unauthorized");
    this.name = "UnauthorizedError";
  }
}

/** Thrown by `forbidden()`; the renderer answers with the error boundary. */
export class ForbiddenError extends Error {
  constructor() {
    super("forbidden");
    this.name = "ForbiddenError";
  }
}

/** Thrown by `redirect()`; the renderer answers with a redirect. */
export class RedirectError extends Error {
  to: string;
  permanent: boolean;

  constructor(to: string, permanent: boolean) {
    super(`redirect to ${to}`);
    this.name = "RedirectError";
    this.to = to;
    this.permanent = permanent;
  }
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

type Segment =
  | {| readonly kind: "static", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "catchAll", readonly name: string |};

function compile(routePath: string): $ReadOnlyArray<Segment> {
  return routePath
    .split("/")
    .filter((segment) => segment !== "")
    .map((segment): Segment => {
      if (segment.startsWith(":") && segment.endsWith("*")) {
        return { kind: "catchAll", name: segment.slice(1, -1) };
      }
      if (segment.startsWith(":")) {
        return { kind: "param", name: segment.slice(1) };
      }
      return { kind: "static", value: segment };
    });
}

/**
 * How specific a route is, for ranking: a static segment outranks a parameter,
 * which outranks a catch-all, and a longer path outranks a shorter one.
 */
function specificity(segments: $ReadOnlyArray<Segment>): number {
  let score = 0;
  for (const segment of segments) {
    score += match (segment) {
      {kind: "static"} => 3,
      {kind: "param"} => 2,
      {kind: "catchAll"} => 1,
    };
  }
  return score;
}

function matchSegments(
  segments: $ReadOnlyArray<Segment>,
  parts: $ReadOnlyArray<string>,
): ?RouteParams {
  const params: { [string]: string | $ReadOnlyArray<string> } = {};
  let index = 0;
  for (const segment of segments) {
    match (segment) {
      {kind: "static", value: const value} => {
        if (parts[index] !== value) {
          return null;
        }
        index += 1;
      }
      {kind: "param", name: const name} => {
        if (index >= parts.length) {
          return null;
        }
        params[name] = decodeSegment(parts[index]);
        index += 1;
      }
      {kind: "catchAll", name: const name} => {
        params[name] = parts.slice(index).map(decodeSegment);
        index = parts.length;
      }
    }
  }
  return index === parts.length ? params : null;
}

function decodeSegment(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

/**
 * Whether this table can render the route in the browser.
 *
 * False only in the client bundle, and only for a route uf decided ships no
 * JavaScript. Every caller that would load a page asks this first, and the two
 * answers are different actions rather than a success and a failure: render
 * it, or let the browser fetch the document.
 */
export function hasClientPage(route: RouteRecord): boolean {
  return route.page != null;
}

/**
 * Match a pathname against the table, preferring the most specific route.
 */
export function matchRoute(routes: $ReadOnlyArray<RouteRecord>, pathname: string): ?RouteMatch {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?RouteMatch = null;
  let bestScore = -1;
  for (const route of routes) {
    const segments = compile(route.path);
    const params = matchSegments(segments, parts);
    if (params == null) {
      continue;
    }
    const score = specificity(segments);
    if (score > bestScore) {
      best = { route, params };
      bestScore = score;
    }
  }
  return best;
}

/**
 * Whether a boundary declared at `segments` is at or above `parts`.
 *
 * The same segment kinds as [`matchSegments`], stopping when the boundary's
 * own segments run out instead of requiring the path to: `/guide` covers
 * `/guide/nope`, and `/guide` covers `/guide` itself.
 */
function covers(segments: $ReadOnlyArray<Segment>, parts: $ReadOnlyArray<string>): boolean {
  let index = 0;
  for (const segment of segments) {
    const next = match (segment) {
      {kind: "static", value: const value} => parts[index] === value ? index + 1 : -1,
      {kind: "param"} => index < parts.length ? index + 1 : -1,
      {kind: "catchAll"} => parts.length,
    };
    if (next === -1) {
      return false;
    }
    index = next;
  }
  return true;
}

/**
 * The nearest boundary above `pathname`, or `null` when none covers it.
 *
 * The one rule both `_uf.not-found.js` and `_uf.error.js` are resolved by, and
 * the same one layouts already follow: nearest means the longest path that
 * covers the URL. It is decided here rather than by the table's order — the
 * table is sorted by path so the generated module is stable, and a resolver
 * that read "nearest" as "first" would silently depend on that sort. Two
 * boundaries can share a path (a route group's directory does not appear in
 * the URL), and then the first in the table wins.
 */
function nearestBoundary<TBoundary: { readonly path: string, ... }>(
  boundaries: $ReadOnlyArray<TBoundary>,
  pathname: string,
): ?TBoundary {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?TBoundary = null;
  let bestDepth = -1;
  for (const boundary of boundaries) {
    const segments = compile(boundary.path);
    if (!covers(segments, parts)) {
      continue;
    }
    if (segments.length > bestDepth) {
      best = boundary;
      bestDepth = segments.length;
    }
  }
  return best;
}

/** Split a URL into its pathname and search string. */
export function splitUrl(url: string): {| readonly pathname: string, readonly search: string |} {
  const hash = url.indexOf("#");
  const withoutHash = hash === -1 ? url : url.slice(0, hash);
  const question = withoutHash.indexOf("?");
  if (question === -1) {
    return { pathname: normalizePathname(withoutHash), search: "" };
  }
  return {
    pathname: normalizePathname(withoutHash.slice(0, question)),
    search: withoutHash.slice(question),
  };
}

function normalizePathname(pathname: string): string {
  if (pathname === "" || pathname === "/") {
    return "/";
  }
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed === "" ? "/" : trimmed;
}

/** Parse a search string into a flat map; a repeated key keeps its last value. */
export function parseSearch(search: string): SearchParams {
  const params: { [string]: string } = {};
  for (const [key, value] of new URLSearchParams(search)) {
    params[key] = value;
  }
  return params;
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

const moduleCache: Map<() => Promise<mixed>, Promise<mixed>> = new Map();

function loadOnce<T>(load: () => Promise<T>): Promise<T> {
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
|};

async function resolveRoute(
  table: RouteTable,
  url: string,
  options?: ResolveOptions,
): Promise<ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const searchParams = parseSearch(search);
  const matched = matchRoute(table.routes, pathname);

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

  // The loader, run here and awaited below — or not awaited at all.
  //
  // A page that suspends while *rendering* has always streamed; a page waiting
  // on its loader could not, because this function awaited the loader before it
  // returned and by the time React saw the tree the data was already in hand.
  // The fallback beside such a page showed for zero milliseconds, which made
  // `_uf.loading.js` useful for the one case a page usually is not slow for.
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
  };
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
): Promise<$ReadOnlyArray<{| readonly above: number, readonly module: TemplateModule |}>> {
  const records = route.templates ?? [];
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
  const records = route.loading ?? [];
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
function routeErrorFor(error: mixed): RouteError {
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
 * layouts *below* the boundary — so `app/guide/[slug]/_uf.layout.js` would
 * wrap a 404 that `app/guide/_uf.not-found.js` answered, which is the layout
 * of the page that just said it does not exist.
 *
 * # The record with no page
 *
 * A project that declares no `_uf.not-found.js` anywhere still has a record —
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
function errorTitle(error: RouteError): string {
  return match (error) {
    {kind: "unauthorized"} => "Sign in required",
    {kind: "forbidden"} => "Not allowed",
    {kind: "thrown"} => "Something went wrong",
  };
}

/**
 * The framework's error page, for a project that declares no `_uf.error.js`.
 *
 * It says which of the three happened and offers the reset, and it does *not*
 * print the thrown error: on the server that message is written for whoever
 * deployed the application — a query, a path, a token in a stack — and this
 * markup is sent to whoever asked for the page. `uf dev` reports the throw in
 * the terminal and `uf build` fails the route, which are the places the person
 * who can act on it is looking.
 */
component DefaultRouteError(error: RouteError, reset: () => void) {
  const title = errorTitle(error);
  const detail = match (error) {
    {kind: "unauthorized"} => "This page needs you to be signed in.",
    {kind: "forbidden"} => "You do not have access to this page.",
    {kind: "thrown"} => "This page could not be rendered.",
  };
  return (
    <main>
      <title>{title}</title>
      <h1>{title}</h1>
      <p>{detail}</p>
      <button type="button" onClick={reset}>
        Try again
      </button>
    </main>
  );
}

/** The component an error module renders: `default`, or the named `Error`. */
function errorComponent(module: ErrorModule): React.ComponentType<ErrorRenderProps> {
  const component = module.default ?? module.Error;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: an error module must export a component as `default` or `Error`",
    );
  }
  return renderable(component);
}

/** The props an error boundary's component receives. */
type ErrorRenderProps = {|
  readonly error: RouteError,
  readonly reset: () => void,
|};

/**
 * The error UI, from whichever module is in scope.
 *
 * One component for both ways in — the class boundary below, which catches a
 * throw while the browser renders, and `ResolvedErrorPage`, which is what the
 * server renders because React's boundaries do not run in `renderToString`.
 * Two paths to the same screen is exactly the pair that drifts.
 */
component RouteErrorView(module: ?ErrorModule, error: RouteError, reset: () => void) {
  if (module == null) {
    return <DefaultRouteError error={error} reset={reset} />;
  }
  const Boundary = errorComponent(module);
  return <Boundary error={error} reset={reset} />;
}

/**
 * The page of a route that resolved to an error.
 *
 * A resolved error route carries the error and the module on the route itself,
 * so this is a static component rather than a closure the resolver builds:
 * `RouteView` composes it in its layouts exactly like a page, which is what
 * makes "inside the layouts above the boundary" one code path and not two.
 *
 * `reset()` here is `router.refresh()` — this route resolved to an error
 * because a loader or an import threw, so re-running the resolution is what
 * trying again means. On the server `refresh` does nothing, which is correct:
 * a static render has nothing to re-run.
 */
component ResolvedErrorPage() {
  const { resolved, router } = useRouterState();
  const reset = useCallback(() => {
    router.refresh().catch(() => {});
  }, [router]);

  if (resolved.error == null) {
    // Unreachable: this module is only ever the page of a resolved error route.
    return null;
  }
  return (
    <RouteErrorView module={resolved.errorBoundary.module} error={resolved.error} reset={reset} />
  );
}

type RouteErrorBoundaryProps = {|
  readonly module: ?ErrorModule,
  readonly resetKey: string,
  readonly children: React.Node,
|};

type RouteErrorBoundaryState = {| readonly error: ?RouteError |};

/**
 * The boundary that catches a throw while the browser renders the subtree.
 *
 * A class, because `getDerivedStateFromError` is React's contract for this and
 * there is no hook that does it — this is the one place in the router where
 * following React's public contract means not using a function component.
 *
 * Recovering on navigation is `componentDidUpdate` watching `resetKey`, not
 * `key={pathname}` on the boundary. Keying it remounts the subtree on *every*
 * navigation, error or not, and everything below the boundary goes with it —
 * which is the layouts, whose whole purpose is to survive navigation with
 * their scroll position and their open sections intact.
 */
class RouteErrorBoundary extends React.Component<RouteErrorBoundaryProps, RouteErrorBoundaryState> {
  constructor(props: RouteErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: mixed): RouteErrorBoundaryState {
    return { error: routeErrorFor(error) };
  }

  componentDidUpdate(previous: RouteErrorBoundaryProps) {
    if (this.state.error != null && previous.resetKey !== this.props.resetKey) {
      this.setState({ error: null });
    }
  }

  render(): React.Node {
    const { error } = this.state;
    if (error == null) {
      return this.props.children;
    }
    return (
      <RouteErrorView
        module={this.props.module}
        error={error}
        reset={() => this.setState({ error: null })}
      />
    );
  }
}

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
// rendering* shows its `_uf.loading.js` fallback instead of leaving the
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

type RouterState = {|
  readonly resolved: ResolvedRoute,
  readonly router: Router,
  readonly pending: boolean,
|};

const RouterContext: React.Context<?RouterState> = createContext(null);

/** The route table the application was started with. */
let installedTable: ?RouteTable = null;

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

/** Props the app root receives from the client and server entries. */
export type AppProps = {|
  readonly url: string,
  readonly initial: ResolvedRoute,
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
 * provider listens to history and to `Link` clicks; a navigation resolves the
 * next route (loading its chunks and running its loader) *before* committing,
 * inside a transition, so the previous page stays interactive meanwhile.
 */
export component RouterProvider(url: string, initial: ResolvedRoute, children: React.Node) {
  const [resolved, setResolved] = useState<ResolvedRoute>(initial);
  const [pending, setPending] = useState<boolean>(false);

  const navigate = useCallback(async (to: string, options?: NavigateOptions): Promise<void> => {
    if (!isBrowser()) {
      return;
    }
    const target = new URL(to, window.location.href);
    const next = target.pathname + target.search;
    // The half of the split that is not about bytes. A route whose page is not
    // in this bundle is not a route this router can render, and pretending
    // otherwise is the silent break: the navigation would resolve to nothing
    // and the visitor would be left on the page they clicked from. The browser
    // has the document, so the browser does the navigation — which is what a
    // link does when there is no JavaScript at all, and what the anchor
    // `Link` renders would have done on its own.
    const matched = matchRoute(routeTable().routes, target.pathname);
    if (matched != null && !hasClientPage(matched.route)) {
      window.location.assign(target.href);
      return;
    }
    setPending(true);
    try {
      const nextResolved = await resolveMatch(routeTable(), next);
      if (options?.replace === true) {
        window.history.replaceState(null, "", next + target.hash);
      } else {
        window.history.pushState(null, "", next + target.hash);
      }
      const commit = () => {
        setResolved(nextResolved);
        setPending(false);
      };
      if (options?.transition === false) {
        startTransition(commit);
      } else {
        withViewTransition(nextResolved.viewTransition, commit);
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
  }, []);

  useEffect(() => {
    if (!isBrowser()) {
      return undefined;
    }
    const onPopState = () => {
      const next = window.location.pathname + window.location.search;
      // Back into a route this bundle has no page for. The history entry is
      // already the browser's — it moved before this listener ran — so the
      // document that belongs to it is what has to be fetched.
      const matched = matchRoute(routeTable().routes, window.location.pathname);
      if (matched != null && !hasClientPage(matched.route)) {
        window.location.reload();
        return;
      }
      resolveMatch(routeTable(), next).then((nextResolved) => {
        // The back button is a navigation, and a navigation that animates in
        // one direction and cuts in the other would read as a bug in the
        // animation rather than as a decision.
        withViewTransition(nextResolved.viewTransition, () => {
          setResolved(nextResolved);
        });
      });
    };
    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, []);

  const router = useMemo<Router>(
    () => ({
      push: (to, options) => navigate(to, options),
      replace: (to) => navigate(to, { replace: true }),
      prefetch: async (to) => {
        if (!isBrowser()) {
          return;
        }
        const target = new URL(to, window.location.href);
        const matched = matchRoute(routeTable().routes, target.pathname);
        const load = matched?.route.page;
        if (matched == null || load == null) {
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
        const nextResolved = await resolveMatch(
          routeTable(),
          window.location.pathname + window.location.search,
        );
        // No view transition, and it is the one place that is right: a refresh
        // is the same URL resolved again, so a transition would animate a page
        // into itself — a cross-fade between two frames of the same thing,
        // which is a flicker with a name.
        startTransition(() => {
          setResolved(nextResolved);
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
    }),
    [navigate],
  );

  const value = useMemo<RouterState>(
    () => ({ resolved, router, pending }),
    [resolved, router, pending],
  );
  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

hook useRouterState(): RouterState {
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
  const { resolved, pending } = useRouterState();
  return {
    path: resolved.path,
    pathname: resolved.pathname,
    params: resolved.params,
    searchParams: resolved.searchParams,
    data: useResolvedData(resolved),
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
hook useResolvedData(resolved: ResolvedRoute): mixed {
  const loader = resolved.deferred;
  return loader == null ? resolved.data : use(loader);
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
  return useResolvedData(useRouterState().resolved);
}

/**
 * Renders the matched page inside its layouts, innermost last, with the
 * document metadata as hoistable head elements.
 *
 * # One walk down the layouts, not three
 *
 * The layouts, the error boundary and the `<Suspense>` boundaries all have to
 * be threaded into the same stack at the depth each was declared at, so this
 * is one descending loop over that depth rather than a pass per kind. `depth`
 * counts the layouts still *outside* the element built so far, which is what
 * `above` means on both a route's `errorBoundary` and each of its `loading`
 * entries — one number, one meaning, one place it is compared.
 *
 * # Where the error boundaries go
 *
 * Two, and they are not the same thing twice. The inner one is the project's
 * `_uf.error.js`, placed at the depth the file sits at, so the layouts above
 * it stay mounted and interactive while the subtree below is replaced — that
 * placement *is* the feature. The outer one has no module and so renders the
 * framework's page; it is what stands between a throw in a root layout, or in
 * the error component itself, and an unmounted document. A single boundary
 * cannot be both: put it outside and a page's throw takes the navigation down
 * with it; put it inside and nothing catches the layout above.
 *
 * # Where the loading boundaries go
 *
 * Inside the layout of the segment that declared the file and outside
 * everything under it, which is what makes the shell arrive first: a renderer
 * streaming this tree can send every layout down to the boundary, and the
 * fallback, before whatever the page is waiting for has resolved. A segment
 * with no `_uf.loading.js` contributes no boundary at all — it is not wrapped
 * in a `<Suspense fallback={null}>` on the way past — so a project that
 * declares none renders the tree it rendered before this existed, and a page
 * that suspends without a boundary above it still fails the way React says it
 * should rather than silently rendering nothing.
 *
 * The error boundary goes *outside* the fallback at the same depth. A throw
 * while the page is resolving has to reach a boundary that is still mounted,
 * and the `<Suspense>` is part of what the throw came out of.
 *
 * # Where the templates go
 *
 * Inside their own segment's layout and outside everything else at that depth
 * — the error boundary, the fallback and the page — which is what makes a
 * template's remount mean "this segment and what is under it" and a layout's
 * persistence mean "this segment's frame". The two files are the same wrapper
 * with opposite answers to one question, so they are one line apart here, and
 * the whole of the difference is the `key` — see [`insideTemplates`], which is
 * that line's other half.
 */
export component RouteView() {
  const { resolved } = useRouterState();
  const { module, above } = resolved.errorBoundary;
  const loader = resolved.deferred;
  // The innermost element, so the `use` inside `AwaitedPage` suspends below
  // every boundary the loop below adds — which is what makes the layouts and
  // the fallback the shell rather than something waiting behind the loader.
  let element: React.Node =
    loader == null ? <RenderedPage data={resolved.data} /> : <AwaitedPage loader={loader} />;

  for (let depth = resolved.layouts.length; depth >= 0; depth -= 1) {
    // Backwards over a root-first list, so the deepest segment's fallback ends
    // up closest to the page. Two segments land on the same depth whenever the
    // inner one declares no layout of its own, and then this order is the only
    // thing that keeps them nested the way the directories are.
    for (let index = resolved.loading.length - 1; index >= 0; index -= 1) {
      const boundary = resolved.loading[index];
      if (boundary.above !== depth) {
        continue;
      }
      const Fallback = loadingComponent(boundary.module);
      element = <Suspense fallback={<Fallback />}>{element}</Suspense>;
    }
    // Placed on `above` alone, and not on there being a module: a `null` one is
    // the framework's own error page, and where it renders is exactly the
    // question ubugeeei-prod/uf#351 asks. A project that declares no
    // `_uf.error.js` has the record the build synthesises for the router root,
    // whose `above` is the root's layouts — so the framework's page appears
    // inside the masthead rather than in place of the document. A table with no
    // record at all answers 0, which puts this boundary outside every layout,
    // where the outer one below already stood.
    //
    // Not around a route that already resolved to its error page: that page is
    // the boundary's own component, and wrapping it in the same boundary would
    // answer a throw inside it with itself.
    if (depth === above && resolved.error == null) {
      element = (
        <RouteErrorBoundary module={module} resetKey={resolved.pathname}>
          {element}
        </RouteErrorBoundary>
      );
    }
    element = insideTemplates(element, resolved, depth);
    if (depth > 0) {
      const Layout = layoutComponent(resolved.layouts[depth - 1]);
      element = <Layout params={resolved.params}>{element}</Layout>;
    }
  }
  return (
    <>
      <Head metadata={resolved.metadata} />
      <RouteErrorBoundary module={null} resetKey={resolved.pathname}>
        {element}
      </RouteErrorBoundary>
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
 */
component RenderedPage(data: mixed) {
  const { resolved } = useRouterState();
  const Page = pageComponent(resolved.page);
  return (
    <>
      <Page params={resolved.params} searchParams={resolved.searchParams} data={data} />
      {loaderDataScript(data)}
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
 * JavaScript source but is sometimes read as if it were.
 * `dangerouslySetInnerHTML` rather than a text child because React escapes a
 * text child and `&quot;` is not JSON any more. `security/no-dangerously-set-
 * inner-html` is about markup that came from somewhere and has to be sanitized
 * before a browser parses it as HTML; this is `JSON.stringify`'s output with
 * `<` escaped, in an element the browser never parses as HTML and never runs.
 * `docs/app/_uf.layout.js` carries the same suppression for the same reason.
 */
function loaderDataScript(data: mixed): React.Node {
  if (data === undefined) {
    return null;
  }
  const json = JSON.stringify(data)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
  const html = { __html: json };
  // uf-lint-disable-next-line security/no-dangerously-set-inner-html
  return <script id={DATA_ID} type="application/json" dangerouslySetInnerHTML={html} />;
}

/**
 * The component a page module renders: its default export, or the named
 * `Page` that `uf create` scaffolds. An MDX page always has a default export.
 */
function pageComponent(module: PageModule): React.ComponentType<PageRenderProps> {
  const component = module.default ?? module.Page;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a page module must export a component as `default` or `Page`",
    );
  }
  return renderable(component);
}

/**
 * The component a loading module renders: `default`, or the named `Loading`.
 *
 * No props, unlike a page or a layout. A fallback is what the router shows
 * when it does not have the route's answer yet, so there is nothing it could
 * be handed that would be true — not `data`, which is the thing being waited
 * for, and not `children`, because it renders instead of them.
 */
function loadingComponent(module: LoadingModule): React.ComponentType<{||}> {
  const component = module.default ?? module.Loading;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a loading module must export a component as `default` or `Loading`",
    );
  }
  return renderable(component);
}

/**
 * `element`, wrapped in every template declared at `depth`.
 *
 * Outside the boundaries at that depth and inside the layout below it, and
 * backwards over a root-first list for the reason the fallbacks are: two
 * segments share a depth whenever the inner one declares no layout, and this
 * order is what keeps them nested the way the directories are.
 *
 * A function beside `RouteView` rather than a third loop inside it, and that
 * is not only for reading: a third nested loop assigning to `element` is what
 * the React Compiler's aliasing inference gave up on, and a component it
 * cannot compile is a component it does not memoise.
 */
function insideTemplates(element: React.Node, resolved: ResolvedRoute, depth: number): React.Node {
  let out = element;
  for (let index = resolved.templates.length - 1; index >= 0; index -= 1) {
    const entry = resolved.templates[index];
    if (entry.above !== depth) {
      continue;
    }
    const Template = templateComponent(entry.module);
    // Keyed on the pathname, which is the whole difference between this file
    // and `_uf.layout.js`: React throws the subtree away and builds it again
    // whenever the key changes, and a navigation that changes only the query
    // string leaves it alone.
    out = (
      <Template key={resolved.pathname} params={resolved.params}>
        {out}
      </Template>
    );
  }
  return out;
}

/**
 * The component a template module renders: `default`, or the named `Template`.
 *
 * The same props a layout receives, because it is a layout in every way but
 * one: it wraps `children`, it may read the route's parameters, and the only
 * difference is that `RouteView` gives the element a `key` so React builds it
 * again on every navigation.
 */
function templateComponent(module: TemplateModule): React.ComponentType<LayoutRenderProps> {
  const component = module.default ?? module.Template;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a template module must export a component as `default` or `Template`",
    );
  }
  return renderable(component);
}

/** The component a layout module renders: `default`, or the named `Layout`. */
function layoutComponent(module: LayoutModule): React.ComponentType<LayoutRenderProps> {
  const component = module.default ?? module.Layout;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a layout module must export a component as `default` or `Layout`",
    );
  }
  return renderable(component);
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
function renderable<TProps extends { ... }>(
  component: RouteComponent,
): React.ComponentType<TProps> {
  // uf-lint-disable-next-line flow/unclear-type
  return component as any;
}

/**
 * One URL from a route's metadata, made absolute if it can be.
 *
 * Open Graph, Twitter and `rel="canonical"` all want an absolute URL, and a
 * route module cannot know the host it is served from — so `metadataBase` is
 * how a site says it once, and this is where it is applied.
 *
 * Three things it deliberately does not do. It does not resolve against the
 * *page's* URL: `Head` renders inside the route and does not know it, and a
 * `metadataBase` is a site-wide fact rather than a per-page one. It does not
 * invent a base: with none declared the value is emitted exactly as written,
 * which is what every page that predates this field already gets. And it does
 * not throw — a `metadataBase` that is not a URL is a mistake in one field,
 * and turning it into a blank page would be a worse answer than an unresolved
 * `og:image`.
 */
function absoluteUrl(value: string, base: void | string): string {
  if (base == null) return value;
  try {
    return new URL(value, base).href;
  } catch {
    return value;
  }
}

/**
 * The `robots` directives, as one `content` string, or `null` for none.
 *
 * `null` rather than an empty string, so a page that declared nothing gets no
 * tag at all: "index, follow" is what a document with no `robots` meta already
 * means, and writing it out tells a crawler what it had already assumed.
 *
 * Each declared field contributes its directive and no field implies another.
 * `index: true` therefore emits `index` rather than nothing — the value is
 * there to overrule a section that said otherwise, and a directive that
 * disappeared because it agreed with the default would be a page saying
 * something and no evidence of it in the markup.
 */
function robotsContent(robots: void | Robots): ?string {
  if (robots == null) {
    return null;
  }
  const directives: Array<string> = [];
  if (robots.index != null) {
    directives.push(robots.index ? "index" : "noindex");
  }
  if (robots.follow != null) {
    directives.push(robots.follow ? "follow" : "nofollow");
  }
  if (robots.maxSnippet != null) {
    directives.push(`max-snippet:${robots.maxSnippet}`);
  }
  if (robots.maxImagePreview != null) {
    directives.push(`max-image-preview:${robots.maxImagePreview}`);
  }
  return directives.length === 0 ? null : directives.join(", ");
}

/**
 * One JSON-LD object as the text of a `<script>`.
 *
 * `<` is escaped so a string inside the data holding `</script>` cannot end
 * the element early — the same escape `server.js` applies to the embedded
 * loader data, and for the same reason: the text is the application's and the
 * element it lands in is terminated by a character sequence rather than by a
 * length. `dataScript` also escapes U+2028 and U+2029; those are about a
 * string being parsed as JavaScript source, and this one never is.
 */
function jsonLdText(entry: JsonLd): string {
  return JSON.stringify(entry).replace(/</g, "\\u003c");
}

/**
 * One JSON-LD object, as the element that carries it.
 *
 * A function rather than an element written inline, because the suppression
 * needs a line of its own; `docs/app/_uf.layout.js` has the same shape for the
 * same reason. `security/no-dangerously-set-inner-html` is about markup that
 * came from somewhere and has to be sanitized before a browser parses it as
 * HTML, and its escape hatch is a `@uniflowed/markdown` sanitizer — the right
 * answer for markup and no answer at all for JSON. This string is
 * `JSON.stringify`'s output with `<` escaped, so nothing in it can close the
 * element, and it is never parsed as HTML. There is also no other spelling:
 * React escapes a text child, so `{"@type":"Article"}` would reach the page as
 * `&quot;@type&quot;`, which is not JSON-LD any more.
 */
function jsonLdScript(entry: JsonLd): React.Node {
  const text = jsonLdText(entry);
  const html = { __html: text };
  // uf-lint-disable-next-line security/no-dangerously-set-inner-html
  return <script key={text} type="application/ld+json" dangerouslySetInnerHTML={html} />;
}

component Head(metadata: Metadata) {
  const { title, description, metadataBase, canonical, robots } = metadata;
  const { alternates, pagination, jsonLd, openGraph, twitter } = metadata;
  const href = canonical != null ? absoluteUrl(canonical, metadataBase) : null;
  const crawler = robotsContent(robots);
  // Read out of `alternates` once rather than through it at every use: the map
  // is read inside a callback, and a refinement of `alternates.languages` does
  // not survive being carried into one.
  const languages = alternates?.languages;
  // A page that said what it is called has said what its card is called. Every
  // site that had to write both wrote the same string twice, and the second
  // one is the one that goes stale — the docs site shipped thirty pages whose
  // share cards carried an image and no title at all.
  //
  // `??`, not `||`: an empty string is a decision, and a page that deliberately
  // has no card title should get none rather than the document's.
  const cardTitle = openGraph?.title ?? title;
  const cardDescription = openGraph?.description ?? description;
  // `og:type` is one of the four properties Open Graph requires. A default is
  // the difference between a document with a card and a document without one,
  // and `website` is right for everything that is not an article or a video.
  const cardType = openGraph?.type ?? "website";
  // Only when the card was asked for. A page with no `twitter.card` gets no
  // Twitter tags at all, which is what a site that never wanted one meant.
  const twitterTitle = twitter != null ? (twitter.title ?? cardTitle) : null;
  const twitterDescription = twitter != null ? (twitter.description ?? cardDescription) : null;
  const twitterImageAlt = twitter != null ? (twitter.imageAlt ?? openGraph?.imageAlt) : null;
  return (
    <>
      {title != null ? <title>{title}</title> : null}
      {description != null ? <meta name="description" content={description} /> : null}
      {crawler != null ? <meta name="robots" content={crawler} /> : null}
      {href != null ? <link rel="canonical" href={href} /> : null}
      {/* The set is reciprocal and includes this page, so a `hreflang` list is
          usually the same list on every page of it — which is why it belongs
          on the layout they share rather than on each of them.

          `hrefLang` is React's spelling and it reaches the markup unchanged,
          which is worth knowing before grepping a document for `hreflang` and
          concluding it is missing. HTML attribute names are case-insensitive,
          so the parser every crawler runs reads it as the same attribute; the
          lowercase spelling is the one React warns about. */}
      {languages != null
        ? Object.keys(languages).map((language) => (
            <link
              key={language}
              rel="alternate"
              hrefLang={language}
              href={absoluteUrl(languages[language], metadataBase)}
            />
          ))
        : null}
      {pagination?.prev != null ? (
        <link rel="prev" href={absoluteUrl(pagination.prev, metadataBase)} />
      ) : null}
      {pagination?.next != null ? (
        <link rel="next" href={absoluteUrl(pagination.next, metadataBase)} />
      ) : null}
      {/* `og:url` *is* the canonical URL of the page, in Open Graph's own
          words, so one declaration answers both rather than asking a project
          to write the same URL twice and keep them in step. */}
      {href != null ? <meta property="og:url" content={href} /> : null}
      {cardTitle != null ? <meta property="og:title" content={cardTitle} /> : null}
      {cardDescription != null ? (
        <meta property="og:description" content={cardDescription} />
      ) : null}
      {/* Only alongside something else. A document with `og:type` and nothing
          more is not a card; it is one meta tag saying the page is a page. */}
      {cardTitle != null || cardDescription != null || openGraph?.images != null ? (
        <meta property="og:type" content={cardType} />
      ) : null}
      {openGraph?.siteName != null ? (
        <meta property="og:site_name" content={openGraph.siteName} />
      ) : null}
      {openGraph?.images != null
        ? openGraph.images.map((image) => (
            <meta key={image} property="og:image" content={absoluteUrl(image, metadataBase)} />
          ))
        : null}
      {openGraph?.imageAlt != null && openGraph?.images != null ? (
        <meta property="og:image:alt" content={openGraph.imageAlt} />
      ) : null}
      {/* `name`, not `property`: Open Graph is RDFa and Twitter's cards are
          not, and a `property="twitter:card"` is ignored by the crawler that
          reads it. */}
      {twitter?.card != null ? <meta name="twitter:card" content={twitter.card} /> : null}
      {twitter?.site != null ? <meta name="twitter:site" content={twitter.site} /> : null}
      {twitter?.creator != null ? <meta name="twitter:creator" content={twitter.creator} /> : null}
      {/* X reads the `og:` tags when these are absent, so these are not
          required — and every validator asks for them anyway, which is a good
          enough reason when the value is one the page has already given. They
          fall back through the card's title to the document's. */}
      {twitterTitle != null ? <meta name="twitter:title" content={twitterTitle} /> : null}
      {twitterDescription != null ? (
        <meta name="twitter:description" content={twitterDescription} />
      ) : null}
      {twitter?.images != null
        ? twitter.images.map((image) => (
            <meta key={image} name="twitter:image" content={absoluteUrl(image, metadataBase)} />
          ))
        : null}
      {twitterImageAlt != null && twitter?.images != null ? (
        <meta name="twitter:image:alt" content={twitterImageAlt} />
      ) : null}
      {/* Last, and not hoisted into `<head>` with the rest: React hoists a
          `<title>`, a `<meta>` and a `<link>`, and not a script whose body it
          would have to carry. JSON-LD is read from anywhere in the document,
          so these render where the route does. */}
      {jsonLd != null ? jsonLd.map(jsonLdScript) : null}
    </>
  );
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
  const { resolved } = useRouterState();
  const base = seo.metadataBase ?? resolved.metadata.metadataBase;
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
  const router = useRouter();
  const prefetched = React.useRef(false);

  const doPrefetch = () => {
    if (prefetch === "off" || prefetched.current || isExternal(to)) {
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
      window.location.assign(to);
    });
  };

  return (
    <a
      {...rest}
      href={to}
      className={className}
      onClick={handleClick}
      onMouseEnter={prefetch === "intent" ? doPrefetch : undefined}
      onFocus={prefetch === "intent" ? doPrefetch : undefined}
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
 */
export function routerView(root: string): React.ComponentType<AppProps> {
  void root;
  component App(url: string, initial: ResolvedRoute) {
    return (
      <RouterProvider url={url} initial={initial}>
        <RouteView />
      </RouterProvider>
    );
  }
  return App;
}

/** Stop rendering the current page and show the not-found page instead. */
export function notFound(): empty {
  throw new NotFoundError();
}

/** Stop rendering the current page and show the error boundary, as a 401. */
export function unauthorized(): empty {
  throw new UnauthorizedError();
}

/** Stop rendering the current page and show the error boundary, as a 403. */
export function forbidden(): empty {
  throw new ForbiddenError();
}

/** Stop rendering the current page and send the visitor elsewhere. */
export function redirect(to: string): empty {
  throw new RedirectError(to, false);
}

/** `redirect`, with a permanent status. */
export function permanentRedirect(to: string): empty {
  throw new RedirectError(to, true);
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
