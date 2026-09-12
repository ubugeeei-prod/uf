// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The file-system router, as the build sees it.
//
// This mirrors `uf_router` in Rust — the same reserved-name grammar
// (`$<role>[.<variant>].js`, plus `.mdx` for pages), the same route path
// syntax (`[param]`, `[...rest]`, `(group)`) and the same sort order — and it
// must keep mirroring it: `uf lint` and `router.js`'s generated types describe
// the routes this module serves, so the two cannot be allowed to disagree.
//
// Everything produced here is a string of JavaScript for a virtual module. The
// route table imports every page and layout lazily, so a route is a chunk of
// its own and the client only downloads what it navigates to.

import { readdirSync, statSync } from "node:fs";
import path from "node:path";

/** The file names the router reserves inside the router root. */
export const RESERVED = Object.freeze({
  layout: "$layout",
  template: "$template",
  page: "$page",
  default: "$default",
  middleware: "$middleware",
  notFound: "$not-found",
  error: "$error",
  loading: "$loading",
  route: "$route",
});

/**
 * Directory names uf reserves inside the router root without serving them.
 *
 * One spelling each, and they are here so `crates/uf_router/tests/
 * reserved_names.rs` can hold this router and `uf_router::RouteSegment` to the
 * same list — the way it already holds the two to the same `$*` roles. A
 * spelling one router refuses and the other serves as a URL is exactly the
 * disagreement that made this necessary.
 *
 * `(.)photo` is Next.js's intercepting route. `@team`, its parallel-route
 * slot, was on this list too: until #267 both fell through to "a literal URL
 * segment", so `@team` became `/@team`, `(.)photo` became `/(.)photo` — the
 * test for a `(group)` is that the segment *ends* in `)` — and the generated
 * `RoutePath` union contained them, so `route("/@team", …)` type checked. A
 * convention served as nonsense is worse than one that is refused, because the
 * project looks like it works.
 *
 * A slot is a route this router serves now; see {@link scanRoutes}. An
 * interception is not: it needs a navigation to carry where it came from,
 * which is a change to what a navigation is rather than to this scan.
 */
export const UNSUPPORTED_SEGMENTS = Object.freeze([
  "(.)photo",
  "(..)photo",
  "(...)photo",
  "(..)(..)photo",
]);

/**
 * The prop names a layout already receives, which a slot may therefore not
 * take.
 *
 * A slot arrives as a prop named after its directory, so `@children` and
 * `@params` are the two names that would land on top of something the layout
 * already has. `@children` is the one somebody actually writes: `children` is
 * what Next.js calls its implicit slot, so it is the first name a person
 * migrating reaches for — and here the page the URL matched always is
 * `children`.
 *
 * The same list as `uf_router::LAYOUT_PROP_NAMES`; see that file for why the
 * collision is refused rather than resolved by precedence.
 */
export const LAYOUT_PROP_NAMES = Object.freeze(["children", "params"]);

/** Template spellings that look conventional elsewhere but uf will not open. */
const UNSUPPORTED_TEMPLATE_FILES = Object.freeze(["template.js", "_uf.template.js"]);

/** Extensions a page or layout may use; `.mdx` is a page written as content. */
const PAGE_EXTENSIONS = [".js", ".jsx", ".mdx"];
const MODULE_EXTENSIONS = [".js", ".jsx"];

/** Application targets the route scanner knows how to select files for. */
export const ROUTE_TARGETS = Object.freeze(["web", "native", "ios", "android"]);

const TARGET_VARIANTS = Object.freeze({
  web: ["web", null],
  native: ["native", null],
  ios: ["ios", "native", null],
  android: ["android", "native", null],
});

/**
 * The route target a loaded `uf.config.js` and an optional CLI flag describe.
 *
 * `react-native` is accepted as the config-shaped spelling of the same target
 * `uf build --target native` selects. The default follows the framework
 * preset rather than the target list: uf's default list names both web and
 * React Native, so the list is a promise the project should keep satisfying,
 * not the one build to run when none was requested.
 */
export function resolveRouteTarget(config = {}, requested = null) {
  const app = config.app ?? {};
  const named =
    requested == null || requested === ""
      ? app.framework === "react-native"
        ? "native"
        : "web"
      : requested === "react-native"
        ? "native"
        : requested;
  if (!ROUTE_TARGETS.includes(named)) {
    throw new Error(
      `uf: ${JSON.stringify(named)} is not an application target; choose web, native, ios or android`,
    );
  }
  const declared = app.targets;
  if (Array.isArray(declared)) {
    const needs = named === "web" ? "web" : "react-native";
    if (!declared.includes(needs)) {
      throw new Error(
        `uf: --target ${named} needs app.targets to include ${JSON.stringify(needs)}`,
      );
    }
  }
  return named;
}

/** Deepest directory nesting the scan will follow. */
const MAX_DEPTH = 32;

/**
 * One route in the table.
 *
 * @typedef {object} Route
 * @property {string} path route path such as `/docs/:slug`
 * @property {string} pattern the same path with `*` for catch-alls, for humans
 * @property {ReadonlyArray<{name: string, catchAll: boolean}>} params
 * @property {string} page absolute path of the page module
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 * @property {ReadonlyArray<{above: number, module: string}>} loading the
 *   `<Suspense>` boundaries in scope, root first; `above` is how many of
 *   `layouts` are outside each one
 * @property {ReadonlyArray<{above: number, module: string}>} templates the
 *   `$template.js` wrappers in scope, root first, with the same `above`
 * @property {ReadonlyArray<Slot>} slots the parallel-route slots in scope,
 *   outermost first
 * @property {boolean} mdx whether the page is MDX content
 */

/**
 * One parallel-route slot — a second thing a layout renders, beside its page.
 *
 * A directory named `@team` contributes no URL segment. It declares a slot on
 * the segment that holds it, and that segment's own layout receives the
 * rendered slot as a `team` prop beside `children`. The slot's pages are
 * matched against the same URL the page is, so `app/dashboard/@team/members/
 * $page.js` is what `/dashboard/members` puts in the slot — not a second
 * page at that path.
 *
 * `above` is how many of the route's `layouts` are outside the slot, counted
 * after the declaring segment's own layout is added — so `layouts[above - 1]`
 * is the layout that receives it. It is the same number, spelled the same way,
 * as a template's and a loading boundary's. The layout has to be the segment's
 * *own*: a slot rendered into an inherited layout would be a prop that layout
 * never declared, on every route below it, so {@link scanRoutes} refuses a slot
 * whose segment has no layout of its own.
 *
 * `defaultPage` is the slot's `$default.js`: what it renders when the URL
 * matches none of its routes. A slot with neither a match nor a default
 * renders nothing, which is what an unaddressed slot on a soft navigation does
 * in Next.js too.
 *
 * @typedef {object} Slot
 * @property {string} name the slot's name, without the `@`
 * @property {number} above how many of the route's layouts are outside it
 * @property {?string} defaultPage absolute path of `$default.*`, or `null`
 * @property {boolean} defaultMdx whether that default is MDX content
 * @property {ReadonlyArray<SlotRoute>} routes what the slot may render, by URL
 */

