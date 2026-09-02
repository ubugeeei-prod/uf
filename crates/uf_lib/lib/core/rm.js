// @flow
//
// `@uniflowed/rm`.

import type { UniflowedConfig } from "./config.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/rm";

export type RuntimeEngine =
  | "uf"
  | "node"
  | "bun"
  | "deno"
  | "edge"
  | "serverless"
  | "container";

export type RuntimeHost = RuntimeEngine;
export type RuntimeAcquisition = "auto";
export type RuntimeApplication = "config-and-host";
export type RuntimeReference = { +name: "uf" | string, +version: string };

export type XdgLayout = {
  +configDir: string,
  +dataDir: string,
  +cacheDir: string,
  +stateDir: string,
  +runtimeDir?: string,
  +binDir: string,
  +shimPath: string,
  +versionsDir: string,
};

export type RuntimeManagerPlan = {
  +engine: RuntimeEngine,
  +hosts: $ReadOnlyArray<RuntimeHost>,
  +acquisition: RuntimeAcquisition,
  +application: RuntimeApplication,
  +steps: $ReadOnlyArray<
    | "read-config"
    | "infer-runtime"
    | "acquire-runtime"
    | "apply-adapters"
    | "verify-doctor",
  >,
};

export type RuntimeUsePlan = {
  +requested: RuntimeReference,
  +layout: XdgLayout,
  +autoSwitch: boolean,
  +steps: $ReadOnlyArray<
    | "resolve-version"
    | "download-runtime"
    | "verify-checksum"
    | "install-version"
    | "write-shim"
    | "activate-version",
  >,
};

export function inferRuntime(
  config: UniflowedConfig,
): RuntimeManagerPlan {
  return nativeRuntimeRequired(MODULE, "inferRuntime");
}

export function useRuntime(specifier: string): RuntimeUsePlan {
  return nativeRuntimeRequired(MODULE, "useRuntime");
}

export function acquireRuntime(
  plan: RuntimeManagerPlan,
): Promise<RuntimeManagerPlan> {
  return nativeRuntimeRequired(MODULE, "acquireRuntime");
}

export function applyRuntime(plan: RuntimeManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "applyRuntime");
}

export function doctor(plan?: RuntimeManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "doctor");
}
