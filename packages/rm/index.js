// @flow
//
// `@uniflowed/rm`.

import type { UniflowedConfig } from "@uniflowed/config";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/rm";

export type RuntimeEngine = "uf" | "node" | "deno" | "bun" | "edge" | "serverless" | "container";

export type RuntimeHost = RuntimeEngine;
export type RuntimeAcquisition = "auto";
export type RuntimeApplication = "config-and-host";
export type RuntimeReference = { readonly name: "uf" | string, readonly version: string };

export type XdgLayout = {
  readonly configDir: string,
  readonly dataDir: string,
  readonly cacheDir: string,
  readonly stateDir: string,
  readonly runtimeDir?: string,
  readonly binDir: string,
  readonly shimPath: string,
  readonly versionsDir: string,
};

export type RuntimeManagerPlan = {
  readonly engine: RuntimeEngine,
  readonly hosts: $ReadOnlyArray<RuntimeHost>,
  readonly acquisition: RuntimeAcquisition,
  readonly application: RuntimeApplication,
  readonly steps: $ReadOnlyArray<
    | "read-config"
    | "infer-runtime"
    | "detect-capability-host"
    | "acquire-runtime"
    | "apply-adapters"
    | "verify-doctor",
  >,
};

export type RuntimeUsePlan = {
  readonly requested: RuntimeReference,
  readonly layout: XdgLayout,
  readonly autoSwitch: boolean,
  readonly steps: $ReadOnlyArray<
    | "resolve-version"
    | "download-runtime"
    | "verify-checksum"
    | "install-version"
    | "write-shim"
    | "activate-version",
  >,
};

export function inferRuntime(config: UniflowedConfig): RuntimeManagerPlan {
  return nativeRuntimeRequired(MODULE, "inferRuntime");
}

export function useRuntime(specifier: string): RuntimeUsePlan {
  return nativeRuntimeRequired(MODULE, "useRuntime");
}

export function acquireRuntime(plan: RuntimeManagerPlan): Promise<RuntimeManagerPlan> {
  return nativeRuntimeRequired(MODULE, "acquireRuntime");
}

export function applyRuntime(plan: RuntimeManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "applyRuntime");
}

export function doctor(plan?: RuntimeManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "doctor");
}
