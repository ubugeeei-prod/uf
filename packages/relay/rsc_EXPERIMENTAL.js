// @flow
// Relay owns request-local caching and the preload protocol. Keep its explicit
// experimental entry point and types rather than inventing a parallel API.
export { createServerEnvironment } from "react-relay/rsc_EXPERIMENTAL.js";
export type { PreloadedQueryRef, ServerEnvironment } from "react-relay/rsc_EXPERIMENTAL.js";
