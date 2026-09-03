// @flow
const sum = a + b * c - ((d / e) % f ** g);
const mixed = a + b * c + d - (e * f * g) / h;
const compare =
  (a < b && b <= c) || (c > d && d >= e) || (a == b && c != d) || (e === f && g !== h);
const bits = (a & b) | (c ^ (((d << 1) >> 2) >>> 3));
const nullish = value ?? fallback;
const mixedLogical = (a ?? b) || c;
const veryLongCondition =
  firstOperand && secondOperand && thirdOperand && fourthOperand && fifthOperand;
const arithmeticBreak =
  leftHandSideValue * rightHandSideValue + anotherValue * yetAnotherValue - finalValueHere;
const modulo = a % (b + c);
const power = (-a) ** 2;
const exponent = a ** (b ** c);
const inOperator = "key" in object && object instanceof Thing;
const unary = !a,
  negative = -b,
  positive = +c,
  bitwiseNot = ~d,
  typeOfIt = typeof e,
  voidIt = void 0,
  deleted = delete f.g;
const doubleNegative = -(-a),
  plusPlus = +(+b),
  notNot = !!c;
const update = i++ + ++j - k-- - --l;
const ternary = condition ? consequent : alternate;
const nestedTernary = a ? b : c ? d : e ? f : g;
const longTernary = someVeryLongConditionExpression
  ? someVeryLongConsequentExpression
  : someVeryLongAlternateExpression;
const ternaryInCall = fn(cond ? left : right);
const memberTernary = (cond ? left : right).property;
const assignment = (x = 1);
const chainedAssignment = (a = b = c = d);
let compound;
compound += 1;
compound -= 1;
compound *= 2;
compound /= 2;
compound %= 3;
compound **= 2;
compound <<= 1;
compound >>= 1;
compound >>>= 1;
compound &= 1;
compound |= 1;
compound ^= 1;
compound &&= 1;
compound ||= 1;
compound ??= 1;
const sequence = (a, b, c);
for (i = 0, j = 10; i < j; i++, j--) {}
const arrow = (x) => x * 2;
const arrowBlock = (x) => {
  return x;
};
const arrowObject = () => ({ key: "value" });
const arrowNoParams = () => {};
const asyncArrow = async (x) => await x;
const generator = function* () {
  yield 1;
  yield* other();
};
const asyncFn = async function () {
  await sleep();
};
const iife = (function () {
  init();
})();
const arrowIife = (() => {
  init();
})();
const newExpr = new Thing(1, 2),
  newNoArgs = new Thing(),
  newMember = new (getClass())(),
  newCall = new (a.b().c)();
const optional = obj?.prop?.[key]?.(arg);
const template = `hello ${name}, you are ${age} years old`;
const tagged = tag`template ${expr}`;
const regex = /ab+c/gi,
  regexWithClass = /[/\]]/,
  regexSlash = /\//;
const numbers = [0xff, 0b1010, 0o777, 1_000_000, 1e5, 1.5, 0.5, 5, 0.0, 1e10, 1e-10, 10n];
const strings = ["single", "double", "it's", 'say "hi"', "mixed \"quotes\" and 'more'", "é\n\t"];
const spreadCall = fn(...args, last);
const spreadArray = [...first, ...second];
const spreadObject = { ...base, override: true };
const computed = {
  [key]: value,
  [`template${key}`]: 1,
  "quoted-key": 2,
  valid_identifier: 3,
  42: 4,
  "43": 5,
};
const shorthand = { a, b, c };
const methods = {
  method() {},
  async asyncMethod() {},
  *gen() {},
  async *asyncGen() {},
  get getter() {
    return 1;
  },
  set setter(v) {},
};
async function awaits() {
  const awaitInChain = (await fetchThing()).json();
  const awaitCall = (await getFn())();
}
const thisAndSuper = this.value;
const importMeta = import.meta.url;
const dynamicImport = import("./module");
const dynamicImportOptions = import("./module", { with: { type: "json" } });
const classExpr = class extends Base {};
const inOp = "a" in b;
const instanceOfOp = a instanceof B;
const parenthesizedArrow = ((x) => x)(1);
const arrowChain = (a) => (b) => (c) => a + b + c;
const longArrowChain = (firstArgument) => (secondArgument) => (thirdArgument) =>
  firstArgument + secondArgument + thirdArgument;
const conditionalBody = (x) => (x ? 1 : 2);
const veryLongObjectLiteralValue = {
  alpha: computeAlpha(input),
  beta: computeBeta(input),
  gamma: computeGamma(input),
};
