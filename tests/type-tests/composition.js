// @flow
//
// The wrong child, in every place this package refuses one, and the right one
// beside it.
//
// This file is *supposed* to fail `uf check`. `packages/ui/index.js` makes one
// claim no other component library can make:
//
//   > `Tabs.List` declares `renders* Tabs.Tab`, so a `<button>` in a tab list
//   > is a *type error* — not a review comment, not a runtime warning, not a
//   > screen reader announcing "button" where the reader expected "tab, 2 of
//   > 5".
//
// Nothing held it to that. The constraints were checked by hand while
// `select.js` and `toast.js` were being written, against a scratch file
// `uf check` was pointed at, and a scratch file is not a test: every `renders*`
// in the package was a promise a refactor could have removed with nothing going
// red. This file is the half of ubugeeei-prod/uf#358 that is not a test
// function, and it covers all twelve of them rather than the six the issue
// listed, because a constraint left out here is a constraint still in the state
// the issue is about.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and that
// the report must contain that text. A line without one must not be reported at
// all — which is the half that matters most here, because one of these can
// break in two directions. A `renders*` that stops rejecting a `<div>` fails
// this file; so does one that starts rejecting the parts it exists to admit,
// which is the failure that reaches a consumer as "this library does not
// compile" rather than as a guarantee quietly gone. Both are written out for
// every constraint the package makes.
//
// The wrong children sit inside `{ … }` rather than being written as ordinary
// children, and that is not a style choice: a `//` comment between two JSX tags
// is *text*, and a stray string is itself a child none of these containers
// accepts — so the marker would create the error it was meant to describe, one
// line above the one it names. Inside an expression container it is a comment
// again.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which every `renders*` below is `any`, every line passes, and the test would
// prove nothing. So the test runs `uf check tests/type-tests packages/ui`, with
// both in one set. `anchoring.js` beside this says the rest of why the fixtures
// live here rather than inside the package they are about.
//
// # The two that fail for a reason a reader would not guess
//
// A `Select.Option` inside a `Menu.Body` is rejected, and it is the case worth
// reading the message for: it is a part of this package, inside a container of
// this package, and it is still wrong — a `menu` owns `menuitem`s, a `listbox`
// owns `option`s, and neither will take the other's. The report names all four
// members of the menu's union, which is what tells a reader what they should
// have written instead.
//
// And `Toast.Region` has no element children at all. It takes a function that
// is handed one notification and *returns* a `Toast.Root`, so its constraint is
// about a return value, and it is the one here a caller meets while writing a
// callback rather than while nesting tags. The report says "in property
// children > the return value", which is the difference said out loud.

import {
  AccordionContent,
  AccordionHeader,
  AccordionItem,
  AccordionRoot,
  AccordionTrigger,
} from "../../packages/ui/accordion.js";
import {
  ComboboxGroup,
  ComboboxGroupLabel,
  ComboboxList,
  ComboboxOption,
} from "../../packages/ui/combobox.js";
import {
  MenuBody,
  MenuCheckboxItem,
  MenuGroup,
  MenuItem,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
} from "../../packages/ui/menu.js";
import {
  NavigationMenuBody,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  NavigationMenuRoot,
} from "../../packages/ui/navigation-menu.js";
import { PaginationContent, PaginationItem, PaginationNext } from "../../packages/ui/pagination.js";
import {
  SelectGroup,
  SelectList,
  SelectOption,
  SelectSeparator,
} from "../../packages/ui/select.js";
import { TabsList, TabsTab } from "../../packages/ui/tabs.js";
import { ToastRegion, ToastRoot, ToastTitle } from "../../packages/ui/toast.js";
import { ToggleGroupItem, ToggleGroupRoot } from "../../packages/ui/toggle-group.js";

// --- Tabs.List ---------------------------------------------------------------
//
// The one `index.js` names, so it goes first.

export const tabs: mixed = (
  <TabsList>
    <TabsTab value="general">General</TabsTab>
    <TabsTab value="billing">Billing</TabsTab>
  </TabsList>
);

export const buttonInATabList: mixed = (
  <TabsList>
    {
      // expect: does not render TabsTab
      <button type="button">General</button>
    }
  </TabsList>
);

// --- Menu.Body ---------------------------------------------------------------
//
// Six members, which is why the report says "union type" rather than naming
// one: a menu owns commands, the two checkable kinds, rules, groups and
// submenus.

export const menu: mixed = (
  <MenuBody>
    <MenuGroup>
      <MenuItem>Open</MenuItem>
    </MenuGroup>
    <MenuSeparator />
    <MenuItem>Save</MenuItem>
    <MenuCheckboxItem>Show hidden files</MenuCheckboxItem>
    <MenuRadioGroup defaultValue="name">
      <MenuRadioItem value="name">Name</MenuRadioItem>
    </MenuRadioGroup>
  </MenuBody>
);

export const divInAMenu: mixed = (
  <MenuBody>
    {
      // expect: does not render union type
      <div>Open</div>
    }
  </MenuBody>
);

// Both of these are parts of this package, and an option is still not a menu
// item. The report names every member of the union, which is what says what
// should have been written instead.
export const optionInAMenu: mixed = (
  <MenuBody>
    {
      // expect: Either SelectOption element does not render MenuCheckboxItem
      <SelectOption value="GB">United Kingdom</SelectOption>
    }
  </MenuBody>
);

