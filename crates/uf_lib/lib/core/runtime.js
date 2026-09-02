// @flow
//
// `@uniflowed/runtime`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/runtime";

export type RuntimeEngine =
  | "uf"
  | "node"
  | "bun"
  | "deno"
  | "edge"
  | "serverless"
  | "container";

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

export function run(entry: string): Promise<void> {
  return nativeRuntimeRequired(MODULE, "run");
}

export function resolve(specifier: string, from?: string): Promise<string> {
  return nativeRuntimeRequired(MODULE, "resolve");
}

export function spawn(
  entry: string,
  options?: $ReadOnly<{
    +runtime?: RuntimeEngine,
    +adapter?: DeployAdapter,
  }>,
): Promise<void> {
  return nativeRuntimeRequired(MODULE, "spawn");
}
