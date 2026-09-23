// @flow
//
// Internal to `@uniflowed/router`: one walk from a resolved route to the tree
// that renders it.
//
// The layouts, the `$loading.js` fallbacks, the error boundaries, the
// templates and the parallel-route slots all have to be threaded into one stack
// at the depth each was declared at, and there must be exactly one place that
// does it. There used to be exactly one — `RouteView` — and it read the route
// from the router's context, which is a place a server composing a tree for
// React Server Components does not have. So the walk is a function of the
// resolved route and the page element, `RouteView` calls it, and so can the
// Flight renderer (ubugeeei-prod/uf#519): two callers of one composition rather
// than two compositions that agree until one of them moves.
//
// Nothing here reads a context or calls a hook. The components it places — a
// route's own modules, the error boundary, the dev-only boundary marks — are
// referenced, not rendered, so what they need is decided where they render.

import * as React from "react";
import { Suspense } from "react";

import { ROOT_ERROR_ID, ROUTE_ERROR_ID, suspenseId } from "./boundary-data.js";
import type { RouteBoundary } from "./boundary-data.js";
import { BoundaryEdge } from "./boundaries.js";
import { RouteErrorBoundary } from "./error-view.js";
import { Head } from "./head.js";
import type { RouteParams, SearchParams } from "./routing.js";
import type {
  LayoutModule,
  LayoutRenderProps,
  LoadingModule,
  PageModule,
  PageRenderProps,
  ResolvedRoute,
  ResolvedSlot,
  ResolvedTemplate,
  TemplateModule,
} from "./resolve.js";
import { renderable } from "./resolve.js";

/** What a caller hands [`composeRoute`] beside the resolved route. */
export type ComposeOptions = {|
  /**
   * The innermost element: the page, and whatever the caller renders beside it.
   *
   * The caller's rather than this module's, because it is the one element that
   * depends on where the route is being rendered — the browser reads the
   * loader's answer out of its router, and a server has it in hand.
   */
  readonly page: React.Node,
  /**
   * The boundary marks `uf dev` draws, keyed by boundary id, or `null` for none.
   *
   * Read only behind [`BOUNDARY_MARKS`], so a build folds every use away.
   */
  readonly marks: ?Map<string, RouteBoundary>,
|};

/** The route a slot renders inside, as much of it as a slot reads. */
type SlotRoute = { readonly pathname: string, readonly searchParams: SearchParams, ... };

/**
 * Whether this bundle marks the boundaries it renders.
 *
 * The same gate, spelled the same way, as `BOUNDARY_MARKS` in `./runtime.js`,
 * which has the argument: `import.meta.hot` is defined while Vite serves and
 * replaced with `undefined` in a build, so every branch it guards here is
 * statically dead in a production bundle and `./boundaries.js` is dropped
 * rather than shipped unused.
 */
const BOUNDARY_MARKS: boolean = import.meta.hot != null;

/**
 * The matched page inside its layouts, innermost last, with the document
 * metadata as hoistable head elements.
 *
 * A function of a resolved route and the page element, and of nothing a render
 * holds: no context is read here, so the same walk builds the tree the browser
 * renders for a single-page application and the tree a server hands to React's
 * Flight renderer. `RouteView` in `./runtime.js` is the browser's caller, and it
 * decides the page element because that is the half that reads the router.
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
 * `$error.js`, placed at the depth the file sits at, so the layouts above
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
 * with no `$loading.js` contributes no boundary at all — it is not wrapped
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
 *
 * # Where the boundary marks go
 *
 * Inside each boundary and around nothing else, under `uf dev` only. A
 * `<Suspense>` and a class boundary each render no element of their own, so the
 * run of nodes one owns is indistinguishable on the page from the layout's own
 * nodes beside it — the marks are what distinguish it, and this loop is the
 * only place that knows which boundary is which. `./boundaries.js` has the
 * mechanism and the argument; every reference to it here is inside a
 * [`BOUNDARY_MARKS`] branch, so a build has none of it. See
 * ubugeeei-prod/uf#520.
 */
