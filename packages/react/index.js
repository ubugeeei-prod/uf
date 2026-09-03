// @flow
//
// `@uniflowed/react`: React, re-exported by name.
//
// This is the real `react` package, not a declaration of it. Naming every
// export rather than `export *` keeps the surface explicit — a name not listed
// here is not part of what uf documents — and lets a bundler drop the ones an
// application never touches. The Flow types come from Flow's own library
// definition for `react`, so `import type { Node } from "@uniflowed/react"`
// is exactly `import type { Node } from "react"`.

import * as React from "react";

export type {
  AbstractComponent,
  ChildrenArray,
  ComponentType,
  Config,
  Context,
  Element,
  ElementConfig,
  ElementProps,
  ElementRef,
  ElementType,
  Key,
  MixedElement,
  Node,
  Portal,
  Ref,
  RefSetter,
  StatelessFunctionalComponent,
} from "react";

export {
  Children,
  Component,
  Fragment,
  Profiler,
  PureComponent,
  StrictMode,
  Suspense,
  act,
  cache,
  cloneElement,
  createContext,
  createElement,
  createRef,
  forwardRef,
  isValidElement,
  lazy,
  memo,
  startTransition,
  use,
  useActionState,
  useCallback,
  useContext,
  useDebugValue,
  useDeferredValue,
  useEffect,
  useId,
  useImperativeHandle,
  useInsertionEffect,
  useLayoutEffect,
  useMemo,
  useOptimistic,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
  useTransition,
  version,
} from "react";

/** The setter shape `useState` hands back. */
export type SetState<S> = S | ((previous: S) => S);

export { React };
export default React;
