// @flow
// Relay owns the runtime, normalized cache, compiler, and public types.
// Named value re-exports preserve Vite's CommonJS interop in the browser;
// export-star alone does not expose a prebundled CommonJS module's members.
// The namespace keeps access to the complete upstream runtime as it evolves.
// uf dev/build use Relay's Babel plugin; uf run relay runs the app's compiler.
export {
  ConnectionHandler,
  EntryPointContainer,
  LocalQueryRenderer,
  MutationTypes,
  ProfilerContext,
  QueryRenderer,
  RangeOperations,
  ReactRelayContext,
  RelayEnvironmentProvider,
  applyOptimisticMutation,
  commitLocalUpdate,
  commitMutation,
  createFragmentContainer,
  createPaginationContainer,
  createRefetchContainer,
  fetchQuery,
  fetchQuery_DEPRECATED,
  graphql,
  loadEntryPoint,
  loadQuery,
  readInlineData,
  requestSubscription,
  useClientQuery,
  useEntryPointLoader,
  useFragment,
  useLazyLoadQuery,
  useMutation,
  useMutationAction_EXPERIMENTAL,
  usePaginationFragment,
  usePrefetchableForwardPaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  useRefetchableFragment,
  useRelayEnvironment,
  useSubscribeToInvalidationState,
  useSubscription,
} from "react-relay";
export type * from "react-relay";
import * as ReactRelay from "react-relay";
export { ReactRelay };