/**
 * One page inside a slot.
 *
 * A `Route` without the parts a slot does not have: no `loading`, no
 * `templates`, and no boundary of its own. Those are the segment's, and they
 * already wrap the layout the slot renders into. Per-slot boundaries are the
 * part of parallel routes uf has not built — see
 * https://github.com/ubugeeei-prod/uf/issues/267 — and {@link scanRoutes}
 * refuses the files rather than leaving them unopened.
 *
 * `layouts` are the layouts *inside* the slot, root first; the ones above it
 * are already rendering, since the slot renders into one of them.
 *
 * @typedef {object} SlotRoute
 * @property {string} path route path such as `/dashboard/members`
 * @property {ReadonlyArray<{name: string, catchAll: boolean}>} params
 * @property {string} page absolute path of the page module
 * @property {ReadonlyArray<string>} layouts absolute paths, slot root first
 * @property {ReadonlyArray<Slot>} slots slots declared inside this slot
 * @property {boolean} mdx whether the page is MDX content
 */

/**
 * One middleware — everything under a directory, guarded before it answers.
 *
 * A flat table keyed by the directory's route path, rather than an array on
 * every route the way layouts are accumulated. That was the first shape and it
 * left two holes: `/dashboard/typo` matches no route, so a per-route array
 * would have rendered the 404 with the guard skipped, and a route handler is
 * in a table of its own, so guarding pages would have guarded half of them.
 * The path is the matcher, so the path is what the table carries.
 *
 * @typedef {object} Middleware
 * @property {string} path route path of the directory it guards, `/` at the root
 * @property {string} module absolute path of the middleware module
 */

/**
 * One route handler — a path that answers a request instead of rendering.
 *
 * @typedef {object} Handler
 * @property {string} path route path such as `/api/users/:id`
 * @property {string} pattern the same path with `*` for catch-alls
 * @property {ReadonlyArray<{name: string, catchAll: boolean}>} params
 * @property {string} module absolute path of the handler module
 */

/**
 * One not-found boundary — the page a path under `path` gets when nothing
 * there matched.
 *
 * A `$not-found.js` is a segment file like `$layout.js`, so a directory
 * declares the 404 for everything beneath it and the resolver takes the
 * nearest one above the path. `layouts` are the layouts in scope *at that
 * directory*, which is what wraps the boundary when it renders.
 *
 * @typedef {object} NotFoundBoundary
 * @property {string} path route path of the directory that declares it
 * @property {?string} page absolute path of the page module, or `null` for the
 *   record the scan synthesises at the router root when a project declares
 *   none — see `scanRoutes`
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 * @property {boolean} mdx whether the page is MDX content
 */

/**
 * One error boundary — what renders in place of the subtree under `path` when
 * something in it throws.
 *
 * The same nearest-ancestor shape as a not-found boundary, and deliberately
 * not the same extensions: an error module is handed an error and a `reset`,
 * which is a component's contract. `.mdx` compiles to a component that takes
 * no such thing, so a `$error.mdx` would be a file the router loads and can
 * never hand its arguments to.
 *
 * @typedef {object} ErrorBoundary
 * @property {string} path route path of the directory that declares it
 * @property {?string} module absolute path of the error module, or `null` for
 *   the synthesised root record
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 */

/**
 * One loading boundary — the fallback for the segment that declares it.
 *
 * Not the nearest-ancestor shape the other two boundaries have, and the
 * difference is the whole of what a fallback is. A not-found or an error
 * boundary is *chosen*: one of them renders, and the resolver picks the
 * nearest above the path. Loading boundaries *nest*: `app/$loading.js` and
 * `app/docs/$loading.js` are two `<Suspense>` elements on one route, one
 * inside the other, and both are in the tree at once. So they accumulate down
 * the walk the way layouts do rather than being matched afterwards, and each
 * route carries the list that applies to it.
 *
 * `above` is the count of the route's `layouts` that sit outside the boundary
 * — the layouts that render immediately, which is what "the shell around a
 * slow page" means. It is the same number, spelled the same way, as
 * `ResolvedRoute["errorBoundary"].above` in the router runtime.
 *
 * @typedef {object} LoadingBoundary
 * @property {number} above how many of the route's layouts are outside it
 * @property {string} module absolute path of the loading module
 */

/**
 * Scan `appRoot` for routes.
 *
 * Returns routes sorted by path, which is the order `uf_router` uses too.
 * Directories that do not exist yield an empty table rather than an error: a
 * library project has no router root, and that is not a mistake.
 *
 * Throws for a directory named the way an intercepting route is spelled: uf
 * does not have interception, and the spelling used to become a literal URL
 * segment. See {@link UNSUPPORTED_SEGMENTS}.
 *
 * A `@slot` directory is a parallel route and is scanned; see {@link Slot}. It
 * throws for the three ways one can be written without being renderable: a
 * slot on a segment with no layout of its own, a `$default.js` that is not
 * directly inside a slot, and a boundary or a handler inside a slot. Each is a
 * file the router would otherwise never open, which is the failure #267 is
 * about.
 *
 * @param {string} appRoot absolute path of the router root (`app/`)
 * @param {{target?: "web" | "native" | "ios" | "android"}} [options]
 * @returns {{
 *   routes: Route[],
 *   handlers: Handler[],
 *   middleware: Middleware[],
 *   notFound: NotFoundBoundary[],
 *   errors: ErrorBoundary[],
 * }}
 */
