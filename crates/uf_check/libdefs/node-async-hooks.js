/**
 * @fileoverview Node's `async_hooks` module, which the vendored `node.js` does
 * not declare at all.
 *
 * Without a declaration, `import { AsyncLocalStorage } from "node:async_hooks"`
 * is an untyped import. Every value from it is `any`, and using the class as a
 * type (`AsyncLocalStorage<RequestContext>`) is "an any-typed value", so the
 * request context, the cache scope and the test runner's output owner in this
 * repository were all unchecked (ubugeeei-prod/uf#1451). Node, Deno and Bun
 * all implement `AsyncLocalStorage`, which is why a uf project reaches for it.
 *
 * Everything the module exports is declared, so that the parts a project did
 * not use before do not become errors now: the hooks API is the one Node
 * documents, and the `AsyncLocalStorage` and `AsyncResource` signatures follow
 * https://nodejs.org/api/async_context.html.
 */

declare module "async_hooks" {
  /** A store that follows an asynchronous call chain. */
  declare class AsyncLocalStorage<T> {
    constructor(options?: { defaultValue?: T, name?: string, ... }): void;
    /** A function that runs `fn` in the context it was bound in. */
    static bind<F extends (...args: $ReadOnlyArray<empty>) => mixed>(fn: F): F;
    /** Capture the current context; the result runs a function inside it. */
    static snapshot(): <R, A extends $ReadOnlyArray<mixed>>(fn: (...args: A) => R, ...args: A) => R;
    /** The store of the current call chain, or `undefined` outside `run`. */
    getStore(): T | void;
    /** Run `callback` with `store` as the current store. */
    run<R, A extends $ReadOnlyArray<mixed>>(store: T, callback: (...args: A) => R, ...args: A): R;
    /** Run `callback` outside any store. */
    exit<R, A extends $ReadOnlyArray<mixed>>(callback: (...args: A) => R, ...args: A): R;
    /** Make `store` current for the rest of this synchronous execution. */
    enterWith(store: T): void;
    /** Stop tracking; `getStore` answers `undefined` from here on. */
    disable(): void;
    readonly name: string;
  }

  /** A resource that carries its creation context to later callbacks. */
  declare class AsyncResource {
    constructor(
      type: string,
      triggerAsyncId?: number | { triggerAsyncId?: number, requireManualDestroy?: boolean, ... },
    ): void;
    static bind<F extends (...args: $ReadOnlyArray<empty>) => mixed>(fn: F, type?: string, thisArg?: mixed): F;
    bind<F extends (...args: $ReadOnlyArray<empty>) => mixed>(fn: F): F;
    runInAsyncScope<R, A extends $ReadOnlyArray<mixed>>(
      fn: (...args: A) => R,
      thisArg?: mixed,
      ...args: A
    ): R;
    emitDestroy(): this;
    asyncId(): number;
    triggerAsyncId(): number;
  }

  declare type HookCallbacks = {
    init?: (asyncId: number, type: string, triggerAsyncId: number, resource: {...}) => void,
    before?: (asyncId: number) => void,
    after?: (asyncId: number) => void,
    destroy?: (asyncId: number) => void,
    promiseResolve?: (asyncId: number) => void,
    ...
  };

  declare interface AsyncHook {
    enable(): this;
    disable(): this;
  }

  declare function createHook(callbacks: HookCallbacks): AsyncHook;
  declare function executionAsyncId(): number;
  declare function triggerAsyncId(): number;
  declare function executionAsyncResource(): {...};
  declare var asyncWrapProviders: { readonly [provider: string]: number, ... };
}

declare module "node:async_hooks" {
  declare module.exports: $Exports<"async_hooks">;
}
