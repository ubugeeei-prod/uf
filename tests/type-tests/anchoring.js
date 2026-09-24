// @flow
//
// Misuses that have to be type errors, and the test that says they are.
//
// Every refusal in this file is a type error `uf check` must raise, suppressed
// where it stands. Every line below is a mistake a consumer can make about
// where an overlay opens, and the whole claim `internal/anchor.js` makes about
// its types is that the checker catches each one at the call rather than
// opening the overlay somewhere else at run time. A claim like that is not
// provable by rendering anything, so it is proved the only way it can be: by
// running the checker and reading what it said.
//
// # How it is read
//
// A `// $FlowExpectedError[code]` comment says that the line after it must be
// reported with that code, and the words after the code say what the report is
// about. Flow suppresses the error, so `uf check` at the repository root stays
// clean, and a suppression that stops matching an error is reported as unused,
// which fails the test. A line without one must not be reported at all — so a
// change that makes any of these *stop* being an error fails the test, and so
// does one that makes something else here start being one.
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

import type { Align, LogicalSide, Side } from "../../packages/ui/internal/anchor.js";
import { Popover, Tooltip } from "../../packages/ui/index.js";

// A side is one of four names, and a typo is not a fifth.
// $FlowExpectedError[incompatible-type] incompatible with Side
export const misspelledSide: Side = "bottmo";

// An alignment is one of three.
// $FlowExpectedError[incompatible-type] incompatible with Align
export const misspelledAlign: Align = "middle";

// And the two are not each other's: `start` aligns, it does not face.
// $FlowExpectedError[incompatible-type] incompatible with Side
export const alignmentIsNotASide: Side = "start";

// $FlowExpectedError[incompatible-type] incompatible with Align
export const sideIsNotAnAlignment: Align = "bottom";

// `Side` stays physical, which is the whole reason `LogicalSide` is a second
// name: a design that puts a popover to the right of a toolbar means the right
// of it in Arabic too, and only a submenu wants the side that follows the
// reading direction.
// $FlowExpectedError[incompatible-type] incompatible with Side
export const logicalIsNotPhysical: Side = "inline-end";

// The parts refuse the same strings, which is where a consumer meets them.
// $FlowExpectedError[incompatible-type] Cannot create Popover.Body element
export const popover: mixed = <Popover.Body side="bottmo">Filters</Popover.Body>;

// $FlowExpectedError[incompatible-type] Cannot create Tooltip.Body element
export const tooltip: mixed = <Tooltip.Body align="middle">Bold</Tooltip.Body>;

// $FlowExpectedError[incompatible-type] Cannot create Tooltip.Body element
export const offset: mixed = <Tooltip.Body sideOffset="8">Bold</Tooltip.Body>;

// A logical side is one of two names, and a typo is not a third.
// $FlowExpectedError[incompatible-type] Cannot create Popover.Body element
export const logicalTypo: mixed = <Popover.Body side="inline-ende">Filters</Popover.Body>;

// What is *not* an error: the four sides and the three alignments themselves.
export const side: Side = "left";
export const align: Align = "end";
// The two logical ones, and a part that takes them: this is what a submenu asks
// for, and what an overlay in a right-to-left page resolves to the left.
export const logical: LogicalSide = "inline-start";
export const inlineEnd: mixed = <Popover.Body side="inline-end">Filters</Popover.Body>;
export const fine: mixed = (
  <Popover.Body align="start" side="right" sideOffset={8}>
    Filters
  </Popover.Body>
);