export function scanRoutes(appRoot, options = {}) {
  const target = resolveRouteTarget({}, options.target ?? "web");
  const routes = [];
  const handlers = [];
  const middleware = [];
  const notFound = [];
  const errors = [];
  if (!isDirectory(appRoot)) return { routes, handlers, middleware, notFound, errors };

  // The layouts in scope at the router root, kept because the two synthesised
  // records below are made of them. See the note beside them.
  let rootLayouts = [];

  const walk = (directory, segments, layouts, loading, templates, slots, depth) => {
    if (depth > MAX_DEPTH) return;
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );

    refuseUnsupportedTemplateFiles(directory, entries);

    // A `$default.js` answers one question — what a slot renders when the
    // URL says nothing about it — and this walk is everywhere a slot is not,
    // so one found here is a file nothing would ever open.
    const strayDefault = findModule(directory, RESERVED.default, PAGE_EXTENSIONS, target);
    if (strayDefault != null) {
      throw new Error(
        `${strayDefault}: \`$default.js\` is what a \`@slot\` renders when the URL says ` +
          "nothing about it, and it belongs directly inside the slot directory — one per slot, " +
          "beside that slot's own pages. Nothing would ever render this one. uf has no " +
          "`default` for `children`: a URL that matches no page is a 404.",
      );
    }

    const ownLayout = findModule(directory, RESERVED.layout, MODULE_EXTENSIONS, target);
    const nextLayouts = ownLayout ? [...layouts, ownLayout] : layouts;
    if (depth === 0) {
      rootLayouts = nextLayouts;
    }

    // Inside this directory's own layout, which is where Next.js puts it and
    // the only placement that makes sense: the fallback is what shows *within*
    // the frame this segment draws, so the frame has to be outside it.
    // `nextLayouts.length` is therefore the count taken after the own layout is
    // added, not before. A segment with a loading file and no layout of its own
    // still gets a boundary — it just shares its parent's frame.
    const ownLoading = findModule(directory, RESERVED.loading, MODULE_EXTENSIONS, target);
    const nextLoading = ownLoading
      ? [...loading, { above: nextLayouts.length, module: ownLoading }]
      : loading;

    // A template accumulates the way a layout does, and is placed the way a
    // loading file is: inside its own segment's layout and outside everything
    // below, so `nextLayouts.length` is taken after the own layout is added.
    // Every template above a route is on that route, one inside the next, for
    // the reason every layout is — the difference between the two is a `key`,
    // not a shape.
    const ownTemplate = findModule(directory, RESERVED.template, MODULE_EXTENSIONS, target);
    const nextTemplates = ownTemplate
      ? [...templates, { above: nextLayouts.length, module: ownTemplate }]
      : templates;

    // Slots before this directory's own page, because the page renders inside
    // the layout that holds them: a slot declared here belongs to every route
    // at or below this segment, the way a template does.
    let nextSlots = slots;
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      const classified = classifyRouteSegment(entry.name);
      if (classified.kind !== "slot") continue;
      nextSlots = [
        ...nextSlots,
        scanSlot(
          directory,
          entry.name,
          classified.name,
          segments,
          ownLayout,
          nextLayouts.length,
          target,
          depth,
        ),
      ];
    }

    // A middleware guards this directory and everything below it, whether or
    // not this directory is itself a route: `app/dashboard/$middleware.js`
    // with no `$page.js` beside it still guards `/dashboard/settings`.
    const ownMiddleware = findModule(directory, RESERVED.middleware, MODULE_EXTENSIONS, target);
    if (ownMiddleware) {
      middleware.push({ path: routeFromSegments(segments).path, module: ownMiddleware });
    }

    const page = findModule(directory, RESERVED.page, PAGE_EXTENSIONS, target);
    if (page) {
      const { path: routePath, pattern, params } = routeFromSegments(segments);
      routes.push({
        path: routePath,
        pattern,
        params,
        page,
        layouts: nextLayouts,
        loading: nextLoading,
        templates: nextTemplates,
        slots: nextSlots,
        mdx: page.endsWith(".mdx"),
      });
    }
    // A handler answers the request itself, so it takes no layouts and is not
    // MDX. It may sit beside a page: `/feed` can render for a browser and
    // `/feed.xml` answer for a reader, and both are the same directory tree.
    const handler = findModule(directory, RESERVED.route, MODULE_EXTENSIONS, target);
    if (handler) {
      const { path: routePath, pattern, params } = routeFromSegments(segments);
      handlers.push({ path: routePath, pattern, params, module: handler });
    }

    // At every depth, not only the root. This read `if (depth === 0)`, so
    // `app/guide/$not-found.js` was never looked for and a reader who
    // followed a stale link into the manual was answered by the site's root
    // 404, outside the manual's own layout. See ubugeeei-prod/uf#263.
    const ownNotFound = findModule(directory, RESERVED.notFound, PAGE_EXTENSIONS, target);
    if (ownNotFound) {
      notFound.push({
        path: routeFromSegments(segments).path,
        page: ownNotFound,
        layouts: nextLayouts,
        mdx: ownNotFound.endsWith(".mdx"),
      });
    }

    // `errors` is the boundaries a project declares, not failures that
    // happened: one entry per directory holding an `$error.js`.
    const ownError = findModule(directory, RESERVED.error, MODULE_EXTENSIONS, target);
    if (ownError) {
      errors.push({
        path: routeFromSegments(segments).path,
        module: ownError,
        layouts: nextLayouts,
      });
    }

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      // A leading dot or underscore is private to the author: `_components/`
      // beside a page is a place to put things, not a route.
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      // Already walked, above, into a table of its own.
      if (classifyRouteSegment(entry.name).kind === "slot") continue;
      // Checked before descending, and after the private-directory test for
      // the same reason `uf_router` prunes them: `app/_drafts/(.)photo/` is not
      // a route uf would have served, so it is not one to refuse.
      const refused = unsupportedSegmentReason(entry.name);
      if (refused != null) {
        throw new Error(`${path.join(directory, entry.name)}: ${refused}`);
      }
      walk(
        path.join(directory, entry.name),
        [...segments, entry.name],
        nextLayouts,
        nextLoading,
        nextTemplates,
        nextSlots,
        depth + 1,
      );
    }
  };

  walk(appRoot, [], [], [], [], [], 0);

  // A boundary at the router root for a project that declared none, carrying
  // the root's layouts and no module of its own.
  //
  // Without it the router had no record to answer an unmatched URL with, so it
  // answered with the framework's page and `layouts: []` — and a site whose
  // root layout owns the masthead, the stylesheet and often `<html>` itself
  // replied to a stale link with a white page saying 404, with no way to leave
  // it. That was never the nearest-ancestor rule failing: the rule had nothing
  // to find. `uf create` scaffolds neither boundary, so this is the state every
  // new project is in until it writes one. See ubugeeei-prod/uf#351.
  //
  // Only when nothing is at `/` already. A `(group)` directory is not a URL
  // segment, so `app/(marketing)/$not-found.js` is a boundary at `/` too and
  // adding a second one there would put a second answer at a path the URL
  // cannot choose between.
  const atRoot = (boundaries) => boundaries.some((boundary) => boundary.path === "/");
  if (!atRoot(notFound)) {
    notFound.push({ path: "/", page: null, layouts: rootLayouts, mdx: false });
  }
  if (!atRoot(errors)) {
    errors.push({ path: "/", module: null, layouts: rootLayouts });
  }

  const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  routes.sort(byPath);
  handlers.sort(byPath);
  // Sorted for a table that does not churn between builds, and for nothing
  // else: `createMiddlewareRunner` re-orders the table root first, because
  // what a chain of guards runs in is depth, not name.
  middleware.sort(byPath);
  // Sorted by path, not by which is nearest: the resolver picks the longest
  // path that covers the URL, so it does not depend on this order, and sorting
  // by nearness would hide that.
  //
  // Two boundaries can share a path, because a `(group)` directory is not a URL
  // segment — `app/$not-found.js` and `app/(marketing)/$not-found.js` are
  // both at `/`, and the URL cannot say which tree it is in. The sort is stable
  // and `walk` records a directory's own boundary before descending, so the
  // shallower file wins, which is the one that is the site's own 404 rather
  // than one section's idea of it. Letting each group own a boundary needs the
  // parallel-route trees uf does not have yet; see ubugeeei-prod/uf#267.
  notFound.sort(byPath);
  errors.sort(byPath);
  return { routes, handlers, middleware, notFound, errors };
}

