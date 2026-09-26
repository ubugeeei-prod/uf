/** Node's request-scoped storage, shared by server and test contexts. */
declare module "async_hooks" {
  declare export class AsyncLocalStorage<T> {
    constructor(options?: { readonly defaultValue?: T, readonly name?: string, ... }): void;
    readonly name: string;
    disable(): void;
    getStore(): T | void;
    enterWith(store: T): void;
    run<R, Args extends $ReadOnlyArray<mixed> = []>(
      store: T,
      callback: (...args: Args) => R,
      ...args: Args
    ): R;
    exit<R, Args extends $ReadOnlyArray<mixed> = []>(
      callback: (...args: Args) => R,
      ...args: Args
    ): R;
    static bind<F extends (...args: $ReadOnlyArray<empty>) => mixed>(callback: F): F;
    static snapshot(): <R, Args extends $ReadOnlyArray<mixed> = []>(
      callback: (...args: Args) => R,
      ...args: Args
    ) => R;
  }
}

declare module "node:async_hooks" {
  declare module.exports: $Exports<"async_hooks">;
}
