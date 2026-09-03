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
// restored to whatever opened a dialog, `aria-describedby` pointing only at
// elements that are in the document. None of it is visible in a screenshot and
// all of it is what separates a component from a div that looks like one.
//
// # Composition is type-checked
//
// This is where Flow says something no other type system can. `Tabs.List`
// declares `renders* Tabs.Tab`, so a `<button>` in a tab list is a *type
// error* — not a review comment, not a runtime warning, not a screen reader
// announcing "button" where the reader expected "tab, 2 of 5". A library
// written in TypeScript can document that constraint; it cannot state it.

import {
  FieldControl,
  FieldDescription,
  FieldError,
  FieldLabel,
  FieldRoot,
} from "./internal/field.js";
import {
  DialogClose,
  DialogContent,
  DialogRoot,
  DialogTitle,
  DialogTrigger,
} from "./internal/dialog.js";
import { TabsList, TabsPanel, TabsRoot, TabsTab } from "./internal/tabs.js";
import { Checkbox, Switch } from "./internal/switch.js";

export { Checkbox, Switch };

/**
 * An accessible form field.
 *
 * `Field.Control` takes a render function rather than rendering an `<input>`,
 * because a field wraps a select, a textarea or somebody else's component just
 * as often, and each needs the same attributes.
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
 *   <Tabs.Root defaultValue="one">
 *     <Tabs.List>
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
 *     <Dialog.Trigger>Open</Dialog.Trigger>
 *     <Dialog.Content>
 *       <Dialog.Title>Are you sure?</Dialog.Title>
 *       <Dialog.Close>Cancel</Dialog.Close>
 *     </Dialog.Content>
 *   </Dialog.Root>
 */
export const Dialog = {
  Root: DialogRoot,
  Trigger: DialogTrigger,
  Content: DialogContent,
  Title: DialogTitle,
  Close: DialogClose,
};
