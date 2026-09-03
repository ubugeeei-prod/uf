// @flow
type Primitive = string | number | boolean | null | void | mixed | any | empty | symbol | bigint;
type Literal = "a" | 1 | true | -1 | 10n;
type Nullable = ?string;
type Arr = string[];
type NullableArray = (?string)[];
type ArrayOfFunctions = (() => void)[];
type Generic = Map<string, Array<number>>;
type Obj = {
  a: string,
  b?: number,
  +c: boolean,
  -d: string,
  readonly e: number,
  [key: string]: mixed,
  (): void,
  [[slot]]: T,
  m(): void,
  ...Spread,
  ...
};
type Exact = {| a: string, b: number |};
type EmptyExact = {||};
type Inexact = { ... };
type Fn = (a: string, b?: number, ...rest: Array<mixed>) => void;
type ThisFn = (this: Context, a: string) => void;
type Union = Alpha | Beta | Gamma;
type LongUnion =
  | "first-option"
  | "second-option"
  | "third-option"
  | "fourth-option"
  | "fifth-option-here";
type ObjectUnion =
  | { kind: "a", value: string }
  | { kind: "b", value: number }
  | { kind: "c", value: boolean };
type HugUnion = { a: string } | null;
type Intersection = A & B & C;
type ObjectIntersection = { a: string } & { b: number } & { c: boolean };
type MixedIntersection = SomeType & { extra: string } & OtherType;
type Tuple = [string, number];
type LabeledTuple = [name: string, age?: number, ...rest: Array<mixed>];
type InexactTuple = [string, ...];
type Indexed = Obj["a"];
type OptionalIndexed = Obj?.["a"];
type KeyOf = keyof Obj;
type TypeOf = typeof value;
type TypeOfMember = typeof obj.prop;
type Conditional = T extends string ? "str" : T extends number ? "num" : "other";
type Infer = T extends Array<infer Element> ? Element : never;
type Mapped = { [K in keyof Obj]: Obj[K] };
type MappedOptional = { +[K in Keys]?: Value };
type ReadOnlyType = $ReadOnly<{ a: string }>;
type FunctionInUnion = (() => void) | string;
type NullableFunction = ?() => void;
type UnionInArray = (A | B)[];
type Renders = renders Component;
type ComponentType = component(a: string, ...rest: Props) renders Node;
type HookType = hook (a: number) => string;
type ImportedType = $Exports<"./module">;
type Qualified = React.Node;
type Existential = *;
type LongGeneric = SomeGenericType<
  FirstTypeArgument,
  SecondTypeArgument,
  ThirdTypeArgument,
  FourthTypeArgument,
>;
type LongFunction = (
  firstParameter: FirstType,
  secondParameter: SecondType,
  thirdParameter: ThirdType,
) => ReturnType;
type LongObject = {
  firstProperty: FirstType,
  secondProperty: SecondType,
  thirdProperty: ThirdType,
  fourth: Fourth,
};
type Params<+T, -U, V: Bound, X extends Y, const Z, W = Default> = T;
type ObjectMethodValue = { method<T>(x: T): T, prop: <T>(x: T) => T };
opaque type Opaque = string;
opaque type OpaqueBounded: Super = string;
export opaque type Exported: Bound = number;
export type { A, B } from "./types";
export type Alias = Original;
interface Iface {
  a: string;
  b(): void;
  +c: number;
}
interface Extended extends Base, Other {
  extra: string;
}
interface WithQualified extends Module.Base {}
declare interface DeclaredIface {
  x: number;
}
const casted = (value: any);
const asCast = value as SomeType;
const asConst = ["a", "b"] as const;
function annotated(a: string, b?: number = 1, ...rest: Array<mixed>): Promise<void> {}
function typeParams<+U, T: Bound = Default>(x: T): U {}
function predicate(x: mixed): boolean %checks {
  return typeof x === "string";
}
function guard(x: mixed): implies x is string {
  return true;
}
function assertsGuard(x: mixed): asserts x is string {}
const typedArrow = (x: number): string => String(x);
const genericArrow = <T>(x: T): T => x;
const genericArrowLong = <TypeParameterOne, TypeParameterTwo>(
  argument: TypeParameterOne,
): TypeParameterTwo => convert(argument);
let annotatedLet: Array<{ a: string, b: number }> = [];
const objectFn = {
  fn(x: string): number {
    return 1;
  },
};
type CallableObject = { (x: number): string, prop: boolean };
type LongUnionInGeneric = Array<
  "first-option" | "second-option" | "third-option" | "fourth-option" | "fifth",
>;
type FunctionReturningUnion = () =>
  | "first-option"
  | "second-option"
  | "third-option"
  | "fourth-option"
  | "x";
type Nested = { a: { b: { c: string } } };
type LongIntersection = FirstLongTypeName &
  SecondLongTypeName &
  ThirdLongTypeName &
  FourthLongTypeName;
