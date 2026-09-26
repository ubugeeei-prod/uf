// @flow
//
// Which store a React subtree uses.
//
// One context and one component. It is separate from the hooks because it
// answers a different question: the hooks ask "what is this atom's value
// here", and this decides what "here" means.
//
// # Why there is a default store at all
//
// Most applications have exactly one, and making every one of them wrap its
// tree in a provider — and every test, and every story — to get it is
// ceremony that buys nothing. So an unwrapped tree reads the default store,
// which is created on first use.
//
// A provider is what you reach for when one is not enough: a server rendering
// two requests in one process, a preview pane holding a draft of the state the
// page behind it shows, a test that wants a clean slate without reloading the
// module. Those are real, which is why the scoping exists, and they are not
// the common case, which is why it is optional.
//
// # Why the fallback store is made with useState
//
// `<Provider>` with no `store` prop owns one. Creating it in the render body
// would make a new store every render, and every re-render would throw the
// tree's state away; `useState` with a lazy initialiser creates it once and
// keeps it for the life of the component, without an effect that would leave
// the first render with no store to read.

import * as React from "@uniflowed/react";
import { createContext, useContext, useState } from "@uniflowed/react";

import type { StoreInstance } from "./store.js";
import { createStore, defaultStore } from "./store.js";

/**
 * `null` means "nobody scoped one", which is different from a store: it is
 * what makes an unwrapped tree fall through to the default rather than throw.
 */
const StoreContext: React.Context<StoreInstance | null> = createContext(null);

/**
 * Give a subtree its own store.
 *
 * Passing `null` asks for a fresh one owned by this component, which is what
 * the public `Provider` does when it is given no store.
 */
export component StoreScope(store: StoreInstance | null, children: React.Node) {
  const [owned] = useState(createStore);
  return <StoreContext.Provider value={store ?? owned}>{children}</StoreContext.Provider>;
}

/**
 * The store a hook should use: the one it was handed, then the one the tree
 * was scoped to, then the default.
 *
 * `useContext` is called unconditionally even when an override was passed,
 * because the alternative is a conditional hook, and because a component that
 * sometimes takes a store prop must not change its hook order when it does.
 */
export hook useStoreInstance(override?: StoreInstance): StoreInstance {
  const scoped = useContext(StoreContext);
  return override ?? scoped ?? defaultStore();
}