/**
 * One `@slot` directory, scanned into a {@link Slot}.
 *
 * Separate from `walk` rather than a mode of it, because the two build
 * different things out of the same tree. `walk` builds URLs and the boundaries
 * around them; this builds what one named place may hold, matched against URLs
 * somebody else's directories define. Folding them together would mean a
 * `loading` accumulator that is dead in half the calls and a route table that
 * is dead in the other half.
 *
 * Nested slots are ordinary: a slot's own layout may declare slots of its own,
 * and they are collected here the same way, so the recursion is the shape of
 * the feature rather than a special case.
 *
 * @param {string} parent the directory that declares the slot
 * @param {string} directoryName the slot directory, `@team` as written
 * @param {string} name the slot's name, `team`
 * @param {ReadonlyArray<string>} segments the declaring segments, for the URL
 * @param {?string} ownLayout the declaring segment's own layout, or `null`
 * @param {number} above how many layouts are outside the slot
 * @param {"web" | "native" | "ios" | "android"} target application target
 * @param {number} depth nesting depth, against `MAX_DEPTH`
 * @returns {Slot}
 */
function scanSlot(parent, directoryName, name, segments, ownLayout, above, target, depth) {
  const directory = path.join(parent, directoryName);
  if (LAYOUT_PROP_NAMES.includes(name)) {
    throw new Error(
      `${directory}: a slot arrives as a prop named after its directory, and \`${name}\` is a ` +
        "prop every layout already receives, so one of the two would silently go missing. " +
        "Rename the slot. The page a URL matches is always `children` — uf has no `@children` " +
        "slot, which is the name Next.js gives that page.",
    );
  }
  // The declaring segment's *own* layout, not the layouts in scope there. A
  // slot is a prop that layout receives beside `children`, so a slot on a
  // segment with no layout has nothing to render into — and rendering it into
  // an inherited one would hand a prop to a layout that never declared it, on
  // every route below.
  if (ownLayout == null) {
    const routePath = routeFromSegments(segments).path;
    throw new Error(
      `${directory}: \`${directoryName}\` is a parallel-route slot and \`${routePath}\` declares ` +
        "no layout of its own, so there is nothing to render the slot into — a slot is a prop " +
        "the segment's own layout receives beside `children`. Add " +
        `\`${path.join(parent, `${RESERVED.layout}.js`)}\`, or move the slot to a segment that ` +
        "has one.",
    );
  }

  const routes = [];
  const defaultPage = findModule(directory, RESERVED.default, PAGE_EXTENSIONS, target);

  const walkSlot = (current, currentSegments, layouts, atSlotRoot, currentDepth) => {
    if (currentDepth > MAX_DEPTH) return;
    // What a slot does not have, said where somebody writing the file will
    // read it rather than by never opening it. This is also the list of what
    // is left of parallel routes; see the issue.
    for (const role of [RESERVED.notFound, RESERVED.error, RESERVED.loading, RESERVED.template]) {
      const found =
        findModule(current, role, MODULE_EXTENSIONS, target) ??
        findModule(current, role, PAGE_EXTENSIONS, target);
      if (found != null) {
        throw new Error(
          `${found}: a \`@slot\` renders a page and the layouts inside the slot, and has no ` +
            `\`${role.slice("$".length)}\` of its own — uf's parallel routes do not carry ` +
            "per-slot boundaries yet, so this file would never be opened. Put it outside " +
            `\`${directoryName}\`, where it covers the whole segment. ` +
            "https://github.com/ubugeeei-prod/uf/issues/267",
        );
      }
    }
    for (const role of [RESERVED.route, RESERVED.middleware]) {
      const found = findModule(current, role, MODULE_EXTENSIONS, target);
      if (found != null) {
        throw new Error(
          `${found}: a \`@slot\` renders inside the page at a URL and answers no request of its ` +
            `own, so \`${role}.js\` here would never run. A slot directory contributes no URL ` +
            `segment, so this would claim \`${routeFromSegments(currentSegments).path}\` — which ` +
            `belongs to the segment that declares the slot. Move it out of \`${directoryName}\`.`,
        );
      }
    }
    // One default per slot, at the slot. A deeper one would be a second answer
    // to a question that is asked once — the URL either addressed this slot or
    // it did not.
    const nestedDefault = findModule(current, RESERVED.default, PAGE_EXTENSIONS, target);
    if (!atSlotRoot && nestedDefault != null) {
      throw new Error(
        `${nestedDefault}: a \`@slot\` has one ` +
          `\`$default.js\`, directly inside \`${directoryName}\`, and this one is deeper, so ` +
          "nothing would ever render it.",
      );
    }

    const layoutHere = findModule(current, RESERVED.layout, MODULE_EXTENSIONS, target);
    const nextLayouts = layoutHere ? [...layouts, layoutHere] : layouts;

    const entries = readdirSync(current, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );

    refuseUnsupportedTemplateFiles(current, entries);

    let nestedSlots = [];
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      const classified = classifyRouteSegment(entry.name);
      if (classified.kind !== "slot") continue;
      nestedSlots = [
        ...nestedSlots,
        scanSlot(
          current,
          entry.name,
          classified.name,
          currentSegments,
          layoutHere,
          nextLayouts.length,
          target,
          currentDepth,
        ),
      ];
    }

    const page = findModule(current, RESERVED.page, PAGE_EXTENSIONS, target);
    if (page) {
      const { path: routePath, params } = routeFromSegments(currentSegments);
      routes.push({
        path: routePath,
        params,
        page,
        layouts: nextLayouts,
        slots: nestedSlots,
        mdx: page.endsWith(".mdx"),
      });
    }

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      const classified = classifyRouteSegment(entry.name);
      if (classified.kind === "slot") continue;
      const refused = unsupportedSegmentReason(entry.name);
      if (refused != null) {
        throw new Error(`${path.join(current, entry.name)}: ${refused}`);
      }
      walkSlot(
        path.join(current, entry.name),
        [...currentSegments, entry.name],
        nextLayouts,
        false,
        currentDepth + 1,
      );
    }
  };

  walkSlot(directory, segments, [], true, depth + 1);

  const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  routes.sort(byPath);
  return {
    name,
    above,
    defaultPage,
    defaultMdx: defaultPage != null && defaultPage.endsWith(".mdx"),
    routes,
  };
}

