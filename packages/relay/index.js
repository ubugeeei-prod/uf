// @flow
//
// `@uniflowed/relay`: Relay, re-exported by name.
//
// This is the real `react-relay`, not a declaration of it — the same shape
// `@uniflowed/react` has for `react`. uf owns orchestration, not
// implementation: Relay is a store, a normaliser, a compiler and a decade of
// work on cache consistency, and a uf-shaped reimplementation of it would be a
// worse Relay that uf then had to keep up to date.
//
// What uf adds is the rest of the toolchain around it. `uf check` type checks
// `graphql` tagged templates against the artifacts Relay's compiler generates,
// `uf transform` rewrites those tags to the artifact they name, and the
// generated files are part of the build rather than a step somebody has to
// remember. The library is Meta's; the integration is uf's.
//
// Naming every export rather than `export *` keeps the surface explicit — a
// name not listed here is not part of what uf documents — and lets a bundler
// drop the ones an application never touches. The Flow types come from Relay's
// own `.js.flow` files, so `import type { Environment } from
// "@uniflowed/relay"` is exactly Relay's own type.

import * as ReactRelay from "react-relay";

export type {
  EntryPoint,
  EntryPointComponent,
  EntryPointProps,
  GraphQLTaggedNode,
  PreloadedQuery,
  RelayProp,
  RelayPaginationProp,
  RelayRefetchProp,
} from "react-relay";

export {
  // The environment, and getting at it.
  RelayEnvironmentProvider,
  ReactRelayContext,
  useRelayEnvironment,

  // Reading data. These are the hooks a modern Relay application is written
  // in; the container factories below are the older API and are here because
  // an application being migrated still has them.
  useClientQuery,
  useFragment,
  useLazyLoadQuery,
  usePaginationFragment,
  usePreloadedQuery,
  usePrefetchableForwardPaginationFragment,
  useRefetchableFragment,
  readInlineData,

  // Loading ahead of render, which is how a router avoids a waterfall.
  loadQuery,
  loadEntryPoint,
  useEntryPointLoader,
  useQueryLoader,
  EntryPointContainer,

  // Writing.
  commitMutation,
  commitLocalUpdate,
  applyOptimisticMutation,
  useMutation,
  MutationTypes,
  RangeOperations,

  // Subscriptions.
  requestSubscription,
  useSubscription,
  useSubscribeToInvalidationState,

  // Imperative fetching, outside a component.
  fetchQuery,

  // The tag. `uf transform` rewrites it to the artifact Relay's compiler
  // generated, so this binding is what a module imports and never what runs.
  graphql,

  // Connections.
  ConnectionHandler,

  // Profiling.
  ProfilerContext,

  // The container API, for applications that have not finished moving to
  // hooks. Not recommended for new code, and not removed: uf does not get to
  // decide when somebody else's migration is over.
  createFragmentContainer,
  createPaginationContainer,
  createRefetchContainer,
  QueryRenderer,
  LocalQueryRenderer,
} from "react-relay";

export { ReactRelay };
