// @flow
//
// The wrong child, in every place this package refuses one, and the right one
// beside it.
//
// Every refusal in this file is a type error `uf check` must raise, suppressed
// where it stands. `npm/ui/index.js` makes one claim no other component
// library can make:
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
// function, and it covers all thirteen of them rather than the six the issue
// listed, because a constraint left out here is a constraint still in the state
// the issue is about.
//
// # How it is read
//
// A `// $FlowExpectedError[code]` comment says that the line after it must be
// reported with that code, and the words after the code say what the report is
// about. Flow suppresses the error, so `uf check` at the repository root stays
// clean, and a suppression that stops matching an error is reported as unused,
// which fails the test. A line without one must not be reported at all — which
// is the half that matters most here, because one of these can break in two
// directions. A `renders*` that stops rejecting a `<div>` fails this file; so
// does one that starts rejecting the parts it exists to admit, which is the
// failure that reaches a consumer as "this library does not compile" rather
// than as a guarantee quietly gone. Both are written out for every constraint
// the package makes.
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
// prove nothing. So the test runs `uf check tests/type-tests npm/ui`, with
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
  Accordion,
  Breadcrumb,
  Combobox,
  Menu,
  NavigationMenu,
  Pagination,
  Select,
  Tabs,
  Toast,
  ToggleGroup,
} from "../../npm/ui/index.js";

// --- Tabs.List ---------------------------------------------------------------
//
// The one `index.js` names, so it goes first.

export const tabs: mixed = (
  <Tabs.List>
    <Tabs.Tab value="general">General</Tabs.Tab>
    <Tabs.Tab value="billing">Billing</Tabs.Tab>
  </Tabs.List>
);

export const buttonInATabList: mixed = (
  <Tabs.List>
    {
      // $FlowExpectedError[incompatible-type] does not render TabsTab
      <button type="button">General</button>
    }
  </Tabs.List>
);

// --- Menu.Body ---------------------------------------------------------------
//
// Six members, which is why the report says "union type" rather than naming
// one: a menu owns commands, the two checkable kinds, rules, groups and
// submenus.

export const menu: mixed = (
  <Menu.Body>
    <Menu.Group>
      <Menu.Item>Open</Menu.Item>
    </Menu.Group>
    <Menu.Separator />
    <Menu.Item>Save</Menu.Item>
    <Menu.CheckboxItem>Show hidden files</Menu.CheckboxItem>
    <Menu.RadioGroup defaultValue="name">
      <Menu.RadioItem value="name">Name</Menu.RadioItem>
    </Menu.RadioGroup>
  </Menu.Body>
);

export const divInAMenu: mixed = (
  <Menu.Body>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>Open</div>
    }
  </Menu.Body>
);

// Both of these are parts of this package, and an option is still not a menu
// item. The report names every member of the union, which is what says what
// should have been written instead.
export const optionInAMenu: mixed = (
  <Menu.Body>
    {
      // $FlowExpectedError[incompatible-type] Either SelectOption element does not render MenuCheckboxItem
      <Select.Option value="GB">United Kingdom</Select.Option>
    }
  </Menu.Body>
);

// --- Combobox.List -----------------------------------------------------------
//
// Options and groups, since ubugeeei-prod/uf#357. A group holds options and its
// own heading and nothing else — the constraint `Select.Group` below now states
// too, since ubugeeei-prod/uf#562.

export const combobox: mixed = (
  <Combobox.List>
    <Combobox.Option value="GB">United Kingdom</Combobox.Option>
    <Combobox.Group>
      <Combobox.GroupLabel>Europe</Combobox.GroupLabel>
      <Combobox.Option value="FR">France</Combobox.Option>
    </Combobox.Group>
  </Combobox.List>
);

export const divInACombobox: mixed = (
  <Combobox.List>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>United Kingdom</div>
    }
  </Combobox.List>
);

export const divInAComboboxGroup: mixed = (
  <Combobox.Group>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>Europe</div>
    }
  </Combobox.Group>
);

// --- Select.List -------------------------------------------------------------

export const select: mixed = (
  <Select.List>
    <Select.Group>
      <Select.Option value="FR">France</Select.Option>
    </Select.Group>
    <Select.Separator />
    <Select.Option value="JP">Japan</Select.Option>
  </Select.List>
);

export const divInASelect: mixed = (
  <Select.List>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>France</div>
    }
  </Select.List>
);

// --- Select.Group ------------------------------------------------------------
//
// ubugeeei-prod/uf#562: the same listbox as `Combobox.Group` above, and it took
// `React.Node` until it was not the last one that did. The two misuses below
// are the two a caller actually writes — a wrapper element around the options,
// and a rule inside the group rather than between groups.

export const selectGroup: mixed = (
  <Select.Group>
    <Select.GroupLabel>Europe</Select.GroupLabel>
    <Select.Option value="FR">France</Select.Option>
  </Select.Group>
);

export const divInASelectGroup: mixed = (
  <Select.Group>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>Europe</div>
    }
  </Select.Group>
);

// A rule separates groups, so it belongs in the list beside them — where
// `select` above puts one and this file's `SelectList` case accepts it. Inside
// a group it is a rule with nothing on one side of it, and now it does not
// compile.
//
// This one reports the members by name rather than saying "union type", for
// `optionInAMenu`'s reason above: the child is a component of this package, so
// the checker has two named things to compare and says which two.
export const separatorInASelectGroup: mixed = (
  <Select.Group>
    {
      // $FlowExpectedError[incompatible-type] Either SelectSeparator element does not render SelectGroupLabel
      <Select.Separator />
    }
  </Select.Group>
);

