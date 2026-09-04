// @flow
//
// `@uniflowed/runtime`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/runtime";

export type RuntimeEngine = "uf" | "node" | "deno" | "bun" | "edge" | "serverless" | "container";

export type CapabilityJsHost = "node" | "deno" | "bun";
export type JavaScriptEngine = "capability-js-host" | "hermes";

export type DeployAdapter =
  | "node"
  | "bun"
  | "deno"
  | "edge"
  | "serverless"
  | "static"
  | "container";

export type RuntimeCapability =
  | "fetch"
  | "streams"
  | "request-response"
  | "url"
  | "headers"
  | "cookies"
  | "timers"
  | "file-system"
  | "tcp"
  | "udp"
  | "tls"
  | "dns"
  | "cron"
  | "s3"
  | "sigv4"
  | "functions"
  | "web-assembly"
  | "workers"
  | "server-actions"
  | "react-server-components"
  | "native-packages"
  | "terminal-ui";

export type RuntimeContract = {
  readonly standard: "winter-tc",
  readonly language: "flow",
  readonly javascriptEngine: JavaScriptEngine,
  readonly hosts: $ReadOnlyArray<RuntimeEngine>,
  readonly capabilities: $ReadOnlyArray<RuntimeCapability>,
};

export function run(entry: string): Promise<void> {
  return nativeRuntimeRequired(MODULE, "run");
}

export function resolve(specifier: string, from?: string): Promise<string> {
  return nativeRuntimeRequired(MODULE, "resolve");
}

export function spawn(
  entry: string,
  options?: $ReadOnly<{
    readonly runtime?: RuntimeEngine,
    readonly adapter?: DeployAdapter,
  }>,
): Promise<void> {
  return nativeRuntimeRequired(MODULE, "spawn");
}
