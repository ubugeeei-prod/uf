// @flow
//
// `@uniflowed/validator/namespace`: every builder under one name.
//
//   import { v } from "@uniflowed/validator";
//   const Account = v.object({ email: v.pipe(v.string(), v.email()) });
//
// The named exports are the primary surface. This is the convenience alias,
// for the many schemas that mention a dozen builders and would otherwise open
// with a dozen-line import, and for readers coming from Valibot who already
// write `v.` in front of everything.
//
// # Why it is a module of its own
//
// Because it is the one module that has to import all of them, and a package
// whose entry point did that would make every application carry every check.
// Here it is reached through a single named re-export from `index.js`, which a
// bundler drops when nothing reads `v`; a project that imports
// `@uniflowed/validator/primitive` directly never resolves this file at all.
//
// # Why it is typed with `typeof`
//
// So the two surfaces cannot drift. Each field's type is the exported
// function's own, which means adding a builder and forgetting to list it here
// is a missing property rather than a silently different signature, and
// renaming one is an error at this file rather than a surprise at a call site.

import {
  email,
  endsWith,
  includes,
  integer,
  isoDate,
  length,
  max,
  maxItems,
  maxLength,
  min,
  minItems,
  minLength,
  multipleOf,
  nonEmpty,
  regex,
  startsWith,
  toLowerCase,
  toUpperCase,
  trim,
  url,
  uuid,
} from "./action.js";
import { array, map, record, set, tuple } from "./collection.js";
import { flatten } from "./issue.js";
import { toJsonSchema } from "./json-schema.js";
import { lazy, lazyAsync } from "./lazy.js";
import { looseObject, object, partial, strictObject } from "./object.js";
import { fallback, nullable, nullish, optional, withDefault } from "./optional.js";
import {
  is,
  parse,
  parseAsync,
  parser,
  safeParse,
  safeParseAsync,
  useValidation,
} from "./parse.js";
import { brand, check, checkAsync, pipe, refine, transform, transformAsync } from "./pipe.js";
import {
  bigint,
  boolean,
  custom,
  date,
  enum_,
  instance,
  literal,
  never,
  null_,
  number,
  string,
  undefined_,
  unknown,
} from "./primitive.js";
import { describe, isAsync } from "./schema.js";
import { intersect, union, variant } from "./union.js";

export const v: {
  readonly string: typeof string,
  readonly number: typeof number,
  readonly bigint: typeof bigint,
  readonly boolean: typeof boolean,
  readonly unknown: typeof unknown,
  readonly never: typeof never,
  readonly null: typeof null_,
  readonly undefined: typeof undefined_,
  readonly literal: typeof literal,
  readonly enum: typeof enum_,
  readonly date: typeof date,
  readonly instance: typeof instance,
  readonly custom: typeof custom,
  readonly object: typeof object,
  readonly strictObject: typeof strictObject,
  readonly looseObject: typeof looseObject,
  readonly partial: typeof partial,
  readonly array: typeof array,
  readonly tuple: typeof tuple,
  readonly record: typeof record,
  readonly map: typeof map,
  readonly set: typeof set,
  readonly union: typeof union,
  readonly variant: typeof variant,
  readonly intersect: typeof intersect,
  readonly optional: typeof optional,
  readonly nullable: typeof nullable,
  readonly nullish: typeof nullish,
  readonly withDefault: typeof withDefault,
  readonly fallback: typeof fallback,
  readonly lazy: typeof lazy,
  readonly lazyAsync: typeof lazyAsync,
  readonly pipe: typeof pipe,
  readonly check: typeof check,
  readonly checkAsync: typeof checkAsync,
  readonly transform: typeof transform,
  readonly transformAsync: typeof transformAsync,
  readonly brand: typeof brand,
  readonly refine: typeof refine,
  readonly minLength: typeof minLength,
  readonly maxLength: typeof maxLength,
  readonly length: typeof length,
  readonly nonEmpty: typeof nonEmpty,
  readonly startsWith: typeof startsWith,
  readonly endsWith: typeof endsWith,
  readonly includes: typeof includes,
  readonly regex: typeof regex,
  readonly email: typeof email,
  readonly url: typeof url,
  readonly uuid: typeof uuid,
  readonly isoDate: typeof isoDate,
  readonly trim: typeof trim,
  readonly toLowerCase: typeof toLowerCase,
  readonly toUpperCase: typeof toUpperCase,
  readonly min: typeof min,
  readonly max: typeof max,
  readonly integer: typeof integer,
  readonly multipleOf: typeof multipleOf,
  readonly minItems: typeof minItems,
  readonly maxItems: typeof maxItems,
  readonly parse: typeof parse,
  readonly safeParse: typeof safeParse,
  readonly parseAsync: typeof parseAsync,
  readonly safeParseAsync: typeof safeParseAsync,
  readonly is: typeof is,
  readonly parser: typeof parser,
  readonly useValidation: typeof useValidation,
  readonly describe: typeof describe,
  readonly isAsync: typeof isAsync,
  readonly flatten: typeof flatten,
  readonly toJsonSchema: typeof toJsonSchema,
} = {
  string,
  number,
  bigint,
  boolean,
  unknown,
  never,
  null: null_,
  undefined: undefined_,
  literal,
  enum: enum_,
  date,
  instance,
  custom,
  object,
  strictObject,
  looseObject,
  partial,
  array,
  tuple,
  record,
  map,
  set,
  union,
  variant,
  intersect,
  optional,
  nullable,
  nullish,
  withDefault,
  fallback,
  lazy,
  lazyAsync,
  pipe,
  check,
  checkAsync,
  transform,
  transformAsync,
  brand,
  refine,
  minLength,
  maxLength,
  length,
  nonEmpty,
  startsWith,
  endsWith,
  includes,
  regex,
  email,
  url,
  uuid,
  isoDate,
  trim,
  toLowerCase,
  toUpperCase,
  min,
  max,
  integer,
  multipleOf,
  minItems,
  maxItems,
  parse,
  safeParse,
  parseAsync,
  safeParseAsync,
  is,
  parser,
  useValidation,
  describe,
  isAsync,
  flatten,
  toJsonSchema,
};