function isDirectory(candidate) {
  try {
    return statSync(candidate).isDirectory();
  } catch {
    return false;
  }
}

function findModule(directory, stem, extensions, target = "web") {
  for (const variant of TARGET_VARIANTS[target] ?? TARGET_VARIANTS.web) {
    for (const extension of extensions) {
      const fileName = variant == null ? `${stem}${extension}` : `${stem}.${variant}${extension}`;
      const candidate = path.join(directory, fileName);
      try {
        if (statSync(candidate).isFile()) return candidate;
      } catch {
        // keep looking
      }
    }
  }
  return null;
}

function refuseUnsupportedTemplateFiles(directory, entries) {
  for (const entry of entries) {
    if (!entry.isFile() || !UNSUPPORTED_TEMPLATE_FILES.includes(entry.name)) {
      continue;
    }
    const file = path.join(directory, entry.name);
    throw new Error(`${file}: ${unsupportedTemplateFileReason(entry.name)}`);
  }
}

function unsupportedTemplateFileReason(fileName) {
  return (
    `\`${fileName}\` looks like a route template, but uf's route template file is ` +
    "`$template.js`. This file would be ignored rather than remounting the route, so it is " +
    "refused; rename it to `$template.js`. https://github.com/ubugeeei-prod/uf/issues/267"
  );
}

/**
 * What one directory name means to the route path.
 *
 * Mirrors `uf_router::classify_route_segment`, which is the same six answers
 * in the same order. The order is load-bearing in one place: an interception
 * marker is a `(…)` *prefix* with a route after it, and a `(group)` is a
 * segment that ends in `)`, so the interception test has to come first or
 * every group would be read as one.
 *
 * @param {string} segment one directory name
 * @returns {{kind: "group"}
 *   | {kind: "param", name: string}
 *   | {kind: "catchAll", name: string}
 *   | {kind: "literal", name: string}
 *   | {kind: "slot", name: string}
 *   | {kind: "interception", marker: string, route: string}}
 */
export function classifyRouteSegment(segment) {
  if (segment.startsWith("@")) return { kind: "slot", name: segment.slice(1) };
  const intercepted = interceptionMarker(segment);
  if (intercepted != null) return { kind: "interception", ...intercepted };
  if (segment.startsWith("(") && segment.endsWith(")")) return { kind: "group" };
  if (segment.startsWith("[...") && segment.endsWith("]")) {
    return { kind: "catchAll", name: segment.slice(4, -1) };
  }
  if (segment.startsWith("[") && segment.endsWith("]")) {
    return { kind: "param", name: segment.slice(1, -1) };
  }
  return { kind: "literal", name: segment };
}

/**
 * The `(.)`-style prefix of `segment` and the route after it, or `null`.
 *
 * One or more of `(.)`, `(..)` and `(...)` — every marker Next.js defines;
 * `(..)(..)` is two of them rather than a fourth — followed by something for
 * them to intercept. A marker with nothing after it names no route and is the
 * `(group)` it has always been.
 */
function interceptionMarker(segment) {
  let consumed = 0;
  while (segment[consumed] === "(") {
    const close = segment.indexOf(")", consumed);
    if (close === -1) break;
    const inner = segment.slice(consumed + 1, close);
    if (inner.length === 0 || inner.length > 3 || /[^.]/.test(inner)) break;
    consumed = close + 1;
  }
  if (consumed === 0 || consumed === segment.length) return null;
  return { marker: segment.slice(0, consumed), route: segment.slice(consumed) };
}

/**
 * Why uf refuses a directory named `segment`, or `null` when it serves it.
 *
 * The message is this router's own rather than `uf_router`'s, because the two
 * are reached differently: the Rust one fails `uf build` and `uf dev` through
 * the route manifest, and this one fails a project driving Vite itself. Both
 * say the same two things — which feature the spelling belongs to, and that it
 * is refused rather than served as a URL.
 */
export function unsupportedSegmentReason(segment) {
  const classified = classifyRouteSegment(segment);
  if (classified.kind === "interception") {
    return (
      `\`${segment}\` is an intercepting route, and uf does not have interception — a navigation ` +
      "carries where it is going and not where it came from, so nothing here could match " +
      `\`${classified.route}\`. It is refused rather than served as the URL segment ` +
      `\`/${segment}\`, which is what it used to become. Move the route to the path it belongs ` +
      "at, or rename the directory. https://github.com/ubugeeei-prod/uf/issues/267"
    );
  }
  return null;
}

/**
 * Turn directory segments into a route path and its parameters.
 *
 * `(group)` segments organise files without appearing in the URL, `[name]`
 * captures one segment, and `[...name]` captures the rest of the path. A
 * `@slot` contributes nothing either — it is a named place a route renders
 * into, matched against the URL of the segment that declares it — so a slot's
 * pages are matched against ordinary paths and add none of their own. An
 * interception never reaches here: {@link scanRoutes} refuses the directory
 * before it walks into it.
 */
export function routeFromSegments(segments) {
  const params = [];
  const out = [];
  for (const segment of segments) {
    const classified = classifyRouteSegment(segment);
    if (classified.kind === "group" || classified.kind === "slot") continue;
    if (classified.kind === "catchAll") {
      params.push({ name: classified.name, catchAll: true });
      out.push(`:${classified.name}*`);
      continue;
    }
    if (classified.kind === "param") {
      params.push({ name: classified.name, catchAll: false });
      out.push(`:${classified.name}`);
      continue;
    }
    out.push(segment);
  }
  const routePath = out.length === 0 ? "/" : `/${out.join("/")}`;
  return { path: routePath, pattern: routePath.replace(/:(\w+)\*/g, "*$1"), params };
}

/**
 * Virtual module ids the router plugin serves.
 *
 * `actions` is the one that does not come from this file's directory scan: it
 * is generated from the RSC manifest by `internal/rsc.js`, because which
 * `"use server"` exports are callable endpoints is an answer about the module
 * graph and not about the filesystem. It is here because it is a virtual
 * module id and this is where they are named, and because
 * `serverModuleSource` below is the only thing that imports it.
 */
export const VIRTUAL = Object.freeze({
  routes: "virtual:uf/routes",
  client: "virtual:uf/client",
  server: "virtual:uf/server",
  actions: "virtual:uf/actions",
});

