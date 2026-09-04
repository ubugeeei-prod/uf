// @flow
//
// `@uniflowed/pm`.

import type { UniflowedConfig } from "@uniflowed/config";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/pm";

export type PackageResolver = "uf-native";
export type PackageScriptPolicy = "forbid" | "explicit-opt-in";
export type PackageStoreStrategy = "content-addressed";
export type PackageLinkMode = "hardlink-then-copy";

export type WorkspacePackage = {
  readonly name: string,
  readonly path: string,
};

export type PackageStore = {
  readonly strategy: PackageStoreStrategy,
  readonly directory: string,
};

export type PackageManagerPlan = {
  readonly resolver: PackageResolver,
  readonly lockfile: "uf.lock" | string,
  readonly registry: string,
  readonly scripts: PackageScriptPolicy,
  readonly store: PackageStore,
  readonly linkMode: PackageLinkMode,
  readonly workspacePackages: $ReadOnlyArray<WorkspacePackage>,
  readonly steps: $ReadOnlyArray<
    | "read-config"
    | "resolve-graph"
    | "verify-integrity"
    | "write-lockfile"
    | "apply-store"
    | "link-workspace",
  >,
};

export function inferFromConfig(config: UniflowedConfig): PackageManagerPlan {
  return nativeRuntimeRequired(MODULE, "inferFromConfig");
}

export function install(plan?: PackageManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "install");
}

export function upgrade(plan?: PackageManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "upgrade");
}
