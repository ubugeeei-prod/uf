// @flow
//
// `@uniflowed/validator`.

import type { NativeHandleInvariant } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/validator";

export opaque type Schema<T> = NativeHandleInvariant<
  "@uniflowed/core/validator#Schema",
  T,
>;

export type Issue = {
  +code: string,
  +message: string,
};

export type Result<T> =
  | { +ok: true, +value: T }
  | { +ok: false, +issues: $ReadOnlyArray<Issue> };

export type Step<TIn, TOut> = (schema: Schema<TIn>) => Schema<TOut>;

export const v: {
  +string: () => Schema<string>,
  +number: () => Schema<number>,
  +boolean: () => Schema<boolean>,
  +object: <T: {...}>(shape: T) => Schema<T>,
  +pipe: <A, B>(schema: Schema<A>, step: Step<A, B>) => Schema<B>,
  +minLength: (value: number) => Step<string, string>,
  +maxLength: (value: number) => Step<string, string>,
  +startsWith: (value: string) => Step<string, string>,
  +min: (value: number) => Step<number, number>,
  +max: (value: number) => Step<number, number>,
  +safeParse: <T>(schema: Schema<T>, value: mixed) => Result<T>,
} = {
  string: (): Schema<string> => nativeRuntimeRequired(MODULE, "v.string"),
  number: (): Schema<number> => nativeRuntimeRequired(MODULE, "v.number"),
  boolean: (): Schema<boolean> => nativeRuntimeRequired(MODULE, "v.boolean"),
  object: <T: {...}>(shape: T): Schema<T> =>
    nativeRuntimeRequired(MODULE, "v.object"),
  pipe: <A, B>(schema: Schema<A>, step: Step<A, B>): Schema<B> =>
    nativeRuntimeRequired(MODULE, "v.pipe"),
  minLength: (value: number): Step<string, string> =>
    nativeRuntimeRequired(MODULE, "v.minLength"),
  maxLength: (value: number): Step<string, string> =>
    nativeRuntimeRequired(MODULE, "v.maxLength"),
  startsWith: (value: string): Step<string, string> =>
    nativeRuntimeRequired(MODULE, "v.startsWith"),
  min: (value: number): Step<number, number> =>
    nativeRuntimeRequired(MODULE, "v.min"),
  max: (value: number): Step<number, number> =>
    nativeRuntimeRequired(MODULE, "v.max"),
  safeParse: <T>(schema: Schema<T>, value: mixed): Result<T> =>
    nativeRuntimeRequired(MODULE, "v.safeParse"),
};

export const string: typeof v.string = v.string;
export const number: typeof v.number = v.number;
export const object: typeof v.object = v.object;
export const pipe: typeof v.pipe = v.pipe;
