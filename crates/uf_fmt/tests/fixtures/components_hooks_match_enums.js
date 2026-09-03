// @flow
component Empty() {}
component Simple(name: string, count?: number = 0) renders Node { return <p>{name}</p>; }
component WithRest(name: string, ...rest: Props) { return null; }
component Renamed("data-id" as dataId: string, className as cls: string = "x") {}
component Generic<T>(value: T) renders? Item<T> { return null; }
component RendersStar(items: Array<Item>) renders* Item { return items; }
export component Exported(a: string) { return a; }
export default component DefaultExported() { return null; }
async component AsyncComponent() { return null; }
component LongParams(firstParameter: FirstType, secondParameter: SecondType, thirdParameter: ThirdType) {}
hook useNothing() {}
hook useCounter(initial: number): [number, () => void] { const [count, setCount] = useState(initial); return [count, () => setCount(count + 1)]; }
export hook useExported<T>(value: T): T { return value; }
hook useLongParameters(firstParameter: FirstType, secondParameter: SecondType, third: ThirdType): Result {}

const literalMatch = match (value) {
  1 => "one",
  "two" => 2,
  true => 3,
  null => 4,
  -1 => 5,
  10n => 6,
  _ => 0,
};
const bindingMatch = match (value) {
  const x => x,
  let y => y,
  var z => z,
  Enum.Member => 1,
  Deep.Member.Path => 2,
  Foo["bar"] => 3,
  Foo[0] => 4,
};
const structuralMatch = match (value) {
  {} => 0,
  {a: 1, b: const two} => two,
  {a: const shorthand, ...} => shorthand,
  {...const rest} => rest,
  [] => 1,
  [1, const second, ...] => second,
  [const head, ...const tail] => tail,
  Point {x: const px, y: 2} => px,
  Pkg.Point {...} => 5,
};
const combinedMatch = match (value) {
  "a" | "b" | "c" => 1,
  {kind: "x"} | {kind: "y"} => 2,
  {kind: "z"} as const whole => whole,
  const n if (n > 10) => "big",
  [const a, const b] if (a === b) => "same",
  _ => "other",
};
const longMatchBodies = match (state) {
  {status: "loading"} => renderLoadingIndicatorWithAVeryLongName(withArguments, andMoreArguments),
  {status: "ready", value: const value} => renderReadyState(value),
};
match (command) {
  "start" => { start(); }
  "stop" => { stop(); }
  const other => { console.log(other); }
  _ => {}
}

enum Empty {}
enum Status { Active, Inactive }
enum Explicit of string { Active = "active", Inactive = "inactive" }
enum Numbers of number { One = 1, Two = 2 }
enum Booleans of boolean { Yes = true, No = false }
enum Symbols of symbol { A, B }
enum BigInts of bigint { Big = 1n }
enum Unknown { A, B, ... }
enum Const { A = "a" }
export enum ExportedEnum { A }
