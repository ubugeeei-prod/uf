// @flow
//
// The unions these components are made of, and the typos that must not compile.
//
// The companion to `anchoring.js`, which does the same job for where an
// anchored overlay opens, and read the same way: this file is *supposed* to
// fail `uf check`, a `// expect:` comment says the line after it must be
// reported and what the report must contain, and a line without one must not be
// reported at all. `anchoring.js`'s header says why it lives here rather than
// inside the package, and why the checker is given the package and this
// directory in one command.
//
// Each of these is a mistake with no other symptom. A misspelt `side` is a
// sheet rendered off the top of the page; a `role` that is not one of the two
// that carry `aria-modal` is a modal announced as a `div`; a `kind` that is not
// a kind is a one-time-code field that quietly accepts letters and shows a
// letter keyboard on a phone; an `orientation` that is not one is a rule that
// tells a reader the page runs the other way. None of them fails at run time
// and none of them is visible in a screenshot.

import type { DialogRole } from "../../packages/ui/dialog.js";
import type { Edge } from "../../packages/ui/sheet.js";
import type { InputOtpKind } from "../../packages/ui/input-otp.js";
import type { Orientation } from "../../packages/ui/separator.js";
import type { SidebarSide } from "../../packages/ui/sidebar.js";
import { CarouselRoot } from "../../packages/ui/carousel.js";
import { DialogBody } from "../../packages/ui/dialog.js";
import { InputOtpRoot } from "../../packages/ui/input-otp.js";
import { Separator } from "../../packages/ui/separator.js";
import { SheetBody, SheetRoot } from "../../packages/ui/sheet.js";
import { SidebarBody, SidebarRoot } from "../../packages/ui/sidebar.js";

// A modal announces itself as one of two things, and a typo is not a third.
// expect: incompatible with DialogRole
export const misspelledRole: DialogRole = "alertdailog";

// And not as any other role either: these are the two that carry `aria-modal`,
// and a `Dialog.Body` that is a landmark is a different component.
// expect: incompatible with DialogRole
export const notEveryRole: DialogRole = "region";

// An edge is one of four names.
// expect: incompatible with Edge
export const misspelledEdge: Edge = "lft";

// A sidebar is never along the top or the bottom — a navigation rail across the
// top of a page is a header — which is why its union has two members and not
// the four a sheet's has.
// expect: incompatible with SidebarSide
export const sidebarsHaveNoTop: SidebarSide = "top";

// The two are not each other's, in that direction as well.
// expect: incompatible with SidebarSide
export const anEdgeIsNotASidebarSide: SidebarSide = "bottom";

// A code is made of digits or of letters and digits, and of nothing else.
// expect: incompatible with InputOtpKind
export const misspelledKind: InputOtpKind = "numberic";

// A rule runs one of two ways. The typo is the one mistake here whose symptom
// is a sentence rather than a layout: `aria-orientation` is what a separator
// tells a reader about the page it is dividing.
// expect: incompatible with Orientation
export const misspelledOrientation: Orientation = "verticle";

// The parts refuse the same strings, which is where a consumer meets them.
// expect: Cannot create SheetRoot element
export const sheet: mixed = <SheetRoot side="lft">Filters</SheetRoot>;

// expect: Cannot create SidebarRoot element
export const sidebar: mixed = <SidebarRoot side="top">Main</SidebarRoot>;

// expect: Cannot create DialogBody element
export const dialog: mixed = <DialogBody role="banner">Settings</DialogBody>;

export const otp: mixed = (
  // The marker is here rather than above the statement because the line it
  // names is the one the checker points at, which is the opening tag.
  // expect: Cannot create InputOtpRoot element
  <InputOtpRoot kind="numberic" label="One-time code" length={6}>
    slots
  </InputOtpRoot>
);

// A carousel that has not been told how many slides it holds cannot label one
// "3 of 7", which is the only way a reader knows where they are.
// expect: Cannot create CarouselRoot element
export const carousel: mixed = <CarouselRoot label="Featured">slides</CarouselRoot>;

// A `<nav>` with no name is announced as "navigation", so the label is not
// optional either — and a page with two of those has told the reader there are
// two and left which is which to a guess.
// expect: Cannot create SidebarBody element
export const unnamed: mixed = <SidebarBody>Main</SidebarBody>;

// expect: Cannot create Separator element
export const rule: mixed = <Separator orientation="verticle" />;

// What is *not* an error: the members themselves, and a call that uses them.
export const edge: Edge = "bottom";
export const side: SidebarSide = "right";
export const role: DialogRole = "alertdialog";
export const kind: InputOtpKind = "alphanumeric";
export const orientation: Orientation = "vertical";
export const divider: mixed = <Separator decorative orientation="vertical" />;
export const fine: mixed = (
  <SheetRoot side="bottom">
    <SheetBody>Filters</SheetBody>
  </SheetRoot>
);
