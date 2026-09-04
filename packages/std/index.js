// @flow
//
// `@uniflowed/std`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/std";

export type StdCategory =
  | "file-system"
  | "types"
  | "pipeline"
  | "environment"
  | "diagnostics"
  | "data"
  | "network"
  | "serialization"
  | "platform"
  | "cloud";

export type StdModule = {
  readonly specifier: string,
  readonly category: StdCategory,
  readonly wintertcAligned: true,
  readonly nativeBinding: true,
  readonly exports: $ReadOnlyArray<string>,
};

export type Result<Ok, Err> =
  | { readonly ok: true, readonly value: Ok }
  | { readonly ok: false, readonly error: Err };

/**
 * Nominal wrapper around `T`.
 *
 * The supertype bound keeps `Brand<"UserId", string>` usable everywhere a
 * `string` is expected while blocking the reverse, so branding never costs a
 * conversion.
 */
export opaque type Brand<Name extends string, T>: T = T;

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | $ReadOnlyArray<JsonValue>
  | { readonly [string]: JsonValue };

export type VirtualPath = { readonly path: string };
export type QueryPair = { readonly key: string, readonly value: string };
export type ByteBuffer = { readonly bytes: Uint8Array };
export type ImportMeta = {
  readonly url: string,
  readonly dirname?: string,
  readonly filename?: string,
};
export type DeferPhase = "microtask" | "idle" | "post-response";
export type DeferredTask = { readonly id: string, readonly phase: DeferPhase };
export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
export type HttpRoute = { readonly method: HttpMethod, readonly path: string };
export type WebSocketMode = "web-socket" | "web-socket-stream";
export type SqlDriverKind = "sqlite" | "postgres" | "mysql";
export type SqlDriver = { readonly kind: SqlDriverKind, readonly preparedByDefault: true };
export type DigestAlgorithm = "fast-hash" | "sha256";
export type OsFamily = "mac-os" | "linux" | "windows" | "unknown";
export type OsInfo = {
  readonly family: OsFamily,
  readonly arch: string,
  readonly availableParallelism: number,
};
export type DnsRecordType = "A" | "AAAA" | "CNAME" | "MX" | "TXT";
export type DnsQuery = { readonly name: string, readonly recordType: DnsRecordType };
export type StreamKind = "readable" | "writable" | "transform";
export type StreamDescriptor = { readonly kind: StreamKind, readonly backpressure: true };
export type UrlParts = { readonly scheme: string, readonly host: string, readonly path: string };
export type WasmModulePlan = { readonly name: string, readonly aheadOfTime: true };
export type GlobPattern = { readonly pattern: string, readonly dotfiles: boolean };
export type MotionEase = "linear" | "out" | "spring";
export type MotionTransition = {
  readonly durationMs: number,
  readonly ease: MotionEase,
  readonly respectsReducedMotion: true,
};
export type TerminalColorDepth = "ansi16" | "ansi256" | "true-color";
export type TerminalCapabilities = {
  readonly columns: number,
  readonly rows: number,
  readonly colorDepth: TerminalColorDepth,
  readonly unicode: boolean,
  readonly mouse: boolean,
  readonly inlineImages: boolean,
  readonly sixel: boolean,
};
export type CronSchedule = {
  readonly minute: string,
  readonly hour: string,
  readonly dayOfMonth: string,
  readonly month: string,
  readonly dayOfWeek: string,
};
export type S3ObjectRequest = {
  readonly bucket: string,
  readonly key: string,
  readonly sigv4: true,
};
export type SigV4Scope = { readonly region: string, readonly service: string };
export type FunctionRuntime = "worker" | "lambda";
export type FunctionDescriptor = {
  readonly name: string,
  readonly runtime: FunctionRuntime,
  readonly entry: string,
};
export type UuidVersion = "v4" | "v7";
export type ZipCompression = "store" | "deflate";
export type ZipEntry = {
  readonly path: string,
  readonly compression: ZipCompression,
  readonly size: number,
};

export function modules(): $ReadOnlyArray<StdModule> {
  return nativeRuntimeRequired(MODULE, "modules");
}

export function wintertc(): "winter-tc" {
  return nativeRuntimeRequired(MODULE, "wintertc");
}

export function virtualPath(path: string): VirtualPath {
  return nativeRuntimeRequired(MODULE, "virtualPath");
}

export function lazy(name: string): mixed {
  return nativeRuntimeRequired(MODULE, "lazy");
}

