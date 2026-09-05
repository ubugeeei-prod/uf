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
// the answer, puts it in state. It is deliberately *not* `useSyncExternalStore`:
// that is for a store whose value a render reads, and reading layout during a
// render is the thing it exists to prevent.
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
// - `tabs.js` — the roving `tabindex`, and automatic versus manual activation.
// - `field.js` — the label, description, error and `aria-invalid` wiring.
// - `switch.js` and `checkbox.js` — the two two-state controls, apart because
//   the third state and the `Enter` key genuinely differ between them.
//
// Every name below is exported from one of those, so a consumer may import
// `@uniflowed/ui` or `@uniflowed/ui/dialog` and get the same thing. The split
// is by primitive because that is the unit a reader looks for, the unit a
// bundler drops, and the unit the WAI-ARIA practices are written in.
//
// `internal/` holds three modules and nothing else, each a rule the primitives
// must apply identically and a consumer must not be able to apply differently:
// `merge-props.js` (the caller's props go on first, the component's semantics
// last), `controlled-state.js` (what "controlled" means here), and
// `roving-focus.js` (how a set of items is found and moved between). Each says
// in its own header why it is unreachable rather than exported. There is no
// `internal/props.js`-shaped bag of helpers: a module that cannot say what it
// is about does not belong in this package.

import { Checkbox } from "./checkbox.js";
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
import { Switch } from "./switch.js";
import { TabsList, TabsPanel, TabsRoot, TabsTab } from "./tabs.js";

export type { ActivationMode } from "./tabs.js";

export { Checkbox, Switch };

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
