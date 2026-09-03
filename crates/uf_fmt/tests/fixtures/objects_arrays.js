// @flow
const empty = {};
const emptyArray = [];
const oneLine = { a: 1, b: 2 };
const expanded = {
  a: 1, b: 2 };
const nestedPreserved = { outer: {
  inner: 1 } };
const longObject = { propertyNumberOne: 1, propertyNumberTwo: 2, propertyNumberThree: 3, propertyNumberFour: 4 };
const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33];
const negatives = [-1, -2, +3, 4];
const matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
const records = [{ id: 1, name: "one" }, { id: 2, name: "two" }];
const mixed = [{ id: 1 }, [2]];
const holes = [, , 1, , ];
const trailingHole = [1, 2, ,];
const longStrings = ["first string value", "second string value", "third string value", "fourth string value"];
const nestedArrays = [[["deep"]]];
const withSpread = [...a, ...b, c];
const objectWithFunctions = { onClick() { handle(); }, onHover: () => hover(), async load() { await go(); } };
const withBlank = {
  first: 1,

  second: 2,


  third: 3,
};
const quoted = { "a": 1, "b-c": 2, "3": 3, "d e": 4, "valid": 5 };
const numericKeys = { 1: "one", 2.5: "two and a half", 0x10: "sixteen" };
const propertyWithShortKey = { a: someVeryLongFunctionCall(withArguments, andMoreArguments, andEvenMoreArguments) };
const propertyWithLongKey = { aVeryLongPropertyKeyName: someVeryLongFunctionCall(withArguments, andMoreArguments, andMore) };
const stringProperty = { key: "a very long string value that will certainly not fit within the print width of the line" };
const conditionalProperty = { key: condition ? valueWhenTrue : valueWhenFalse, other: a || b || c || d || e || f || g || h || i };
const memberChainProperty = { key: some.deeply.nested.member.expression.chain.that.goes.on.and.on.for.a.while.longer };
function destructure({ alpha, beta, gamma }, [one, two], { nested: { deep } } = {}) {}
const { a, b: { c }, d = 1 } = value;
const { longPropertyNameOne, longPropertyNameTwo, longPropertyNameThree, longPropertyNameFour } = props;
const [x, y] = point;
const [[deepA], { deepB }] = pairs;
arr = [
  1, 2, 3];
