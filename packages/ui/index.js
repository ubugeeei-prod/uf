// @flow
//
// `@uniflowed/ui`: components you own, with the behaviour you would get wrong.
//
// The premise is the one shadcn established and it is the right one: a
// component library that ships styles is a library you fight, so these ship
// none. Every part takes `className` and every other DOM prop and passes it
// through; what they contribute is the part that is genuinely hard and
// genuinely invisible when it is missing.
//
// That part is behaviour, and specifically keyboard and screen-reader
// behaviour: a roving `tabindex` so a twelve-tab list does not take twelve Tab
// presses to get past, a focus trap that actually cannot be escaped, focus
// restored to whatever opened a dialog, typeahead in a menu, an
// `aria-activedescendant` that names an option still in the document. None of
// it is visible in a screenshot and all of it is what separates a component
// from a `div` that looks like one.
//
// Each primitive implements the WAI-ARIA authoring practices pattern for it —
// the roles, the `aria-*` wiring, the focus management and the whole keyboard
// map — and each module's header says which interaction it exists to get right
// and what a naive version breaks.
//
// # Composition is type-checked
//
// This is where Flow says something no other type system can. `Tabs.List`
// declares `renders* Tabs.Tab`, so a `<button>` in a tab list is a *type
// error* — not a review comment, not a runtime warning, not a screen reader
// announcing "button" where the reader expected "tab, 2 of 5". `Menu.Body` and
// `Combobox.List` state the same constraint about what may appear inside a
// menu and a listbox, which ARIA also requires and which nothing else checks.
// A library written in TypeScript can document those constraints; it cannot
// state them.
//
// # There is no copy step, and `render` is what replaces it
//
// shadcn's product is not a component. It is `npx shadcn add dialog`, which
// writes the source into your repository so that you own it and change it —
// and around that sit `init`, `view`, `search`, `build`, `migrate`, `eject`, an
// MCP server, a `components.json` and a registry format anybody can publish to.
// uf answers the headline question the other way. The roadmap says "typed
// imports, preset styles, and **no copy step**", and that answer takes
// something away, so it is argued here rather than assumed.
// ubugeeei-prod/uf#303 is where it was argued; this is the decision.
//
// **Why not.** A copy is a fork with no upstream, and four things follow. A
// focus trap fixed here reaches everyone who upgrades and reaches nobody who
// copied. The composition constraints above are checked *across the boundary*:
// `Tabs.List` declaring `renders* Tabs.Tab` means something while the library
// is imported and means nothing once the source has been pasted into an
// application, because then it is the application's own component and Flow has
// nothing left to hold it to. `sideEffects: false` and one subpath per
// primitive already give a bundler everything a copy would. And the
// accessibility work stays in one place with one suite over it, rather than in
// every consumer's repository at the version they took it at.
//
// **What a caller gets instead of owning the source.** The reason people copy
// is to change the markup, so that has to be answered or this is a worse
// library for the same use. The answer is `render`, which every part that
// renders an element of its own is growing:
//
//     <Menu.Item render={(props) => <a href="/settings" {...props} />}>
//       Settings
//     </Menu.Item>
//
// The part computes every attribute, every id, every composed handler and every
// ref exactly as it would have, and hands them to the caller to put on their
// own element; `children` is among them, so a caller who spreads and
// self-closes still gets what was written between the tags. What it does *not*
// hand over is anything true of the element rather than of the part —
// `type="button"` stays on the `<button>` branch.
//
// It is deliberately not Radix's `asChild`. Cloning a child hides which props
// arrived and in what order; a function is handed the object, so a caller can
// read it, order it themselves, and drop one on purpose. And the part is still
// the part: `Menu.Body`'s `renders*` still rejects a `<div>` where a
// `Menu.Item` belongs, because a `Menu.Item` rendered as an `<a>` is a
// `Menu.Item`. That is the half a copied source cannot keep, and it is what
// makes "no copy step" a trade rather than a loss.
//
// **Where it is, today.** `Dialog`, `AlertDialog`, `Sheet`, `Drawer`, `Menu`,
// `ContextMenu`, `Menubar`, `Tabs`, `Switch` and `Checkbox` are complete —
// every part of each either takes `render` or renders no element to hand over —
// along with `Field.Control`, `Tooltip.Trigger`, `HoverCard.Trigger` and
// `Sidebar.Item`, which had it first. The rest do not have it yet, and that is
// the remainder of #303. A documented escape hatch that is not there is worse
// than an undocumented one that is, so the state of every part is a table in
// `tests/library/ui.test.js` rather than a claim in this paragraph: it names
// all of them, in three lists, and four tests hold each list to the files. A
// part added to this barrel is in none of them and the suite says so.
//
// **Are the `internal/` modules ever public?** No, and the consequence is
// worth stating rather than leaving as an omission. `merge-props.js`,
// `roving-focus.js`, `controlled-state.js` and the rest each explain in their
// own header why exporting them would publish a weaker promise than the
// components make — a consumer who could reach `merge-props.js` could build a
// part that spreads `rest` last, which is the failure it exists to prevent. So
// there is no third-party primitive that participates the way these do, and no
// registry of them: the uf-shaped equivalent of publishing a component is a
// pull request against this package, where its keyboard map gets the same suite
// as everything else. That is a real cost of the decision and the right side of
// it for a library whose value is that the hard parts are correct. What a third
// party *can* build on is the escape hatch itself, which is the whole public
// surface it needs: a component of theirs given to `render` receives the props
// this package would have used, and their own composition sits inside a part
// that is still checked.
//
// **What the CLI adds: nothing new.** There is no `uf add`, no
// `components.json` and no registry, and none is planned. The one affordance
// `shadcn view` has that is worth having is "tell me what this component is
// made of", and `uf inspect --json` already answers it: every component, its
// parts and its readiness, out of `crates/uf_lib/src/ui.rs`, which
// `cargo test -p uf_lib` holds to this barrel in both directions. A
// `uf explain Dialog` that also printed the keyboard map and the ARIA is the
// one thing #303 leaves open; it is a nicer front end for a table that already
// exists, not a copy step, and this decision does not depend on it.
// `uf.config.js` is the one configuration surface by design, so a UI option, if
// there is ever one to make, belongs there rather than in a second file.
//
// # Styling is a default, not a dependency
//
// Nothing here imports StyleX, and nothing here has a StyleX-shaped type. A
// consumer styling with plain CSS, CSS Modules or anything else gets exactly
// the same components with exactly the same behaviour; the design-system layer
// that adds uf's default styles is built *on* these, not into them.
//
// # What is not here, and where it went instead
//
// About twenty of the catalogue this package is measured against have no
// behaviour at all. Badge, Card, Button, Input, Textarea, Label and Aspect
// Ratio are, between them, a class list and a `<div>`. For a library whose
// product *is* the styles that is coherent — you copy them in and you own
// them. It is not coherent here: a `Badge` with no styles is a `<span>`, a
// `Card` with no styles is a `<div>`, and shipping them from a package that
// ships no styles would make this a library of empty elements. Shipping them
// from `@uniflowed/stylex` would make *that* a component library, which its
// own header forbids. So for a long time the answer on record was both and
// neither, which is ubugeeei-prod/uf#298.
//
// The line that holds is not "styled versus headless". It is **whether the
// thing has a decision in it**:
//
// - An ARIA decision, a state machine or a keyboard requirement makes it a
//   component here, even when it renders a single element. `Progress` is one
//   `<div>`, and it belongs, because the conditional that omits
//   `aria-valuenow` when the amount is unknown — rather than sending
//   `aria-valuenow={0}`, which says "nothing has happened" — is the whole
//   component. `Toggle` and `Checkbox` are the same shape for the same reason.
// - Anything that is only a class list belongs in `@uniflowed/stylex/preset`,
//   which already has the right form: `buttonStyles({ tone, size })`,
//   `cardStyles()`, `textStyles({ size, tone })` and `fieldStyles()` return
//   `{ className }` for a caller to spread onto their own element. That is a
//   better answer than a `<Badge>`, not a lesser one — a component wrapping a
//   `<button>` takes away `type="submit"`, `formAction`, the ref and every
//   attribute nobody thought to forward, and gives back a class name the
//   caller could have written.
//
// This is stated rather than left as an omission, because an omission reads as
// an oversight and the next contributor closes it with a `<Badge>`.
// `crates/uf_lib/src/ui.rs` carries the same decision as data: those entries
// are `Declined` with the preset functions that replace them named on each,
// and `cargo test -p uf_lib` fails if a name there stops existing.
//
// Five of that twenty are on the other side of the line and now ship — Alert,
// Avatar, Breadcrumb, Separator and Skeleton — each for one specific reason,
// and each of them one or two elements:
//
// - **Alert** is the one whose usual shape is arguably wrong to copy.
//   `role="alert"` is a live region, an element already in the document when
//   the page loads announces on insertion or not at all, and a permanently
//   rendered "your trial ends soon" box carrying that role is either an
//   interruption on every page load or silence. So the role is behind `live`,
//   and a static callout does not get one. `field.js` already makes that call
//   correctly for `Field.Error`.
// - **Avatar** is a three-state machine — loading, loaded, failed — with the
//   fallback held back so a cached image does not flash somebody's initials,
//   and with `alt=""` by default, because an avatar beside a name that puts
//   the name in `alt` makes every screen reader say it twice.
// - **Breadcrumb** is `Pagination`'s shape one door along: a named `<nav>`, one
//   `aria-current="page"`, and separators hidden so the trail is not read as
//   "Home slash Settings slash Billing".
// - **Separator** is two lines with one decision in them, and it is the
//   decision `Progress` is: `role="separator"` with an `aria-orientation` for a
//   boundary a reader should be told about, `aria-hidden` for a rule that is
//   only a rule.
// - **Skeleton** is the one that silently makes a page worse. A screen of
//   skeletons is a screen of empty boxes, so the boxes are `aria-hidden`, the
//   region they stand in is `aria-busy`, and a live region that was empty for
//   one commit says "Loading" — which is #289's rule met at the moment it bites
//   hardest, because a skeleton screen is busy on its very first render.
//
// # What these components promise React
//
// Nothing here mutates during a render, reads a ref during a render, or depends
// on a render happening exactly once — so React Compiler's memoization and
// ordinary `memo` are both safe, and none of it needs an escape hatch. The
// refs that exist (`triggerRef`, `pendingFocus`, the typeahead buffer) are
// written only from event handlers and effects, and nothing renders them.
//
// Where a component has to learn something the DOM knows — how many options a
// caller filtered down to, which item the arrow key should move to — it reads
// the document in an effect or an event handler and, if a render depends on
// the answer, puts it in state. That is deliberately *not*
// `useSyncExternalStore`: the DOM is not a store whose value a render may
// read, and reading layout during a render is the thing that API exists to
// prevent.
//
// One component does use it, and it is the case the API is actually for.
// `toast("Saved")` is called from an event handler or a `catch`, so the queue
// of notifications lives outside React — an atom in `@uniflowed/state`, read
// through `useSyncExternalStore` with the cached immutable snapshots and the
// consistent server snapshot that requires. A queue is a store; the DOM is not.
//
// # Server and client
//
// Every module that manages focus, listens to the document or holds state
// declares `"use client"`, because each of those needs a browser. That is a
// property of the components, not of the application: an RSC page may import
// this package from a Server Component, and only the parts that need the client
// join the client bundle.
//
// # How the package is laid out
//
// One root module per primitive, each with its own `exports` subpath, each
// named after the thing it implements:
//
// - `dialog.js` — the focus trap, focus restore, scroll lock and inert page,
//   and the three props the four components below it differ from it by.
// - `alert-dialog.js`, `sheet.js`, `drawer.js` and `sidebar.js` — the four
//   built on that one. An alert dialog is modal and cannot be dismissed by
//   pressing beside it; a sheet is a dialog with an edge; a drawer is a sheet
//   with a gesture, and therefore with WCAG 2.5.7's keyboard equivalent of it;
//   a sidebar is most often neither modal nor a dialog, and becomes both on a
//   narrow viewport.
// - `carousel.js`, `scroll-area.js` and `input-otp.js` — the three that replace
//   something the browser already does, and so the three that have to be better
//   than what they replaced. Each module's header says what it gives that the
//   plain element does not; if it ever stops being true, the component should
//   be deleted rather than fixed.
// - `menu.js`, `context-menu.js` and `menubar.js` — the arrow keys, typeahead,
//   submenus and `Escape` stacking, and the two components that are that
//   behaviour opened differently. A context menu is opened by the right button,
//   by `Shift+F10` and by a long press, and opens at a *point*; a menubar is a
//   row of them with one tab stop and arrows that walk between the open menus.
//   shadcn's fourth menu, Dropdown Menu, is `menu.js` under another name and is
//   deliberately not a second export.
// - `combobox.js` — `aria-activedescendant` over a filtered list, the count a
//   screen reader is told, and the option groups that make a command palette a
//   composition rather than a seventh module.
// - `select.js` — the other half of the combobox pattern: the select-only one,
//   with typeahead, option groups and a value a form can submit.
// - `tabs.js` — the roving `tabindex`, and automatic versus manual activation.
// - `toast.js` — the live region that was watching before there was anything
//   to announce, and the countdown that stops.
// - `field.js` — the label, description, error and `aria-invalid` wiring.
// - `switch.js`, `checkbox.js` and `toggle.js` — the three two-state controls,
//   apart because a reader is told something different by each, and because the
//   third state and the `Enter` key genuinely differ between them.
// - `radio-group.js` — one answer out of several, and the tab stop an
//   unanswered group would otherwise not have.
// - `toggle-group.js` — a row of toggle buttons as one control, whose `single`
//   mode is a radio group and is rendered by `radio-group.js` rather than
//   written a second time.
// - `collapsible.js`, `accordion.js` and `navigation-menu.js` — the disclosure
//   pattern on its own, stacked, and applied to a site's navigation. The third
//   of those exists as much to prevent `role="menu"` from being used for a list
//   of links as to provide anything.
// - `slider.js`, `resizable.js` and `progress.js` — the three that report a
//   number in a range. A window splitter is a slider wearing a separator's
//   role, which is why it is beside one rather than with the layout.
// - `table.js` and `pagination.js` — the sort that is announced, the selection
//   that can be mixed, and the rows a page is not showing.
// - `popover.js`, `tooltip.js` and `hover-card.js` — the three anchored
//   overlays, which are one component seen from three distances: one you
//   click, one you hover, and one you hover and then read. They are three
//   modules because what a reader is told differs in every one — a popover is
//   a dialog that is not modal, a tooltip describes its trigger and may never
//   take focus, a hover card is neither and holds links — and because a flag
//   selecting between them would be one flag every behaviour had to read.
// - `alert.js`, `avatar.js`, `breadcrumb.js`, `separator.js` and `skeleton.js`
//   — the five above that look like a class list and are not. One or two
//   elements each, and one conditional each: whether a callout announces
//   itself, which of three states an image is in, whether the last crumb is a
//   link, whether a rule is in the accessibility tree, and whether anybody is
//   told the page is loading.
//
// Every name below is exported from one of those, so a consumer may import
// `@uniflowed/ui` or `@uniflowed/ui/dialog` and get the same thing. The split
// is by primitive because that is the unit a reader looks for, the unit a
// bundler drops, and the unit the WAI-ARIA practices are written in.
//
// `internal/` holds ten modules and nothing else, each a rule the primitives
// must apply identically and a consumer must not be able to apply differently:
// `merge-props.js` (the caller's props go on first, the component's semantics
// last), `controlled-state.js` (what "controlled" means here),
// `roving-focus.js` (how a set of items is found and moved between),
// `disclosure.js` (how a button says whether a region is showing, and how a
// closed region stays findable), `form-value.js` (what a `<form>` submits for a
// control the browser has never heard of), `range.js` (the arithmetic that
// keeps `aria-valuemin`, `aria-valuemax` and `aria-valuenow` true about each
// other), `anchor.js` (where an overlay goes, and what it does when it does not
// fit where it was asked to go), `focus.js` (which elements a reader can reach,
// which a focus trap and a popover want opposite things from), and
// `hover-intent.js` (what WCAG requires of content shown on hover or focus,
// which is three clauses and one mechanism), and `menu-tree.js` (the chain of
// open menus the three menu components share, and what `Escape` and choosing an
// item are defined in terms of). Each says in its own header why it is
// unreachable rather than exported. There is no `internal/props.js`-shaped
// bag of helpers: a module that cannot say what it is about does not belong in
// this package.

