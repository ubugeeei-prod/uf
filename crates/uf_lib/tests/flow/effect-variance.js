// @flow
//
// Fixture, never shipped: `crates/uf_lib/tests/` sits outside the published
// package, whose `files` field only reaches `crates/uf_lib/lib/core`.
//
// It records what the variance sigils on the shipped opaque types promise, so
// the guarantee is written down as code a type checker can settle rather than
// as a comment. The Rust side enforces the other half in
// `covariant_opaque_types_are_defined_with_a_covariant_carrier`: a `+T` sigil
// must be backed by a carrier that only ever produces a `T`.

import type { Effect, Task } from "@uniflowed/core/effect";
import type { Cell } from "@uniflowed/core/flow-cell";

type Animal = { +name: string };
type Dog = { +name: string, +breed: string };

declare var dogEffect: Effect<Dog>;
declare var animalEffect: Effect<Animal>;
declare var dogTask: Task<Dog>;
declare var dogCell: Cell<Dog>;

// `Effect<+T>` is covariant: a more specific effect flows into a more general
// one, exactly as the `+` sigil claims.
const widenedEffect: Effect<Animal> = dogEffect;

// `Task<+T>` carries the same guarantee.
const widenedTask: Task<Animal> = dogTask;

// The other direction must not typecheck: covariance is one-way.
// $FlowExpectedError[incompatible-type] - Effect<Animal> is not an Effect<Dog>
const narrowedEffect: Effect<Dog> = animalEffect;

// `Cell<T>` has no sigil and must stay invariant, because a cell is written as
// well as read: widening it would let a `Dog` cell be filled with a `Cat`.
// $FlowExpectedError[incompatible-type] - Cell<Dog> is not a Cell<Animal>
const widenedCell: Cell<Animal> = dogCell;

export type VarianceFixture = {
  +widenedEffect: typeof widenedEffect,
  +widenedTask: typeof widenedTask,
  +narrowedEffect: typeof narrowedEffect,
  +widenedCell: typeof widenedCell,
};
