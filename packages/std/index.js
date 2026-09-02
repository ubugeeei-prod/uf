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
  +specifier: string,
  +category: StdCategory,
  +wintertcAligned: true,
  +nativeBinding: true,
  +exports: $ReadOnlyArray<string>,
};

export type Result<Ok, Err> =
  | { +ok: true, +value: Ok }
  | { +ok: false, +error: Err };

/**
 * Nominal wrapper around `T`.
 *
 * The supertype bound keeps `Brand<"UserId", string>` usable everywhere a
 * `string` is expected while blocking the reverse, so branding never costs a
 * conversion.
 */
export opaque type Brand<Name: string, T>: T = T;

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | $ReadOnlyArray<JsonValue>
  | { +[string]: JsonValue };

export type VirtualPath = { +path: string };
export type QueryPair = { +key: string, +value: string };
export type ByteBuffer = { +bytes: Uint8Array };
export type ImportMeta = {
  +url: string,
  +dirname?: string,
  +filename?: string,
};
export type DeferPhase = "microtask" | "idle" | "post-response";
export type DeferredTask = { +id: string, +phase: DeferPhase };
export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
export type HttpRoute = { +method: HttpMethod, +path: string };
export type WebSocketMode = "web-socket" | "web-socket-stream";
export type SqlDriverKind = "sqlite" | "postgres" | "mysql";
export type SqlDriver = { +kind: SqlDriverKind, +preparedByDefault: true };
export type DigestAlgorithm = "fast-hash" | "sha256";
export type OsFamily = "mac-os" | "linux" | "windows" | "unknown";
export type OsInfo = {
  +family: OsFamily,
  +arch: string,
  +availableParallelism: number,
};
export type DnsRecordType = "A" | "AAAA" | "CNAME" | "MX" | "TXT";
export type DnsQuery = { +name: string, +recordType: DnsRecordType };
export type StreamKind = "readable" | "writable" | "transform";
export type StreamDescriptor = { +kind: StreamKind, +backpressure: true };
export type UrlParts = { +scheme: string, +host: string, +path: string };
export type WasmModulePlan = { +name: string, +aheadOfTime: true };
export type GlobPattern = { +pattern: string, +dotfiles: boolean };
export type MotionEase = "linear" | "out" | "spring";
export type MotionTransition = {
  +durationMs: number,
  +ease: MotionEase,
  +respectsReducedMotion: true,
};
export type TerminalColorDepth = "ansi16" | "ansi256" | "true-color";
export type TerminalCapabilities = {
  +columns: number,
  +rows: number,
  +colorDepth: TerminalColorDepth,
  +unicode: boolean,
  +mouse: boolean,
  +inlineImages: boolean,
  +sixel: boolean,
};
export type CronSchedule = {
  +minute: string,
  +hour: string,
  +dayOfMonth: string,
  +month: string,
  +dayOfWeek: string,
};
export type S3ObjectRequest = { +bucket: string, +key: string, +sigv4: true };
export type SigV4Scope = { +region: string, +service: string };
export type FunctionRuntime = "worker" | "lambda";
export type FunctionDescriptor = {
  +name: string,
  +runtime: FunctionRuntime,
  +entry: string,
};
export type UuidVersion = "v4" | "v7";
export type ZipCompression = "store" | "deflate";
export type ZipEntry = {
  +path: string,
  +compression: ZipCompression,
  +size: number,
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
): $ReadOnlyArray<{ +key: string, +value: string }> {
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

export function detectYaml(
  source: string,
): "mapping" | "sequence" | "scalar" | "empty" {
  return nativeRuntimeRequired(MODULE, "detectYaml");
}

export function clamp(value: number, min: number, max: number): number {
  return nativeRuntimeRequired(MODULE, "clamp");
}

export function lerp(start: number, end: number, amount: number): number {
  return nativeRuntimeRequired(MODULE, "lerp");
}

export function osInfo(
  family: OsFamily,
  arch: string,
  availableParallelism: number,
): OsInfo {
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

export function terminalCapabilities(
  columns: number,
  rows: number,
): TerminalCapabilities {
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

export function digest(
  algorithm: DigestAlgorithm,
  bytes: Uint8Array,
): string {
  return nativeRuntimeRequired(MODULE, "digest");
}

export function parseUuid(
  value: string,
): ?{ +value: string, +version?: number } {
  return nativeRuntimeRequired(MODULE, "parseUuid");
}

export function importMeta(url: string): ImportMeta {
  return nativeRuntimeRequired(MODULE, "importMeta");
}