import {
  AccordionContent,
  AccordionHeader,
  AccordionItem,
  AccordionRoot,
  AccordionTrigger,
} from "./accordion.js";
import { AlertDescription, AlertRoot, AlertTitle } from "./alert.js";
import {
  AlertDialogAction,
  AlertDialogBody,
  AlertDialogCancel,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogOverlay,
  AlertDialogRoot,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "./alert-dialog.js";
import { AvatarFallback, AvatarImage, AvatarRoot } from "./avatar.js";
import {
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbRoot,
  BreadcrumbSeparator,
} from "./breadcrumb.js";
import {
  CarouselContent,
  CarouselItem,
  CarouselNext,
  CarouselPause,
  CarouselPrevious,
  CarouselRoot,
} from "./carousel.js";
import {
  CalendarDay,
  CalendarMonth,
  CalendarNext,
  CalendarPrevious,
  CalendarRoot,
} from "./calendar.js";
import { Checkbox } from "./checkbox.js";
import { ContextMenuRoot, ContextMenuTrigger } from "./context-menu.js";
import { CollapsibleContent, CollapsibleRoot, CollapsibleTrigger } from "./collapsible.js";
import {
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxGroupLabel,
  ComboboxInput,
  ComboboxLabel,
  ComboboxList,
  ComboboxOption,
  ComboboxRoot,
  ComboboxStatus,
} from "./combobox.js";
import {
  DatePickerCalendar,
  DatePickerInput,
  DatePickerRoot,
  DatePickerTrigger,
} from "./date-picker.js";
import {
  DialogBody,
  DialogClose,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogRoot,
  DialogTitle,
  DialogTrigger,
} from "./dialog.js";
import {
  DrawerBody,
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHandle,
  DrawerHeader,
  DrawerOverlay,
  DrawerRoot,
  DrawerTitle,
  DrawerTrigger,
} from "./drawer.js";
import { FieldControl, FieldDescription, FieldError, FieldLabel, FieldRoot } from "./field.js";
import { HoverCardBody, HoverCardRoot, HoverCardTrigger } from "./hover-card.js";
import { InputOtpGroup, InputOtpRoot, InputOtpSeparator, InputOtpSlot } from "./input-otp.js";
import {
  MenuBody,
  MenuCheckboxItem,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuSub,
  MenuSubTrigger,
  MenuTrigger,
} from "./menu.js";
import { MenubarMenu, MenubarRoot, MenubarTrigger } from "./menubar.js";
import {
  NavigationMenuBody,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  NavigationMenuRoot,
  NavigationMenuTrigger,
} from "./navigation-menu.js";
import {
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
  PaginationRoot,
} from "./pagination.js";
import { PopoverBody, PopoverRoot, PopoverTrigger } from "./popover.js";
import { Progress } from "./progress.js";
import { RadioGroupIndicator, RadioGroupItem, RadioGroupRoot } from "./radio-group.js";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "./resizable.js";
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaViewport } from "./scroll-area.js";
import {
  SelectGroup,
  SelectGroupLabel,
  SelectLabel,
  SelectList,
  SelectOption,
  SelectRoot,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./select.js";
import { Separator } from "./separator.js";
import {
  SheetBody,
  SheetClose,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetOverlay,
  SheetRoot,
  SheetTitle,
  SheetTrigger,
} from "./sheet.js";
import {
  SidebarBody,
  SidebarFooter,
  SidebarHeader,
  SidebarItem,
  SidebarRoot,
  SidebarTrigger,
} from "./sidebar.js";
import { SkeletonBox, SkeletonRoot } from "./skeleton.js";
import { SliderRange, SliderRoot, SliderThumb, SliderTrack } from "./slider.js";
import { Switch } from "./switch.js";
import {
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRoot,
  TableRow,
  TableRowHeader,
  TableRowSelect,
  TableSelectAll,
} from "./table.js";
import { TabsList, TabsPanel, TabsRoot, TabsTab } from "./tabs.js";
import {
  ToastAction,
  ToastClose,
  ToastDescription,
  ToastRegion,
  ToastRoot,
  ToastTitle,
  dismissAllToasts,
  dismissToast,
  toast,
  updateToast,
} from "./toast.js";
import { Toggle } from "./toggle.js";
import { ToggleGroupItem, ToggleGroupRoot } from "./toggle-group.js";
import { TooltipBody, TooltipProvider, TooltipRoot, TooltipTrigger } from "./tooltip.js";

export type { AccordionType } from "./accordion.js";
// A date, however a caller had one to hand: a `PlainDate` from
// `@uniflowed/temporal`, or the ISO 8601 string a form field or a URL carries.
export type { DateValue } from "./calendar.js";
export type { ActivationMode } from "./tabs.js";
// What a modal announces itself as, for a caller who holds one in a variable.
// Two members, not a string: see `dialog.js`.
export type { DialogRole } from "./dialog.js";
// Which edge of the viewport a sheet or a drawer is attached to. `Sidebar` has
// its own two-member union, because a sidebar is never on the top or bottom.
export type { Edge } from "./sheet.js";
export type { InputOtpKind } from "./input-otp.js";
export type { SidebarSide } from "./sidebar.js";
// Where an anchored overlay opens, for a caller who holds one in a variable or
// a prop of their own. Unions rather than strings, so `side="botom"` is a type
// error at the call rather than an overlay that quietly opens somewhere else.
// `LogicalSide` is the same four plus `inline-start` and `inline-end`, which
// are the ones that mean "the way the reader reads" - what a submenu opens
// onto, and the left of the page in Arabic.
export type { Align, LogicalSide, Side } from "./popover.js";
export type { Sort } from "./table.js";
export type { Notification, ToastChanges, ToastOptions, Urgency } from "./toast.js";
export type { ToggleGroupType } from "./toggle-group.js";

/**
 * The five that are one component rather than a namespace of parts.
 *
 * Each takes `render`, so the control or line a design system already has — a
 * `<div>` with a knob drawn in it, somebody's `<Pressable>`, a presentational
 * meter shell — keeps the role, state, keys and attributes while being their
 * element. See the module headers and the table in `packages/ui/ui.test.js`.
 */
export { Checkbox, Progress, Separator, Switch, Toggle };

/**
 * Queueing a notification, from anywhere.
 *
 * Functions rather than a hook, because the places a notification comes from —
 * an event handler, a `catch`, a Server Action's error path — do not have a
 * component to hold state in. `Toast.Region` is what displays them.
 *
 *     const id = toast("Uploading…", { duration: null });
 *     updateToast(id, { content: "Uploaded", duration: 4000 });
 *     toast("Could not save", { urgency: "assertive" });
 */
export { dismissAllToasts, dismissToast, toast, updateToast };

/**
 * An accessible form field.
 *
 *   <Field.Root invalid={error != null}>
 *     <Field.Label>Email</Field.Label>
 *     <Field.Control render={(props) => <input type="email" {...props} />} />
 *     <Field.Description>We will not share it.</Field.Description>
 *     <Field.Error>{error}</Field.Error>
 *   </Field.Root>
 *
 * Inside a form, `field` replaces the hand-written `invalid`: the form says
 * whether the field is wrong and what the message is, and the field composes
 * every `aria-*` from that in one place. `@uniflowed/form`'s `useFieldSource`
 * is what produces one, and `field.js`'s header says why the hook lives there
 * rather than here.
 *
 *   const email = useFieldSource(form, "email", { required: "We need one" });
 *   <Field.Root field={email}>…<Field.Error /></Field.Root>
 *
 * `group` is for a set with no single control to point a `<label for>` at — a
 * radio group, a checkbox group, three selects making a date. The root becomes
 * `role="group"` named by the label, and the description and the error describe
 * the set.
 */
export const Field = {
  Root: FieldRoot,
  Label: FieldLabel,
  Control: FieldControl,
  Description: FieldDescription,
  Error: FieldError,
};

/**
 * Tabs, with the arrow-key behaviour the pattern requires.
 *
 * `activationMode="manual"` moves focus without selecting, for panels that cost
 * something to show.
 *
 *   <Tabs.Root defaultValue="one">
 *     <Tabs.List aria-label="Sections">
 *       <Tabs.Tab value="one">One</Tabs.Tab>
 *       <Tabs.Tab value="two">Two</Tabs.Tab>
 *     </Tabs.List>
 *     <Tabs.Panel value="one">…</Tabs.Panel>
 *     <Tabs.Panel value="two">…</Tabs.Panel>
 *   </Tabs.Root>
 *
 * Every part takes `render`, so a tab that is also a route — `<Tabs.Tab
 * render={(props) => <a href="#billing" {...props} />}>` — is still a tab, with
 * the roving tab stop and the `aria-controls` a tab has. `Tabs.List`'s
 * `renders* Tabs.Tab` is unaffected, because it is the *part* it constrains.
 */
export const Tabs = {
  Root: TabsRoot,
  List: TabsList,
  Tab: TabsTab,
  Panel: TabsPanel,
};

/**
 * A button and the region it shows, with the three attributes that say so.
 *
 * The content stays in the document while it is closed, so the browser's
 * find-in-page can still reach the text in it.
 *
 *   <Collapsible.Root>
 *     <Collapsible.Trigger>Details</Collapsible.Trigger>
 *     <Collapsible.Content>…</Collapsible.Content>
 *   </Collapsible.Root>
 */
export const Collapsible = {
  Root: CollapsibleRoot,
  Trigger: CollapsibleTrigger,
  Content: CollapsibleContent,
};

/**
 * A stack of disclosures that know about each other.
 *
 * `Accordion.Header` takes the heading `level`, because which heading an
 * accordion's sections are depends on where the accordion sits. Each panel is a
 * region named after the header that opens it.
 *
 *   <Accordion.Root type="single">
 *     <Accordion.Item value="shipping">
 *       <Accordion.Header level={3}>
 *         <Accordion.Trigger>Shipping</Accordion.Trigger>
 *       </Accordion.Header>
 *       <Accordion.Content>…</Accordion.Content>
 *     </Accordion.Item>
 *   </Accordion.Root>
 */
export const Accordion = {
  Root: AccordionRoot,
  Item: AccordionItem,
  Header: AccordionHeader,
  Trigger: AccordionTrigger,
  Content: AccordionContent,
};

/**
 * Site navigation: a list of links behind buttons, and not a `menu`.
 *
 *   <NavigationMenu.Root aria-label="Main">
 *     <NavigationMenu.List>
 *       <NavigationMenu.Item value="docs">
 *         <NavigationMenu.Trigger>Docs</NavigationMenu.Trigger>
 *         <NavigationMenu.Body>
 *           <NavigationMenu.Link href="/guide">Guide</NavigationMenu.Link>
 *         </NavigationMenu.Body>
 *       </NavigationMenu.Item>
 *     </NavigationMenu.List>
 *   </NavigationMenu.Root>
 */
export const NavigationMenu = {
  Root: NavigationMenuRoot,
  List: NavigationMenuList,
  Item: NavigationMenuItem,
  Trigger: NavigationMenuTrigger,
  Body: NavigationMenuBody,
  Link: NavigationMenuLink,
};

/**
 * One answer out of several, with the arrow keys that check as they move.
 *
 * `Tab` reaches the chosen answer, or the first one while there is none, and
 * leaves the whole group in one press. `name` puts the answer where a form can
 * submit it.
 *
 *   <Field.Root>
 *     <Field.Label>Plan</Field.Label>
 *     <Field.Control
 *       render={(props) => (
 *         <RadioGroup.Root {...props} defaultValue="free" name="plan">
 *           <RadioGroup.Item value="free">
 *             Free <RadioGroup.Indicator>●</RadioGroup.Indicator>
 *           </RadioGroup.Item>
 *           <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
 *         </RadioGroup.Root>
 *       )}
 *     />
 *   </Field.Root>
 */
export const RadioGroup = {
  Root: RadioGroupRoot,
  Item: RadioGroupItem,
  Indicator: RadioGroupIndicator,
};

/**
 * A row of toggle buttons that behaves as one control.
 *
 * `type="multiple"` is a group of toggle buttons, any number of them pressed.
 * `type="single"` is a radio group drawn as segments, and is rendered by
 * `RadioGroup` rather than written a second time.
 *
 *   <ToggleGroup.Root aria-label="Formatting" type="multiple">
 *     <ToggleGroup.Item value="bold">B</ToggleGroup.Item>
 *     <ToggleGroup.Item value="italic">I</ToggleGroup.Item>
 *   </ToggleGroup.Root>
 */
export const ToggleGroup = {
  Root: ToggleGroupRoot,
  Item: ToggleGroupItem,
};

/**
 * A modal dialog: focus moved in, kept in, and given back.
 *
 *   <Dialog.Root>
 *     <Dialog.Trigger>Delete</Dialog.Trigger>
 *     <Dialog.Overlay />
 *     <Dialog.Body>
 *       <Dialog.Header>
 *         <Dialog.Title>Delete this project?</Dialog.Title>
 *         <Dialog.Description>This cannot be undone.</Dialog.Description>
 *       </Dialog.Header>
 *       <Dialog.Footer>
 *         <Dialog.Close>Cancel</Dialog.Close>
 *       </Dialog.Footer>
 *     </Dialog.Body>
 *   </Dialog.Root>
 *
 * Every part takes `render`. `Dialog.Title` is an `<h2>` by default and the
 * level is a fact about the page around it rather than about the dialog, so
 * `render={(props) => <h3 {...props} />}` is how a caller says which — without
 * losing the id `aria-labelledby` points at. `AlertDialog`, `Sheet` and
 * `Drawer` are made of these parts and pass `render` straight through.
 */
export const Dialog = {
  Root: DialogRoot,
  Trigger: DialogTrigger,
  Overlay: DialogOverlay,
  Body: DialogBody,
  Header: DialogHeader,
  Footer: DialogFooter,
  Title: DialogTitle,
  Description: DialogDescription,
  Close: DialogClose,
};

/**
 * The confirmation: modal, announced as an alert, and not dismissible by a
 * press beside it.
 *
 * Focus lands on `Cancel` rather than on the first thing in the dialog, and the
 * description is required — `role="alertdialog"` exists to announce one, so an
 * alert dialog without it interrupts the reader to say nothing.
 *
 *   <AlertDialog.Root>
 *     <AlertDialog.Trigger>Delete</AlertDialog.Trigger>
 *     <AlertDialog.Overlay />
 *     <AlertDialog.Body>
 *       <AlertDialog.Header>
 *         <AlertDialog.Title>Delete this project?</AlertDialog.Title>
 *         <AlertDialog.Description>This cannot be undone.</AlertDialog.Description>
 *       </AlertDialog.Header>
 *       <AlertDialog.Footer>
 *         <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
 *         <AlertDialog.Action onClick={remove}>Delete</AlertDialog.Action>
 *       </AlertDialog.Footer>
 *     </AlertDialog.Body>
 *   </AlertDialog.Root>
 */
export const AlertDialog = {
  Root: AlertDialogRoot,
  Trigger: AlertDialogTrigger,
  Overlay: AlertDialogOverlay,
  Body: AlertDialogBody,
  Header: AlertDialogHeader,
  Footer: AlertDialogFooter,
  Title: AlertDialogTitle,
  Description: AlertDialogDescription,
  Action: AlertDialogAction,
  Cancel: AlertDialogCancel,
};

/**
 * A modal dialog attached to an edge of the viewport.
 *
 * `side` is a union rather than a class name, and every part reports it as
 * `data-side` — the same attribute `Popover.Body` writes, so one stylesheet
 * rule covers every overlay in this package.
 *
 *   <Sheet.Root side="left">
 *     <Sheet.Trigger>Filters</Sheet.Trigger>
 *     <Sheet.Overlay />
 *     <Sheet.Body>
 *       <Sheet.Title>Filters</Sheet.Title>
 *       <Sheet.Close>Done</Sheet.Close>
 *     </Sheet.Body>
 *   </Sheet.Root>
 */
export const Sheet = {
  Root: SheetRoot,
  Trigger: SheetTrigger,
  Overlay: SheetOverlay,
  Body: SheetBody,
  Header: SheetHeader,
  Footer: SheetFooter,
  Title: SheetTitle,
  Description: SheetDescription,
  Close: SheetClose,
};

/**
 * The sheet you can drag away, with the keyboard that can do everything the
 * drag can.
 *
 * `Drawer.Handle` is a `role="slider"` over the snap points: the arrow keys
 * move between them, `Home` and `End` go to the ends, and the closing key at
 * the smallest snap point closes it. WCAG 2.5.7 also wants a single-pointer
 * alternative, so a drawer with a handle and no `Drawer.Close` raises.
 *
 *   <Drawer.Root side="bottom" snapPoints={[0.4, 1]}>
 *     <Drawer.Trigger>Details</Drawer.Trigger>
 *     <Drawer.Overlay />
 *     <Drawer.Body>
 *       <Drawer.Handle label="Resize the details" />
 *       <Drawer.Title>Details</Drawer.Title>
 *       <Drawer.Close>Close</Drawer.Close>
 *     </Drawer.Body>
 *   </Drawer.Root>
 */
export const Drawer = {
  Root: DrawerRoot,
  Trigger: DrawerTrigger,
  Overlay: DrawerOverlay,
  Body: DrawerBody,
  Handle: DrawerHandle,
  Header: DrawerHeader,
  Footer: DrawerFooter,
  Title: DrawerTitle,
  Description: DrawerDescription,
  Close: DrawerClose,
};

/**
 * Navigation beside the page, which becomes a modal sheet on a narrow one.
 *
 * `Sidebar.Item` takes a `label` and keeps it as the button's accessible name
 * the moment the sidebar collapses to icons — which is the whole reason a
 * collapsing sidebar is a component rather than a class.
 *
 *   <Sidebar.Root defaultOpen={fromCookie}>
 *     <Sidebar.Trigger>Menu</Sidebar.Trigger>
 *     <Sidebar.Body label="Main">
 *       <Sidebar.Item label="Settings">
 *         <Gear /> Settings
 *       </Sidebar.Item>
 *     </Sidebar.Body>
 *   </Sidebar.Root>
 */
export const Sidebar = {
  Root: SidebarRoot,
  Trigger: SidebarTrigger,
  Header: SidebarHeader,
  Body: SidebarBody,
  Footer: SidebarFooter,
  Item: SidebarItem,
};

/**
 * Slides, one at a time, that a reader can stop and cannot fall into.
 *
 * `Carousel.Pause` is WCAG 2.2.2's mechanism and must be the first focusable
 * thing inside the carousel; the slides that are not showing are `inert`, so
 * `Tab` cannot reach a link nobody can see.
 *
 *   <Carousel.Root autoplay={5000} count={3} label="Featured">
 *     <Carousel.Pause />
 *     <Carousel.Content>
 *       <Carousel.Item index={0}>…</Carousel.Item>
 *       <Carousel.Item index={1}>…</Carousel.Item>
 *       <Carousel.Item index={2}>…</Carousel.Item>
 *     </Carousel.Content>
 *     <Carousel.Previous />
 *     <Carousel.Next />
 *   </Carousel.Root>
 */
export const Carousel = {
  Root: CarouselRoot,
  Content: CarouselContent,
  Item: CarouselItem,
  Pause: CarouselPause,
  Previous: CarouselPrevious,
  Next: CarouselNext,
};

/**
 * An overflow container a keyboard can actually scroll.
 *
 * `role="region"`, a name and `tabindex="0"`, because a scroll container is not
 * focusable in every browser and one that is not is one a keyboard reader can
 * see the top of and nothing else.
 *
 *   <ScrollArea.Root label="Release notes">
 *     <ScrollArea.Viewport>…</ScrollArea.Viewport>
 *     <ScrollArea.Scrollbar orientation="vertical" />
 *   </ScrollArea.Root>
 */
export const ScrollArea = {
  Root: ScrollAreaRoot,
  Viewport: ScrollAreaViewport,
  Scrollbar: ScrollAreaScrollbar,
};

/**
 * A one-time code: six boxes drawn over one real `<input>`.
 *
 * One input, so `autocomplete="one-time-code"` works, a paste fills every box,
 * and a form submits one value under one name.
 *
 *   <InputOtp.Root label="One-time code" length={6} name="code">
 *     <InputOtp.Group>
 *       <InputOtp.Slot index={0} />
 *       <InputOtp.Slot index={1} />
 *       <InputOtp.Slot index={2} />
 *     </InputOtp.Group>
 *     <InputOtp.Separator>-</InputOtp.Separator>
 *     <InputOtp.Group>
 *       <InputOtp.Slot index={3} />
 *       <InputOtp.Slot index={4} />
 *       <InputOtp.Slot index={5} />
 *     </InputOtp.Group>
 *   </InputOtp.Root>
 */
export const InputOtp = {
  Root: InputOtpRoot,
  Group: InputOtpGroup,
  Slot: InputOtpSlot,
  Separator: InputOtpSeparator,
};

/**
 * A menu, with the keyboard map every native menu has had for thirty years.
 *
 *   <Menu.Root>
 *     <Menu.Trigger>File</Menu.Trigger>
 *     <Menu.Body>
 *       <Menu.Group>
 *         <Menu.Label>Recent</Menu.Label>
 *         <Menu.Item onSelect={open}>Open…</Menu.Item>
 *       </Menu.Group>
 *       <Menu.Separator />
 *       <Menu.Sub>
 *         <Menu.SubTrigger>Export</Menu.SubTrigger>
 *         <Menu.Body>
 *           <Menu.Item onSelect={png}>PNG</Menu.Item>
 *         </Menu.Body>
 *       </Menu.Sub>
 *     </Menu.Body>
 *   </Menu.Root>
 *
 * Every part takes `render`, which is what makes a menu of links possible — and
 * a menu of links is the most ordinary menu there is:
 *
 *   <Menu.Item render={(props) => <a href="/settings" {...props} />}>
 *     Settings
 *   </Menu.Item>
 *
 * The `<a>` keeps the middle click, the context menu and the status bar; the
 * item keeps the role, the id, the roving tab stop and the press that closes
 * the tree. See the module header for why that is the answer to "no copy step".
 */
export const Menu = {
  Root: MenuRoot,
  Trigger: MenuTrigger,
  Body: MenuBody,
  Item: MenuItem,
  CheckboxItem: MenuCheckboxItem,
  RadioGroup: MenuRadioGroup,
  RadioItem: MenuRadioItem,
  Separator: MenuSeparator,
  Group: MenuGroup,
  Label: MenuLabel,
  Sub: MenuSub,
  SubTrigger: MenuSubTrigger,
};

/**
 * The same menu, opened by the right button — and by the keyboard.
 *
 * `Shift+F10`, the `ContextMenu` key and a long press all open it, because a
 * command reachable only by right-click is reachable only by a pointer, which
 * is a WCAG 2.1.1 failure. `context-menu.js` says why the trigger is in the tab
 * order and when to take it out again.
 *
 * The body needs an `aria-label`: its trigger is a table row or a canvas rather
 * than a short name, so unlike `Menu.Body` it cannot name itself after one.
 *
 *   <ContextMenu.Root>
 *     <ContextMenu.Trigger>{row}</ContextMenu.Trigger>
 *     <ContextMenu.Body aria-label="Row actions">
 *       <ContextMenu.Item onSelect={rename}>Rename…</ContextMenu.Item>
 *       <ContextMenu.CheckboxItem defaultChecked>Show hidden</ContextMenu.CheckboxItem>
 *     </ContextMenu.Body>
 *   </ContextMenu.Root>
 */
export const ContextMenu = {
  Root: ContextMenuRoot,
  Trigger: ContextMenuTrigger,
  Body: MenuBody,
  Item: MenuItem,
  CheckboxItem: MenuCheckboxItem,
  RadioGroup: MenuRadioGroup,
  RadioItem: MenuRadioItem,
  Separator: MenuSeparator,
  Group: MenuGroup,
  Label: MenuLabel,
  Sub: MenuSub,
  SubTrigger: MenuSubTrigger,
};

/**
 * A row of menus that behaves as one control: File, Edit, View.
 *
 * One tab stop for the whole bar, arrows between the menus, and — the part that
 * is always missing — arrows *while a menu is open* that close it and open the
 * next one, so a reader walks File → Edit → View without pressing Escape.
 *
 *   <Menubar.Root aria-label="Main">
 *     <Menubar.Menu value="file">
 *       <Menubar.Trigger>File</Menubar.Trigger>
 *       <Menubar.Body>
 *         <Menubar.Item onSelect={open}>Open…</Menubar.Item>
 *       </Menubar.Body>
 *     </Menubar.Menu>
 *   </Menubar.Root>
 */
export const Menubar = {
  Root: MenubarRoot,
  Menu: MenubarMenu,
  Trigger: MenubarTrigger,
  // `Menu.Body` itself: a bar's menu is a root menu, and `menubar.js`'s header
  // says why a wrapper with the same defaults would be a second place to drift.
  Body: MenuBody,
  Item: MenuItem,
  CheckboxItem: MenuCheckboxItem,
  RadioGroup: MenuRadioGroup,
  RadioItem: MenuRadioItem,
  Separator: MenuSeparator,
  Group: MenuGroup,
  Label: MenuLabel,
  Sub: MenuSub,
  SubTrigger: MenuSubTrigger,
};

/**
 * A text field with a list of options, navigated without leaving the field.
 *
 * The caller filters; the component keeps the ARIA wiring true while they do.
 *
 *   <Combobox.Root inputValue={query} onInputValueChange={setQuery}>
 *     <Combobox.Label>Country</Combobox.Label>
 *     <Combobox.Input />
 *     <Combobox.List>
 *       <Combobox.Group>
 *         <Combobox.GroupLabel>Europe</Combobox.GroupLabel>
 *         {european.map((each) => (
 *           <Combobox.Option key={each} value={each}>{each}</Combobox.Option>
 *         ))}
 *       </Combobox.Group>
 *     </Combobox.List>
 *     <Combobox.Empty>No matches.</Combobox.Empty>
 *     <Combobox.Status />
 *   </Combobox.Root>
 *
 * `Combobox.Label` names the field and `Combobox.GroupLabel` names a group of
 * options, which is why there are two of them.
 */
export const Combobox = {
  Root: ComboboxRoot,
  Label: ComboboxLabel,
  Input: ComboboxInput,
  List: ComboboxList,
  Option: ComboboxOption,
  Group: ComboboxGroup,
  GroupLabel: ComboboxGroupLabel,
  Empty: ComboboxEmpty,
  Status: ComboboxStatus,
};

/**
 * The other half of the combobox pattern: a button, a list, and no typing.
 *
 * Use a native `<select>` when a native `<select>` will do — `select.js` says
 * so first and means it. This is for the popup a `<select>` cannot draw.
 *
 *   <Select.Root defaultValue="GB" name="country">
 *     <Select.Label>Country</Select.Label>
 *     <Select.Trigger>
 *       <Select.Value placeholder="Choose one" />
 *     </Select.Trigger>
 *     <Select.List>
 *       <Select.Group>
 *         <Select.GroupLabel>Europe</Select.GroupLabel>
 *         <Select.Option value="GB">United Kingdom</Select.Option>
 *         <Select.Option value="FR">France</Select.Option>
 *       </Select.Group>
 *       <Select.Separator />
 *       <Select.Option value="JP">Japan</Select.Option>
 *     </Select.List>
 *   </Select.Root>
 *
 * `Select.Label` names the field and `Select.GroupLabel` names a group of
 * options. shadcn has one `SelectLabel` and it is the second of those; a select
 * needs both, so they are two parts here.
 */
export const Select = {
  Root: SelectRoot,
  Label: SelectLabel,
  Trigger: SelectTrigger,
  Value: SelectValue,
  List: SelectList,
  Option: SelectOption,
  Group: SelectGroup,
  GroupLabel: SelectGroupLabel,
  Separator: SelectSeparator,
};

/**
 * A dialog that is not modal, anchored to the button that opened it.
 *
 * Focus moves in, `Escape` closes it and gives focus back, and `Tab` *leaves* —
 * the page behind a popover is still there, still scrollable and still
 * tabbable, which is every way in which it is not a `Dialog`.
 *
 *   <Popover.Root>
 *     <Popover.Trigger>Filters</Popover.Trigger>
 *     <Popover.Body align="start" side="bottom" sideOffset={8}>
 *       <label>
 *         Only mine <input type="checkbox" />
 *       </label>
 *     </Popover.Body>
 *   </Popover.Root>
 *
 * `Popover.Body` reports where it ended up as `data-side` and `data-align`, and
 * writes the trigger's width and the room it had as custom properties, so a
 * stylesheet can point an arrow and cap a height without measuring anything.
 */
export const Popover = {
  Root: PopoverRoot,
  Trigger: PopoverTrigger,
  Body: PopoverBody,
};

/**
 * A month of dates, as one stop in the page's tab order.
 *
 * The grid is `role="grid"`, the arrow keys move by a day and by a week, and
 * `PageUp` and `PageDown` change the month - with `Shift`, the year. Running off
 * the end of a month shows the next one and lands on its first day, and the
 * month is announced in a live region when it changes.
 *
 *   <Calendar.Root defaultValue="2026-10-14" onValueChange={setWhen}>
 *     <Calendar.Previous>Previous month</Calendar.Previous>
 *     <Calendar.Next>Next month</Calendar.Next>
 *     <Calendar.Month />
 *   </Calendar.Root>
 *
 * `Calendar.Month` takes a function child when a day needs more than its number
 * in it - a dot for an appointment, a price for a night - and it is handed the
 * date and returns a `Calendar.Day`.
 *
 * Dates are `@uniflowed/temporal`'s `PlainDate`, or the ISO strings it reads.
 * `isDateDisabled` marks a day unavailable *without* making it unreachable: it
 * is `aria-disabled` and the arrow keys still land on it, which is the opposite
 * of what a disabled menu item does and the only way a reader can find out which
 * days are unavailable.
 */
export const Calendar = {
  Root: CalendarRoot,
  Previous: CalendarPrevious,
  Next: CalendarNext,
  Month: CalendarMonth,
  Day: CalendarDay,
};

/**
 * A field somebody types a date into, and a calendar for the times they would
 * rather point at one.
 *
 *   <DatePicker.Root onValueChange={setWhen} value={when}>
 *     <DatePicker.Input aria-label="Arrive on" />
 *     <DatePicker.Trigger>Choose a date</DatePicker.Trigger>
 *     <DatePicker.Calendar>
 *       <Calendar.Previous>Previous month</Calendar.Previous>
 *       <Calendar.Next>Next month</Calendar.Next>
 *       <Calendar.Month />
 *     </DatePicker.Calendar>
 *   </DatePicker.Root>
 *
 * The field is the control and the grid is the second way in: `Escape` and a
 * chosen date both put focus back on the field. `format` and `parse` are ISO
 * 8601 both ways unless a caller passes their own - `date-picker.js` says why a
 * locale format is not this package's to guess.
 */
export const DatePicker = {
  Root: DatePickerRoot,
  Input: DatePickerInput,
  Trigger: DatePickerTrigger,
  Calendar: DatePickerCalendar,
};

/**
 * A phrase about a control, on hover and on focus, that WCAG would accept.
 *
 * Dismissible with `Escape`, hoverable — the pointer can travel onto it —  and
 * never focusable. It does not open on touch, deliberately, so the trigger must
 * carry its own name for a reader holding a phone.
 *
 *   <Tooltip.Provider delayDuration={700} skipDelayDuration={300}>
 *     <Tooltip.Root>
 *       <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
 *       <Tooltip.Body>Bold (⌘B)</Tooltip.Body>
 *     </Tooltip.Root>
 *     <Tooltip.Root>
 *       <Tooltip.Trigger aria-label="Italic">I</Tooltip.Trigger>
 *       <Tooltip.Body>Italic (⌘I)</Tooltip.Body>
 *     </Tooltip.Root>
 *   </Tooltip.Provider>
 *
 * `Tooltip.Provider` is what makes the second icon in that toolbar answer at
 * once instead of making the reader wait the delay again. A tooltip outside one
 * is a complete tooltip with a delay of its own.
 */
export const Tooltip = {
  Provider: TooltipProvider,
  Root: TooltipRoot,
  Trigger: TooltipTrigger,
  Body: TooltipBody,
};

/**
 * The preview a name expands into: hovered, focused, and full of links.
 *
 * Not a tooltip — its contents are reachable, by pointer and by `Tab` — and not
 * a dialog, because nothing about it is modal.
 *
 *   <HoverCard.Root>
 *     <HoverCard.Trigger render={(props) => <a href="/ada" {...props}>@ada</a>} />
 *     <HoverCard.Body>
 *       <p>Ada Lovelace</p>
 *       <a href="/ada/notes">Notes</a>
 *     </HoverCard.Body>
 *   </HoverCard.Root>
 */
export const HoverCard = {
  Root: HoverCardRoot,
  Trigger: HoverCardTrigger,
  Body: HoverCardBody,
};

/**
 * Notifications, in a live region that was watching before them.
 *
 * Render `Toast.Region` once, in the layout; `toast()` from anywhere.
 *
 *   <Toast.Region>
 *     {(each) => (
 *       <Toast.Root>
 *         <Toast.Title>{each.content}</Toast.Title>
 *         <Toast.Action onClick={undo}>Undo</Toast.Action>
 *         <Toast.Close />
 *       </Toast.Root>
 *     )}
 *   </Toast.Region>
 */
export const Toast = {
  Region: ToastRegion,
  Root: ToastRoot,
  Title: ToastTitle,
  Description: ToastDescription,
  Action: ToastAction,
  Close: ToastClose,
};

/**
 * A value in a range, with `role="slider"` on the thumb where it belongs.
 *
 * One thumb or two; a range is the same component with a second one, each
 * bounded by its neighbour and each needing its own name.
 *
 *   <Slider.Root defaultValue={[20, 60]} valueText={(each) => `£${each}`}>
 *     <Slider.Track>
 *       <Slider.Range />
 *     </Slider.Track>
 *     <Slider.Thumb aria-label="Minimum" index={0} />
 *     <Slider.Thumb aria-label="Maximum" index={1} />
 *   </Slider.Root>
 */
export const Slider = {
  Root: SliderRoot,
  Track: SliderTrack,
  Range: SliderRange,
  Thumb: SliderThumb,
};

/**
 * Two panes and the splitter between them, operable from the keyboard.
 *
 *   <Resizable.PanelGroup defaultValue={30}>
 *     <Resizable.Panel primary>Files</Resizable.Panel>
 *     <Resizable.Handle label="Resize the file list" />
 *     <Resizable.Panel>Editor</Resizable.Panel>
 *   </Resizable.PanelGroup>
 */
export const Resizable = {
  PanelGroup: ResizablePanelGroup,
  Panel: ResizablePanel,
  Handle: ResizableHandle,
};

/**
 * A table, with the four things about one nobody gets right by hand.
 *
 * A real `<table>`, deliberately not a `role="grid"` — `table.js` says why —
 * and its own live region, so a re-sort is something a reader is told about
 * rather than something that happens silently behind them.
 *
 *   <Table.Root onSortChange={setSort} rowCount={500} rowOffset={90} sort={sort}>
 *     <Table.Caption>People</Table.Caption>
 *     <Table.Header>
 *       <Table.Row>
 *         <Table.Head>
 *           <Table.SelectAll checked={all} onCheckedChange={setAll} />
 *         </Table.Head>
 *         <Table.Head column="name">Name</Table.Head>
 *       </Table.Row>
 *     </Table.Header>
 *     <Table.Body>
 *       {page.map((person, at) => (
 *         <Table.Row index={at} key={person.id}>
 *           <Table.Cell>
 *             <Table.RowSelect
 *               checked={chosen.has(person.id)}
 *               label={`Select ${person.name}`}
 *               onCheckedChange={(on) => choose(person.id, on)}
 *             />
 *           </Table.Cell>
 *           <Table.RowHeader>{person.name}</Table.RowHeader>
 *         </Table.Row>
 *       ))}
 *     </Table.Body>
 *   </Table.Root>
 */
export const Table = {
  Root: TableRoot,
  Caption: TableCaption,
  Header: TableHeader,
  Body: TableBody,
  Row: TableRow,
  Head: TableHead,
  RowHeader: TableRowHeader,
  Cell: TableCell,
  SelectAll: TableSelectAll,
  RowSelect: TableRowSelect,
};

/**
 * The navigation a paginated table needs, and the sentence that says it moved.
 *
 *   <Pagination.Root page={4} pageCount={25}>
 *     <Pagination.Content>
 *       <Pagination.Previous disabled={page === 1} href={hrefFor(page - 1)}>‹</Pagination.Previous>
 *       <Pagination.Item current href={hrefFor(4)}>4</Pagination.Item>
 *       <Pagination.Next href={hrefFor(page + 1)}>›</Pagination.Next>
 *     </Pagination.Content>
 *   </Pagination.Root>
 */
export const Pagination = {
  Root: PaginationRoot,
  Content: PaginationContent,
  Item: PaginationItem,
  Previous: PaginationPrevious,
  Next: PaginationNext,
};

/**
 * The trail above the page, read as places rather than as punctuation.
 *
 *   <Breadcrumb.Root>
 *     <Breadcrumb.List>
 *       <Breadcrumb.Item>
 *         <Breadcrumb.Link href="/">Home</Breadcrumb.Link>
 *       </Breadcrumb.Item>
 *       <Breadcrumb.Separator>/</Breadcrumb.Separator>
 *       <Breadcrumb.Item>
 *         <Breadcrumb.Page>Billing</Breadcrumb.Page>
 *       </Breadcrumb.Item>
 *     </Breadcrumb.List>
 *   </Breadcrumb.Root>
 *
 * `Pagination`'s shape one door along: a `<nav>` with a name, one
 * `aria-current="page"`, and the separators out of the accessibility tree so
 * the trail is not announced as "Home slash Settings slash Billing". The last
 * crumb is a `Breadcrumb.Page` and not a link, because it is where the reader
 * already is.
 */
export const Breadcrumb = {
  Root: BreadcrumbRoot,
  List: BreadcrumbList,
  Item: BreadcrumbItem,
  Link: BreadcrumbLink,
  Page: BreadcrumbPage,
  Separator: BreadcrumbSeparator,
};

/**
 * A callout, and the `live` that decides whether anybody is interrupted by it.
 *
 *   <Alert.Root>
 *     <Alert.Title>Your trial ends on Friday</Alert.Title>
 *     <Alert.Description>Add a card to keep your projects.</Alert.Description>
 *   </Alert.Root>
 *
 *   {error != null && (
 *     <Alert.Root live>
 *       <Alert.Title>Could not save</Alert.Title>
 *       <Alert.Description>{error}</Alert.Description>
 *     </Alert.Root>
 *   )}
 *
 * The first has no role at all: it was there when the page loaded, so a live
 * region would announce it on every load or never, and neither is what anybody
 * wanted. The second appeared because something happened, which is what
 * `role="alert"` is for. `alert.js`'s header says why there is no polite
 * version of this and why `Toast` is that instead.
 */
export const Alert = {
  Root: AlertRoot,
  Title: AlertTitle,
  Description: AlertDescription,
};

/**
 * A picture of a person, and the two states it is not in yet.
 *
 *   <Avatar.Root>
 *     <Avatar.Image src={person.photo} />
 *     <Avatar.Fallback>{initials(person.name)}</Avatar.Fallback>
 *   </Avatar.Root>
 *
 * The fallback is absent while the image is loading and present once it has
 * failed, held back long enough that a cached image never flashes initials.
 * `alt` defaults to `""`, because an avatar beside the name it belongs to is
 * decorative and a component that helpfully puts the name there makes every
 * screen reader say it twice; pass `alt` where the picture is the only thing
 * identifying the person.
 */
export const Avatar = {
  Root: AvatarRoot,
  Image: AvatarImage,
  Fallback: AvatarFallback,
};

/**
 * The grey boxes, and the sentence that stops them being an empty page.
 *
 *   <Skeleton.Root busy={pending}>
 *     {pending ? <Skeleton.Box /> : <Invoices rows={invoices} />}
 *   </Skeleton.Root>
 *
 * The boxes are `aria-hidden`, the region is `aria-busy`, and a live region
 * that was mounted empty for a commit says "Loading" — a skeleton screen is
 * busy on its first render, so a region rendered with its message already in it
 * announces nothing at all. Keep the root mounted across the load and toggle
 * `busy`; unmounting it takes the region away before it can say the wait is
 * over.
 */
export const Skeleton = {
  Root: SkeletonRoot,
  Box: SkeletonBox,
};
