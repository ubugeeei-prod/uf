/**
 * React 19 Parcel Flight entry points. Flight decodes a caller's model, so
 * its model parameter is generic; transport arguments remain platform typed.
 */
declare module "react-server-dom-parcel/client.browser" {
  declare export type Options = {
    readonly temporaryReferences?: Set<mixed>,
    ...
  };
  declare export function createFromFetch<T = mixed>(
    response: Promise<Response>,
    options?: Options,
  ): Promise<T>;
  declare export function createFromReadableStream<T = mixed>(
    stream: ReadableStream<Uint8Array>,
    options?: Options,
  ): Promise<T>;
  declare export function setServerCallback(
    callback: (id: string, args: Array<mixed>) => Promise<mixed>,
  ): void;
  declare export function createServerReference<Args extends $ReadOnlyArray<mixed>, Result>(
    id: string,
    exportName: string,
  ): (...args: Args) => Promise<Result>;
  declare export function createTemporaryReferenceSet(): Set<mixed>;
  declare export function encodeReply(
    value: mixed,
    options?: { readonly signal?: AbortSignal, readonly temporaryReferences?: Set<mixed>, ... },
  ): Promise<string | FormData>;
  declare export function registerServerReference<F extends (...args: $ReadOnlyArray<empty>) => mixed>(
    reference: F,
    id: string,
  ): F;
}

declare module "react-server-dom-parcel/client.edge" {
  declare export type Options = {
    readonly temporaryReferences?: Set<mixed>,
    ...
  };
  declare export function createFromFetch<T = mixed>(
    response: Promise<Response>,
    options?: Options,
  ): Promise<T>;
  declare export function createFromReadableStream<T = mixed>(
    stream: ReadableStream<Uint8Array>,
    options?: Options,
  ): Promise<T>;
  declare export function createServerReference<Args extends $ReadOnlyArray<mixed>, Result>(
    id: string,
  ): (...args: Args) => Promise<Result>;
  declare export function createTemporaryReferenceSet(): Set<mixed>;
  declare export function encodeReply(
    value: mixed,
    options?: { readonly signal?: AbortSignal, readonly temporaryReferences?: Set<mixed>, ... },
  ): Promise<string | FormData>;
  declare export function registerServerReference<F extends (...args: $ReadOnlyArray<empty>) => mixed>(
    reference: F,
    id: string,
    encodeFormAction?: (id: string, args: Promise<Array<mixed>>) => mixed,
  ): F;
}

declare module "react-server-dom-parcel/server" {
  declare export type Options = {
    readonly onError?: (error: mixed) => string | void,
    readonly signal?: AbortSignal,
    readonly identifierPrefix?: string,
    readonly temporaryReferences?: WeakMap<Object, string>,
    ...
  };
  declare export function renderToReadableStream(model: mixed, options?: Options): ReadableStream<Uint8Array>;
  declare export function registerServerReference<F extends (...args: $ReadOnlyArray<empty>) => mixed>(
    reference: F,
    id: string,
    exportName: string,
  ): F;
  declare export function createTemporaryReferenceSet(): WeakMap<Object, string>;
  declare export function decodeReply<T = mixed>(
    body: string | FormData,
    options?: { readonly temporaryReferences?: WeakMap<Object, string>, ... },
  ): Promise<T>;
  declare export function decodeReplyFromAsyncIterable<T = mixed>(
    body: AsyncIterable<[string, mixed]>,
    options?: { readonly temporaryReferences?: WeakMap<Object, string>, ... },
  ): Promise<T>;
  declare export function decodeAction(body: FormData): Promise<(() => Promise<mixed>) | null>;
  declare export function decodeFormState(actionResult: mixed, body: FormData): Promise<mixed>;
  declare export function createClientReference(
    id: string,
    exportName: string,
    bundles: $ReadOnlyArray<string>,
    importMap?: { readonly [string]: string },
  ): mixed;
  declare export function registerServerActions(manifest: { readonly [string]: mixed }): void;
  declare export function loadServerAction(id: string): Promise<(...args: Array<mixed>) => Promise<mixed>>;
}
