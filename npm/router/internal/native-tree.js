// @flow
import type { RouteModule, RouteTable } from "./routing.js";

export type NativeLayout = {
  readonly segment: string,
  readonly file: string,
  readonly module: RouteModule<>,
};
export type NativeNode = {
  name: string,
  module: ?RouteModule<>,
  layout: boolean,
  children: Array<NativeNode>,
};
export type NativeTree = { root: NativeNode, paths: Map<string, $ReadOnlyArray<string>> };

/** Module identity comes from the generated table; group names never become URLs. */
export function nativeTree(table: RouteTable<>, layouts: $ReadOnlyArray<NativeLayout>): NativeTree {
  const rootLayout = layouts.find((layout) => layout.segment === "/");
  const root: NativeNode = { name: "root", module: rootLayout?.module, layout: true, children: [] };
  const metadata = new Map(layouts.map((layout) => [layout.module, layout]));
  const paths = new Map<string, $ReadOnlyArray<string>>();
  for (const route of table.routes) {
    if (route.page == null) continue;
    let parent = root;
    const path = [];
    for (const load of route.layouts) {
      if (load === rootLayout?.module) continue;
      const layout = metadata.get(load);
      if (layout == null)
        throw new Error(`Native navigation: missing layout metadata for ${route.path}`);
      const name = `layout:${layout.segment}`;
      let child = parent.children.find((node) => node.name === name);
      if (child == null) {
        child = { name, module: load, layout: true, children: [] };
        parent.children.push(child);
      }
      parent = child;
      path.push(name);
    }
    parent.children.push({ name: route.path, module: route.page, layout: false, children: [] });
    path.push(route.path);
    paths.set(route.path, path);
  }
  return { root, paths };
}

export type NavigationPayload = {
  readonly ufHref: string,
  readonly ufParams: { readonly [string]: string | $ReadOnlyArray<string> },
};
export type NavigationState = {
  readonly key?: string,
  readonly type?: string,
  readonly index?: number,
  readonly routes: $ReadOnlyArray<{
    readonly name: string,
    readonly params?: NavigationPayload,
    readonly state?: NavigationState,
  }>,
};

export function stateForPath(
  names: $ReadOnlyArray<string>,
  payload: NavigationPayload,
): NavigationState {
  const [name, ...rest] = names;
  if (name == null) throw new Error("Native navigation: an empty screen path cannot be opened");
  return {
    routes: [
      {
        name,
        ...(rest.length === 0 ? { params: payload } : { state: stateForPath(rest, payload) }),
      },
    ],
  };
}

export function hrefFromState(state: NavigationState): string | null {
  const route = state.routes[state.index ?? state.routes.length - 1];
  if (route == null) return null;
  return route.state == null ? (route.params?.ufHref ?? null) : hrefFromState(route.state);
}

/** React Navigation's nested navigate payload, separate from user route params. */
export function paramsForPath(names: $ReadOnlyArray<string>, payload: NavigationPayload): mixed {
  const [screen, ...rest] = names;
  return screen == null ? payload : { screen, params: paramsForPath(rest, payload) };
}
