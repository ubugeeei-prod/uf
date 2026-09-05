// @flow

// Flow's comment types: annotations that a file can carry and still run
// under bare `node`. Normalising them away is not a layout decision, it is a
// file that needed no build step and now needs one. See ubugeeei-prod/uf#126.

function greet(name /*: string */) /*: string */ {
  return "hi " + name;
}

const total /*: number */ = 1;
const plain: number = 2;

function mixed(a /*: string */, b: number, c /*: ?Array<string> */) {
  return a;
}

class Holder {
  field /*: string */ = "x";
  method(x /*: number */) /*: void */ {}
}

const arrow = (x /*: number */) /*: number */ => x;

// The bytes come back untouched, spacing included. A real annotation would be
// re-printed as `{ [string]: string }`; inside the comment it is text.
const table /*: {[string]: string} */ = {};
const wide /*: Array<{product: string, packagePath: string, packageName: string}> */ = [];

// Ordinary annotations next to them are still normalised.
const spaced: { [string]: string } = {};

// The declaration form: a whole statement inside the comment. The block goes
// out as it was written — a `node` that reads this file sees a comment, and
// that is the entire point of the syntax.
/*:: import type { Schema } from "./schema"; */

/*:: type Named = {name:string}; */

/*::
type Pair = {a:string, b:string};
type Triple = {a:string,b:string,c:string};
*/

/*:: export type Out = { v : number } ; */

// Type arguments, which are the ones that break a file most plainly: the
// angle brackets are a syntax error to anything that is not Flow.
const byPath = new Map /*:: <string, Named> */();
const plainArgs = new Map<string, number>();

// A block can hold a class *member* rather than a statement. Parcel writes
// one to declare an iterator a Flow interface asks for, and the class body
// has to leave it alone the same way a statement list does.
export class Symbols {
  /*::
  @@iterator(): Iterator<[string, {|local: string|}]> { return ({}: any); }
  */
  #value: string;

  size() /*: number */ {
    return 0;
  }
}
