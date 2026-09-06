// @flow
//
// Misuses of `@uniflowed/state`'s utilities that have to be type errors, and
// the test that says they are.
//
// This file is *supposed* to fail `uf check`. `jotai/utils` is the place where
// a wrong type shows up as an `any` spreading through an application — a
// selection whose slice was inferred from the wrong side, a persisted atom
// whose value is treated as the thing it will eventually hold — and every
// claim ubugeeei-prod/uf#292 and ubugeeei-prod/uf#317 make about these constructors is that the checker
// stops that at the call. A claim like that is not provable by running
// anything, so it is proved the only way it can be: by running the checker and
// reading what it said.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and that
// the report must contain that text. A line without one must not be reported at
// all — so a change that makes any of these *stop* being an error fails the
// test, and so does one that makes something else here start being one.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which every type below is `any`, every line passes, and the test would prove
// nothing. So the test runs `uf check tests/type-tests packages/state`, with
// both in one set. `anchoring.js` is the same arrangement for `packages/ui`,
// and its header says why neither file lives inside the package it checks.

import type { Loadable } from "../../packages/state/index.js";
import {
  RESET,
  atom,
  atomWithAsyncStorage,
  atomWithReducer,
  atomWithReset,
  freezeAtom,
  read,
  selectAtom,
  write,
} from "../../packages/state/index.js";

type User = { readonly name: string, readonly avatar: string };

const count = atom(1);
const user = atom<User>({ name: "Ada", avatar: "a.png" });
const tags = atom<Array<string>>(["a"]);

// --- selectAtom -------------------------------------------------------------
//
// The slice's type comes from what the selection returns, not from what it
// reads. Getting that backwards is how a `number` becomes a `string` three
// files away.

const nameLength = selectAtom(user, (current) => current.name.length);

// expect: number
export const lengthIsNotAString: string = read(nameLength);

// The selection is handed the source's value, so a method the source does not
// have is refused where it is written rather than where it is called.
// expect: toUpperCase
export const numberHasNoCase: mixed = selectAtom(count, (current) => current.toUpperCase());

// A selection is readable and nothing else: there is no write that could put a
// slice back into what it came from.
// expect: ReadonlyAtom<number> is incompatible with
export const sliceIsNotWritable: mixed = write(nameLength, 4);

// `equals` compares two slices, not two sources.
export const equalsComparesSlices: mixed = selectAtom(
  user,
  (current) => current.name,
  // expect: User is incompatible with string
  (previous: User, next: User) => previous.name === next.name,
);

// --- atomWithReducer --------------------------------------------------------
//
// The reason to have it at all: the value and the argument are different
// types, so a component cannot dispatch a state.

const counter = atomWithReducer<number, "increment" | "clear">(0, (current, action) =>
  action === "increment" ? current + 1 : 0,
);

// expect: 3 is incompatible with
export const stateIsNotAnAction: mixed = write(counter, 3);

// expect: is incompatible with "increment" | "clear"
export const typoIsNotAnAction: mixed = write(counter, "incremen");

// And it still reads as its value.
// expect: number is incompatible with string
export const valueIsNotAnAction: string = read(counter);

// --- atomWithReset ----------------------------------------------------------

const theme = atomWithReset("light");

// A plain atom does not answer to RESET, and the symbol is opaque, so that is
// a type error rather than a write that silently stores a symbol.
// expect: Reset is incompatible with
export const plainAtomHasNoReset: mixed = write(count, RESET);

// --- freezeAtom -------------------------------------------------------------
//
// Freezing changes nothing about the type. An implementation that lost it —
// walking the value as `mixed` and handing back what it walked — would type
// every frozen atom as `mixed`, and this is what says it does not.

// expect: Array<string> is incompatible with number
export const frozenKeepsItsType: number = read(freezeAtom(tags));

// --- atomWithAsyncStorage ---------------------------------------------------
//
// The whole argument of ubugeeei-prod/uf#317 is that the value is a `Loadable<T>` and not a
// `T`. If these pass, the constructor is the thing the issue declined.

const draft = atomWithAsyncStorage("draft", "", {
  getItem: () => Promise.resolve(""),
  setItem: () => Promise.resolve(),
  removeItem: () => Promise.resolve(),
});

// expect: is incompatible with string
export const valueIsNotTheData: string = read(draft);

// The setter takes the value, not the loadable a caller happens to be holding.
// expect: object literal is incompatible with
export const setterDoesNotTakeALoadable: mixed = write(draft, {
  state: "hasData",
  data: "typed",
});

// And its reducer is handed the loadable, because a write can happen before
// the first read has settled. A reducer written as if the value were there is
// refused here rather than reading `undefined` at run time.
// expect: toUpperCase
export const reducerSeesTheLoadable: mixed = write(draft, (current) => current.toUpperCase());

// --- what is *not* an error -------------------------------------------------
//
// Without these the fixture would pass just as happily on a package whose
// every export was `any`.

export const sliceReadsAsItsSlice: number = read(nameLength);
export const reducerTakesItsAction: void = write(counter, "increment");
export const resettableTakesReset: void = write(theme, RESET);
export const resettableTakesItsValue: void = write(theme, "dark");
export const frozenReadsAsItsValue: Array<string> = read(freezeAtom(tags));
export const asyncStorageReadsAsALoadable: Loadable<string> = read(draft);
export const asyncStorageTakesItsValue: void = write(draft, "typed");
export const asyncStorageTakesReset: void = write(draft, RESET);
export const asyncStorageReducerSeesTheState: void = write(draft, (current) =>
  current.state === "hasData" ? current.data : "",
);
