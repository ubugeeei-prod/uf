// @flow
import * as React from "react";
import { Pressable } from "react-native";
import {
  CommonActions,
  NavigationContainer,
  StackActions,
  createNavigationContainerRef,
  useRoute,
} from "@react-navigation/native";
import { resolveNativeNavigation, createNativeRouter, createNativeLinking } from "./native.js";
import type { NativeLinkSource } from "./native.js";
import type { RouteTable } from "./internal/routing.js";
import type {
  NativeLayout,
  NativeNode,
  NativeTree,
  NavigationPayload,
  NavigationState,
} from "./internal/native-tree.js";
import { nativeTree, stateForPath, hrefFromState, paramsForPath } from "./internal/native-tree.js";

export interface NavigatorPair {
  readonly Navigator: React.ComponentType<{ ... }>;
  readonly Screen: React.ComponentType<{ ... }>;
}
export type FileRouter = {
  readonly push: (href: string) => void,
  readonly replace: (href: string) => void,
  readonly back: () => void,
  readonly prefetch: (href: string) => Promise<void>,
};
type Scope = {
  tree: NativeTree,
  node: NativeNode,
  screens: Map<NativeNode, React.ComponentType<{ ... }>>,
  stack: NavigatorPair,
  tabs: ?NavigatorPair,
  router: FileRouter,
};
const ScopeContext: React.Context<Scope | null> = React.createContext(null);

function useScope(): Scope {
  const scope = React.useContext(ScopeContext);
  if (scope == null)
    throw new Error(
      "Native navigation: mount createNativeNavigation().Root before using its navigator or hooks",
    );
  return scope;
}

/** The application supplies its own React Navigation navigator factories. */
export function createNativeNavigation(options: {
  table: RouteTable<>,
  layouts: $ReadOnlyArray<NativeLayout>,
  stack: NavigatorPair,
  tabs?: NavigatorPair,
  initialHref?: string,
  links?: NativeLinkSource,
}): { Root: React.ComponentType<{ ... }>, router: FileRouter, linking: { ... } } {
  const { table, layouts, stack, tabs } = options;
  const tree = nativeTree(table, layouts);
  const ref = createNavigationContainerRef();
  const screens = new Map<NativeNode, React.ComponentType<{ ... }>>();
  const resolve = (href: string): { names: $ReadOnlyArray<string>, payload: NavigationPayload } => {
    const event = resolveNativeNavigation(table, href);
    const names = tree.paths.get(event.route);
    if (names == null) throw new Error(`Native navigation: no screen for ${event.route}`);
    return { names, payload: { ufHref: event.href, ufParams: event.params } };
  };
  function actionFor(
    names: $ReadOnlyArray<string>,
    payload: NavigationPayload,
    kind: "push" | "replace",
  ): { readonly [string]: mixed } {
    if (!ref.isReady()) throw new Error("Native navigation: NavigationContainer is not ready");
    let state: NavigationState = ref.getRootState();
    let depth = 0;
    while (depth < names.length - 1) {
      const active = state.routes[state.index ?? 0];
      if (active?.name !== names[depth] || active?.state == null) break;
      state = active.state;
      depth += 1;
    }
    const name = names[depth];
    const params = paramsForPath(names.slice(depth + 1), payload);
    const action =
      state.type === "stack"
        ? StackActions[kind](name, params)
        : CommonActions.navigate({ name, params });
    return { ...action, target: state.key };
  }
  function navigate(href: string, kind: "push" | "replace"): void {
    const { names, payload } = resolve(href);
    ref.dispatch(actionFor(names, payload, kind));
  }
  const router: FileRouter = {
    push: (href) => navigate(href, "push"),
    replace: (href) => navigate(href, "replace"),
    back: () => {
      if (ref.canGoBack()) ref.goBack();
    },
    prefetch: createNativeRouter(table, {}).prefetch,
  };
  const source = options.links == null ? null : createNativeLinking(table, options.links);
  const linking = {
    prefixes: [""],
    getInitialURL: source?.getInitialURL ?? (async () => null),
    subscribe: source?.subscribe ?? (() => () => {}),
    getStateFromPath: (href: string) => {
      const { names, payload } = resolve(
        href.startsWith("/") || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(href) ? href : `/${href}`,
      );
      return stateForPath(names, payload);
    },
    getPathFromState: (state: NavigationState) => hrefFromState(state) ?? "/",
    getActionFromState: (state: NavigationState) => {
      const href = hrefFromState(state);
      if (href == null) return undefined;
      const { names, payload } = resolve(href);
      return actionFor(names, payload, "push");
    },
  };
  function build(node: NativeNode): React.ComponentType<{ ... }> {
    const load = node.module;
    const Content =
      load == null
        ? Stack
        : React.lazy(async () => {
            const module: $FlowFixMe = await load();
            const Component = module.default ?? (node.layout ? module.Layout : module.Page);
            if (
              typeof Component !== "function" &&
              (Component == null || typeof Component !== "object")
            ) {
              throw new Error(
                `Native navigation: ${node.name} must export ${node.layout ? "Layout" : "Page"} or default`,
              );
            }
            return { default: Component };
          });
    function Screen(props: { ... }): React.Node {
      return (
        <ScopeContext value={{ tree, node, screens, stack, tabs, router }}>
          <React.Suspense fallback={null}>
            <Content {...props} />
          </React.Suspense>
        </ScopeContext>
      );
    }
    screens.set(node, Screen);
    for (const child of node.children) build(child);
    return Screen;
  }
  const RootScreen = build(tree.root);
  const initialState =
    options.initialHref == null ? undefined : linking.getStateFromPath(options.initialHref);
  function Root(props: { ... }): React.Node {
    return (
      <NavigationContainer {...props} ref={ref} linking={linking} initialState={initialState}>
        <RootScreen />
      </NavigationContainer>
    );
  }
  return { Root, router, linking };
}

/** Any React Navigation navigator can consume this segment's direct children. */
export component Navigator(navigator: NavigatorPair, ...props: { ... }) {
  const scope = useScope();
  const Container = navigator.Navigator;
  const Screen = navigator.Screen;
  return (
    <Container {...props}>
      {scope.node.children.map((node) => (
        <Screen key={node.name} name={node.name} component={scope.screens.get(node)} />
      ))}
    </Container>
  );
}

export component Stack(...props: { ... }) {
  return <Navigator {...props} navigator={useScope().stack} />;
}

export component Tabs(...props: { ... }) {
  const tabs = useScope().tabs;
  if (tabs == null)
    throw new Error("Native navigation: supply the app's tab navigator to createNativeNavigation");
  return <Navigator {...props} navigator={tabs} />;
}

export function useNativeRouter(): FileRouter {
  return useScope().router;
}
export function useParams(): { readonly [string]: string | $ReadOnlyArray<string> } {
  const route = useRoute();
  return route.params?.ufParams ?? {};
}

export component Link(
  href: string,
  children: React.Node,
  replace: boolean = false,
  ...props: { ... }
) {
  const router = useNativeRouter();
  return (
    <Pressable
      {...props}
      accessibilityRole="link"
      onPress={() => (replace ? router.replace(href) : router.push(href))}
    >
      {children}
    </Pressable>
  );
}
