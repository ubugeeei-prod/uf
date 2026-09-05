// @flow
// A ternary's test keeps its own indentation unless the ternary sits in an
// argument list, and `new Error(…)` and `import(…)` are not argument lists for
// this purpose — Prettier asks `isCallExpression`, which `new` is not.
// react-native's Touchable.js is where the difference shows.

function _receiveSignal(signal: Signal, e: GestureResponderEvent) {
  throw new Error(
    'Unrecognized signal `' + signal + '` or state `' + curState + '` for Touchable responder `' + typeof this.state.touchable.responderID === 'number' ? this.state.touchable.responderID : 'host component' + '`',
  );
}

async function grandparents() {
  // `new`: the test's right operand stays at the ternary's indentation.
  const A = new Error(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy);
  // A plain call: it is indented.
  const B = callit(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy);
  // An optional call is a call.
  const J = obj?.m(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy);
  // `import(…)` is not.
  const H = await import(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy);
  // Neither is a computed member lookup, an array, or a bare initializer.
  const K = obj[aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy];
  const D = [aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy];
  const G = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy;
  // A `return` is its own rule, one clause earlier.
  return aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb === "numberXY" ? xxxxxxxxxxxxxxxx : yyyyyyyyyyyyyyyy;
}
