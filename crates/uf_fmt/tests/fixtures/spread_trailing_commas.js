// @flow
// A spread as the last element takes the trailing comma; a pattern's rest
// cannot, because nothing may follow it.

type Matrix3d = [
  ...[number, number, number, number],
  ...[number, number, number, number],
  ...[number, number, number, number],
  ...[number, number, number, number],
];

type Head = [number, ...Rest];

const collected = [
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
  ...ccccccccccccccc,
];

call(
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
  ...ccccccccccccccc,
);

const [
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
  ...ccccccccccccccc
] = source;

function withRest(
  aaaaaaaaaaaaaaaaaaaaaaaaaaa: number,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb: number,
  ...rest: Args
) {}

type Inexact = { a: number, ... };
