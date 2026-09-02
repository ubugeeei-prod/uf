#!/usr/bin/env uf
// @flow
/**
 * Doc comment for the module.
 *
 *   Indented continuation that must not be reflowed even if it is very long.
 */

/* leading block */ const first = 1; // trailing line

const obj = {
  // comment before a key
  a: 1, // after a value
  /* between */ b: 2,
  c: /* inline before value */ 3,
};

function f(
  // comment inside a parameter list
  a,
  b /* after the last parameter */
) {
  /* first statement */ return a + b; // tail
}

const call = compute(/* no args */);

if (flag) {
  // only a comment
} else {
  /* else branch */
}

const arr = [
  1, // one
  2, /* two */
];

/* a block comment
   spanning several lines
     with its own indentation */
export default f;
// trailing comment at end of file
