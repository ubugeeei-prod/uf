// @flow
//
// The unions these components are made of, and the typos that must not compile.
//
// The companion to `anchoring.js`, which does the same job for where an
// anchored overlay opens, and read the same way: each misuse carries a
// `$FlowExpectedError[code]` comment naming the error it must raise, a
// suppression that stops matching is reported as unused and fails the test, and
// a line without one must not be reported at all. `anchoring.js`'s header says
// why it lives here rather than inside the package, and why the checker is
// given the package and this directory in one command.
//
// Each of these is a mistake with no other symptom. A misspelt `side` is a
// sheet rendered off the top of the page; a `role` that is not one of the two
// that carry `aria-modal` is a modal announced as a `div`; a `kind` that is not
// a kind is a one-time-code field that quietly accepts letters and shows a
// letter keyboard on a phone; an `orientation` that is not one is a rule that
// tells a reader the page runs the other way. None of them fails at run time
// and none of them is visible in a screenshot.

import type { DialogRole } from "../../npm/ui/dialog.js";
import type { Edge } from "../../npm/ui/sheet.js";
import type { InputOtpKind } from "../../npm/ui/input-otp.js";
import type { Orientation } from "../../npm/ui/separator.js";
import type { SidebarSide } from "../../npm/ui/sidebar.js";
import { Carousel, Dialog, InputOtp, Sheet, Sidebar } from "../../npm/ui/index.js";
import { Separator } from "../../npm/ui/separator.js";

// A modal announces itself as one of two things, and a typo is not a third.
// $FlowExpectedError[incompatible-type] incompatible with DialogRole
export const misspelledRole: DialogRole = "alertdailog";

// And not as any other role either: these are the two that carry `aria-modal`,
// and a `Dialog.Body` that is a landmark is a different component.
// $FlowExpectedError[incompatible-type] incompatible with DialogRole
export const notEveryRole: DialogRole = "region";

// An edge is one of four names.
// $FlowExpectedError[incompatible-type] incompatible with Edge
export const misspelledEdge: Edge = "lft";

// A sidebar is never along the top or the bottom — a navigation rail across the
// top of a page is a header — which is why its union has two members and not
// the four a sheet's has.
// $FlowExpectedError[incompatible-type] incompatible with SidebarSide
export const sidebarsHaveNoTop: SidebarSide = "top";

// The two are not each other's, in that direction as well.
// $FlowExpectedError[incompatible-type] incompatible with SidebarSide
export const anEdgeIsNotASidebarSide: SidebarSide = "bottom";

// A code is made of digits or of letters and digits, and of nothing else.
// $FlowExpectedError[incompatible-type] incompatible with InputOtpKind
export const misspelledKind: InputOtpKind = "numberic";

// A rule runs one of two ways. The typo is the one mistake here whose symptom
// is a sentence rather than a layout: `aria-orientation` is what a separator
// tells a reader about the page it is dividing.
// $FlowExpectedError[incompatible-type] incompatible with Orientation
export const misspelledOrientation: Orientation = "verticle";

// The parts refuse the same strings, which is where a consumer meets them.
// $FlowExpectedError[incompatible-type] Cannot create Sheet.Root element
export const sheet: mixed = <Sheet.Root side="lft">Filters</Sheet.Root>;

// $FlowExpectedError[incompatible-type] Cannot create Sidebar.Root element
export const sidebar: mixed = <Sidebar.Root side="top">Main</Sidebar.Root>;

// $FlowExpectedError[incompatible-type] Cannot create Dialog.Body element
export const dialog: mixed = <Dialog.Body role="banner">Settings</Dialog.Body>;

export const otp: mixed = (
  // The marker is here rather than above the statement because the line it
  // names is the one the checker points at, which is the opening tag.
  // $FlowExpectedError[incompatible-type] Cannot create InputOtp.Root element
  <InputOtp.Root kind="numberic" label="One-time code" length={6}>
    slots
  </InputOtp.Root>
);

// A carousel that has not been told how many slides it holds cannot label one
// "3 of 7", which is the only way a reader knows where they are.
// $FlowExpectedError[incompatible-type] Cannot create Carousel.Root element
export const carousel: mixed = <Carousel.Root label="Featured">slides</Carousel.Root>;

// A `<nav>` with no name is announced as "navigation", so the label is not
// optional either — and a page with two of those has told the reader there are
// two and left which is which to a guess.
// $FlowExpectedError[incompatible-type] Cannot create Sidebar.Body element
export const unnamed: mixed = <Sidebar.Body>Main</Sidebar.Body>;

// $FlowExpectedError[incompatible-type] Cannot create Separator element
export const rule: mixed = <Separator orientation="verticle" />;

// What is *not* an error: the members themselves, and a call that uses them.
export const edge: Edge = "bottom";
export const side: SidebarSide = "right";
export const role: DialogRole = "alertdialog";
export const kind: InputOtpKind = "alphanumeric";
export const orientation: Orientation = "vertical";
export const divider: mixed = <Separator decorative orientation="vertical" />;
export const fine: mixed = (
  <Sheet.Root side="bottom">
    <Sheet.Body>Filters</Sheet.Body>
  </Sheet.Root>
);
