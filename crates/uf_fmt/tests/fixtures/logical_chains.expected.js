// @flow

// A chain the source wrote right-nested. The parentheses are redundant for
// `&&`, `||` and `??`, so they are not printed, and what is left has to lay
// out as one chain — see ubugeeei-prod/uf#133.
const rightNested =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb &&
  cccccccccccccccccccccccccccccccccc;
const leftNested =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb &&
  cccccccccccccccccccccccccccccccccc;
const flat =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb &&
  cccccccccccccccccccccccccccccccccc;

const orRight =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ||
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb ||
  ccccccccccccccccccccccccccc;
const nullishRight =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ?? bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb ?? ccccccccccccccccccc;

// Short enough to stay on one line, whichever way it was written.
const short = a && b && c;
const shortOr = a || b || c;
const shortNullish = a ?? b ?? c;

// Mixed operators keep their parentheses and their group.
const mixed =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
  (b || c);
const mixedLeft = (a && b) || c;
const mixedOther = a || (b && c);

// Arithmetic and bitwise are not associative to the formatter: the
// parentheses stay, and so does the group.
const plus =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa +
  (b + c);
const times =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa * (b * c);
const bitOr =
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | (b | c);

// The shape prepack writes, which is where this was found.
const found =
  descriptor instanceof PropertyDescriptor &&
  descriptor.get === undefined &&
  descriptor.set === undefined;

// A right operand that is an object still hugs the operator.
const inlined = someCondition &&
  anotherConditionThatIsQuiteLong && { key: value, other: thing, third: item };

if (
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb &&
  cccccccccccccccccccccccccccccccc
) {
  run();
}

function returnsIt() {
  return (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa &&
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb &&
    cccccccccccccccccccccccccccccc
  );
}
