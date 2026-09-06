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
// # Styling is a default, not a dependency
//
// Nothing here imports StyleX, and nothing here has a StyleX-shaped type. A
// consumer styling with plain CSS, CSS Modules or anything else gets exactly
// the same components with exactly the same behaviour; the design-system layer
// that adds uf's default styles is built *on* these, not into them.
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
// of notifications lives outside React — a store at module scope in
// `toast.js`, read through `useSyncExternalStore` with the cached immutable
// snapshots and the consistent server snapshot that requires. A queue is a
// store; the DOM is not.
//
// That store should be an atom in `@uniflowed/state`, and was one. It is
// written out by hand in `toast.js` because `@uniflowed/ui` is published to
// npm and `@uniflowed/state` is not: its name has never been bound, and
// binding it takes a person with an `npm login` session and a 2FA prompt —
// ubugeeei-prod/uf#210. A published package whose dependency is missing
// installs as nothing, `ETARGET` on the first thing a user types, so this
// package cannot declare that dependency until the name exists. The queue
// moves back to `@uniflowed/state` when #210 binds it. The local store is the
// shippable design, not the better one.
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
// - `dialog.js` — the focus trap, focus restore, scroll lock and inert page.
// - `menu.js` — the arrow keys, typeahead, submenus and `Escape` stacking.
// - `combobox.js` — `aria-activedescendant` over a filtered list, and the
//   count a screen reader is told.
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
//
// Every name below is exported from one of those, so a consumer may import
// `@uniflowed/ui` or `@uniflowed/ui/dialog` and get the same thing. The split
// is by primitive because that is the unit a reader looks for, the unit a
// bundler drops, and the unit the WAI-ARIA practices are written in.
//
// `internal/` holds six modules and nothing else, each a rule the primitives
// must apply identically and a consumer must not be able to apply differently:
// `merge-props.js` (the caller's props go on first, the component's semantics
// last), `controlled-state.js` (what "controlled" means here),
// `roving-focus.js` (how a set of items is found and moved between),
// `disclosure.js` (how a button says whether a region is showing, and how a
// closed region stays findable), `form-value.js` (what a `<form>` submits for a
// control the browser has never heard of), and `range.js` (the arithmetic that
// keeps `aria-valuemin`, `aria-valuemax` and `aria-valuenow` true about each
// other). Each says in its own header why it is unreachable rather than
// exported. There is no `internal/props.js`-shaped bag of helpers: a module
// that cannot say what it is about does not belong in this package.

import {
  AccordionContent,
  AccordionHeader,
  AccordionItem,
  AccordionRoot,
  AccordionTrigger,
} from "./accordion.js";
import { Checkbox } from "./checkbox.js";
import { CollapsibleContent, CollapsibleRoot, CollapsibleTrigger } from "./collapsible.js";
import {
  ComboboxEmpty,
  ComboboxInput,
  ComboboxLabel,
  ComboboxList,
  ComboboxOption,
  ComboboxRoot,
  ComboboxStatus,
} from "./combobox.js";
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
import { FieldControl, FieldDescription, FieldError, FieldLabel, FieldRoot } from "./field.js";
import {
  MenuBody,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRoot,
  MenuSeparator,
  MenuSub,
  MenuSubTrigger,
  MenuTrigger,
} from "./menu.js";
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
import { Progress } from "./progress.js";
import { RadioGroupIndicator, RadioGroupItem, RadioGroupRoot } from "./radio-group.js";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "./resizable.js";
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

export type { AccordionType } from "./accordion.js";
export type { ActivationMode } from "./tabs.js";
export type { Sort } from "./table.js";
export type { Notification, ToastChanges, ToastOptions, Urgency } from "./toast.js";
export type { ToggleGroupType } from "./toggle-group.js";

export { Checkbox, Progress, Switch, Toggle };

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
 */
export const Menu = {
  Root: MenuRoot,
  Trigger: MenuTrigger,
  Body: MenuBody,
  Item: MenuItem,
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
 *       {matches.map((each) => (
 *         <Combobox.Option key={each} value={each}>{each}</Combobox.Option>
 *       ))}
 *     </Combobox.List>
 *     <Combobox.Empty>No matches.</Combobox.Empty>
 *     <Combobox.Status />
 *   </Combobox.Root>
 */
export const Combobox = {
  Root: ComboboxRoot,
  Label: ComboboxLabel,
  Input: ComboboxInput,
  List: ComboboxList,
  Option: ComboboxOption,
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
