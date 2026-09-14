// @flow
//
// `@uniflowed/config`.

export type {
  BuilderSpec,
  CapabilityJsHost,
  CoverageThresholds,
  DeployAdapter,
  PackageManagerPreference,
  PackageManagerSpec,
  Permissions,
  PluginEntry,
  RuleLevel,
  RuntimeEngine,
  RuntimeSpec,
  SizeBudget,
  TaskDefinition,
  TestRunnerSpec,
  UniflowedConfig,
} from "./internal/schema.js";

export { defineConfig } from "./internal/schema.js";
