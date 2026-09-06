// @flow
//
// Misuses that have to be type errors, and the test that says they are.
//
// This file is *supposed* to fail `uf check`. Every line below is a mistake a
// consumer can make about where an overlay opens, and the whole claim
// `internal/anchor.js` makes about its types is that the checker catches each
// one at the call rather than opening the overlay somewhere else at run time. A
// claim like that is not provable by rendering anything, so it is proved the
// only way it can be: by running the checker and reading what it said.
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
// which `Side` is `any`, every line below passes, and the test would prove
// nothing. So the test runs `uf check tests/type-tests packages/ui`, with both
// in one set.
//
// It is here rather than inside `packages/ui` because everything under
// `packages/` is shipped: `crates/uf_lib/tests/package_surface.rs` requires
// every module there to be reachable through an `exports` subpath or to be an
// `internal/` one, and npm's `files` patterns match at any depth, so a
// `type-tests/` directory in the package would be published — a file of
// deliberate type errors, with a React element built at import time, inside a
// package that promises neither.

import type { Align, Side } from "../../packages/ui/internal/anchor.js";
import { PopoverBody } from "../../packages/ui/popover.js";
import { TooltipBody } from "../../packages/ui/tooltip.js";

// A side is one of four names, and a typo is not a fifth.
// expect: incompatible with Side
export const misspelledSide: Side = "bottmo";

// An alignment is one of three.
// expect: incompatible with Align
export const misspelledAlign: Align = "middle";

// And the two are not each other's: `start` aligns, it does not face.
// expect: incompatible with Side
export const alignmentIsNotASide: Side = "start";

// expect: incompatible with Align
export const sideIsNotAnAlignment: Align = "bottom";

// The parts refuse the same strings, which is where a consumer meets them.
// expect: Cannot create PopoverBody element
export const popover: mixed = <PopoverBody side="bottmo">Filters</PopoverBody>;

// expect: Cannot create TooltipBody element
export const tooltip: mixed = <TooltipBody align="middle">Bold</TooltipBody>;

// expect: Cannot create TooltipBody element
export const offset: mixed = <TooltipBody sideOffset="8">Bold</TooltipBody>;

// What is *not* an error: the four sides and the three alignments themselves.
export const side: Side = "left";
export const align: Align = "end";
export const fine: mixed = (
  <PopoverBody align="start" side="right" sideOffset={8}>
    Filters
  </PopoverBody>
);
