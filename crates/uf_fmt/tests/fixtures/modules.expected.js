// @flow
import defaultExport from "module";
import * as namespace from "module";
import { named } from "module";
import { a as b, c } from "module";
import def, { named2 } from "module";
import def2, * as ns from "module";
import {} from "module";
import "side-effect";
import type { TypeOnly } from "module";
import type Def from "module";
import typeof TypeofDef from "module";
import { type T, typeof U, v } from "module";
import json from "./data.json" with { type: "json" };
import {
  aVeryLongImportedNameNumberOne,
  aVeryLongImportedNameNumberTwo,
  aVeryLongImportedNameNumberThree,
} from "./a/long/module/path";
import {
  one,
  two,
  three,
  four,
  five,
  six,
  seven,
  eight,
  nine,
  ten,
  eleven,
  twelve,
  thirteen,
} from "./numbers";

export const constant = 1;
export let variable = 2;
export function fn() {}
export async function asyncFn() {}
export function* gen() {}
export class Klass {}
export default function () {}
export { a, b as c };
export {};
export * from "./all";
export * as everything from "./all";
export { x, y } from "./xy";
export { default as renamed } from "./default";
export type { TypeA, TypeB } from "./types";
export type Local = string;
export opaque type Opaque = number;
export interface Iface {}
export {
  aVeryLongExportedNameNumberOne,
  aVeryLongExportedNameNumberTwo,
  aVeryLongExportedNameNumberThree,
};