// --- Toast.Region ------------------------------------------------------------
//
// The constraint that is about a return value rather than about a child.

export const toast: mixed = (
  <Toast.Region>
    {(notification) => (
      <Toast.Root>
        <Toast.Title>{notification.content}</Toast.Title>
      </Toast.Root>
    )}
  </Toast.Region>
);

export const divFromAToastRegion: mixed = (
  <Toast.Region>
    {
      // $FlowExpectedError[incompatible-type] in property children > the return value
      () => <div />
    }
  </Toast.Region>
);

// --- Pagination.Content ------------------------------------------------------

export const pagination: mixed = (
  <Pagination.Content>
    <Pagination.Item current>1</Pagination.Item>
    <Pagination.Next />
  </Pagination.Content>
);

export const divInAPagination: mixed = (
  <Pagination.Content>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>1</div>
    }
  </Pagination.Content>
);

// --- Breadcrumb.List ---------------------------------------------------------
//
// The same constraint as `Pagination.Content` and for the same reason: an
// `<ol>` may hold nothing but `<li>`, and the separators between the crumbs are
// `<li>`s too — hidden ones — because there is nowhere else for them to be.

export const breadcrumb: mixed = (
  <Breadcrumb.List>
    <Breadcrumb.Item>
      <Breadcrumb.Link href="/">Home</Breadcrumb.Link>
    </Breadcrumb.Item>
    <Breadcrumb.Separator>/</Breadcrumb.Separator>
    <Breadcrumb.Item>
      <Breadcrumb.Page>Billing</Breadcrumb.Page>
    </Breadcrumb.Item>
  </Breadcrumb.List>
);

export const divInABreadcrumb: mixed = (
  <Breadcrumb.List>
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>Home</div>
    }
  </Breadcrumb.List>
);

// --- ToggleGroup.Root --------------------------------------------------------

export const toggleGroup: mixed = (
  <ToggleGroup.Root>
    <ToggleGroup.Item value="bold">Bold</ToggleGroup.Item>
  </ToggleGroup.Root>
);

export const buttonInAToggleGroup: mixed = (
  <ToggleGroup.Root>
    {
      // $FlowExpectedError[incompatible-type] does not render ToggleGroupItem
      <button type="button">Bold</button>
    }
  </ToggleGroup.Root>
);

// --- NavigationMenu ----------------------------------------------------------
//
// Three constraints in one component, which is why all three are written out:
// the root holds lists, a list holds entries, and the group an entry opens
// holds links. The third is the one with a behaviour behind it — a `<button>`
// in there is a control that does not go anywhere, in a group whose whole
// purpose is that `Tab` reaches somewhere.

export const navigationMenu: mixed = (
  <NavigationMenu.Root>
    <NavigationMenu.List>
      <NavigationMenu.Item value="products">
        <NavigationMenu.Body>
          <NavigationMenu.Link href="/pricing">Pricing</NavigationMenu.Link>
        </NavigationMenu.Body>
      </NavigationMenu.Item>
    </NavigationMenu.List>
  </NavigationMenu.Root>
);

export const itemInANavigationMenuRoot: mixed = (
  <NavigationMenu.Root>
    {
      // $FlowExpectedError[incompatible-type] does not render NavigationMenuList
      <NavigationMenu.Item value="products">Products</NavigationMenu.Item>
    }
  </NavigationMenu.Root>
);

export const divInANavigationMenuList: mixed = (
  <NavigationMenu.List>
    {
      // $FlowExpectedError[incompatible-type] does not render NavigationMenuItem
      <div>Products</div>
    }
  </NavigationMenu.List>
);

export const buttonInANavigationMenuBody: mixed = (
  <NavigationMenu.Body>
    {
      // $FlowExpectedError[incompatible-type] does not render NavigationMenuLink
      <button type="button">Pricing</button>
    }
  </NavigationMenu.Body>
);

// --- Accordion ---------------------------------------------------------------
//
// And the one `renders` in the package that is not a `renders*`:
// `Accordion.Header` holds exactly one trigger, because a heading with two
// buttons in it is a heading whose accessible name is both of them.

export const accordion: mixed = (
  <Accordion.Root>
    <Accordion.Item value="shipping">
      <Accordion.Header>
        <Accordion.Trigger>Shipping</Accordion.Trigger>
      </Accordion.Header>
      <Accordion.Content>Two days.</Accordion.Content>
    </Accordion.Item>
  </Accordion.Root>
);

export const headerInAnAccordionRoot: mixed = (
  <Accordion.Root>
    {
      // $FlowExpectedError[incompatible-type] does not render AccordionItem
      <Accordion.Header>
        <Accordion.Trigger>Shipping</Accordion.Trigger>
      </Accordion.Header>
    }
  </Accordion.Root>
);

export const divInAnAccordionItem: mixed = (
  <Accordion.Item value="shipping">
    {
      // $FlowExpectedError[incompatible-type] does not render union type
      <div>Shipping</div>
    }
  </Accordion.Item>
);

export const divInAnAccordionHeader: mixed = (
  <Accordion.Header>
    {
      // $FlowExpectedError[incompatible-type] does not render AccordionTrigger
      <div>Shipping</div>
    }
  </Accordion.Header>
);