export function defer(id: string, phase?: DeferPhase): DeferredTask {
  return nativeRuntimeRequired(MODULE, "defer");
}

export function parseDotEnv(
  source: string,
): $ReadOnlyArray<{ readonly key: string, readonly value: string }> {
  return nativeRuntimeRequired(MODULE, "parseDotEnv");
}

export function colorize(
  value: string,
  style: "bold" | "dim" | "red" | "green" | "cyan",
  enabled?: boolean,
): string {
  return nativeRuntimeRequired(MODULE, "colorize");
}

export function fastHash(value: string | Uint8Array): number {
  return nativeRuntimeRequired(MODULE, "fastHash");
}

export function timingSafeEqual(left: Uint8Array, right: Uint8Array): boolean {
  return nativeRuntimeRequired(MODULE, "timingSafeEqual");
}

export function bufferFromUtf8(value: string): ByteBuffer {
  return nativeRuntimeRequired(MODULE, "bufferFromUtf8");
}

export function toHex(buffer: ByteBuffer): string {
  return nativeRuntimeRequired(MODULE, "toHex");
}

export function parseQuery(query: string): $ReadOnlyArray<QueryPair> {
  return nativeRuntimeRequired(MODULE, "parseQuery");
}

export function stringifyQuery(query: $ReadOnlyArray<QueryPair>): string {
  return nativeRuntimeRequired(MODULE, "stringifyQuery");
}

export function parseJson(source: string): JsonValue {
  return nativeRuntimeRequired(MODULE, "parseJson");
}

export function stringifyJson(value: JsonValue): string {
  return nativeRuntimeRequired(MODULE, "stringifyJson");
}

export function parseToml(source: string): mixed {
  return nativeRuntimeRequired(MODULE, "parseToml");
}

export function detectYaml(source: string): "mapping" | "sequence" | "scalar" | "empty" {
  return nativeRuntimeRequired(MODULE, "detectYaml");
}

export function clamp(value: number, min: number, max: number): number {
  return nativeRuntimeRequired(MODULE, "clamp");
}

export function lerp(start: number, end: number, amount: number): number {
  return nativeRuntimeRequired(MODULE, "lerp");
}

export function osInfo(family: OsFamily, arch: string, availableParallelism: number): OsInfo {
  return nativeRuntimeRequired(MODULE, "osInfo");
}

export function dnsQuery(name: string, recordType: DnsRecordType): DnsQuery {
  return nativeRuntimeRequired(MODULE, "dnsQuery");
}

export function joinPath(parts: $ReadOnlyArray<string>): string {
  return nativeRuntimeRequired(MODULE, "joinPath");
}

export function normalizePath(path: string): string {
  return nativeRuntimeRequired(MODULE, "normalizePath");
}

export function stream(kind: StreamKind): StreamDescriptor {
  return nativeRuntimeRequired(MODULE, "stream");
}

export function parseUrl(value: string): ?UrlParts {
  return nativeRuntimeRequired(MODULE, "parseUrl");
}

export function wasmModule(name: string): WasmModulePlan {
  return nativeRuntimeRequired(MODULE, "wasmModule");
}

export function glob(pattern: string): GlobPattern {
  return nativeRuntimeRequired(MODULE, "glob");
}

export function motion(durationMs: number, ease: MotionEase): MotionTransition {
  return nativeRuntimeRequired(MODULE, "motion");
}

export function terminalCapabilities(columns: number, rows: number): TerminalCapabilities {
  return nativeRuntimeRequired(MODULE, "terminalCapabilities");
}

export function parseCron(source: string): ?CronSchedule {
  return nativeRuntimeRequired(MODULE, "parseCron");
}

export function s3Object(bucket: string, key: string): S3ObjectRequest {
  return nativeRuntimeRequired(MODULE, "s3Object");
}

export function sigv4Scope(region: string, service: string): SigV4Scope {
  return nativeRuntimeRequired(MODULE, "sigv4Scope");
}

export function defineFunction(
  name: string,
  runtime: FunctionRuntime,
  entry: string,
): FunctionDescriptor {
  return nativeRuntimeRequired(MODULE, "defineFunction");
}

export function digest(algorithm: DigestAlgorithm, bytes: Uint8Array): string {
  return nativeRuntimeRequired(MODULE, "digest");
}

export function parseUuid(value: string): ?{ readonly value: string, readonly version?: number } {
  return nativeRuntimeRequired(MODULE, "parseUuid");
}

export function importMeta(url: string): ImportMeta {
  return nativeRuntimeRequired(MODULE, "importMeta");
}
