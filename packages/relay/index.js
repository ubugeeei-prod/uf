// @flow
//
// `@uniflowed/relay`: Relay, re-exported.
//
// This is the real `react-relay`, not a declaration of it. uf owns
// orchestration, not implementation: Relay is a store, a normaliser, a compiler
// and a decade of work on cache consistency, and a uf-shaped reimplementation
// would be a worse Relay that uf then had to keep up with.
//
// What uf adds is the toolchain around it. `uf check` type checks `graphql`
// tagged templates against the artifacts Relay's compiler generates, and
// `uf transform` rewrites those tags to the artifact they name. The library is
// Meta's; the integration is uf's.
//
// `export *` rather than a list of names: the list was a second copy of
// Relay's export surface, and Relay adds hooks faster than uf releases.
//
// Relay is a peer dependency, so its version is the project's. An application
// can pin or override one with its package manager's `resolutions` without
// waiting for a uf release.

export * from "react-relay";

import * as ReactRelay from "react-relay";

export { ReactRelay };