export function composeRoute(resolved: ResolvedRoute, options: ComposeOptions): React.Node {
  const { module, above } = resolved.errorBoundary;
  const { marks } = options;
  // The innermost element, so a page that suspends — on its loader or on
  // anything else — suspends below every boundary the loop below adds, which is
  // what makes the layouts and the fallback the shell rather than something
  // waiting behind the page.
  let element: React.Node = options.page;

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
      element = (
        <Suspense fallback={<Fallback />}>
          {BOUNDARY_MARKS ? insideBoundary(marks?.get(suspenseId(index)), element) : element}
        </Suspense>
      );
    }
    // Placed on `above` alone, and not on there being a module: a `null` one is
    // the framework's own error page, and where it renders is exactly the
    // question ubugeeei-prod/uf#351 asks. A project that declares no
    // `$error.js` has the record the build synthesises for the router root,
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
          {BOUNDARY_MARKS ? insideBoundary(marks?.get(ROUTE_ERROR_ID), element) : element}
        </RouteErrorBoundary>
      );
    }
    element = insideTemplates(element, resolved, depth);
    if (depth > 0) {
      const Layout = layoutComponent(resolved.layouts[depth - 1]);
      // The slots declared on this layout's own segment, beside `children`.
      // Spread rather than passed as one `slots` object, because a slot is a
      // prop a layout declares by name — `component Dashboard(children, team)`
      // — and a bag would make every layout destructure a map to find out
      // whether the router had anything for it.
      // The spread first and `params` after it, so that a slot named after a
      // prop the layout already has loses rather than wins. `@params` and
      // `@children` are refused by the scan, and this is the second line of
      // that defence for a table written by hand: losing a slot is a hole in
      // the page, and overwriting `params` is every route in the segment
      // rendering against the wrong parameters.
      element = (
        <Layout {...slotsAt(resolved.slots, depth, resolved)} params={resolved.params}>
          {element}
        </Layout>
      );
    }
  }
  if (needsRootStreamFrame(resolved)) {
    element = <RootStreamFrame>{element}</RootStreamFrame>;
  }
  return (
    <>
      <Head metadata={resolved.metadata} />
      <RouteErrorBoundary module={null} resetKey={resolved.pathname}>
        {BOUNDARY_MARKS ? insideBoundary(marks?.get(ROOT_ERROR_ID), element) : element}
      </RouteErrorBoundary>
    </>
  );
}

/**
 * `children`, between the two marks of `boundary`.
 *
 * `children` unchanged when there is no boundary to mark, so a caller never has
 * to ask twice. The marks are the first and last children of a fragment rather
 * than a wrapper's, so the nodes between them are siblings of them, and every
 * position in the fragment is fixed — an edge going from `null` to a hidden
 * mark after mount is an insertion beside `children` and not around it,
 * which is why it costs no remount.
 *
 * Here rather than beside the marks in `./boundaries.js`, because this is a
 * factory and that is a client module: a server composing a tree for React
 * Server Components calls this, and the edges it places stay references to
 * the module that renders them.
 */
export function insideBoundary(boundary: ?RouteBoundary, children: React.Node): React.Node {
  if (boundary == null) {
    return children;
  }
  return (
    <>
      <BoundaryEdge boundary={boundary} edge="open" />
      {children}
      <BoundaryEdge boundary={boundary} edge="close" />
    </>
  );
}

/**
 * Whether the outermost route fallback needs one host element above it.
 *
 * React can flush a shell whose suspended boundary is inside any host element,
 * but not one whose boundary is a direct child of the render root. A route with
 * no layout and a root `$loading.js` is exactly that second tree: every
 * framework component above it renders no element, so the fallback waits for
 * the page it was meant to stand in for. A root layout is already the element
 * that can carry it, and deeper fallbacks sit inside a layout by construction.
 */
function needsRootStreamFrame(resolved: ResolvedRoute): boolean {
  return resolved.layouts.length === 0 && resolved.loading.some((boundary) => boundary.above === 0);
}

component RootStreamFrame(children: React.Node) {
  return (
    <div data-uf-stream-root="" style={{ display: "contents" }}>
      {children}
    </div>
  );
}

/**
 * The component a page module renders: its default export, or the named
 * `Page` that `uf create` scaffolds. An MDX page always has a default export.
 */
