// @flow
const plain = `no substitutions`;
const multi = `line one
line two ${value}
  indented ${other}
`;
const nestedTemplate = `outer ${`inner ${deep}`} done`;
const longTemplate = `this template literal is quite long and has a substitution ${withAValue} and then keeps going for a while`;
const tagged = css`
  color: ${color};
  background: ${background};
`;
const untagged = sql`SELECT * FROM users WHERE id = ${id}`;
const objectInTemplate = `${{ a: 1 }.a}`;
const callInTemplate = `${fn(a, b)}`;
const conditionalInTemplate = `${cond ? "yes" : "no"}`;
const memberInTemplate = `${obj.prop.deep}`;
const templateWithBackslash = `escaped \` backtick and \${not} a substitution`;
const templateInCall = fn(`template ${arg}`);
const templateNewlineArg = fn(`
  multi
  line
`);
const templateExpr = `${
  veryLongExpressionInsideTemplateLiteralThatShouldNotBeBrokenByThePrinterAtAll +
  anotherLongExpression
}`;

function noParams() {}
function oneParam(a) {}
function defaults(a = 1, b = { key: "value" }, c = [1, 2]) {}
function destructured({ a, b }) {}
function destructuredWithDefault({ a, b } = {}) {}
function arrayDestructured([a, b]) {}
function rest(a, ...rest) {}
function longParams(firstParameterName, secondParameterName, thirdParameterName, fourthParameterName) {}
function longParamsWithDefaults(firstParameterName = 1, secondParameterName = 2, thirdParameterName = 3) {}
function typedLongParams(first: FirstType, second: SecondType, third: ThirdType, fourth: FourthType): void {}
function hugged({ alpha, beta, gamma, delta, epsilon, zeta, eta, theta, iota, kappa, lambda, mu }) {}
function huggedTyped({ alpha, beta }: { alpha: string, beta: number, gamma: boolean, delta: string, epsilon: number }) {}
function returnsObjectType(): { a: string, b: number, c: boolean, d: string, e: number, f: boolean, g: string } {}
async function asyncFn() {}
function* generatorFn() {}
async function* asyncGenerator() {}
const arrowSingle = (x) => x;
const arrowMulti = (a, b) => a + b;
const arrowAsync = async () => {};
const arrowRest = (...args) => args;
const arrowDefault = (a = 1) => a;
const arrowDestructured = ({ a, b }) => a + b;
const arrowLong = (firstParameterName, secondParameterName, thirdParameterName, fourthParameterName) => firstParameterName;
const arrowBodyCall = (x) => someFunction(x, withAnotherArgument, andYetAnotherOne, andOneMoreToMakeItLong);
const arrowBodyObject = (x) => ({ key: x, another: 1, more: 2, evenMore: 3, andMore: 4, andEvenMore: 5, andSoOn: 6 });
const arrowBodyArray = (x) => [x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x];
const arrowBodyTernary = (x) => x ? somethingQuiteLongToReturnWhenTrue : somethingQuiteLongToReturnWhenFalse;
const arrowBodyBinary = (x) => x.someProperty + x.anotherProperty + x.yetAnotherProperty + x.oneMoreProperty + x.last;
const arrowBodySequence = (x) => (a, b);
const arrowBodyAwait = async (x) => await x;
const arrowBodyJsx = (x) => <div>{x}</div>;
const arrowReturningArrow = (a) => (b) => a + b;
const arrowChainCallee = ((a) => (b) => a + b)(1)(2);
const assignedArrowChain = (firstArgument) => (secondArgument) => (thirdArgument) => firstArgument + secondArgument + thirdArgument + 1;
promise.then((result) => (result.ok ? result.value : defaultValueThatIsQuiteLongIndeed) + anotherValueToAdd);
const curried = (a) => (b) => (c) => (d) => (e) => (f) => (g) => (h) => (i) => (j) => (k) => a + b;
const withTypeParams = <T>(value: T): Array<T> => [value];
const named = function name() {};
const generatorExpr = function* () {};
