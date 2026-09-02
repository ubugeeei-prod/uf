// @flow
export type {
  RuntimeAcquisition,
  RuntimeApplication,
  RuntimeEngine,
  RuntimeHost,
  RuntimeManagerPlan,
  RuntimeReference,
  RuntimeUsePlan,
  XdgLayout,
} from "../core/rm.js";
export {
  acquireRuntime,
  applyRuntime,
  doctor,
  inferRuntime,
  useRuntime,
} from "../core/rm.js";
