// @flow

export type Handler = (event: string, count?: number) => void;

export type Anonymous = (string, number) => boolean;

export type Rest = (...items: Array<string>) => number;

export type Bounded<T: { id: string, ... }> = (value: T) => T;

export type Defaulted<T = string> = (value: T) => T;

export type Varied<+Out, -In> = (input: In) => Out;

export type Constructor = new (size: number) => Array<string>;

export function isString(value: mixed): value is string {
  return typeof value === "string";
}

export function identity<T>(value: T): T {
  return value;
}

export function pad(raw: string, width?: number, ...rest: Array<string>): string {
  return raw;
}