/**
 * The source of `virtual:uf/routes`.
 *
 * Each page and layout is a lazy `import()`, so a route is a chunk of its own.
 * Layouts are deduplicated into one table so a layout shared by fifty routes
 * is one dynamic import, not fifty. Middleware needs no deduplication: it is
 * already one entry per file, keyed by the path it guards.
 *
 * # The client's copy is not the server's
 *
 * `shipsPage` is how the server/client split reaches the bundle. A route it
 * answers `false` for keeps its path and its parameters — the router still has
 * to *match* the URL, so that a link into it can hand the navigation back to
 * the browser — and loses its `page`, its `layouts` and its `loading`
 * boundaries, which are the only `import()` calls in this table. Nothing in
 * the browser can then reach the module through the router, so Rollup emits no
 * chunk for it and none for anything only it reached.
 *
 * Omitted by leaving the key out rather than by writing `page: null`, because
 * the two say different things to a bundler: a property whose value is an
 * `import()` is a chunk whether or not anything reads it.
 *
 * # Except for its styles
 *
 * A route that ships no JavaScript still has to *look* right, and a uf build
 * takes its stylesheets from the client graph: `assetsFromManifest` walks the
 * client entry's imports and links the CSS it finds, so a module removed from
 * that graph takes its rules out of every page in the site. That is a silent
 * visual break, and it is worse than shipping the module.
 *
 * So each module a dropped route was the only reader of comes back at the top
 * of this file as a bare `import <file>;` — a side-effect import, with no
 * binding read from it. Its stylesheet is a side effect and survives; its
 * components, its helpers and everything only they referenced are unused
 * exports and do not. A layout a *kept* route still uses is left out of that
 * list: it is already here as a lazy import, and a static one as well would
 * pull it into the entry chunk.
 *
 * The default answers `true` for every route, which is the whole table, no
 * side-effect imports, and exactly what this emitted before the split existed.
 * `virtual:uf/server` is generated with the default and always will be: the
 * server renders every route, so its table is the complete one.
 *
 * # `relativeTo`, and the one string in this table a browser can read
 *
 * Every `import()` here is a specifier Vite resolves and rewrites to a chunk
 * URL, so no absolute path survives the build — except `file`, which is a
 * string. It is the route's source path, kept for diagnostics: the middleware
 * table's is what names a module in an error, and `router.js`'s generated
 * types are about the same files.
 *
 * The server's table can hold an absolute path; it is read on the machine that
 * has those files. The browser's cannot, because that table is downloaded:
 * uf's own manual shipped `/home/<user>/…/docs/app/guide/cache/$page.mdx`
 * for each of thirty-four routes to every visitor, which publishes the build
 * machine's layout and its user's name for nothing — the browser has no
 * filesystem to resolve them against and reads them only in a message.
 *
 * So the client call passes the project root and every `file` here is emitted
 * relative to it. Diagnostics keep a path a person can act on — a shorter one
 * — and a deploy stops describing the machine it was built on.
 *
 * @param {{
 *   routes: Route[],
 *   handlers?: Handler[],
 *   middleware?: Middleware[],
 *   notFound?: NotFoundBoundary[],
 *   errors?: ErrorBoundary[],
 * }} table
 * @param {{
 *   shipsPage?: (route: Route) => boolean,
 *   relativeTo?: string,
 * }} [options]
 */
