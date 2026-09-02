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
  +name: string,
  +path: string,
};

export type PackageStore = {
  +strategy: PackageStoreStrategy,
  +directory: string,
};

export type PackageManagerPlan = {
  +resolver: PackageResolver,
  +lockfile: "uf.lock" | string,
  +registry: string,
  +scripts: PackageScriptPolicy,
  +store: PackageStore,
  +linkMode: PackageLinkMode,
  +workspacePackages: $ReadOnlyArray<WorkspacePackage>,
  +steps: $ReadOnlyArray<
    | "read-config"
    | "resolve-graph"
    | "verify-integrity"
    | "write-lockfile"
    | "apply-store"
    | "link-workspace",
  >,
};

export function inferFromConfig(
  config: UniflowedConfig,
): PackageManagerPlan {
  return nativeRuntimeRequired(MODULE, "inferFromConfig");
}

export function install(plan?: PackageManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "install");
}

export function upgrade(plan?: PackageManagerPlan): Promise<void> {
  return nativeRuntimeRequired(MODULE, "upgrade");
}
