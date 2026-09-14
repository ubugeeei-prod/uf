// @flow
//
// Internal to `@uniflowed/router`: what renders in place of a subtree that threw.
//
// The framework's error page, the component that picks between it and a
// project's `$error.js`, the page a route that resolved to its error boundary
// renders, and the class boundary that catches a throw while the browser
// renders. Every one of them is the browser's: a boundary is a class, and a
// retry is a navigation, so none of this can run in a graph resolved under
// React's `react-server` condition — which is why it is out of `./runtime.js`'s
// top half and in a module of its own. See ubugeeei-prod/uf#519.

import * as React from "react";

import type { RouteError } from "./routing.js";
import type { ErrorModule } from "./resolve.js";
import { errorTitle, renderable, routeErrorFor } from "./resolve.js";
import { useRouterState } from "./runtime.js";

/**
 * The framework's error page, for a project that declares no `$error.js`.
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
export component ResolvedErrorPage() {
  const { resolved, router } = useRouterState();
  const reset = () => {
    router.refresh().catch(() => {});
  };

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
export class RouteErrorBoundary extends React.Component<
  RouteErrorBoundaryProps,
  RouteErrorBoundaryState,
> {
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
