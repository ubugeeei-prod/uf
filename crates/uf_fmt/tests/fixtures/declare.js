// @flow
declare var globalVar: number;
declare let globalLet: string;
declare const globalConst: boolean;
declare function declared(a: string, b: number): void;
declare function overloaded(a: string): string;
declare function overloaded(a: number): number;
declare function withPredicate(x: mixed): boolean %checks(typeof x === "string");
declare class DeclaredClass<T> extends Base<T> mixins Mixin implements Iface {
  static create(): DeclaredClass<T>;
  method(): void;
  +readOnly: string;
  prop: T;
  [key: string]: mixed;
  constructor(value: T): void;
  get accessor(): number;
  set accessor(v: number): void;
}
declare export class ExportedClass {}
declare export function exportedFn(): void;
declare export var exportedVar: number;
declare export default string;
declare export default class DefaultClass {}
declare export { a, b as c } from "./mod";
declare export * from "./all";
declare export type ExportedType = string;
declare export opaque type ExportedOpaque: string;
declare export interface ExportedIface {}
declare export enum ExportedEnum { A, B }
declare type DeclaredAlias = number;
declare opaque type DeclaredOpaque;
declare opaque type DeclaredOpaqueBounded: string;
declare interface DeclaredInterface { prop: string }
declare enum DeclaredEnum { A, B }
declare module "external-module" {
  declare export default function main(): void;
  declare export var version: string;
  declare module.exports: { main: () => void };
}
declare module Namespaced {
  declare var inner: number;
}
declare module.exports: { x: number, y: string };
declare component DeclaredComponent(a: string, b?: number) renders Node;
declare component NoRenders();
declare hook useDeclared(a: number): string;
declare namespace NS { declare var x: number; }
declare export component ExportedComponent(a: string);
declare export hook useExported(): void;