// --- Combobox.List -----------------------------------------------------------
//
// Options and groups, since ubugeeei-prod/uf#357. A group holds options and its
// own heading and nothing else, which is a constraint `Select.Group` does not
// state yet.

export const combobox: mixed = (
  <ComboboxList>
    <ComboboxOption value="GB">United Kingdom</ComboboxOption>
    <ComboboxGroup>
      <ComboboxGroupLabel>Europe</ComboboxGroupLabel>
      <ComboboxOption value="FR">France</ComboboxOption>
    </ComboboxGroup>
  </ComboboxList>
);

export const divInACombobox: mixed = (
  <ComboboxList>
    {
      // expect: does not render union type
      <div>United Kingdom</div>
    }
  </ComboboxList>
);

export const divInAComboboxGroup: mixed = (
  <ComboboxGroup>
    {
      // expect: does not render union type
      <div>Europe</div>
    }
  </ComboboxGroup>
);

// --- Select.List -------------------------------------------------------------

export const select: mixed = (
  <SelectList>
    <SelectGroup>
      <SelectOption value="FR">France</SelectOption>
    </SelectGroup>
    <SelectSeparator />
    <SelectOption value="JP">Japan</SelectOption>
  </SelectList>
);

export const divInASelect: mixed = (
  <SelectList>
    {
      // expect: does not render union type
      <div>France</div>
    }
  </SelectList>
);

// --- Toast.Region ------------------------------------------------------------
//
// The constraint that is about a return value rather than about a child.

export const toast: mixed = (
  <ToastRegion>
    {(notification) => (
      <ToastRoot>
        <ToastTitle>{notification.content}</ToastTitle>
      </ToastRoot>
    )}
  </ToastRegion>
);

export const divFromAToastRegion: mixed = (
  <ToastRegion>
    {
      // expect: in property children > the return value
      () => <div />
    }
  </ToastRegion>
);

// --- Pagination.Content ------------------------------------------------------

export const pagination: mixed = (
  <PaginationContent>
    <PaginationItem current>1</PaginationItem>
    <PaginationNext />
  </PaginationContent>
);

export const divInAPagination: mixed = (
  <PaginationContent>
    {
      // expect: does not render union type
      <div>1</div>
    }
  </PaginationContent>
);

// --- ToggleGroup.Root --------------------------------------------------------

export const toggleGroup: mixed = (
  <ToggleGroupRoot>
    <ToggleGroupItem value="bold">Bold</ToggleGroupItem>
  </ToggleGroupRoot>
);

export const buttonInAToggleGroup: mixed = (
  <ToggleGroupRoot>
    {
      // expect: does not render ToggleGroupItem
      <button type="button">Bold</button>
    }
  </ToggleGroupRoot>
);

// --- NavigationMenu ----------------------------------------------------------
//
// Three constraints in one component, which is why all three are written out:
// the root holds lists, a list holds entries, and the group an entry opens holds
// links. The third is the one with a behaviour behind it — a `<button>` in
// there is a control that does not go anywhere, in a group whose whole purpose
// is that `Tab` reaches somewhere.

export const navigationMenu: mixed = (
  <NavigationMenuRoot>
    <NavigationMenuList>
      <NavigationMenuItem value="products">
        <NavigationMenuBody>
          <NavigationMenuLink href="/pricing">Pricing</NavigationMenuLink>
        </NavigationMenuBody>
      </NavigationMenuItem>
    </NavigationMenuList>
  </NavigationMenuRoot>
);

export const itemInANavigationMenuRoot: mixed = (
  <NavigationMenuRoot>
    {
      // expect: does not render NavigationMenuList
      <NavigationMenuItem value="products">Products</NavigationMenuItem>
    }
  </NavigationMenuRoot>
);

export const divInANavigationMenuList: mixed = (
  <NavigationMenuList>
    {
      // expect: does not render NavigationMenuItem
      <div>Products</div>
    }
  </NavigationMenuList>
);

export const buttonInANavigationMenuBody: mixed = (
  <NavigationMenuBody>
    {
      // expect: does not render NavigationMenuLink
      <button type="button">Pricing</button>
    }
  </NavigationMenuBody>
);

// --- Accordion ---------------------------------------------------------------
//
// And the one `renders` in the package that is not a `renders*`:
// `Accordion.Header` holds exactly one trigger, because a heading with two
// buttons in it is a heading whose accessible name is both of them.

export const accordion: mixed = (
  <AccordionRoot>
    <AccordionItem value="shipping">
      <AccordionHeader>
        <AccordionTrigger>Shipping</AccordionTrigger>
      </AccordionHeader>
      <AccordionContent>Two days.</AccordionContent>
    </AccordionItem>
  </AccordionRoot>
);

export const headerInAnAccordionRoot: mixed = (
  <AccordionRoot>
    {
      // expect: does not render AccordionItem
      <AccordionHeader>
        <AccordionTrigger>Shipping</AccordionTrigger>
      </AccordionHeader>
    }
  </AccordionRoot>
);

export const divInAnAccordionItem: mixed = (
  <AccordionItem value="shipping">
    {
      // expect: does not render union type
      <div>Shipping</div>
    }
  </AccordionItem>
);

export const divInAnAccordionHeader: mixed = (
  <AccordionHeader>
    {
      // expect: does not render AccordionTrigger
      <div>Shipping</div>
    }
  </AccordionHeader>
);