export function pageComponent(module: PageModule): React.ComponentType<PageRenderProps> {
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
export function loadingComponent(module: LoadingModule): React.ComponentType<{||}> {
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
export function insideTemplates(
  element: React.Node,
  resolved: {
    readonly pathname: string,
    readonly params: RouteParams,
    readonly templates: $ReadOnlyArray<ResolvedTemplate>,
    ...
  },
  depth: number,
): React.Node {
  let out = element;
  for (let index = resolved.templates.length - 1; index >= 0; index -= 1) {
    const entry = resolved.templates[index];
    if (entry.above !== depth) {
      continue;
    }
    const Template = templateComponent(entry.module);
    // Keyed on the pathname, which is the whole difference between this file
    // and `$layout.js`: React throws the subtree away and builds it again
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
 * The slots declared at `depth`, as the props the layout there receives.
 *
 * One object per layout rather than one lookup per slot, so the common case —
 * a project with no slots at all — allocates nothing and spreads nothing.
 *
 * A slot the URL addressed and that has no `$default.js` is `null` rather
 * than absent: a layout that declares `team` receives `team` on every route,
 * so `{team ?? <Empty />}` is a thing a project can write and rely on.
 */
export function slotsAt(
  slots: $ReadOnlyArray<ResolvedSlot>,
  depth: number,
  route: SlotRoute,
): SlotProps {
  if (slots.length === 0) {
    return EMPTY_SLOTS;
  }
  const props: { key?: empty, [string]: React.Node } = {};
  for (const slot of slots) {
    if (slot.above === depth) {
      // `null` rather than an element that renders nothing, and the difference
      // is the whole of what the prop is for: `{team ?? <Empty />}` has to be
      // able to tell "this slot has nothing in it" from "this slot rendered
      // something empty", and an element is never `null`.
      props[slot.name] =
        slot.page == null ? null : (
          <SlotView slot={slot} pathname={route.pathname} searchParams={route.searchParams} />
        );
    }
  }
  return props;
}

/**
 * A layout's slots, as the props they are spread into.
 *
 * `key` is named out of the indexer, the way `@uniflowed/ui`'s `Rest` names it:
 * the object is spread onto an element, and a `React.Node` is not a key. No
 * slot is called `key` — a slot is a prop the layout declares, and React
 * never hands a component its key.
 */
export type SlotProps = { readonly key?: empty, readonly [string]: React.Node };

/** One object for every layout on a project that declares no slot. */
const EMPTY_SLOTS: SlotProps = Object.freeze({});

/**
 * One slot's tree: its page, inside the layouts declared under the slot, with
 * the slots those layouts declare in turn.
 *
 * The same composition [`RouteView`] does and deliberately not the same
 * function. A route's tree carries the things a slot does not have — the error
 * boundary, the `<Suspense>` fallbacks, the templates, the head — and folding
 * a second, simpler case into that loop would be four `if`s asking which of the
 * two this is. What the two share is the *order*, page innermost and layouts
 * backwards over a root-first list, and that is short enough to be right twice.
 *
 * A slot with no page is never rendered through this component at all —
 * [`slotsAt`] hands the layout `null` instead, so the layout can tell an empty
 * slot from one that rendered something empty. The guard below is what makes
 * that a fact about one place rather than a convention two places share.
 */
component SlotView(slot: ResolvedSlot, pathname: string, searchParams: SearchParams) {
  // The pathname and the search string are the route's, and they are handed
  // down rather than read from the router: a slot matches the path and the
  // query belongs to the URL rather than to either match, and a component that
  // reads no context is one a server can render for React Server Components.
  const page = slot.page;
  if (page == null) {
    return null;
  }
  // Except in an interception, whose URL is not the one the route on screen was
  // resolved for. The page it renders reads the intercepted URL's query, and
  // its templates and error boundary are keyed on the intercepted pathname — so
  // a second photo opened in the modal remounts what the first one mounted, the
  // way a navigation between two pages does.
  const at = slot.intercepted?.pathname ?? pathname;
  const query = slot.intercepted?.searchParams ?? searchParams;
  const Page = pageComponent(page);
  // The slot module's own export, looked up by slot: `pageComponent` hands back
  // its `default` or `Page` as it is, so this is the same component on every
  // render of the same slot. The React Compiler cannot see through the lookup
  // and reports a component created during render.
  // uf-lint-disable-next-line react-compiler/static-components
  let element: React.Node = <Page params={slot.params} searchParams={query} data={undefined} />;
  const templateContext = {
    pathname: at,
    params: slot.params,
    templates: slot.templates,
  };
  for (let depth = slot.layouts.length; depth >= 0; depth -= 1) {
    for (let index = slot.loading.length - 1; index >= 0; index -= 1) {
      const boundary = slot.loading[index];
      if (boundary.above !== depth) {
        continue;
      }
      const Fallback = loadingComponent(boundary.module);
      // The same lookup as `Page` above: `loadingComponent` hands back the
      // boundary module's own export as it is, so this is the same component
      // on every render of that boundary.
      // uf-lint-disable-next-line react-compiler/static-components
      element = <Suspense fallback={<Fallback />}>{element}</Suspense>;
    }
    const errorBoundary = slot.errorBoundary;
    if (errorBoundary != null && errorBoundary.above === depth) {
      element = (
        <RouteErrorBoundary module={errorBoundary.module} resetKey={`${at}:${slot.name}`}>
          {element}
        </RouteErrorBoundary>
      );
    }
    element = insideTemplates(element, templateContext, depth);
    if (depth > 0) {
      const Layout = layoutComponent(slot.layouts[depth - 1]);
      element = (
        // The layout module's own export, looked up by depth, for the same
        // reason as `Page` above.
        // uf-lint-disable-next-line react-compiler/static-components
        <Layout {...slotsAt(slot.slots, depth, { pathname, searchParams })} params={slot.params}>
          {element}
        </Layout>
      );
    }
  }
  return element;
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
