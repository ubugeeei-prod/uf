// @flow
//
// `@uniflowed/hmr`.

export type {
  ChangeKind,
  ConnectOptions,
  HmrClient,
  HmrUpdate,
  RefreshHandler,
  ReloadReason,
  UpdateKind,
  UpdateModule,
  UpdateRole,
} from "./internal/client.js";

export {
  HMR_ENDPOINT,
  LOG_PREFIX,
  UPDATE_EVENT,
  connect,
  isFullReload,
  nativeChannel,
  parseUpdate,
} from "./internal/client.js";

export type { BrowserDiagnostic, DiagnosticSeverity } from "./internal/diagnostics.js";

export { DIAGNOSTIC_ENDPOINT, reportDiagnostic } from "./internal/diagnostics.js";
