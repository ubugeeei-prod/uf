// @flow
const shortValue = 1;
const aVeryLongVariableNameThatTakesUpSpace = anotherVeryLongFunctionName(
  withSomeArguments,
  andMore,
);
const chained = one.two.three.four.five.six.seven.eight.nine.ten.eleven.twelve.thirteen.fourteen;
const stringValue =
  "a very long string literal that does not fit on the line with the declaration at all";
const templateValue = `a very long template literal that does not fit on the line with the declaration at all`;
const numberValue = 12345678901234567890123456789012345678901234567890123456789012345678901234567890;
const booleanValue = someVeryLongConditionThatIsABooleanExpressionResult && anotherConditionHereToo;
const conditional = someVeryLongConditionExpression
  ? consequentValueWhenTrue
  : alternateValueWhenFalse;
const conditionalWithBinaryTest =
  firstOperand && secondOperand ? consequentValueWhenTrue : alternateValue;
const classValue = class extends SomeVeryLongBaseClassName {
  method() {
    return 1;
  }
};
const requireValue = require("a-module-with-a-very-long-name-that-goes-on-and-on-and-on-and-on-and-on");
const memberChain = this.props.someProperty.anotherProperty.yetAnotherProperty.finalProperty.value;
const callChain = object.method().anotherMethod().yetAnotherMethod().finalMethod().result.value;
async function awaiting() {
  const awaited = await someAsyncFunctionWithALongName(argumentNumberOne, argumentNumberTwo, three);
}
const unary = !someVeryLongConditionExpressionThatIsNegatedAndDoesNotFitOnTheLineAtAllReally;
const arrow = (argument) => someFunctionCall(argument, anotherArgument, yetAnotherArgument, more);
const object = { key: "value", another: "value", yetAnother: "value", stillMore: "value", last: 1 };
const array = [firstElement, secondElement, thirdElement, fourthElement, fifthElement, sixth];
const nested = {
  outer: { inner: { deep: { deeper: { deepest: "a value that pushes this past" } } } },
};
const destructured = ({ alpha, beta, gamma, delta, epsilon, zeta, eta, theta, iota, kappa } =
  source);
const {
  alpha: renamedAlpha,
  beta: renamedBeta,
  gamma: renamedGamma = defaultValue,
} = anotherSource;
const [firstElementName, secondElementName, thirdElementName, fourthElementName] = tupleSource;
const typed: SomeVeryLongTypeName<WithTypeArguments, AndMore> = someFunctionCall(
  withArguments,
  more,
);
const typedGeneric: Map<string, Array<{ key: string, value: number }>> = new Map();
const genericArrow: GenericFunction<Input, Output> = <T>(input: T): Output => transform(input);
a.b.c.d = someVeryLongFunctionCall(withSomeArguments, andMoreArguments, andEvenMoreArguments, x);
this.state = { loading: true, error: null, data: undefined, retries: 0, lastAttempt: Date.now() };
module.exports = { first, second, third, fourth, fifth, sixth, seventh, eighth, ninth, tenth, x };
exports.handler = async function handler(event, context) {
  return { statusCode: 200, body: "" };
};
a = b = c = someVeryLongExpressionThatDoesNotFitOnASingleLineWithAllOfTheAssignmentsHere;
x.y = z.w = veryLongValueExpressionThatGoesOnForAWhileAndThenSomeMoreCharactersHere + 1;
const fluid = someCondition && anotherCondition && yetAnotherCondition && finalConditionHere;
const shortKeyObject = {
  a: someVeryLongFunctionCallExpression(withArguments, andMoreArguments, andMore),
};
const longKeyObject = {
  aReasonablyLongKey: someVeryLongFunctionCallExpression(withArguments, andMore),
};
const regexValue =
  /a very long regular expression pattern that goes on and on and on and on and on/gi;
let uninitialized;
let multiple = 1,
  declarations = 2,
  here = 3;
let multipleLong = someLongInitializerExpression(withArguments),
  another = anotherInitializer(more);
const bigNumberLiteral = 1e100;
const poorlyBreakable = this.someProperty.someMethod().anotherProperty.finalMethod(shortArg);