export function routesModuleSource(table, options = {}) {
  const shipsPage = options.shipsPage ?? (() => true);
  const relativeTo = options.relativeTo ?? null;
  /**
   * A `file` as this table should state it.
   *
   * Relative even when that means leading `..` segments — a module outside the
   * project root is rare and a `../` path still says where it is without
   * saying where the machine is, which is the whole property. Separators are
   * POSIX because this string is read wherever the bundle is opened rather
   * than where it was written.
   */
  const displayFile = (file) => {
    if (relativeTo == null || file == null) {
      return file;
    }
    return path.relative(relativeTo, file).split(path.sep).join("/");
  };
  const layoutIds = new Map();
  const layoutImports = [];
  const layoutId = (file) => {
    let id = layoutIds.get(file);
    if (id === undefined) {
      id = `layout${layoutIds.size}`;
      layoutIds.set(file, id);
      layoutImports.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  // Loading modules are deduplicated into a table of their own, for the reason
  // layouts are: one `app/$loading.js` is the fallback of every route under
  // it, and fifty copies of the same `import()` would be fifty chunks of the
  // same file.
  //
  // They are static imports rather than lazy ones, and that is not an
  // oversight. React decides to show a fallback *synchronously*, during the
  // render that suspended, so a fallback still waiting on its own `import()` is
  // a fallback that is not there at the only moment it is wanted — the same
  // reasoning as the error boundaries below, arrived at from the other
  // direction. `resolveMatch` awaits them with the layouts, before it renders.
  const loadingIds = new Map();
  const loadingImports = [];
  const loadingId = (file) => {
    let id = loadingIds.get(file);
    if (id === undefined) {
      id = `loading${loadingIds.size}`;
      loadingIds.set(file, id);
      loadingImports.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  // Templates are deduplicated for the reason layouts are — one
  // `app/$template.js` wraps every route under it — and are lazy for the
  // reason layouts are too: a template is part of the route's own tree rather
  // than a fallback React has to have in hand at the moment something goes
  // wrong, so it is awaited with the layouts before the first render.
  const templateIds = new Map();
  const templateImports = [];
  const templateId = (file) => {
    let id = templateIds.get(file);
    if (id === undefined) {
      id = `template${templateIds.size}`;
      templateIds.set(file, id);
      templateImports.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  // Slots are hoisted like layouts and deduplicated by identity rather than by
  // file: one `scanRoutes` slot record is shared by every route at or below the
  // segment that declares it, so the object is the key. A slot holds a whole
  // route table of its own, and emitting it once per route below it would be
  // that table copied into the bundle once per route.
  //
  // Nested slots are emitted before the slot that holds them, because a `const`
  // cannot read one declared after it.
  const slotIds = new Map();
  const slotDefinitions = [];
  const slotFiles = new Set();
  const slotId = (slot) => {
    let id = slotIds.get(slot);
    if (id !== undefined) {
      return id;
    }
    const routes = slot.routes.map((route) => {
      slotFiles.add(route.page);
      for (const file of route.layouts) slotFiles.add(file);
      const nested = route.slots.map(slotId);
      return `    {
      path: ${JSON.stringify(route.path)},
      params: ${JSON.stringify(route.params)},
      mdx: ${route.mdx},
      file: ${JSON.stringify(displayFile(route.page))},
      page: () => import(${JSON.stringify(route.page)}),
      layouts: [${route.layouts.map(layoutId).join(", ")}],
      slots: [${nested.join(", ")}],
    }`;
    });
    // After the routes, so a nested slot's `const` is already emitted.
    id = `slot${slotIds.size}`;
    slotIds.set(slot, id);
    if (slot.defaultPage != null) {
      slotFiles.add(slot.defaultPage);
    }
    const fallback =
      slot.defaultPage == null
        ? "    defaultPage: null,"
        : `    defaultPage: () => import(${JSON.stringify(slot.defaultPage)}),
    defaultFile: ${JSON.stringify(displayFile(slot.defaultPage))},`;
    slotDefinitions.push(`const ${id} = {
    name: ${JSON.stringify(slot.name)},
    above: ${slot.above},
${fallback}
    defaultMdx: ${slot.defaultMdx},
    routes: [
${routes.join(",\n")}
    ],
  };`);
    return id;
  };

  const entries = table.routes.map((route) => {
    if (!shipsPage(route)) {
      return `  {
    path: ${JSON.stringify(route.path)},
    params: ${JSON.stringify(route.params)},
    mdx: ${route.mdx},
    file: ${JSON.stringify(displayFile(route.page))},
    layouts: [],
    loading: [],
    templates: [],
    slots: [],
  }`;
    }
    const layouts = route.layouts.map(layoutId);
    const loading = (route.loading ?? []).map(
      (boundary) => `{ above: ${boundary.above}, module: ${loadingId(boundary.module)} }`,
    );
    const templates = (route.templates ?? []).map(
      (entry) => `{ above: ${entry.above}, module: ${templateId(entry.module)} }`,
    );
    const slots = (route.slots ?? []).map(slotId);
    return `  {
    path: ${JSON.stringify(route.path)},
    params: ${JSON.stringify(route.params)},
    mdx: ${route.mdx},
    file: ${JSON.stringify(displayFile(route.page))},
    page: () => import(${JSON.stringify(route.page)}),
    layouts: [${layouts.join(", ")}],
    loading: [${loading.join(", ")}],
    templates: [${templates.join(", ")}],
    slots: [${slots.join(", ")}],
  }`;
  });

  // A boundary the scan synthesised has no module to import — the framework's
  // own page renders in its place — so it emits `null` where a declared one
  // emits a loader, and a name for `file` rather than a path nothing wrote.
  // See the note in `scanRoutes` and ubugeeei-prod/uf#351.
  const SYNTHESISED = JSON.stringify("@uniflowed/router");
  const boundaryModule = (file) =>
    file == null ? "null" : `() => import(${JSON.stringify(file)})`;
  const boundaryFile = (file) => (file == null ? SYNTHESISED : JSON.stringify(displayFile(file)));

  // A list, because a not-found is a segment file: every directory may declare
  // one and the router takes the nearest above the path. `layoutId` is the
  // same table the routes use, so a boundary that shares a layout with a page
  // shares its dynamic import too.
  const notFoundEntries = (table.notFound ?? []).map(
    (boundary) => `  {
    path: ${JSON.stringify(boundary.path)},
    mdx: ${boundary.mdx},
    file: ${boundaryFile(boundary.page)},
    page: ${boundaryModule(boundary.page)},
    layouts: [${boundary.layouts.map(layoutId).join(", ")}],
  }`,
  );

  // An error boundary is loaded with the route it guards rather than when it
  // is needed: React decides to render a boundary's fallback synchronously,
  // during the render that threw, so a module that still has to be imported is
  // a module that is not there when the only chance to use it arrives.
  const errorEntries = (table.errors ?? []).map(
    (boundary) => `  {
    path: ${JSON.stringify(boundary.path)},
    file: ${boundaryFile(boundary.module)},
    module: ${boundaryModule(boundary.module)},
    layouts: [${boundary.layouts.map(layoutId).join(", ")}],
  }`,
  );

  // Handlers are a separate table because nothing on the client wants them:
  // a route handler answers a request, so shipping its module to the browser
  // would ship server code to the page.
  const handlerEntries = (table.handlers ?? []).map(
    (handler) => `  {
    path: ${JSON.stringify(handler.path)},
    params: ${JSON.stringify(handler.params)},
    file: ${JSON.stringify(displayFile(handler.module))},
    load: () => import(${JSON.stringify(handler.module)}),
  }`,
  );

  // Middleware is a table of its own for the same reason, and for a stronger
  // one: it is where an application puts the check it does not want a user to
  // read. `clientModuleSource` imports `routes`, `notFound` and `errors` and
  // nothing else, so a middleware module is reachable from the server entry
  // alone.
  const middlewareEntries = (table.middleware ?? []).map(
    (entry) => `  {
    path: ${JSON.stringify(entry.path)},
    file: ${JSON.stringify(displayFile(entry.module))},
    load: () => import(${JSON.stringify(entry.module)}),
  }`,
  );

  // Last, because it is defined by what everything above did *not* import: a
  // layout a kept route also uses is already in the graph as a lazy chunk, and
  // importing it here as well would pull it into the entry chunk instead.
  const carried = new Set([
    ...layoutIds.keys(),
    ...loadingIds.keys(),
    ...templateIds.keys(),
    ...slotFiles,
  ]);
  const styleOnlyImports = [];
  for (const route of table.routes) {
    if (shipsPage(route)) {
      continue;
    }
    const files = [
      route.page,
      ...route.layouts,
      ...(route.loading ?? []).map((it) => it.module),
      ...(route.templates ?? []).map((it) => it.module),
      ...slotModuleFiles(route.slots ?? []),
    ];
    for (const file of files) {
      if (carried.has(file)) {
        continue;
      }
      carried.add(file);
      styleOnlyImports.push(`import ${JSON.stringify(file)};`);
    }
  }

  return `${[...styleOnlyImports, ...layoutImports, ...loadingImports, ...templateImports, ...slotDefinitions].join("\n")}
export const routes = [
${entries.join(",\n")}
];
export const handlers = [
${handlerEntries.join(",\n")}
];
export const middleware = [
${middlewareEntries.join(",\n")}
];
export const notFound = [
${notFoundEntries.join(",\n")}
];
export const errors = [
${errorEntries.join(",\n")}
];
export default routes;
`;
}

/**
 * Every module a slot tree holds, flattened.
 *
 * For the side-effect imports a dropped route needs: a slot's pages, its
 * layouts and its default are as much a part of that route's stylesheets as
 * its own page is, and a slot nested inside one is too.
 *
 * @param {ReadonlyArray<Slot>} slots
 * @returns {Array<string>}
 */
function slotModuleFiles(slots) {
  const files = [];
  for (const slot of slots) {
    if (slot.defaultPage != null) files.push(slot.defaultPage);
    for (const route of slot.routes) {
      files.push(route.page, ...route.layouts, ...slotModuleFiles(route.slots));
    }
  }
  return files;
}

/**
 * The source of `virtual:uf/client`: hydrate the document with the app.
 *
 * The current route's modules are loaded *before* hydration so the first
 * render is synchronous and matches the server's HTML; a lazy import during
 * hydration would suspend and React would fall back to a client render.
 *
 * # Strict Mode is a generated constant, not a runtime check
 *
 * `strictMode` is written into this module as a literal, so a production build
 * gets `hydrate({ … })` with the argument absent and Rollup has nothing to
 * decide. It would have been shorter to have `hydrate` read `import.meta.hot`
 * — the way `client.js` gates the hydration reporter — and that would have been
 * one signal answering two questions: `uf.config.js` can turn Strict Mode off
 * (ubugeeei-prod/uf#516) and `import.meta.hot` cannot be told about it. A
 * project that sets `app.react.strictMode: false` gets a dev server that
 * hydrates the way its deployment does, which is the whole of the escape
 * hatch.
 *
 * # Navigation is a generated constant for a different reason
 *
 * `app.rendering.navigation` decides whether the client router takes a link
 * over, and it is written in here for the reason Strict Mode is not: it must
 * be the *same* in development and in the build. A `uf dev` whose links
 * resolve in the page and a deployment whose links fetch a document are two
 * applications, and the one a person is looking at is the one that is not
 * deployed. So this is generated from the config with no `isProduction` beside
 * it, and `uf dev`, `uf build` and `uf preview` all get what the project asked
 * for.
 *
 * `"client"` is emitted as an absent argument rather than as
 * `navigation: "client"`, so the module a default project gets is byte for
 * byte the one it got before this option existed.
 *
 * # And whether it hydrates at all
 *
 * `mount` is the third generated constant and the one that changes which
 * function is imported. A `["csr"]` build wrote one shell with an empty root,
 * so there is no markup to attach to and `render` is what starts the
 * application; every other build has markup, and `hydrate` attaches to it.
 *
 * One import or the other, rather than one import and a branch, because they
 * are two different React entry points: a bundle that mounts by hydrating has
 * no reason to carry `createRoot`, and a bundle that renders has none to carry
 * `hydrateRoot`. Which one is in the module decides which one is in the build.
 *
 * @param {string} appEntry the project's `app.js`, as an import specifier
 * @param {{
 *   strictMode?: boolean,
 *   navigation?: "client" | "document",
 *   mount?: "hydrate" | "render",
 * }} [options]
 */
export function clientModuleSource(appEntry, options = {}) {
  const strictMode = options.strictMode === true ? ", strictMode: true" : "";
  const navigation = options.navigation === "document" ? ', navigation: "document"' : "";
  const mount = options.mount === "render" ? "render" : "hydrate";
  return `import { ${mount} } from "@uniflowed/router/client";
import { routes, notFound, errors } from ${JSON.stringify(VIRTUAL.routes)};
import App from ${JSON.stringify(appEntry)};
${mount}({ App, routes, notFound, errors${strictMode}${navigation} });
`;
}

/**
 * The source of `virtual:uf/server`: answer one request.
 *
 * Three exports, and the order a host calls them in is the whole of how the
 * two halves of the table compose. `runMiddleware` first, because a middleware
 * guards a *path* — it has to run for a page, for a route handler, and for a
 * path under it that matches neither, so it belongs above route resolution
 * rather than inside it. `notFound` and `errors` go the other way: they are
 * boundaries chosen *during* a render, once resolution knows which route was
 * asked for and whether it threw, which is why they are `createRenderer`'s
 * arguments and not a step of their own. The two never compete for the same
 * request — one decides whether the router is reached at all, the others
 * decide what the router renders when it is.
 *
 * `callAction` goes between the two, and its position is the same argument
 * made twice. Below `runMiddleware`, because an action call is a request to a
 * path and the guard on that path is owed the same say over it as over the
 * page — which is why the call is a `POST` to the page's own URL rather than
 * to a reserved one. Above `dispatch`, because a request that names an action
 * has named it: letting it fall through to a route handler that happens to sit
 * at the same path would answer somebody's action with somebody else's
 * function. It declines every request that carries no action id, so a project
 * with no actions pays one `headers.get` per request and nothing else.
 *
 * `internal/serve.js` and `driver.js` call them in that order, and
 * `packages/vite/index.js` does the same for a project driving Vite itself.
 *
 * `render` and `prerender` are two exports rather than one with a flag, because
 * a host is one or the other: a server streams, a build writes files. See the
 * header of `packages/router/server.js` for why React needs both told apart.
 *
 * `shellDocument` is the third and is neither: it renders no route, because a
 * `["csr"]` build has none to render at build time. It is re-exported straight
 * from the router rather than closed over the table, which says the true thing
 * about it — the shell is a function of the assets alone, and the route table
 * has nothing to do with a document that is no route's.
 *
 * `beginRequest` is the fourth, and it is re-exported rather than imported by
 * the host for a reason that is easy to get wrong: `@uniflowed/server` keeps
 * the request in an `AsyncLocalStorage` held by *its module*, and a bundled
 * application has its own copy of that module inlined. A host that imported
 * `beginRequest` from its own `node_modules` would establish a request in a
 * second storage, and every `cookies()` in the application would still be
 * outside one. So the bundle hands the host the entry point that belongs to
 * the bundle. `uf preview`, `uf start`, `uf dev` and the compiled binary all
 * take it from here; see ubugeeei-prod/uf#389.
 *
 * Through `@uniflowed/router/server` rather than `@uniflowed/server/host`,
 * because this source is resolved from the *project's* directory and a project
 * depends on the router, not on the router's own dependency. It is also the
 * shorter proof of the paragraph above: the copy the router dispatches and
 * renders with is by construction the copy the host is handed.
 */
export function serverModuleSource(appEntry) {
  return `import {
  createActionDispatcher,
  createDispatcher,
  createMiddlewareRunner,
  createRenderer,
} from "@uniflowed/router/server";
import { routes, handlers, middleware, notFound, errors } from ${JSON.stringify(VIRTUAL.routes)};
import { actions } from ${JSON.stringify(VIRTUAL.actions)};
import App from ${JSON.stringify(appEntry)};
export { routes, handlers, middleware, notFound, errors };
export { beginRequest } from "@uniflowed/router/server";
const renderer = createRenderer({ App, routes, notFound, errors });
export const render = renderer.render;
export const prerender = renderer.prerender;
export { shellDocument } from "@uniflowed/router/server";
export const dispatch = createDispatcher({ handlers });
export const callAction = createActionDispatcher({ actions });
export const runMiddleware = createMiddlewareRunner({ middleware });
`;
}
