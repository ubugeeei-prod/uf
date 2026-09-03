/**
 * @flow strict
 * @format
 */

// Leading line comment.
const a = 1; // trailing line comment
const b = /* inline block */ 2;
const c = 3; /* trailing block */

/* Block before statement. */
function documented(/* no params */) {}
function params(a /* first */, b /* second */) {}
function trailingParam(a, b /* last */) {}
function body() {
  // only a comment
}
function returnsWithComment() {
  return (
    // explains the value
    value
  );
}

const object = {
  // leading property comment
  key: 1, // trailing property comment
  /* block before key */ other: 2,
  last: 3,
  // dangling at the end
};
const emptyWithComment = {
  // nothing here
};
const array = [
  1, // one
  2,
  // three
  3,
];
const emptyArrayWithComment = [
  // empty
];

call(/* dangling */);
call(a, /* between */ b);
call(
  // leading argument comment
  a,
  b, // trailing argument comment
);

if (condition) {
  // inside if
} else {
  // inside else
}
if (x) {
  doX();
} // trailing if comment
else {
  doY();
}
if (x)
  y(); // trailing
// before else
else z();

switch (value) {
  // before case
  case 1:
    // inside case
    break;
  default:
  // inside default
}

const arrow = () => {
  // body comment
};
const arrowWithComment = (
  // param comment
  x,
) => x;
const fn = function (/* inline */) {};

class Klass {
  // class body comment
  method() {} // trailing method comment

  // before property
  property = 1;
}
class EmptyClass {
  // empty class
}

type T = {
  // leading
  a: string, // trailing
  /* block */ b: number,
};
type Union =
  // first
  | A
  // second
  | B;

// $FlowFixMe[incompatible-type]
const suppressed: string = 1;
// $FlowExpectedError
const expected: number = "x";

const ternary = condition
  ? // when true
    a
  : // when false
    b;

const chained = promise
  // first step
  .then(a)
  // second step
  .then(b);

let value = 1; // after equals

function last() {
  return 1;
  // after return
}

export default {
  // comment in export
};

/* multi
   line
   block */
const multi = 1;

/**
 * Indented doc block
 *   with a continuation
 */
const indented = 2;

// final comment
