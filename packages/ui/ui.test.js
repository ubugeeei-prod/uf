// @flow
//
// `@uniflowed/ui`.
//
// These test the part that is invisible: what a screen reader is told, and what
// the keyboard does. A snapshot of the markup would pass while every one of
// these was broken.
//
// Nothing here asserts on a class name, an element name where a role will do,
// or the shape of the tree. Every assertion is either "a reader is told X" or
// "this key does Y", because those are the two promises this package makes and
// the two things a refactor must not be allowed to break quietly.
//
// The one exception is the last block, which runs `uf check` over the package.
// The type of the props a caller may spread is part of what this package
// promises too, and it is not a promise any amount of rendering can check.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { afterEach, beforeEach, describe, expect, fn, it, uft } from "@uniflowed/test";
import {
  accessibleName,
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  within,
} from "@uniflowed/react-testing";
// The clock behind `Temporal.Now`, so that "today" in a calendar is a fact this
// file states rather than one the machine happens to hold. `temporal.test.js`
// installs one for every case that involves now; a calendar needs it in exactly
// one case, and names `today` in the rest.
import { fixedClock, setClock } from "@uniflowed/core/clock";
import type { PlainDate } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";
import {
  Accordion,
  Alert,
  AlertDialog,
  Avatar,
  Breadcrumb,
  Calendar,
  Carousel,
  Checkbox,
  Collapsible,
  Combobox,
  ContextMenu,
  DatePicker,
  Dialog,
  Drawer,
  Field,
  HoverCard,
  InputOtp,
  Menu,
  Menubar,
  NavigationMenu,
  Pagination,
  Popover,
  Progress,
  RadioGroup,
  Resizable,
  ScrollArea,
  Select,
  Separator,
  Sheet,
  Sidebar,
  Skeleton,
  Slider,
  Switch,
  Table,
  Tabs,
  Toast,
  Toggle,
  ToggleGroup,
  Tooltip,
  dismissAllToasts,
  toast,
  updateToast,
} from "@uniflowed/ui";
// The other half of ubugeeei-prod/uf#297. `Field` and `@uniflowed/form` each
// used to compute `aria-describedby` and `aria-invalid`, and the assertion that
// they now agree cannot be written from inside either package alone.
import { useFieldSource, useForm } from "@uniflowed/form";

// By path, not by subpath export, the way `highlight.test.js` reaches one:
// `internal/` is not part of any package's public surface, and the whole
// argument for keeping the positioning there is that a consumer cannot get a
// weaker copy of it. The arithmetic is still the one thing in this package a
// test can hold to an exact number, so it is reached where it lives.
import type { Align, Placement, Rect, Side } from "./internal/anchor.js";
import { placeOverlay, useAnchor } from "./internal/anchor.js";
// And the same, for the other half of a calendar: `internal/date-grid.js` says
// which date a key means, over dates rather than over elements, and the month
// boundaries are the cases worth being exhaustive about.
import { moveDate, movementForDateKey, weeksOf } from "./internal/date-grid.js";
// The same reasoning, for the same reason: `focusable()` is this package's
// definition of what `Tab` reaches, and "a slide nobody can see is not one of
// them" is a claim about that definition rather than about a rendered tree.
import { focusable } from "./internal/focus.js";
// `document.body` is `HTMLBodyElement | null` — a parsed document need not have
// one — so every `fireEvent` aimed at the page itself narrows through here
// rather than thirteen times over. See ubugeeei-prod/uf#573.
import { bodyOf } from "../../tests/library/dom.js";
// The negative type tests below run `uf check` and read what it said; this is
// the harness that does it, shared with the five other suites that make the
// same kind of claim. See its module header for why the package goes to the
// checker in the same command as the fixture.
import type { CheckReport } from "../../tests/library/type-tests.js";
import {
  everyMisuseIsReported,
  repositoryRoot as repository,
  ufBinary as UF,
} from "../../tests/library/type-tests.js";

/**
 * Every `aria-*` reference in the document that names an id nothing has.
 *
 * A dangling reference is the failure this package's docs keep coming back to:
 * a screen reader handed an `aria-labelledby` pointing at a missing element
 * announces *nothing at all*, rather than falling back to the element's own
 * text. It is silent, it looks correct in the markup, and it is the single
 * easiest way to make a component worse than the plain HTML it replaced.
 */
function danglingReferences(): Array<string> {
  const attributes = [
    "aria-labelledby",
    "aria-describedby",
    "aria-controls",
    "aria-activedescendant",
  ];
  const dangling = [];
  for (const attribute of attributes) {
    for (const element of Array.from(document.querySelectorAll(`[${attribute}]`))) {
      const value = element.getAttribute(attribute) ?? "";
      for (const id of value.split(/\s+/).filter(Boolean)) {
        if (document.getElementById(id) == null) {
          dangling.push(`<${element.tagName.toLowerCase()} ${attribute}="${id}">`);
        }
      }
    }
  }
  return dangling;
}

/**
 * Switch away from the document and back, the way another tab does.
 *
 * `visibilityState` is a getter rather than a property, so it is redefined
 * rather than assigned. Both halves are needed: the event is what
 * `useDocumentVisible` subscribes to, and the property is what it reads when
 * the event arrives.
 */
function hideDocument(hidden: boolean): void {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    // Annotated, because Flow types `visibilityState` as the four states the
    // specification names and infers a bare `string` from the conditional.
    get: (): "hidden" | "visible" => (hidden ? "hidden" : "visible"),
  });
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

/**
 * Give an element a box, because this DOM gives every element a zero one.
 *
 * A slider turns a press into a value by measuring its track, and a track that
 * is nought pixels wide has no values in it. This is the one place these tests
 * pretend about layout, and it pretends about exactly four numbers.
 */
function measure(
  element: HTMLElement,
  box: {|
    readonly left: number,
    readonly width: number,
    readonly top: number,
    readonly height: number,
  |},
): void {
  const rect = {
    left: box.left,
    width: box.width,
    top: box.top,
    height: box.height,
    right: box.left + box.width,
    bottom: box.top + box.height,
    x: box.left,
    y: box.top,
  };
  (element as $FlowFixMe).getBoundingClientRect = () => rect;
}

/**
 * Answer every media query the same way, or stop answering them at all.
 *
 * This document has no `matchMedia`, which is what `useMediaQuery` treats as
 * "no viewport to ask" — so a sidebar is wide and nothing prefers reduced
 * motion unless a test says otherwise. `null` puts it back.
 */
function answerMediaQueries(matches: boolean | null): void {
  const host: $FlowFixMe = window;
  host.matchMedia =
    matches == null
      ? undefined
      : (query: string) => ({
          matches,
          media: query,
          addEventListener: () => {},
          removeEventListener: () => {},
        });
}

/**
 * Put a whole string into a control the way a paste does.
 *
 * `userEvent.type` sends one character at a time, which is the opposite of what
 * a paste is, and this document has no clipboard. A paste is one `input` event
 * carrying the whole string, so that is what this is — written through the
 * prototype's setter for the reason `react-testing`'s own `setValue` gives:
 * React remembers the last value it wrote to the node and skips an event whose
 * value it believes it already knows.
 *
 * It is also how a `Backspace` is spelled here. The harness's `keyboard` does
 * not edit text — `Backspace` has no printable form, so it dispatches the key
 * and changes nothing — and the claim being tested is not that the browser
 * deletes a character. It is what the component does with the `input` event
 * afterwards.
 */
function replaceValue(field: HTMLElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), "value")?.set;
  if (setter != null) {
    setter.call(field, value);
  } else {
    (field as $FlowFixMe).value = value;
  }
  fireEvent.input(field, { data: value });
}

describe("Field", () => {
  component EmailField(invalid: boolean) {
    return (
      <Field.Root invalid={invalid}>
        <Field.Label>Email address</Field.Label>
        <Field.Control render={(props) => <input type="email" {...props} />} />
        <Field.Description>We will not share it.</Field.Description>
        <Field.Error>That is not an email address.</Field.Error>
      </Field.Root>
    );
  }

  it("points the label at the control", () => {
    render(<EmailField invalid={false} />);
    // Found by its label, which only works if the wiring is right.
    expect(screen.getByLabelText("Email address").getAttribute("type")).toBe("email");
  });

  it("describes the control with the help text", () => {
    render(<EmailField invalid={false} />);
    const control = screen.getByLabelText("Email address");
    const described = control.getAttribute("aria-describedby") ?? "";
    const help = screen.getByText("We will not share it.");
    expect(described.split(" ")).toContain(help.getAttribute("id"));
  });

  it("says nothing about validity while the field is valid", () => {
    render(<EmailField invalid={false} />);
    expect(screen.getByLabelText("Email address")).not.toHaveAttribute("aria-invalid");
    expect(screen.queryByText("That is not an email address.")).toBe(null);
  });

  it("marks the control invalid and describes it with the error", () => {
    render(<EmailField invalid={true} />);
    const control = screen.getByLabelText("Email address");
    expect(control).toHaveAttribute("aria-invalid", "true");
    const error = screen.getByRole("alert");
    const described = control.getAttribute("aria-describedby") ?? "";
    expect(described.split(" ")).toContain(error.getAttribute("id"));
  });

  it("never points aria-describedby at an element that is not there", () => {
    render(
      <Field.Root>
        <Field.Label>Name</Field.Label>
        <Field.Control render={(props) => <input {...props} />} />
      </Field.Root>,
    );
    expect(screen.getByLabelText("Name")).not.toHaveAttribute("aria-describedby");
  });

  it("gives each field its own ids", () => {
    render(
      <div>
        <EmailField invalid={false} />
        <EmailField invalid={false} />
      </div>,
    );
    const [first, second] = screen.getAllByLabelText("Email address");
    expect(first.getAttribute("id")).not.toBe(second.getAttribute("id"));
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<Field.Label>orphan</Field.Label>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Field.Label must be rendered inside a Field.Root");
  });
});

describe("Field: bound to a form", () => {
  // ubugeeei-prod/uf#297. `Field` and `@uniflowed/form` each computed
  // `aria-describedby` and `aria-invalid`, and spreading both onto one input
  // meant the later one won — so the control was described by the form's
  // message *or* by `Field.Description`, depending on argument order, and never
  // by both. These are the assertions that failed before one place owned them.

  type Signup = {| readonly email: string |};

  component SignupForm(onServerError?: boolean = false) {
    const form = useForm<Signup>({ defaultValues: { email: "" } });
    const email = useFieldSource(form, "email", { required: "We need an email address" });
    return (
      <form onSubmit={form.handleSubmit(() => {})}>
        <Field.Root field={email}>
          <Field.Label>Email address</Field.Label>
          <Field.Control render={(props) => <input type="email" {...props} />} />
          <Field.Description>We will not share it.</Field.Description>
          <Field.Error />
        </Field.Root>
        <button type="submit">Sign up</button>
        {onServerError ? (
          <button
            onClick={() =>
              form.setError(
                "email",
                { message: "That address is already taken" },
                { shouldFocus: true },
              )
            }
            type="button"
          >
            Pretend the server answered
          </button>
        ) : null}
      </form>
    );
  }

  const submit = async () => {
    await userEvent.click(screen.getByRole("button", { name: "Sign up" }));
  };

  it("takes its validity from the form rather than from a prop", async () => {
    render(<SignupForm />);
    expect(screen.getByLabelText("Email address")).not.toHaveAttribute("aria-invalid");
    await submit();
    // No `invalid` prop is passed anywhere in the markup above: the form is the
    // one thing that knows, and it is now the one thing that says.
    expect(screen.getByLabelText("Email address")).toHaveAttribute("aria-invalid", "true");
  });

  it("describes the control with the help text and the error at once", async () => {
    render(<SignupForm />);
    await submit();
    const control = screen.getByLabelText("Email address");
    const described = (control.getAttribute("aria-describedby") ?? "").split(" ");
    const help = screen.getByText("We will not share it.");
    const error = screen.getByRole("alert");
    // A token list with both ids in it, which is the assertion the collision
    // failed: one of the two used to overwrite the other outright.
    expect(described).toContain(help.getAttribute("id"));
    expect(described).toContain(error.getAttribute("id"));
    expect(danglingReferences()).toEqual([]);
  });

  it("shows the message the form gave, without being handed it", async () => {
    render(<SignupForm />);
    await submit();
    expect(screen.getByRole("alert").textContent).toBe("We need an email address");
  });

  it("moves focus to the first field that failed", async () => {
    render(<SignupForm />);
    await submit();
    expect(screen.getByLabelText("Email address")).toHaveFocus();
  });

  it("says a field is required before it is wrong", () => {
    render(<SignupForm />);
    const control = screen.getByLabelText("Email address");
    // `aria-invalid` says a field is wrong after it has been checked; this says
    // it is required before, which is the announcement that prevents the error
    // rather than reporting it. It comes from the rule, not from a prop.
    expect(control).toHaveAttribute("aria-required", "true");
    expect(control).not.toHaveAttribute("aria-invalid");
  });

  it("carries the form's own binding onto the control", () => {
    render(<SignupForm />);
    // One spread, not two that overwrite each other: the store's `name` is on
    // the element beside the attributes the field computed.
    expect(screen.getByLabelText("Email address")).toHaveAttribute("name", "email");
  });

  it("reports an error the server sent the same way, and moves focus to it", async () => {
    render(<SignupForm onServerError />);
    await userEvent.click(screen.getByRole("button", { name: "Pretend the server answered" }));
    const control = screen.getByLabelText("Email address");
    expect(control).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert").textContent).toBe("That address is already taken");
    expect(control).toHaveFocus();
    expect(danglingReferences()).toEqual([]);
  });

  it("says nothing about validity while the form is happy", async () => {
    render(<SignupForm />);
    const control = screen.getByLabelText("Email address");
    await userEvent.type(control, "someone@example.com");
    await submit();
    expect(screen.getByLabelText("Email address")).not.toHaveAttribute("aria-invalid");
    expect(screen.queryByRole("alert")).toBe(null);
  });
});

describe("Field: a group that a label cannot point at", () => {
  component PlanField(invalid?: boolean = false) {
    return (
      <Field.Root group invalid={invalid} required>
        <Field.Label>Plan</Field.Label>
        <RadioGroup.Root defaultValue="free">
          <RadioGroup.Item value="free">Free</RadioGroup.Item>
          <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
        </RadioGroup.Root>
        <Field.Description>You can change this later.</Field.Description>
        <Field.Error>Choose a plan.</Field.Error>
      </Field.Root>
    );
  }

  it("names the set with a group rather than with a label that points at nothing", () => {
    render(<PlanField />);
    const group = screen.getByRole("group");
    expect(accessibleName(group)).toBe("Plan");
    // `<label for>` names one form control. Aimed at the wrapper around a set of
    // radios it points at something that is not a form control, which every
    // browser ignores — silently.
    expect(document.querySelectorAll("label").length).toBe(0);
    expect(danglingReferences()).toEqual([]);
  });

  it("describes the set, and reports the set's validity", () => {
    render(<PlanField invalid />);
    const group = screen.getByRole("group");
    const described = (group.getAttribute("aria-describedby") ?? "").split(" ");
    expect(described).toContain(screen.getByText("You can change this later.").getAttribute("id"));
    expect(described).toContain(screen.getByRole("alert").getAttribute("id"));
    expect(group).toHaveAttribute("aria-invalid", "true");
    expect(danglingReferences()).toEqual([]);
  });

  it("leaves the radios their own roles", () => {
    render(<PlanField />);
    // The group names the set; the members are still a radio group with its own
    // roving tab stop, which is `radio-group.js`'s job and not this one's.
    expect(screen.getByRole("radiogroup")).toBeInTheDocument();
    expect(screen.getAllByRole("radio").length).toBe(2);
  });
});

describe("Tabs", () => {
  component Example(
    activationMode?: "automatic" | "manual" = "automatic",
    orientation?: "horizontal" | "vertical" = "horizontal",
  ) {
    return (
      <Tabs.Root activationMode={activationMode} defaultValue="one" orientation={orientation}>
        <Tabs.List aria-label="Sections">
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
          <Tabs.Tab value="three">Three</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first panel</Tabs.Panel>
        <Tabs.Panel value="two">second panel</Tabs.Panel>
        <Tabs.Panel value="three">third panel</Tabs.Panel>
      </Tabs.Root>
    );
  }

  it("announces itself as a tab list of tabs", () => {
    render(<Example />);
    expect(screen.getByRole("tablist")).toBeInTheDocument();
    expect(screen.getAllByRole("tab").length).toBe(3);
  });

  it("renders only the selected panel", () => {
    render(<Example />);
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
    expect(screen.queryByText("second panel")).toBe(null);
  });

  it("keeps exactly one tab in the page's tab order", () => {
    render(<Example />);
    const stops = screen.getAllByRole("tab").filter((tab) => tab.getAttribute("tabindex") === "0");
    // The whole point of a roving tabindex: Tab moves past the list in one
    // press instead of one press per tab.
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("One");
  });

  it("selects on click and moves the tab stop with the selection", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
    expect(screen.getByRole("tab", { name: "Two" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "One" })).toHaveAttribute("tabindex", "-1");
  });

  it("moves between tabs with the arrow keys", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
  });

  it("leaves the page's own arrow keys alone in a horizontal list", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    // ArrowDown scrolls the page. A tab list that swallows it has taken
    // scrolling away from every reader who uses the keyboard to read.
    expect(fireEvent.keyDown(screen.getByRole("tab", { name: "One" }), { key: "ArrowDown" })).toBe(
      true,
    );
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
  });

  it("uses the up and down arrows when it is vertical, and says so", async () => {
    render(<Example orientation="vertical" />);
    expect(screen.getByRole("tablist")).toHaveAttribute("aria-orientation", "vertical");
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
  });

  it("wraps at the ends", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tabpanel").textContent).toBe("third panel");
  });

  it("jumps to the first and last tab with Home and End", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("tabpanel").textContent).toBe("third panel");
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
  });

  it("moves focus with the selection, so one key press is one tab", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Two" })).toHaveFocus();
  });

  it("ties each tab to the panel it controls, in both directions", () => {
    render(<Example />);
    const tab = screen.getByRole("tab", { name: "One" });
    const panel = screen.getByRole("tabpanel");
    expect(tab.getAttribute("aria-controls")).toBe(panel.getAttribute("id"));
    expect(panel.getAttribute("aria-labelledby")).toBe(tab.getAttribute("id"));
  });

  it("does not point a tab at a panel that is not rendered", () => {
    render(<Example />);
    // Panels are mounted on demand, so an unselected tab has nothing to name.
    // Naming it anyway tells a reader there is somewhere to go and then has
    // nowhere to send them.
    expect(screen.getByRole("tab", { name: "Two" })).not.toHaveAttribute("aria-controls");
    expect(danglingReferences()).toEqual([]);
  });

  it("reports the selection to a controlled parent", async () => {
    const onValueChange = fn();
    render(
      <Tabs.Root defaultValue="one" onValueChange={onValueChange}>
        <Tabs.List>
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
      </Tabs.Root>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(onValueChange).toHaveBeenCalledWith("two");
  });

  it("does not select a disabled tab", async () => {
    render(
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab disabled value="two">
            Two
          </Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
      </Tabs.Root>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(screen.getByRole("tabpanel").textContent).toBe("first");
  });
});

describe("Tabs: manual activation", () => {
  component Deferred() {
    return (
      <Tabs.Root activationMode="manual" defaultValue="one">
        <Tabs.List aria-label="Sections">
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
          <Tabs.Tab value="three">Three</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first panel</Tabs.Panel>
        <Tabs.Panel value="two">second panel</Tabs.Panel>
        <Tabs.Panel value="three">third panel</Tabs.Panel>
      </Tabs.Root>
    );
  }

  it("moves focus without selecting", async () => {
    render(<Deferred />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    // The point of manual activation: arrowing past three tabs whose panels
    // each fetch must not start three fetches.
    expect(screen.getByRole("tab", { name: "Two" })).toHaveFocus();
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
    expect(screen.getByRole("tab", { name: "Two" })).toHaveAttribute("aria-selected", "false");
  });

  it("selects on Enter", async () => {
    render(<Deferred />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
  });

  it("selects on Space", async () => {
    render(<Deferred />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{End}");
    await userEvent.keyboard(" ");
    expect(screen.getByRole("tabpanel").textContent).toBe("third panel");
  });
});

describe("Tabs: a disabled tab is skipped and still announced", () => {
  component WithDisabled() {
    return (
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab disabled value="two">
            Two
          </Tabs.Tab>
          <Tabs.Tab value="three">Three</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
        <Tabs.Panel value="three">third</Tabs.Panel>
      </Tabs.Root>
    );
  }

  it("keeps the disabled tab in the accessibility tree", () => {
    render(<WithDisabled />);
    // `aria-disabled`, not the native `disabled`: a reader is told the section
    // exists and is unavailable, rather than finding a gap they cannot ask
    // about.
    const tab = screen.getByRole("tab", { name: "Two" });
    expect(tab).toHaveAttribute("aria-disabled", "true");
    expect(screen.getAllByRole("tab").length).toBe(3);
  });

  it("steps over a disabled tab instead of landing on it", async () => {
    render(<WithDisabled />);
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    // Selecting the disabled tab changed the panel to one whose tab cannot
    // take focus, and every tab past it became unreachable by keyboard.
    expect(screen.getByRole("tabpanel").textContent).toBe("third");
    expect(screen.getByRole("tab", { name: "Three" })).toHaveFocus();
  });

  it("steps over it backwards too", async () => {
    render(<WithDisabled />);
    await userEvent.click(screen.getByRole("tab", { name: "Three" }));
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tabpanel").textContent).toBe("first");
  });

  it("lands End on the last enabled tab", async () => {
    render(
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
          <Tabs.Tab disabled value="three">
            Three
          </Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
        <Tabs.Panel value="three">third</Tabs.Panel>
      </Tabs.Root>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{End}");
    // `End` aims at the last tab and has to search *backwards* when it is
    // disabled. Inferring the direction from the target index wrapped around to
    // the first tab instead.
    expect(screen.getByRole("tabpanel").textContent).toBe("second");
  });
});

describe("Dialog", () => {
  component Example() {
    return (
      <Dialog.Root>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Overlay />
        <Dialog.Body>
          <Dialog.Header>
            <Dialog.Title>Are you sure?</Dialog.Title>
            <Dialog.Description>This cannot be undone.</Dialog.Description>
          </Dialog.Header>
          <button type="button">Confirm</button>
          <Dialog.Footer>
            <Dialog.Close>Cancel</Dialog.Close>
          </Dialog.Footer>
        </Dialog.Body>
      </Dialog.Root>
    );
  }

  it("is closed until it is opened", () => {
    render(<Example />);
    expect(screen.queryByRole("dialog")).toBe(null);
    const trigger = screen.getByRole("button", { name: "Open" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    // Nothing to control yet, so nothing is named.
    expect(trigger).not.toHaveAttribute("aria-controls");
  });

  it("opens, and says the rest of the page is unavailable", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    // Its accessible name and its description come from its own parts, not
    // from a guess, and both point at elements that exist.
    expect(dialog.getAttribute("aria-labelledby")).toBe(
      screen.getByRole("heading").getAttribute("id"),
    );
    expect(dialog.getAttribute("aria-describedby")).toBe(
      screen.getByText("This cannot be undone.").getAttribute("id"),
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("claims no name at all when it has no title", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body aria-label="Settings">
          <button type="button">Done</button>
        </Dialog.Body>
      </Dialog.Root>,
    );
    // Rather than naming the id a title *would* have had, which is the dangling
    // reference that makes a screen reader announce nothing at all.
    const dialog = screen.getByRole("dialog");
    expect(dialog).not.toHaveAttribute("aria-labelledby");
    expect(dialog).toHaveAttribute("aria-label", "Settings");
  });

  it("adds no landmark for its header and footer", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    // A `<header>` inside a dialog is a second `banner` landmark, which a
    // reader finds in the landmark list and cannot explain.
    expect(screen.queryByRole("banner")).toBe(null);
    expect(screen.queryByRole("contentinfo")).toBe(null);
  });

  it("moves focus to the first thing worth acting on", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByRole("button", { name: "Confirm" })).toHaveFocus();
  });

  it("wraps Tab at the end rather than letting it leave", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const dialog = screen.getByRole("dialog");
    const confirm = within(dialog).getByRole("button", { name: "Confirm" });
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });

    // From the last stop, forward. Without the trap this lands on the page
    // behind the dialog, which the reader cannot see and cannot get back from.
    cancel.focus();
    fireEvent.keyDown(cancel, { key: "Tab" });
    expect(confirm).toHaveFocus();
  });

  it("wraps Shift+Tab at the start", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const dialog = screen.getByRole("dialog");
    const confirm = within(dialog).getByRole("button", { name: "Confirm" });
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });

    confirm.focus();
    fireEvent.keyDown(confirm, { key: "Tab", shiftKey: true });
    expect(cancel).toHaveFocus();
  });

  it("keeps Tab inside a dialog with nothing focusable in it", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Nothing to do</Dialog.Title>
        </Dialog.Body>
      </Dialog.Root>,
    );
    const dialog = screen.getByRole("dialog");
    // The dialog itself takes focus, and Tab has nowhere to go.
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(dialog).toHaveFocus();
  });

  it("takes the rest of the page out of the document while it is open", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    // `aria-hidden` for the screen reader and `inert` for the browser: the
    // second is what stops a click or a Tab reaching the page behind without
    // depending on this component's key handling being reached at all.
    expect(trigger).toHaveAttribute("aria-hidden", "true");
    expect(trigger).toHaveAttribute("inert");
    await userEvent.keyboard("{Escape}");
    expect(trigger).not.toHaveAttribute("aria-hidden");
    expect(trigger).not.toHaveAttribute("inert");
  });

  it("holds the page still while it is open, and gives it back", async () => {
    const before = document.body.style.overflow;
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    // A wheel over a modal that scrolls the document loses the reader's place
    // in the page they will come back to.
    expect(document.body.style.overflow).toBe("hidden");
    await userEvent.keyboard("{Escape}");
    expect(document.body.style.overflow).toBe(before);
  });

  it("closes on Escape", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("gives focus back to whatever opened it", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    // Otherwise focus falls to `<body>`, the next Tab starts at the top of the
    // page, and the reader has to find their place again.
    expect(trigger).toHaveFocus();
  });

  it("closes from its own close button, and gives focus back", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBe(null);
    expect(trigger).toHaveFocus();
  });

  it("closes on a press outside it", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.pointerDown(bodyOf());
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("stays open for a press inside it", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.pointerDown(screen.getByRole("heading"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("reports opening and closing to a controlled parent", async () => {
    const onOpenChange = fn();
    render(
      <Dialog.Root onOpenChange={onOpenChange}>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Body>
          <Dialog.Title>Title</Dialog.Title>
        </Dialog.Body>
      </Dialog.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(onOpenChange).toHaveBeenCalledWith(true);
  });
});

describe("Dialog: two of them stacked", () => {
  component Stacked() {
    return (
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Outer</Dialog.Title>
          <Dialog.Root>
            <Dialog.Trigger>Open inner</Dialog.Trigger>
            <Dialog.Body>
              <Dialog.Title>Inner</Dialog.Title>
              <button type="button">Inner action</button>
            </Dialog.Body>
          </Dialog.Root>
        </Dialog.Body>
      </Dialog.Root>
    );
  }

  it("closes only the one in front", async () => {
    render(<Stacked />);
    const trigger = screen.getByRole("button", { name: "Open inner" });
    await userEvent.click(trigger);
    expect(screen.getAllByRole("dialog").length).toBe(2);

    await userEvent.keyboard("{Escape}");
    // One Escape is one dismissal. Two stacked dialogs nest in the DOM, so
    // without stopping the event the outer dialog's handler saw it too and one
    // press closed both.
    const remaining = screen.getAllByRole("dialog");
    expect(remaining.length).toBe(1);
    expect(within(remaining[0]).getByRole("heading").textContent).toBe("Outer");
  });

  it("gives focus back to the inner trigger, not to the page", async () => {
    render(<Stacked />);
    const trigger = screen.getByRole("button", { name: "Open inner" });
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
  });

  it("hands the page back the state the outer dialog left it in", async () => {
    render(<Stacked />);
    const trigger = screen.getByRole("button", { name: "Open inner" });
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    // The outer dialog is still open, so the page behind *both* of them must
    // still be inert — the inner dialog's cleanup must not undo the outer's.
    expect(document.body.style.overflow).toBe("hidden");
    expect(trigger).not.toHaveAttribute("inert");
  });
});

describe("Alert dialog", () => {
  component Confirm() {
    return (
      <AlertDialog.Root>
        <AlertDialog.Trigger>Delete the project</AlertDialog.Trigger>
        <AlertDialog.Overlay />
        <AlertDialog.Body>
          <AlertDialog.Header>
            <AlertDialog.Title>Delete this project?</AlertDialog.Title>
            <AlertDialog.Description>This cannot be undone.</AlertDialog.Description>
          </AlertDialog.Header>
          <AlertDialog.Footer>
            <AlertDialog.Action>Delete</AlertDialog.Action>
            <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
          </AlertDialog.Footer>
        </AlertDialog.Body>
      </AlertDialog.Root>
    );
  }

  const open = async () => {
    render(<Confirm />);
    await userEvent.click(screen.getByRole("button", { name: "Delete the project" }));
  };

  it("announces an alert dialog as an alert dialog, and always describes it", async () => {
    await open();
    const dialog = screen.getByRole("alertdialog");
    // `alertdialog` rather than `dialog`, which is what makes a screen reader
    // announce the description as soon as focus arrives rather than waiting to
    // be asked.
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog.getAttribute("aria-describedby")).toBe(
      screen.getByText("This cannot be undone.").getAttribute("id"),
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("does not close an alert dialog on a press outside it", async () => {
    await open();
    fireEvent.pointerDown(bodyOf());
    // The inverse of `Dialog`'s "closes on a press outside it", and the two
    // together are what say the behaviour is a choice. A confirmation that
    // disappears when the reader clicks slightly beside it has given no
    // indication which way it went.
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    // Escape still does, because a modal a reader cannot leave from the
    // keyboard is a trap and declining is what Escape means.
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBe(null);
  });

  it("puts focus on the action the caller named", async () => {
    await open();
    // Cancel, and not `Delete` — which is the first focusable element in the
    // dialog, and is where a plain `Dialog` would have put it.
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Delete" })).not.toHaveFocus();
  });

  it("closes from either answer, and gives focus back", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.queryByRole("alertdialog")).toBe(null);
    expect(screen.getByRole("button", { name: "Delete the project" })).toHaveFocus();
  });

  it("refuses to be an alert with nothing to announce", () => {
    // `role="alertdialog"` exists to announce a description. One without a
    // description interrupts the reader to say only its title, which is worse
    // than the plain dialog it replaced.
    expect(() =>
      render(
        <AlertDialog.Root defaultOpen>
          <AlertDialog.Body>
            <AlertDialog.Title>Delete this project?</AlertDialog.Title>
            <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
          </AlertDialog.Body>
        </AlertDialog.Root>,
      ),
    ).toThrow("AlertDialog.Description");
  });
});

describe("Sheet", () => {
  it("is a modal dialog that says which edge it came from", async () => {
    render(
      <Sheet.Root side="left">
        <Sheet.Trigger>Filters</Sheet.Trigger>
        <Sheet.Overlay data-testid="backdrop" />
        <Sheet.Body>
          <Sheet.Title>Filters</Sheet.Title>
          <Sheet.Close>Done</Sheet.Close>
        </Sheet.Body>
      </Sheet.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));

    const dialog = screen.getByRole("dialog", { name: "Filters" });
    // Every modal promise is `Dialog`'s, unchanged. What a sheet adds is the
    // edge, as an attribute a stylesheet can read — the same `data-side`
    // `Popover.Body` writes, and the one `Drawer` and `Sidebar` are defined
    // against.
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAttribute("data-side", "left");
    expect(document.querySelector('[data-testid="backdrop"]')).toHaveAttribute("data-side", "left");
  });
});

describe("Drawer", () => {
  component Example() {
    return (
      <Drawer.Root defaultOpen side="bottom" snapPoints={[0.4, 1]}>
        <Drawer.Overlay />
        <Drawer.Body>
          <Drawer.Handle label="Resize the details" />
          <Drawer.Title>Details</Drawer.Title>
          <Drawer.Description>What we know so far.</Drawer.Description>
          <Drawer.Close>Close</Drawer.Close>
        </Drawer.Body>
      </Drawer.Root>
    );
  }

  it("lets a drawer be closed without dragging", async () => {
    render(<Example />);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // WCAG 2.5.7 wants everything a drag achieves achievable without one, and
    // WCAG 2.1.1 wants it from the keyboard. Both, with no pointer at all: the
    // closing key at the smallest snap point is "drag it off the edge".
    screen.getByRole("slider", { name: "Resize the details" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("reaches every snap point the drag can", async () => {
    render(<Example />);
    const handle = screen.getByRole("slider", { name: "Resize the details" });
    // A slider over the snap points, so a reader is told where they are in a
    // list rather than left with a grip that does nothing.
    expect(handle).toHaveAttribute("aria-valuemin", "0");
    expect(handle).toHaveAttribute("aria-valuemax", "1");
    expect(handle).toHaveAttribute("aria-valuenow", "0");
    expect(handle).toHaveAttribute("aria-valuetext", "40%");

    handle.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(handle).toHaveAttribute("aria-valuenow", "1");
    expect(handle).toHaveAttribute("aria-valuetext", "100%");

    await userEvent.keyboard("{Home}");
    expect(handle).toHaveAttribute("aria-valuenow", "0");
    await userEvent.keyboard("{End}");
    expect(handle).toHaveAttribute("aria-valuenow", "1");
  });

  it("refuses a drag that is the only way out", () => {
    expect(() =>
      render(
        <Drawer.Root defaultOpen>
          <Drawer.Body>
            <Drawer.Handle label="Resize the details" />
            <Drawer.Title>Details</Drawer.Title>
          </Drawer.Body>
        </Drawer.Root>,
      ),
    ).toThrow("2.5.7");
  });
});

describe("Sidebar", () => {
  afterEach(() => {
    answerMediaQueries(null);
  });

  component Example(defaultOpen?: boolean = true) {
    return (
      <Sidebar.Root defaultOpen={defaultOpen}>
        <Sidebar.Trigger>Menu</Sidebar.Trigger>
        <Sidebar.Body label="Main">
          <Sidebar.Item label="Settings">Settings</Sidebar.Item>
        </Sidebar.Body>
      </Sidebar.Root>
    );
  }

  it("keeps the sidebar's buttons named while it is collapsed", async () => {
    render(<Example />);
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Menu" }));
    // Collapsed to icons, a button whose name came from its text is announced
    // as "button". The name has to survive whatever the stylesheet does to the
    // text, so it moves into `aria-label` rather than depending on it.
    const entry = screen.getByRole("button", { name: "Settings" });
    expect(entry).toHaveAttribute("aria-label", "Settings");
    expect(entry).toHaveAttribute("data-collapsed", "true");
  });

  it("is a named navigation landmark, and says whether it is showing", async () => {
    render(<Example />);
    // A `<div>` here is navigation a reader has to find by tabbing through it;
    // a landmark is one their software offers to jump to.
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();

    const trigger = screen.getByRole("button", { name: "Menu" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("is not modal while it is part of the page", async () => {
    render(<Example />);
    // Nothing is inert, nothing is announced as modal, and the reader can use
    // what is beside it. That is the whole difference from the other three.
    expect(screen.queryByRole("dialog")).toBe(null);
    expect(screen.getByRole("button", { name: "Menu" })).not.toHaveAttribute("inert");
  });

  it("becomes a modal dialog on a narrow viewport, and gives focus back", async () => {
    answerMediaQueries(true);
    render(<Example defaultOpen={false} />);
    const trigger = screen.getByRole("button", { name: "Menu" });
    expect(screen.queryByRole("navigation")).toBe(null);

    await userEvent.click(trigger);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(within(dialog).getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    // Focus moved in, because it is a dialog now and the page behind it is
    // gone.
    expect(within(dialog).getByRole("button", { name: "Settings" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBe(null);
    // And back out again, to the button that opened it. The transition has to
    // move focus correctly in both directions or the reader is stranded at one
    // end of it.
    expect(trigger).toHaveFocus();
  });
});

describe("Menu", () => {
  component Example() {
    return (
      <Menu.Root>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item>Open</Menu.Item>
          <Menu.Item>Save</Menu.Item>
          <Menu.Separator />
          <Menu.Item>Rename</Menu.Item>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("says what the trigger does before anything is open", () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "File" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).not.toHaveAttribute("aria-controls");
    expect(screen.queryByRole("menu")).toBe(null);
  });

  it("names the menu it controls once there is one", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    const trigger = screen.getByRole("button", { name: "File" });
    expect(trigger.getAttribute("aria-controls")).toBe(screen.getByRole("menu").getAttribute("id"));
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(danglingReferences()).toEqual([]);
  });

  it("opens onto the first item", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });

  it("opens onto the last item for ArrowUp", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "File" });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowUp" });
    // The last entry of a long menu is usually the destructive one, and
    // reaching it should not mean arrowing past everything else.
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("opens onto the first item for ArrowDown", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "File" });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });

  it("moves with the arrows and wraps at both ends", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Save" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("jumps to the ends with Home and End", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });

  it("keeps exactly one item in the tab order, and moves it with focus", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    await userEvent.keyboard("{ArrowDown}");
    const stops = screen
      .getAllByRole("menuitem")
      .filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("Save");
  });

  it("steps over a separator", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    await userEvent.keyboard("{ArrowUp}");
    // The separator is announced as a group boundary and is never landed on.
    expect(screen.getByRole("separator")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "File" });
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBe(null);
    expect(trigger).toHaveFocus();
  });

  it("closes on Tab and lets the key carry on through the page", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    const item = screen.getByRole("menuitem", { name: "Open" });
    // Not prevented: Tab is how a reader goes *past* a menu, rather than
    // through its items one at a time.
    expect(fireEvent.keyDown(item, { key: "Tab" })).toBe(true);
    expect(screen.queryByRole("menu")).toBe(null);
  });

  it("closes on a press outside without dragging focus back", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "File" });
    await userEvent.click(trigger);
    fireEvent.pointerDown(bodyOf());
    expect(screen.queryByRole("menu")).toBe(null);
    // The reader pressed somewhere else on purpose; taking focus back to the
    // trigger would undo the thing they just did.
    expect(trigger).not.toHaveFocus();
  });

  it("runs the item and closes everything when one is chosen", async () => {
    const onSelect = fn();
    render(
      <Menu.Root>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item onSelect={onSelect}>Open</Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "File" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Open" }));
    expect(onSelect).toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBe(null);
  });
});

describe("Menu: typeahead", () => {
  component Example() {
    return (
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item>Open</Menu.Item>
          <Menu.Item>Save</Menu.Item>
          <Menu.Item>Save as…</Menu.Item>
          <Menu.Item>Rename</Menu.Item>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("goes to an item by its first letter", async () => {
    render(<Example />);
    await userEvent.keyboard("r");
    // A thirty-item menu without this is thirty arrow presses.
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("accumulates the letters into a prefix", async () => {
    render(<Example />);
    await userEvent.keyboard("sa");
    expect(screen.getByRole("menuitem", { name: "Save" })).toHaveFocus();
  });

  it("cycles between items starting with the same letter", async () => {
    render(<Example />);
    await userEvent.keyboard("s");
    expect(screen.getByRole("menuitem", { name: "Save" })).toHaveFocus();
    await userEvent.keyboard("s");
    // Repeating one letter is how a reader reaches the second "Save…".
    expect(screen.getByRole("menuitem", { name: "Save as…" })).toHaveFocus();
  });

  it("stays put when nothing matches", async () => {
    render(<Example />);
    await userEvent.keyboard("z");
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });
});

describe("Menu: a disabled item is skipped and still announced", () => {
  component Example() {
    return (
      <Menu.Root defaultOpen>
        <Menu.Trigger>Edit</Menu.Trigger>
        <Menu.Body>
          <Menu.Item>Cut</Menu.Item>
          <Menu.Item disabled>Paste</Menu.Item>
          <Menu.Item>Delete</Menu.Item>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("keeps it in the accessibility tree", () => {
    render(<Example />);
    const paste = screen.getByRole("menuitem", { name: "Paste" });
    // A reader is told "Paste, menu item, dimmed" and learns the command exists
    // and is unavailable. A native `disabled` leaves a silent gap instead.
    expect(paste).toHaveAttribute("aria-disabled", "true");
    expect(screen.getAllByRole("menuitem").length).toBe(3);
  });

  it("steps over it with the arrows", async () => {
    render(<Example />);
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
  });

  it("steps over it with typeahead", async () => {
    render(<Example />);
    await userEvent.keyboard("p");
    expect(screen.getByRole("menuitem", { name: "Cut" })).toHaveFocus();
  });

  it("does nothing when it is chosen", async () => {
    const onSelect = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>Edit</Menu.Trigger>
        <Menu.Body>
          <Menu.Item disabled onSelect={onSelect}>
            Paste
          </Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Paste" }));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });
});

describe("Menu: submenus", () => {
  component Example() {
    return (
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item>Open</Menu.Item>
          <Menu.Sub>
            <Menu.SubTrigger>Export</Menu.SubTrigger>
            <Menu.Body>
              <Menu.Item>PNG</Menu.Item>
              <Menu.Item>SVG</Menu.Item>
            </Menu.Body>
          </Menu.Sub>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("says the sub-trigger opens a menu", () => {
    render(<Example />);
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getAllByRole("menu").length).toBe(1);
  });

  // Focusing a `menuitem` runs the roving tab stop's `onFocus`, which is a
  // React state update; `act` is what tells React it happened. The trigger of
  // the outer menu is a plain button and needs none.
  it("opens on ArrowRight and lands on the first item", async () => {
    render(<Example />);
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    fireEvent.keyDown(trigger, { key: "ArrowRight" });
    expect(screen.getAllByRole("menu").length).toBe(2);
    expect(screen.getByRole("menuitem", { name: "PNG" })).toHaveFocus();
    expect(screen.getByRole("menuitem", { name: "Export" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("closes on ArrowLeft and comes back to the item that opened it", async () => {
    render(<Example />);
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    fireEvent.keyDown(trigger, { key: "ArrowRight" });
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getAllByRole("menu").length).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Export" })).toHaveFocus();
  });

  it("leaves the parent menu open when Escape closes the submenu", async () => {
    render(<Example />);
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    fireEvent.keyDown(trigger, { key: "ArrowRight" });
    await userEvent.keyboard("{Escape}");
    // A submenu is a DOM descendant of its parent, so without stopping the
    // event one Escape closed the whole tree at once.
    expect(screen.getAllByRole("menu").length).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Open" })).toBeInTheDocument();
  });

  it("keeps the arrow keys of the two menus apart", async () => {
    render(<Example />);
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    fireEvent.keyDown(trigger, { key: "ArrowRight" });
    await userEvent.keyboard("{ArrowDown}");
    // The parent menu must not also move: a submenu's items are inside its
    // parent's element, and a plain `querySelectorAll` treats them as the
    // parent's own.
    expect(screen.getByRole("menuitem", { name: "SVG" })).toHaveFocus();
  });

  it("closes the whole tree when an item in the submenu is chosen", async () => {
    const onSelect = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Sub defaultOpen>
            <Menu.SubTrigger>Export</Menu.SubTrigger>
            <Menu.Body>
              <Menu.Item onSelect={onSelect}>PNG</Menu.Item>
            </Menu.Body>
          </Menu.Sub>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "PNG" }));
    expect(onSelect).toHaveBeenCalled();
    // Leaving the parent open after a command has run is a state no native
    // menu has ever been in.
    expect(screen.queryByRole("menu")).toBe(null);
  });
});

describe("Menu: named groups", () => {
  it("names the group after its label", () => {
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Group>
            <Menu.Label>Recent</Menu.Label>
            <Menu.Item>report.pdf</Menu.Item>
          </Menu.Group>
        </Menu.Body>
      </Menu.Root>,
    );
    const group = screen.getByRole("group");
    const label = group.getAttribute("aria-labelledby") ?? "";
    expect(document.getElementById(label)?.textContent).toBe("Recent");
    expect(danglingReferences()).toEqual([]);
  });

  it("claims no name when there is no label", () => {
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Group>
            <Menu.Item>report.pdf</Menu.Item>
          </Menu.Group>
        </Menu.Body>
      </Menu.Root>,
    );
    expect(screen.getByRole("group")).not.toHaveAttribute("aria-labelledby");
  });
});

describe("Menu: the checkable items", () => {
  component ViewMenu(onSort?: (value: string) => void) {
    return (
      <Menu.Root defaultOpen>
        <Menu.Trigger>View</Menu.Trigger>
        <Menu.Body>
          <Menu.CheckboxItem>Show hidden files</Menu.CheckboxItem>
          <Menu.CheckboxItem defaultChecked>Show sidebar</Menu.CheckboxItem>
          <Menu.Separator />
          <Menu.RadioGroup defaultValue="name" onValueChange={onSort}>
            <Menu.Label>Sort by</Menu.Label>
            <Menu.RadioItem value="name">Name</Menu.RadioItem>
            <Menu.RadioItem value="date">Date modified</Menu.RadioItem>
          </Menu.RadioGroup>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("toggles a checkable item without closing the menu", async () => {
    render(<ViewMenu />);
    const box = screen.getByRole("menuitemcheckbox", { name: "Show hidden files" });
    expect(box).toHaveAttribute("aria-checked", "false");
    await userEvent.click(box);
    expect(box).toHaveAttribute("aria-checked", "true");
    // The whole point. Checking three boxes is one visit to the menu on every
    // platform, and a checkbox built on `Menu.Item` made it three.
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("keeps exactly one radio item checked, and tells the caller which", async () => {
    const onSort = fn();
    render(<ViewMenu onSort={onSort} />);
    const chosen = () =>
      screen
        .getAllByRole("menuitemradio")
        .filter((item) => item.getAttribute("aria-checked") === "true")
        .map((item) => item.textContent);

    expect(chosen()).toEqual(["Name"]);
    await userEvent.click(screen.getByRole("menuitemradio", { name: "Date modified" }));
    // One before and one after: the group owns the value, which is what stops
    // two items from believing they are both checked.
    expect(chosen()).toEqual(["Date modified"]);
    expect(onSort).toHaveBeenCalledWith("date");
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("names the radio group after its label and points at nothing missing", () => {
    render(<ViewMenu />);
    const group = screen.getByRole("group");
    expect(accessibleName(group)).toBe("Sort by");
    expect(danglingReferences()).toEqual([]);
  });

  it("steps across the checkable items with the arrows and with typeahead", async () => {
    render(<ViewMenu />);
    // `ITEM_SELECTOR` named these two roles before there was a component that
    // rendered them; this is the assertion that the promise was real.
    expect(screen.getByRole("menuitemcheckbox", { name: "Show hidden files" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitemcheckbox", { name: "Show sidebar" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitemradio", { name: "Name" })).toHaveFocus();
    await userEvent.keyboard("d");
    expect(screen.getByRole("menuitemradio", { name: "Date modified" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitemradio", { name: "Date modified" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitemcheckbox", { name: "Show hidden files" })).toHaveFocus();
  });

  it("keeps exactly one of them in the tab order", async () => {
    render(<ViewMenu />);
    await userEvent.keyboard("{ArrowDown}");
    const stops = [
      ...screen.getAllByRole("menuitemcheckbox"),
      ...screen.getAllByRole("menuitemradio"),
    ].filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("Show sidebar");
  });

  it("lets onSelect keep a command's menu open, and closes it otherwise", async () => {
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item onSelect={(event) => event.preventDefault()}>Open</Menu.Item>
          <Menu.Item>Save</Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Open" }));
    // `preventDefault()` is "I handled this", which is the same sentence
    // `composeHandlers` reads between a caller's handler and the component's.
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("menuitem", { name: "Save" }));
    expect(screen.queryByRole("menu")).toBe(null);
  });

  it("closes on a checkable item that asked to", async () => {
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>View</Menu.Trigger>
        <Menu.Body>
          <Menu.CheckboxItem closeOnSelect>Show hidden files</Menu.CheckboxItem>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: "Show hidden files" }));
    expect(screen.queryByRole("menu")).toBe(null);
  });

  it("lets a parent own a checkable item and refuse a change", async () => {
    component Refusing() {
      const [on, setOn] = useState(false);
      return (
        <Menu.Root defaultOpen>
          <Menu.Trigger>View</Menu.Trigger>
          <Menu.Body>
            <Menu.CheckboxItem checked={on} onCheckedChange={() => setOn(false)}>
              Show hidden files
            </Menu.CheckboxItem>
          </Menu.Body>
        </Menu.Root>
      );
    }
    render(<Refusing />);
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: "Show hidden files" }));
    expect(screen.getByRole("menuitemcheckbox", { name: "Show hidden files" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });
});

describe("Context menu", () => {
  component Row() {
    return (
      <ContextMenu.Root>
        <ContextMenu.Trigger>Invoice 2026-04</ContextMenu.Trigger>
        <ContextMenu.Body aria-label="Row actions">
          <ContextMenu.Item>Rename…</ContextMenu.Item>
          <ContextMenu.Item>Delete</ContextMenu.Item>
        </ContextMenu.Body>
      </ContextMenu.Root>
    );
  }

  it("opens from the keyboard with Shift+F10, and lands on the first item", () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    // Reachable at all: a command that only a right-click can reach is a
    // command a keyboard cannot reach, which is WCAG 2.1.1.
    expect(trigger).toHaveAttribute("tabindex", "0");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Rename…" })).toHaveFocus();
  });

  it("opens from the ContextMenu key as well", () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Rename…" })).toHaveFocus();
  });

  it("keeps the browser's own menu away and opens where the pointer was", () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    // `false` is "prevented": the platform's Back/Reload menu would otherwise
    // cover the one the page has for what the reader pressed on.
    expect(fireEvent.contextMenu(trigger, { clientX: 220, clientY: 140 })).toBe(false);
    const menu = screen.getByRole("menu");
    measure(menu, { height: 60, left: 0, top: 0, width: 120 });
    fireEvent.scroll(document);
    // At the point, not against the row: a menu that appeared at the top-left
    // corner of a table row is a menu the reader has to go and find.
    expect(menu.style.position).toBe("fixed");
    expect(menu.style.left).toBe("220px");
    expect(menu.style.top).toBe("140px");
  });

  it("goes against the trigger when the keyboard opened it", () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    measure(trigger, { height: 40, left: 100, top: 200, width: 300 });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu");
    measure(menu, { height: 60, left: 0, top: 0, width: 120 });
    fireEvent.scroll(document);
    // There is no pointer, so the menu belongs where the reader's focus is.
    expect(menu.style.top).toBe("240px");
    expect(menu.style.left).toBe("100px");
  });

  it("is named by the caller rather than after the row it hangs off", () => {
    render(<Row />);
    fireEvent.contextMenu(screen.getByText("Invoice 2026-04"), { clientX: 10, clientY: 10 });
    const menu = screen.getByRole("menu");
    expect(accessibleName(menu)).toBe("Row actions");
    // Naming it after the trigger would announce the whole row as the menu's
    // name, which is why `ContextMenu.Trigger` registers itself as the thing
    // focus returns to and not as a label.
    expect(menu).not.toHaveAttribute("aria-labelledby");
    expect(danglingReferences()).toEqual([]);
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "F10", shiftKey: true });
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBe(null);
    expect(trigger).toHaveFocus();
  });

  it("lets a caller take the trigger out of the tab order", () => {
    render(
      <ContextMenu.Root>
        <ContextMenu.Trigger tabIndex={-1}>
          <button type="button">Invoice 2026-04</button>
        </ContextMenu.Trigger>
        <ContextMenu.Body aria-label="Row actions">
          <ContextMenu.Item>Rename…</ContextMenu.Item>
        </ContextMenu.Body>
      </ContextMenu.Root>,
    );
    // The escape hatch the module header offers a caller whose trigger already
    // contains something focusable — two hundred rows is otherwise two hundred
    // extra stops. A `tabIndex` written after the caller's spread would win
    // over it silently, which is a documented promise that does nothing.
    const trigger = screen.getByRole("button", { name: "Invoice 2026-04" }).parentElement;
    expect(trigger).toHaveAttribute("tabindex", "-1");
    // And the keys still arrive, because the handler is on the trigger and the
    // event bubbles up to it from whatever the caller put inside.
    screen.getByRole("button", { name: "Invoice 2026-04" }).focus();
    fireEvent.keyDown(screen.getByRole("button", { name: "Invoice 2026-04" }), {
      key: "F10",
      shiftKey: true,
    });
    expect(screen.getByRole("menuitem", { name: "Rename…" })).toHaveFocus();
  });

  it("still has the keyboard map of a menu inside it", async () => {
    render(<Row />);
    const trigger = screen.getByText("Invoice 2026-04");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "F10", shiftKey: true });
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Rename…" })).toHaveFocus();
  });
});

describe("Menubar", () => {
  component Bar(direction?: "ltr" | "rtl" = "ltr") {
    return (
      <div dir={direction}>
        <Menubar.Root aria-label="Main">
          <Menubar.Menu value="file">
            <Menubar.Trigger>File</Menubar.Trigger>
            <Menubar.Body>
              <Menubar.Item>New</Menubar.Item>
              <Menubar.Item>Open</Menubar.Item>
            </Menubar.Body>
          </Menubar.Menu>
          <Menubar.Menu value="edit">
            <Menubar.Trigger>Edit</Menubar.Trigger>
            <Menubar.Body>
              <Menubar.Item>Undo</Menubar.Item>
            </Menubar.Body>
          </Menubar.Menu>
          <Menubar.Menu value="view">
            <Menubar.Trigger>View</Menubar.Trigger>
            <Menubar.Body>
              <Menubar.Item>Zoom in</Menubar.Item>
            </Menubar.Body>
          </Menubar.Menu>
        </Menubar.Root>
      </div>
    );
  }

  /** How many menus are showing, which must never be two. */
  const openMenus = () => screen.queryAllByRole("menu").length;

  it("takes one stop in the tab order for the whole bar", () => {
    render(<Bar />);
    expect(accessibleName(screen.getByRole("menubar"))).toBe("Main");
    const stops = screen
      .getAllByRole("menuitem")
      .filter((trigger) => trigger.getAttribute("tabindex") === "0");
    // Six menus behind six tab presses is what makes an application menubar
    // something a keyboard user goes around rather than through.
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("File");
  });

  it("moves between the menus with the arrows and the ends", async () => {
    render(<Bar />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("menuitem", { name: "Edit" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "View" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("menuitem", { name: "File" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "File" })).toHaveFocus();
  });

  it("moves the tab stop with the focus", async () => {
    render(<Bar />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    const stops = screen
      .getAllByRole("menuitem")
      .filter((trigger) => trigger.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("Edit");
  });

  it("opens a menu onto its first item, and onto its last for ArrowUp", async () => {
    render(<Bar />);
    const file = screen.getByRole("menuitem", { name: "File" });
    file.focus();
    fireEvent.keyDown(file, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "New" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "File" }), { key: "ArrowUp" });
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });

  it("walks File to Edit to View with a menu open, and never shows two", async () => {
    render(<Bar />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(openMenus()).toBe(1);
    expect(screen.getByRole("menuitem", { name: "New" })).toHaveFocus();

    await userEvent.keyboard("{ArrowRight}");
    // Closed and reopened as one assignment, so there is never a frame with two
    // of them — the part of a menubar that is always missing.
    expect(openMenus()).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Undo" })).toHaveFocus();

    await userEvent.keyboard("{ArrowRight}");
    expect(openMenus()).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Zoom in" })).toHaveFocus();

    await userEvent.keyboard("{ArrowLeft}");
    expect(openMenus()).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Undo" })).toHaveFocus();
  });

  it("closes on Escape and leaves focus on the trigger, in the bar", async () => {
    render(<Bar />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Escape}");
    expect(openMenus()).toBe(0);
    expect(screen.getByRole("menuitem", { name: "File" })).toHaveFocus();
  });

  it("says what a trigger controls only while there is a menu to control", async () => {
    render(<Bar />);
    const file = screen.getByRole("menuitem", { name: "File" });
    expect(file).toHaveAttribute("aria-haspopup", "menu");
    expect(file).toHaveAttribute("aria-expanded", "false");
    expect(file).not.toHaveAttribute("aria-controls");
    await userEvent.click(file);
    expect(screen.getByRole("menuitem", { name: "File" })).toHaveAttribute("aria-expanded", "true");
    expect(danglingReferences()).toEqual([]);
  });

  it("mirrors the bar's arrows in a right-to-left page", async () => {
    render(<Bar direction="rtl" />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    // The inline axis, so the key that means "the next menu" is the one
    // pointing the way the page reads.
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("menuitem", { name: "Edit" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("menuitem", { name: "File" })).toHaveFocus();
  });

  it("leaves the keys inside a menu to the menu", async () => {
    render(<Bar />);
    screen.getByRole("menuitem", { name: "File" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    // `Home` in an open menu is that menu's first item, not the bar's first
    // menu: `Menu.Body` claims it and stops it before the bar hears it.
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "New" })).toHaveFocus();
    expect(openMenus()).toBe(1);
  });
});

describe("right to left", () => {
  // The bug this block exists for is invisible twice over: the page renders
  // identically, the pointer still works, and the reading order is still right
  // — only the keyboard walks backwards. In a right-to-left page the first item
  // of a row is the rightmost one, so `ArrowLeft` is the reader's "next", and
  // every set in this package took its answer from one hard-coded pair of keys.
  //
  // The LTR cases above are the other half of the evidence: if they still pass
  // and these do too, the mirroring is conditional rather than swapped.

  component Sections() {
    return (
      <Tabs.Root defaultValue="one">
        <Tabs.List aria-label="Sections">
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
          <Tabs.Tab value="three">Three</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first panel</Tabs.Panel>
        <Tabs.Panel value="two">second panel</Tabs.Panel>
        <Tabs.Panel value="three">third panel</Tabs.Panel>
      </Tabs.Root>
    );
  }

  component Export() {
    return (
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item>Open</Menu.Item>
          <Menu.Sub>
            <Menu.SubTrigger>Export</Menu.SubTrigger>
            <Menu.Body>
              <Menu.Item>PNG</Menu.Item>
              <Menu.Item>SVG</Menu.Item>
            </Menu.Body>
          </Menu.Sub>
        </Menu.Body>
      </Menu.Root>
    );
  }

  it("walks a horizontal tab list the way the reader reads", async () => {
    render(
      <div dir="rtl">
        <Sections />
      </div>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
  });

  it("leaves Home and End naming the first and last tab in reading order", async () => {
    render(
      <div dir="rtl">
        <Sections />
      </div>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    // Unmirrored on purpose: `Home` is the first tab a reader reads, which in
    // an RTL row is the rightmost one, and `moveTo` already walks the document
    // in reading order.
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("tabpanel").textContent).toBe("third panel");
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("tabpanel").textContent).toBe("first panel");
  });

  it("reads a direction a caller wrote in CSS rather than with the attribute", async () => {
    render(
      <div style={{ direction: "rtl" }}>
        <Sections />
      </div>,
    );
    // No `dir` anywhere, so this is the computed style answering — which is
    // the half of `directionOf` an attribute walk on its own would miss.
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
  });

  it("lets a left-to-right island inside a right-to-left page keep its own keys", async () => {
    render(
      <div dir="rtl">
        <div dir="ltr">
          <Sections />
        </div>
      </div>,
    );
    // `closest` stops at the nearest ancestor carrying a `dir`, so the island
    // is not dragged along by the page around it.
    await userEvent.click(screen.getByRole("tab", { name: "One" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tabpanel").textContent).toBe("second panel");
  });

  it("opens a submenu with the arrow that points at it", async () => {
    render(
      <div dir="rtl">
        <Export />
      </div>,
    );
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    // A submenu opens onto the inline end, which in RTL is the left. Pressing
    // the key aimed at it used to close the menu the reader was standing in.
    fireEvent.keyDown(trigger, { key: "ArrowLeft" });
    expect(screen.getAllByRole("menu").length).toBe(2);
    expect(screen.getByRole("menuitem", { name: "PNG" })).toHaveFocus();
  });

  it("closes a submenu with the arrow that points away from it", async () => {
    render(
      <div dir="rtl">
        <Export />
      </div>,
    );
    const trigger = screen.getByRole("menuitem", { name: "Export" });
    act(() => trigger.focus());
    fireEvent.keyDown(trigger, { key: "ArrowLeft" });
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getAllByRole("menu").length).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Export" })).toHaveFocus();
  });

  it("leaves the vertical arrows alone, because the page still runs downwards", async () => {
    render(
      <div dir="rtl">
        <Export />
      </div>,
    );
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Export" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  });
});

describe("Combobox", () => {
  const FRUIT = ["Apple", "Apricot", "Banana", "Cherry"];

  component Example(disabledOption?: string) {
    const [query, setQuery] = useState("");
    const shown = FRUIT.filter((each) => each.toLowerCase().startsWith(query.toLowerCase()));
    return (
      <Combobox.Root inputValue={query} onInputValueChange={setQuery}>
        <Combobox.Label>Fruit</Combobox.Label>
        <Combobox.Input />
        <Combobox.List>
          {shown.map((each) => (
            <Combobox.Option disabled={each === disabledOption} key={each} value={each}>
              {each}
            </Combobox.Option>
          ))}
        </Combobox.List>
        <Combobox.Empty>Nothing matched.</Combobox.Empty>
        <Combobox.Status />
      </Combobox.Root>
    );
  }

  it("announces itself as a combobox with a list, before anything is open", () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(input).not.toHaveAttribute("aria-controls");
    expect(input).not.toHaveAttribute("aria-activedescendant");
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("takes its name from its label, and names the list the same way", async () => {
    render(<Example />);
    expect(screen.getByLabelText("Fruit")).toBe(screen.getByRole("combobox"));
    await userEvent.type(screen.getByRole("combobox"), "a");
    const list = screen.getByRole("listbox");
    expect(document.getElementById(list.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Fruit",
    );
  });

  it("opens as the reader types, and names the list it controls", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "ap");
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(input.getAttribute("aria-controls")).toBe(
      screen.getByRole("listbox").getAttribute("id"),
    );
    expect(screen.getAllByRole("option").length).toBe(2);
    expect(danglingReferences()).toEqual([]);
  });

  it("moves a second cursor through the list without moving focus", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    await userEvent.keyboard("{ArrowDown}");
    // The reader is still typing, so real focus must not move. The screen
    // reader is told which option is current through `aria-activedescendant`
    // instead — the half that a highlight drawn in CSS does not do.
    expect(input).toHaveFocus();
    const active = input.getAttribute("aria-activedescendant") ?? "";
    expect(document.getElementById(active)?.textContent).toBe("Apple");
  });

  it("wraps at the ends of the list", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "ap");
    await userEvent.keyboard("{ArrowUp}");
    const active = input.getAttribute("aria-activedescendant") ?? "";
    expect(document.getElementById(active)?.textContent).toBe("Apricot");
  });

  it("opens without choosing anything on Alt+ArrowDown", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    input.focus();
    fireEvent.keyDown(input, { key: "ArrowDown", altKey: true });
    expect(input).toHaveAttribute("aria-expanded", "true");
    // Looking at the options is not the same as picking one.
    expect(input).not.toHaveAttribute("aria-activedescendant");
  });

  it("closes on Alt+ArrowUp", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    fireEvent.keyDown(input, { key: "ArrowUp", altKey: true });
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("takes the active option on Enter and closes", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("combobox")).toHaveValue("Apple");
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("marks the chosen option as selected", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Enter}");
    // Back to a query that shows both, so the selection can be told apart from
    // the filter: `aria-selected` is what a reader is told about the option
    // they already chose, and it has to survive the field being retyped.
    await userEvent.clear(input);
    await userEvent.type(input, "ap");
    expect(screen.getByRole("option", { name: "Apple" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: "Apricot" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("leaves Enter to the form when nothing is highlighted", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    // Not prevented, so a combobox inside a form still submits it.
    expect(fireEvent.keyDown(input, { key: "Enter" })).toBe(true);
  });

  it("closes on Escape, and clears the field on the next one", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "ap");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("combobox")).toHaveValue("ap");
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("combobox")).toHaveValue("");
  });

  it("closes on Tab without taking the highlight", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    await userEvent.keyboard("{ArrowDown}");
    expect(fireEvent.keyDown(input, { key: "Tab" })).toBe(true);
    expect(screen.queryByRole("listbox")).toBe(null);
    // A list that commits whatever happened to be highlighted turns a keystroke
    // meant to leave the field into an edit.
    expect(screen.getByRole("combobox")).toHaveValue("a");
  });

  it("leaves Home and End to the text cursor", async () => {
    render(<Example />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "a");
    // A combobox that steals them has made its own text field harder to edit
    // than a plain input.
    expect(fireEvent.keyDown(input, { key: "Home" })).toBe(true);
    expect(input).not.toHaveAttribute("aria-activedescendant");
  });

  it("takes an option that is clicked, and gives the field back its focus", async () => {
    render(<Example />);
    await userEvent.type(screen.getByRole("combobox"), "ap");
    await userEvent.click(screen.getByRole("option", { name: "Apricot" }));
    expect(screen.getByRole("combobox")).toHaveValue("Apricot");
    expect(screen.getByRole("combobox")).toHaveFocus();
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("closes on a press outside it", async () => {
    render(<Example />);
    await userEvent.type(screen.getByRole("combobox"), "a");
    fireEvent.pointerDown(bodyOf());
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("says how many options matched, in a region that was already there", async () => {
    render(<Example />);
    // The live region is in the document before the list is, on purpose: one
    // added in the same commit as its content is usually not announced at all.
    expect(screen.getByRole("status")).toBeInTheDocument();
    await userEvent.type(screen.getByRole("combobox"), "ap");
    expect(screen.getByRole("status").textContent).toBe("2 results available.");
    await userEvent.type(screen.getByRole("combobox"), "p");
    expect(screen.getByRole("status").textContent).toBe("1 result available.");
  });

  it("says when nothing matched, and shows the empty state", async () => {
    render(<Example />);
    await userEvent.type(screen.getByRole("combobox"), "zz");
    expect(screen.getByRole("status").textContent).toBe("No results available.");
    expect(screen.getByText("Nothing matched.")).toBeInTheDocument();
    // The empty state is beside the list, not inside it: a listbox may only
    // hold options, and a "no matches" row inside one is announced as an option
    // a reader can choose.
    expect(screen.queryAllByRole("option").length).toBe(0);
  });

  it("steps over a disabled option and still announces it", async () => {
    render(<Example disabledOption="Apple" />);
    const input = screen.getByRole("combobox");
    await userEvent.type(input, "ap");
    expect(screen.getByRole("option", { name: "Apple" })).toHaveAttribute("aria-disabled", "true");
    await userEvent.keyboard("{ArrowDown}");
    const active = input.getAttribute("aria-activedescendant") ?? "";
    expect(document.getElementById(active)?.textContent).toBe("Apricot");
  });
});

describe("Combobox: the active option never outlives the list", () => {
  component Switching() {
    const [wide, setWide] = useState(true);
    return (
      <div>
        <button onClick={() => setWide(false)} type="button">
          Filter
        </button>
        <Combobox.Root defaultOpen>
          <Combobox.Label>Fruit</Combobox.Label>
          <Combobox.Input />
          <Combobox.List>
            {(wide ? ["Apple", "Banana"] : ["Cherry"]).map((each) => (
              <Combobox.Option key={each} value={each}>
                {each}
              </Combobox.Option>
            ))}
          </Combobox.List>
        </Combobox.Root>
      </div>
    );
  }

  it("drops the highlight when the option it named is filtered away", async () => {
    render(<Switching />);
    const input = screen.getByRole("combobox");
    input.focus();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input).toHaveAttribute("aria-activedescendant");

    await userEvent.click(screen.getByRole("button", { name: "Filter" }));
    // Otherwise `aria-activedescendant` names an id that has left the document,
    // and a screen reader announces nothing where it used to announce the
    // current option.
    expect(input).not.toHaveAttribute("aria-activedescendant");
    expect(danglingReferences()).toEqual([]);
  });
});

describe("Combobox: option groups", () => {
  // The same four claims `Select: option groups` makes, because they are the
  // same two parts — and one more that only a combobox can get wrong. A select
  // announces nothing about how many options there are; a combobox does, out of
  // a live region, and a heading counted as a result tells a reader who typed
  // two letters that five things matched when three did.
  //
  // Until ubugeeei-prod/uf#357 a group here was not a wrong shape, it was a
  // type error: `Combobox.List` declared `renders* ComboboxOption`, so the
  // markup could not be written at all.

  component Example() {
    return (
      <Combobox.Root defaultOpen>
        <Combobox.Label>Country</Combobox.Label>
        <Combobox.Input />
        <Combobox.List>
          <Combobox.Group>
            <Combobox.GroupLabel>Europe</Combobox.GroupLabel>
            <Combobox.Option value="FR">France</Combobox.Option>
            <Combobox.Option value="DE">Germany</Combobox.Option>
          </Combobox.Group>
          <Combobox.Group>
            <Combobox.GroupLabel>Asia</Combobox.GroupLabel>
            <Combobox.Option value="JP">Japan</Combobox.Option>
          </Combobox.Group>
        </Combobox.List>
        <Combobox.Status />
      </Combobox.Root>
    );
  }

  const cursor = (): string | void =>
    document.getElementById(
      screen.getByRole("combobox").getAttribute("aria-activedescendant") ?? "",
    )?.textContent ?? undefined;

  it("names a group after its label", () => {
    render(<Example />);
    const [europe, asia] = screen.getAllByRole("group");
    expect(document.getElementById(europe.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Europe",
    );
    expect(document.getElementById(asia.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Asia",
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("claims no name when there is no label", () => {
    render(
      <Combobox.Root defaultOpen>
        <Combobox.Input />
        <Combobox.List>
          <Combobox.Group>
            <Combobox.Option value="FR">France</Combobox.Option>
          </Combobox.Group>
        </Combobox.List>
      </Combobox.Root>,
    );
    // An `aria-labelledby` naming an id nothing has makes a screen reader
    // announce nothing at all, which is worse than an unnamed group.
    expect(screen.getByRole("group")).not.toHaveAttribute("aria-labelledby");
  });

  it("crosses group boundaries without ever landing on a heading", async () => {
    render(<Example />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(cursor()).toBe("France");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    // Straight from the last option of one group to the first of the next,
    // over the heading between them: `itemsOf` asks for `[role="option"]`
    // whose nearest listbox is this list, and a group is not a listbox.
    expect(cursor()).toBe("Japan");
  });

  it("counts the options in a grouped list and not the headings", () => {
    render(<Example />);
    // Three options under two headings. A count of five would be a live region
    // telling a reader there are two more things here than they can choose.
    expect(screen.getByRole("status").textContent).toBe("3 results available.");
    expect(screen.getAllByRole("option").length).toBe(3);
  });
});

describe("Select", () => {
  component Example(defaultValue?: string | null = null, disabledOption?: string) {
    return (
      <Select.Root defaultValue={defaultValue}>
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Option disabled={disabledOption === "FR"} value="FR">
            France
          </Select.Option>
          <Select.Option disabled={disabledOption === "DE"} value="DE">
            Germany
          </Select.Option>
          <Select.Option disabled={disabledOption === "JP"} value="JP">
            Japan
          </Select.Option>
          <Select.Option disabled={disabledOption === "UY"} value="UY">
            Uruguay
          </Select.Option>
        </Select.List>
      </Select.Root>
    );
  }

  /** The keyboard path into the list, which is what most of these are about. */
  const openFromTheKeyboard = async () => {
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}");
  };

  /** What `aria-activedescendant` currently names, read the way a reader is told it. */
  const cursor = (): string | void =>
    document.getElementById(
      screen.getByRole("combobox").getAttribute("aria-activedescendant") ?? "",
    )?.textContent ?? undefined;

  it("announces itself as a combobox with a listbox, and never as a text field", () => {
    render(<Example />);
    const trigger = screen.getByRole("combobox");
    // The two halves of the pattern differ here and nowhere more visibly: the
    // editable one is a text field the reader types in, and this one is a
    // button. A select announced as a textbox invites a reader to type into
    // something that will never take a character.
    expect(screen.queryByRole("textbox")).toBe(null);
    expect(trigger).toHaveAttribute("aria-haspopup", "listbox");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).not.toHaveAttribute("aria-controls");
    expect(trigger).not.toHaveAttribute("aria-activedescendant");
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("takes its name from its label and leaves its own content to be the value", async () => {
    render(<Example />);
    // `role="combobox"` is not a role that takes its name from its content, and
    // a `<label for>` does not name a `<button>` either — so without the
    // `aria-labelledby` this is a combobox announced as "combobox" and nothing
    // else, while looking perfectly correct in the markup.
    expect(screen.getByRole("combobox", { name: "Country" })).toBeInTheDocument();
    await openFromTheKeyboard();
    const list = screen.getByRole("listbox");
    expect(document.getElementById(list.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Country",
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("opens on Enter, on Space and on ArrowDown", async () => {
    for (const key of ["{Enter}", " ", "{ArrowDown}"]) {
      render(<Example />);
      screen.getByRole("combobox").focus();
      await userEvent.keyboard(key);
      expect(screen.getByRole("combobox")).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByRole("listbox")).toBeInTheDocument();
      cleanup();
    }
  });

  it("closes on Escape without changing the selection", async () => {
    render(<Example defaultValue="DE" />);
    await openFromTheKeyboard();
    await userEvent.keyboard("{ArrowDown}");
    // The cursor has moved off Germany, and Escape must not take what it is on.
    expect(cursor()).toBe("Japan");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("combobox").textContent).toBe("Germany");
  });

  it("keeps focus on the trigger and moves a second cursor", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    // The same promise `Combobox` makes and for the same reason: a highlight
    // drawn in CSS moves the same pixels and tells a screen reader nothing.
    expect(screen.getByRole("combobox")).toHaveFocus();
    expect(cursor()).toBe("France");
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("combobox")).toHaveFocus();
    expect(cursor()).toBe("Germany");
  });

  it("goes to an option by typing its first letters", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    await userEvent.keyboard("ur");
    // The key every hand-written select leaves out, and the one that makes a
    // list of two hundred countries usable at all.
    expect(cursor()).toBe("Uruguay");
  });

  it("opens the list and goes there when the first letter arrives closed", async () => {
    render(<Example />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("j");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(cursor()).toBe("Japan");
  });

  it("jumps to the first and last option with Home and End", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    await userEvent.keyboard("{End}");
    // The exact inverse of the Combobox, which leaves both keys to the text
    // cursor. The pair of tests is the clearest statement of why there are two
    // components rather than one with a flag.
    expect(cursor()).toBe("Uruguay");
    await userEvent.keyboard("{Home}");
    expect(cursor()).toBe("France");
  });

  it("opens onto the option already chosen", async () => {
    render(<Example defaultValue="JP" />);
    await openFromTheKeyboard();
    // A list of two hundred countries opened onto "Afghanistan" when the reader
    // had already chosen Zimbabwe is a list they arrow through twice.
    expect(cursor()).toBe("Japan");
  });

  it("answers Home and End with a position rather than with the selection", async () => {
    render(<Example defaultValue="JP" />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{End}");
    // Those two keys name a position, and answering "the last one" with "the
    // one you already chose" is not an answer to the question that was asked.
    expect(cursor()).toBe("Uruguay");
  });

  it("stops at the ends rather than wrapping", async () => {
    render(<Example />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(cursor()).toBe("Uruguay");
    await userEvent.keyboard("{ArrowDown}");
    // A native menu cycles and a native select stops; the reader's expectation
    // comes from the platform control the widget imitates, which is why this
    // differs from `menu.js` on purpose.
    expect(cursor()).toBe("Uruguay");
  });

  it("takes the option under the cursor on Enter and closes", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("combobox").textContent).toBe("Germany");
    expect(screen.getByRole("combobox")).toHaveFocus();
  });

  it("takes the option under the cursor on Tab and moves on", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    await userEvent.keyboard("{ArrowDown}");
    // The opposite of what `Combobox` does with this key, and deliberately.
    // There is nothing typed here to lose: moving the cursor *is* the act of
    // choosing, and a select that discarded it on Tab would be the only select
    // on the machine that did. Not prevented, so focus still moves on.
    expect(fireEvent.keyDown(screen.getByRole("combobox"), { key: "Tab" })).toBe(true);
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("combobox").textContent).toBe("Germany");
  });

  it("moving the cursor does not change the value", async () => {
    const onValueChange = fn();
    render(
      <Select.Root onValueChange={onValueChange}>
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Option value="FR">France</Select.Option>
          <Select.Option value="DE">Germany</Select.Option>
        </Select.List>
      </Select.Root>,
    );
    await openFromTheKeyboard();
    await userEvent.keyboard("{ArrowDown}{ArrowUp}{ArrowDown}");
    // Selection-follows-focus is what a native select does on Windows, and
    // copying it here would fire the caller's validation, form store or server
    // mutation once per arrow press.
    expect(onValueChange.mock.calls.length).toBe(0);
    await userEvent.keyboard("{Enter}");
    expect(onValueChange.mock.calls.length).toBe(1);
  });

  it("marks the chosen option as selected", async () => {
    render(<Example defaultValue="JP" />);
    await openFromTheKeyboard();
    expect(screen.getByRole("option", { name: "Japan" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: "France" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("steps over a disabled option and still announces it", async () => {
    render(<Example disabledOption="DE" />);
    await openFromTheKeyboard();
    // Announced, not removed: a reader can tell the option exists and is
    // unavailable, rather than finding a gap where it used to be.
    expect(screen.getByRole("option", { name: "Germany" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await userEvent.keyboard("{ArrowDown}");
    expect(cursor()).toBe("Japan");
  });

  it("opens with no cursor at all on Alt+ArrowDown", async () => {
    render(<Example />);
    const trigger = screen.getByRole("combobox");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown", altKey: true });
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    // Looking at the options is not the same as moving among them.
    expect(trigger).not.toHaveAttribute("aria-activedescendant");
  });

  it("takes an option that is clicked and leaves focus on the trigger", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    await userEvent.click(screen.getByRole("option", { name: "Uruguay" }));
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("combobox").textContent).toBe("Uruguay");
    // Without the option's `pointerdown` guard the trigger blurs, and the next
    // keystroke arrives at the document instead of at this widget.
    expect(screen.getByRole("combobox")).toHaveFocus();
  });

  it("closes on a press outside it", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    fireEvent.pointerDown(bodyOf());
    expect(screen.queryByRole("listbox")).toBe(null);
  });

  it("never names an option that has left the list", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    expect(screen.getByRole("combobox")).toHaveAttribute("aria-activedescendant");
    await userEvent.keyboard("{Escape}");
    // Closed, the cursor names nothing: an `aria-activedescendant` pointing at
    // an id that has left the document makes a reader hear nothing where it
    // used to hear the current option.
    expect(screen.getByRole("combobox")).not.toHaveAttribute("aria-activedescendant");
    expect(danglingReferences()).toEqual([]);
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<Select.Trigger>orphan</Select.Trigger>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Select.Trigger must be rendered inside a Select.Root");
  });
});

describe("Select: what the trigger shows for a value", () => {
  component Example(defaultValue?: string | null = null) {
    return (
      <Select.Root defaultValue={defaultValue}>
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Option value="GB">United Kingdom</Select.Option>
          <Select.Option value="JP">Japan</Select.Option>
        </Select.List>
      </Select.Root>
    );
  }

  it("shows the placeholder while nothing is chosen", () => {
    render(<Example />);
    expect(screen.getByRole("combobox").textContent).toBe("Choose one");
  });

  it("keeps showing the option's own text after the list has closed", async () => {
    render(<Example />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}{Enter}");
    // The options are unmounted with the list, which is exactly when the
    // trigger needs to say what the value is called. A registry that forgot on
    // unmount would blank the trigger the instant the reader chose something.
    expect(screen.queryByRole("option")).toBe(null);
    expect(screen.getByRole("combobox").textContent).toBe("United Kingdom");
  });

  it("shows the value itself for an option it has never rendered", () => {
    render(<Example defaultValue="JP" />);
    // The documented boundary: a value that arrived from outside, for a list
    // that has not been opened. "JP" is wrong and true; the placeholder there
    // would be wrong and confident, telling a reader nothing is chosen when
    // something is. `Select.Value`'s children are the way out.
    expect(screen.getByRole("combobox").textContent).toBe("JP");
  });

  it("lets the caller say what a value is called", () => {
    render(
      <Select.Root defaultValue="JP">
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose one">Japan</Select.Value>
        </Select.Trigger>
        <Select.List>
          <Select.Option value="JP">Japan</Select.Option>
        </Select.List>
      </Select.Root>,
    );
    expect(screen.getByRole("combobox").textContent).toBe("Japan");
  });
});

describe("Select: option groups", () => {
  component Example() {
    return (
      <Select.Root>
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Group>
            <Select.GroupLabel>Europe</Select.GroupLabel>
            <Select.Option value="FR">France</Select.Option>
            <Select.Option value="DE">Germany</Select.Option>
          </Select.Group>
          <Select.Separator />
          <Select.Group>
            <Select.GroupLabel>Asia</Select.GroupLabel>
            <Select.Option value="JP">Japan</Select.Option>
          </Select.Group>
        </Select.List>
      </Select.Root>
    );
  }

  const openFromTheKeyboard = async () => {
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}");
  };

  const cursor = (): string | void =>
    document.getElementById(
      screen.getByRole("combobox").getAttribute("aria-activedescendant") ?? "",
    )?.textContent ?? undefined;

  it("names a group after its label", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    const [europe, asia] = screen.getAllByRole("group");
    expect(document.getElementById(europe.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Europe",
    );
    expect(document.getElementById(asia.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
      "Asia",
    );
    expect(danglingReferences()).toEqual([]);
  });

  it("claims no name when there is no label", async () => {
    render(
      <Select.Root defaultOpen>
        <Select.Trigger>
          <Select.Value />
        </Select.Trigger>
        <Select.List>
          <Select.Group>
            <Select.Option value="FR">France</Select.Option>
          </Select.Group>
        </Select.List>
      </Select.Root>,
    );
    // An `aria-labelledby` naming an id nothing has makes a screen reader
    // announce nothing at all, which is worse than an unnamed group.
    expect(screen.getByRole("group")).not.toHaveAttribute("aria-labelledby");
  });

  it("crosses group boundaries without ever landing on a label", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    expect(cursor()).toBe("France");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    // Straight from the last option of one group to the first of the next,
    // over the label and the separator between them.
    expect(cursor()).toBe("Japan");
  });

  it("keeps the rule between groups out of the accessibility tree", async () => {
    render(<Example />);
    await openFromTheKeyboard();
    // The one place this differs from `Menu.Separator`. A `listbox` may own
    // `option` and `group` and nothing else, so a `role="separator"` inside one
    // is a child ARIA does not allow; the rule is decoration and says so.
    expect(screen.queryByRole("separator")).toBe(null);
    expect(screen.getAllByRole("option").length).toBe(3);
  });
});

describe("what a form submits for a control the browser has never heard of", () => {
  /**
   * Submit the form in `container` and hand back what it carried for `field`.
   *
   * The entry list is built inside the `submit` handler, which is where a
   * Server Action or a `fetch` of the form would build it, and from the
   * document's own `FormData` rather than the global one — the suite runs on
   * Node, whose `FormData` has no constructor that takes an element, and the
   * document is happy-dom's.
   */
  const submitted = (container: mixed, field: string): mixed => {
    let carried: mixed = null;
    const form: $FlowFixMe = (container as $FlowFixMe).querySelector("form");
    const FormData: $FlowFixMe = form.ownerDocument.defaultView.FormData;
    form.addEventListener("submit", (event: $FlowFixMe) => {
      event.preventDefault();
      carried = new FormData(form).get(field);
    });
    fireEvent.submit(form);
    return carried;
  };

  it("submits the value and not the label", () => {
    const { container } = render(
      <form>
        <Select.Root defaultValue="GB" name="country">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    // The bug this exists for: the reader chose "United Kingdom" and the server
    // was waiting for `GB`. A `div` wearing a role is not a listed element, so
    // a form collected nothing at all for it until there was a control to find.
    expect(submitted(container, "country")).toBe("GB");
  });

  it("submits what the reader chose, not what it started as", async () => {
    const { container } = render(
      <form>
        <Select.Root name="country">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
            <Select.Option value="JP">Japan</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(submitted(container, "country")).toBe("JP");
  });

  it("carries the field even when the reader chose nothing", () => {
    const { container } = render(
      <form>
        <Select.Root name="country">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    // A key missing from the payload and a key present and empty are different
    // questions to a server, and this is the second one.
    expect(submitted(container, "country")).toBe("");
  });

  it("submits nothing for a select nobody named", () => {
    const { container } = render(
      <form>
        <Select.Root defaultValue="GB">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    // A select driving a filter has nothing to submit, and a field the caller
    // never named is not one this package should invent.
    expect(submitted(container, "country")).toBe(null);
  });

  it("submits nothing for a select that is disabled", () => {
    const { container } = render(
      <form>
        <Select.Root defaultValue="GB" disabled name="country">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    // What a native `<select disabled>` does, and what a caller who disabled
    // the widget expects.
    expect(submitted(container, "country")).toBe(null);
  });

  it("submits the combobox's value and not the text in its field", async () => {
    const { container } = render(
      <form>
        <Combobox.Root defaultOpen name="country">
          <Combobox.Label>Country</Combobox.Label>
          <Combobox.Input />
          <Combobox.List>
            <Combobox.Option value="GB">United Kingdom</Combobox.Option>
          </Combobox.List>
        </Combobox.Root>
      </form>,
    );
    await userEvent.click(screen.getByRole("option", { name: "United Kingdom" }));
    // The field now reads "United Kingdom", which is the label. The same hole
    // as the Select's and the same answer.
    expect(screen.getByRole("combobox")).toHaveValue("United Kingdom");
    expect(submitted(container, "country")).toBe("GB");
  });

  it("never puts a second combobox in the accessibility tree", () => {
    render(
      <form>
        <Select.Root defaultValue="GB" name="country">
          <Select.Label>Country</Select.Label>
          <Select.Trigger>
            <Select.Value placeholder="Choose one" />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="GB">United Kingdom</Select.Option>
          </Select.List>
        </Select.Root>
      </form>,
    );
    // The reason the hidden control is an `<input type="hidden">` and not a
    // concealed `<select>`: a real one is focusable, so a reader tabbing in
    // hears the styled combobox and then a second, invisible one with the same
    // options — and `aria-hidden` on a focusable element is itself the
    // violation it was reached for to avoid.
    expect(screen.getAllByRole("combobox").length).toBe(1);
    expect(screen.queryAllByRole("listbox").length).toBe(0);
  });
});

describe("one Escape is one dismissal", () => {
  it("closes a menu inside a dialog without closing the dialog", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Settings</Dialog.Title>
          <Menu.Root>
            <Menu.Trigger>Theme</Menu.Trigger>
            <Menu.Body>
              <Menu.Item>Light</Menu.Item>
            </Menu.Body>
          </Menu.Root>
        </Dialog.Body>
      </Dialog.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Theme" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBe(null);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("closes a combobox list inside a dialog without closing the dialog", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Settings</Dialog.Title>
          <Combobox.Root>
            <Combobox.Label>Fruit</Combobox.Label>
            <Combobox.Input />
            <Combobox.List>
              <Combobox.Option value="apple">Apple</Combobox.Option>
            </Combobox.List>
          </Combobox.Root>
        </Dialog.Body>
      </Dialog.Root>,
    );
    await userEvent.type(screen.getByRole("combobox"), "a");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("closes a popover inside a dialog without closing the dialog", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Settings</Dialog.Title>
          <Popover.Root>
            <Popover.Trigger>Filters</Popover.Trigger>
            <Popover.Body>
              <button type="button">Only mine</button>
            </Popover.Body>
          </Popover.Root>
        </Dialog.Body>
      </Dialog.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    await userEvent.keyboard("{Escape}");
    // Two things with `role="dialog"` were open and one Escape closed the
    // inner one; the dialog behind it is the reader's place in the page.
    expect(screen.queryByRole("button", { name: "Only mine" })).toBe(null);
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
  });

  it("closes a tooltip inside a dialog without closing the dialog", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Settings</Dialog.Title>
          <Tooltip.Root openDelay={0}>
            <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
            <Tooltip.Body>Bold</Tooltip.Body>
          </Tooltip.Root>
        </Dialog.Body>
      </Dialog.Root>,
    );
    fireEvent.pointerEnter(screen.getByRole("button", { name: "Bold" }));
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    // The tooltip answers the key from the document, because it holds no
    // focus — and having answered it, the dialog must not answer it as well.
    expect(screen.queryByRole("tooltip")).toBe(null);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("closes a select's list inside a dialog without closing the dialog", async () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Settings</Dialog.Title>
          <Select.Root>
            <Select.Label>Theme</Select.Label>
            <Select.Trigger>
              <Select.Value placeholder="Choose one" />
            </Select.Trigger>
            <Select.List>
              <Select.Option value="light">Light</Select.Option>
            </Select.List>
          </Select.Root>
        </Dialog.Body>
      </Dialog.Root>,
    );
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).toBe(null);
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    // And the second Escape, with the list already closed, belongs to the
    // dialog: a select that swallowed it would trap a reader who opened a list
    // by accident inside a modal they now cannot dismiss.
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBe(null);
  });
});

describe("where an anchored overlay goes", () => {
  // The arithmetic, called with numbers. This DOM computes no layout at all, so
  // a test that went through a component could only assert *that* a position
  // was applied; these are the only assertions in this file that can say the
  // position was the right one. A real-layout check belongs to `@uniflowed/vrt`.

  /** A viewport with room in it, so a case that is not about a collision has none. */
  const room: Rect = { height: 800, width: 1000, x: 0, y: 0 };

  /**
   * A placement request with defaults, so each case names only what it is about.
   */
  function place(request: {
    readonly anchor: Rect,
    readonly overlay: Rect,
    readonly align?: Align,
    readonly alignOffset?: number,
    readonly avoidCollisions?: boolean,
    readonly collisionPadding?: number,
    readonly direction?: "ltr" | "rtl",
    readonly side?: Side,
    readonly sideOffset?: number,
    readonly viewport?: Rect,
  }): Placement {
    return placeOverlay({
      align: request.align ?? "center",
      alignOffset: request.alignOffset ?? 0,
      anchor: request.anchor,
      avoidCollisions: request.avoidCollisions ?? true,
      collisionPadding: request.collisionPadding ?? 0,
      direction: request.direction ?? "ltr",
      overlay: request.overlay,
      side: request.side ?? "bottom",
      sideOffset: request.sideOffset ?? 0,
      viewport: request.viewport ?? room,
    });
  }

  /** A trigger in the middle of the page, with room on every side of it. */
  const trigger: Rect = { height: 40, width: 200, x: 400, y: 300 };

  it("puts it against the side it was asked for", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    expect(place({ anchor: trigger, overlay, side: "bottom" }).y).toBe(340);
    expect(place({ anchor: trigger, overlay, side: "top" }).y).toBe(240);
    expect(place({ anchor: trigger, overlay, side: "right" }).x).toBe(600);
    expect(place({ anchor: trigger, overlay, side: "left" }).x).toBe(250);
  });

  it("leaves the gap it was asked for", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    expect(place({ anchor: trigger, overlay, side: "bottom", sideOffset: 8 }).y).toBe(348);
    expect(place({ anchor: trigger, overlay, side: "top", sideOffset: 8 }).y).toBe(232);
  });

  it("aligns to the start, the middle or the end of the trigger", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    expect(place({ align: "start", anchor: trigger, overlay }).x).toBe(400);
    // 400 + 100 − 75: the middle of the trigger, less half the overlay.
    expect(place({ align: "center", anchor: trigger, overlay }).x).toBe(425);
    expect(place({ align: "end", anchor: trigger, overlay }).x).toBe(450);
  });

  it("aligns along the other axis when the overlay is beside the trigger", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    expect(place({ align: "start", anchor: trigger, overlay, side: "right" }).y).toBe(300);
    expect(place({ align: "end", anchor: trigger, overlay, side: "right" }).y).toBe(280);
  });

  it("flips to the opposite side when there is no room, and says which", () => {
    // A trigger at the bottom of the page, and an overlay taller than what is
    // left under it. This is the menu that opens upwards.
    const anchor = { height: 40, width: 200, x: 400, y: 740 };
    const placement = place({ anchor, overlay: { height: 200, width: 150, x: 0, y: 0 } });
    expect(placement.side).toBe("top");
    expect(placement.y).toBe(540);
  });

  it("stays where it was asked when it fits", () => {
    const placement = place({ anchor: trigger, overlay: { height: 60, width: 150, x: 0, y: 0 } });
    expect(placement.side).toBe("bottom");
    expect(placement.shift).toBe(0);
  });

  it("slides along the trigger rather than off the edge of the page", () => {
    // The case a flip cannot answer, and the reason #345 is not "no": the
    // overlay is wider than the room on *either* side of a trigger near the
    // right edge, so flipping the alignment moves it and it still overflows.
    // 900 + 300 is 1200 in a viewport 1000 wide; 1000 − 300 is where it goes.
    const anchor = { height: 40, width: 80, x: 900, y: 300 };
    const placement = place({
      align: "start",
      anchor,
      overlay: { height: 60, width: 300, x: 0, y: 0 },
    });
    expect(placement.x).toBe(700);
    expect(placement.shift).toBe(-200);
    // The alignment it reports is still the one that was asked for: a
    // `data-align` that changed under a stylesheet would move the arrow to the
    // wrong end of an overlay that only slid.
    expect(placement.align).toBe("start");
    expect(placement.side).toBe("bottom");
  });

  it("slides the other way at the other edge", () => {
    const anchor = { height: 40, width: 80, x: 20, y: 300 };
    const placement = place({
      align: "end",
      anchor,
      collisionPadding: 8,
      overlay: { height: 60, width: 300, x: 0, y: 0 },
    });
    expect(placement.x).toBe(8);
    expect(placement.shift).toBe(208);
  });

  it("slides an overlay beside its trigger too", () => {
    const anchor = { height: 40, width: 80, x: 400, y: 760 };
    const placement = place({
      align: "start",
      anchor,
      overlay: { height: 200, width: 150, x: 0, y: 0 },
      side: "right",
    });
    // Beside the trigger, so the cross axis is the vertical one: 800 − 200.
    expect(placement.y).toBe(600);
    expect(placement.x).toBe(480);
  });

  it("pins an overlay wider than the page to the leading edge", () => {
    // No position satisfies both edges. Pushing it off the far one would hide
    // the end of it; `availableWidth` is what a stylesheet reads to stop it
    // being this wide in the first place.
    const anchor = { height: 40, width: 80, x: 100, y: 300 };
    const placement = place({
      anchor,
      collisionPadding: 8,
      overlay: { height: 60, width: 1200, x: 0, y: 0 },
      viewport: { height: 800, width: 400, x: 0, y: 0 },
    });
    expect(placement.x).toBe(8);
    expect(placement.availableWidth).toBe(384);
  });

  it("takes the side with more room when neither has enough", () => {
    const anchor = { height: 40, width: 80, x: 100, y: 300 };
    const placement = place({
      anchor,
      overlay: { height: 500, width: 150, x: 0, y: 0 },
      viewport: { height: 400, width: 1000, x: 0, y: 0 },
    });
    // Above has 300 and below has 60, and neither fits 500. The reader sees as
    // much of it as the page allows, and `availableHeight` says how much.
    expect(placement.side).toBe("top");
    expect(placement.availableHeight).toBe(300);
  });

  it("says how much room there was on the side it chose", () => {
    const anchor = { height: 40, width: 200, x: 400, y: 660 };
    const placement = place({
      anchor,
      collisionPadding: 10,
      overlay: { height: 60, width: 150, x: 0, y: 0 },
      sideOffset: 4,
    });
    // 800 − 10 − 700 − 4: the page, less the padding, less the trigger's
    // bottom edge, less the gap.
    expect(placement.availableHeight).toBe(86);
    // And across it, which is what an overlay may grow to after sliding.
    expect(placement.availableWidth).toBe(980);
  });

  it("aligns to the reading direction, not to the left", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    // `start` is the right-hand edge of the trigger in a right-to-left page,
    // so the overlay's right edge meets the trigger's: 600 − 150.
    expect(place({ align: "start", anchor: trigger, direction: "rtl", overlay }).x).toBe(450);
    expect(place({ align: "end", anchor: trigger, direction: "rtl", overlay }).x).toBe(400);
    // A right-to-left page still runs top to bottom, so an overlay beside its
    // trigger is unaffected.
    expect(
      place({ align: "start", anchor: trigger, direction: "rtl", overlay, side: "right" }).y,
    ).toBe(300);
  });

  it("nudges along the axis in the direction the page reads", () => {
    const overlay = { height: 60, width: 150, x: 0, y: 0 };
    expect(place({ align: "start", alignOffset: 10, anchor: trigger, overlay }).x).toBe(410);
    expect(
      place({ align: "start", alignOffset: 10, anchor: trigger, direction: "rtl", overlay }).x,
    ).toBe(440);
  });

  it("does nothing at all when it is told not to avoid collisions", () => {
    const anchor = { height: 40, width: 80, x: 900, y: 740 };
    const placement = place({
      align: "start",
      anchor,
      avoidCollisions: false,
      overlay: { height: 200, width: 300, x: 0, y: 0 },
    });
    // Off the bottom and off the right, exactly as asked. A caller who has laid
    // the page out themselves is not second-guessed.
    expect(placement.side).toBe("bottom");
    expect(placement.x).toBe(900);
    expect(placement.y).toBe(780);
    expect(placement.shift).toBe(0);
  });
});

describe("an anchored overlay follows its trigger", () => {
  // What a DOM without layout can be asked. `getBoundingClientRect` answers
  // zero for everything here, so each case stubs the two boxes it is about and
  // then asserts that the position was *recomputed and applied* — which is the
  // half of positioning that can be wrong in a way this suite can see. Whether
  // 240 pixels looks right belongs to `@uniflowed/vrt`.

  component Example() {
    return (
      <Popover.Root>
        <Popover.Trigger>Filters</Popover.Trigger>
        <Popover.Body>
          <button type="button">Only mine</button>
        </Popover.Body>
      </Popover.Root>
    );
  }

  /** Ask for a fresh measurement, the way a scroll anywhere in the page does. */
  const reflow = () => {
    fireEvent.scroll(document);
  };

  it("positions the overlay against its trigger", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
    await userEvent.click(trigger);

    const body = screen.getByRole("dialog");
    measure(body, { height: 60, left: 0, top: 0, width: 120 });
    reflow();

    // Fixed, so the coordinates are the viewport's and an ancestor with
    // `overflow: hidden` does not cut the overlay in half.
    expect(body.style.position).toBe("fixed");
    // Under the trigger, centred on it: 200 + 40, and 100 + 40 − 60.
    expect(body.style.top).toBe("240px");
    expect(body.style.left).toBe("80px");
    expect(body).toHaveAttribute("data-side", "bottom");
    expect(body).toHaveAttribute("data-align", "center");
  });

  it("measures again when the page scrolls under it", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
    await userEvent.click(trigger);
    const body = screen.getByRole("dialog");
    measure(body, { height: 60, left: 0, top: 0, width: 120 });
    reflow();
    expect(body.style.top).toBe("240px");

    // The trigger has moved up the page, which is what a scroll does to it.
    measure(trigger, { height: 40, left: 100, top: 120, width: 80 });
    reflow();
    expect(body.style.top).toBe("160px");
  });

  it("measures again when the window is resized", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
    await userEvent.click(trigger);
    const body = screen.getByRole("dialog");
    measure(body, { height: 60, left: 0, top: 0, width: 120 });
    fireEvent.resize(window);
    expect(body.style.top).toBe("240px");

    measure(trigger, { height: 40, left: 300, top: 200, width: 80 });
    fireEvent.resize(window);
    expect(body.style.left).toBe("280px");
  });

  it("opens upwards when there is no room below, and says so", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    // The window here is 768 tall, so a trigger at 700 has 28 pixels under it.
    measure(trigger, { height: 40, left: 100, top: 700, width: 80 });
    await userEvent.click(trigger);
    const body = screen.getByRole("dialog");
    measure(body, { height: 200, left: 0, top: 0, width: 120 });
    reflow();

    expect(body).toHaveAttribute("data-side", "top");
    expect(body.style.top).toBe("500px");
  });

  it("does not report the last opening's side on the first frame of the next one", async () => {
    // Read during render rather than after the effects have run, because the
    // gap this is about is exactly one commit wide: `PopoverBody` returns
    // `null` while it is closed but stays mounted, so its `useAnchor` state
    // survives, and the commit that reopens it already has `open === true`.
    // The measurement is taken from an effect, which runs after paint — so a
    // stylesheet drawing an arrow from `data-side` draws it from whatever that
    // commit said. `render` + `act` flushes the effect before any assertion on
    // the DOM could see it, which is why this reads the hook.
    const sides: Array<{| readonly open: boolean, readonly side: Side |}> = [];

    component Probe(open: boolean) {
      const anchorRef = React.useRef<HTMLElement | null>(null);
      const overlayRef = React.useRef<HTMLElement | null>(null);
      const anchored = useAnchor({
        align: "center",
        alignOffset: 0,
        anchorRef,
        avoidCollisions: true,
        collisionPadding: 0,
        open,
        overlayRef,
        side: "bottom",
        sideOffset: 0,
      });
      sides.push({ open, side: anchored.side });
      return (
        <div>
          <span ref={anchorRef}>trigger</span>
          {open ? <div ref={overlayRef} data-side={anchored.side} /> : null}
        </div>
      );
    }

    component Host() {
      const [open, setOpen] = useState<boolean>(false);
      return (
        <div>
          <button type="button" onClick={() => setOpen((was) => !was)}>
            Toggle
          </button>
          <Probe open={open} />
        </div>
      );
    }

    render(<Host />);
    const toggle = screen.getByRole("button", { name: "Toggle" });

    // Open with no room below, so it flips and `settled` remembers "top".
    await act(async () => {
      await userEvent.click(toggle);
    });
    const overlay = document.querySelector("[data-side]");
    if (overlay == null) {
      throw new Error("no overlay");
    }
    measure(screen.getByText("trigger"), { height: 40, left: 100, top: 700, width: 80 });
    measure(overlay as $FlowFixMe, { height: 200, left: 0, top: 0, width: 120 });
    fireEvent.scroll(document);
    expect(sides[sides.length - 1].side).toBe("top");

    await act(async () => {
      await userEvent.click(toggle);
    });

    // Room below now. The assertion is on every render of the second opening,
    // including the first, before any effect has measured anything.
    measure(screen.getByText("trigger"), { height: 40, left: 100, top: 100, width: 80 });
    const before = sides.length;
    await act(async () => {
      await userEvent.click(toggle);
    });
    const reopened = sides.slice(before).filter((frame) => frame.open);
    expect(reopened.length).toBeGreaterThan(0);
    expect(reopened.map((frame) => frame.side)).toEqual(reopened.map(() => "bottom"));
  });

  it("slides along the trigger rather than off the side of the page", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    // The window is 1024 wide; an overlay 400 wide aligned to a trigger at 900
    // would end at 1300, and no flip of the alignment brings it back.
    measure(trigger, { height: 40, left: 900, top: 200, width: 80 });
    await userEvent.click(trigger);
    const body = screen.getByRole("dialog");
    measure(body, { height: 60, left: 0, top: 0, width: 400 });
    reflow();

    expect(body.style.left).toBe("624px");
    // Still centred as far as the stylesheet is concerned: what moved is where
    // it sits, not which end of it the arrow belongs on — which is what
    // `--uf-anchor-shift` is for.
    expect(body).toHaveAttribute("data-align", "center");
    expect(body.style.getPropertyValue("--uf-anchor-shift")).toBe("-116px");
  });

  it("hands a stylesheet the two measurements it cannot make", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    measure(trigger, { height: 40, left: 100, top: 600, width: 240 });
    await userEvent.click(trigger);
    const body = screen.getByRole("dialog");
    measure(body, { height: 60, left: 0, top: 0, width: 120 });
    reflow();

    // The trigger's width, so a popup that must match it can, and the room
    // that was left, so one that would overflow can scroll instead.
    expect(body.style.getPropertyValue("--uf-anchor-trigger-width")).toBe("240px");
    expect(body.style.getPropertyValue("--uf-anchor-available-height")).toBe("128px");
    expect(body.style.getPropertyValue("--uf-anchor-available-width")).toBe("1024px");
  });
});

describe("the three sets are anchored to their triggers", () => {
  // ubugeeei-prod/uf#256's other half. `Popover`, `Tooltip` and `HoverCard` were
  // written against `internal/anchor.js`; `Menu.Body`, `Combobox.List` and
  // `Select.List` came before it and each rendered exactly where it was
  // written — clipped by the first ancestor with `overflow: hidden`, which in a
  // table row or a card is most of the time, and off the bottom of the page
  // when the trigger was near it.
  //
  // What this DOM can be asked is the same as for the popover cases above: that
  // a position was computed and applied, and which side it chose.
  // `getBoundingClientRect` answers zero for everything here, so each case stubs
  // the two boxes it is about. Whether 240 pixels looks right is
  // `@uniflowed/vrt`'s question.

  /** Ask for a fresh measurement, the way a scroll anywhere in the page does. */
  const reflow = () => {
    fireEvent.scroll(document);
  };

  /**
   * A `ResizeObserver` this file can fire by hand.
   *
   * There is none in this DOM, and the case it is for fires nothing else: a
   * trigger that *grows* — a button whose label changed, a field that gained a
   * second line — moves the overlay without a scroll or a window resize
   * happening at all. Written as a plain function rather than a class because
   * `new` on one still produces the returned object, and a capitalised
   * identifier holding a constructor is a React component as far as `uf lint`
   * is concerned.
   */
  const resizeCallbacks: Array<() => void> = [];
  function installResizeObserver(): () => void {
    const host: $FlowFixMe = window;
    const previous = host.ResizeObserver;
    host.ResizeObserver = function (callback: () => void) {
      resizeCallbacks.push(callback);
      return { disconnect: () => {}, observe: () => {} };
    };
    return () => {
      host.ResizeObserver = previous;
      resizeCallbacks.length = 0;
    };
  }

  component Clipped() {
    return (
      <div style={{ overflow: "hidden" }}>
        <Menu.Root>
          <Menu.Trigger>File</Menu.Trigger>
          <Menu.Body>
            <Menu.Item>Open</Menu.Item>
            <Menu.Item>Save</Menu.Item>
          </Menu.Body>
        </Menu.Root>
      </div>
    );
  }

  it("takes a menu out of an ancestor that would clip it, without moving it", async () => {
    render(<Clipped />);
    const trigger = screen.getByRole("button", { name: "File" });
    measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
    await userEvent.click(trigger);

    const menu = screen.getByRole("menu");
    measure(menu, { height: 60, left: 0, top: 0, width: 120 });
    reflow();

    // Fixed, so the `overflow: hidden` above it is not its business: 200 + 40.
    expect(menu.style.position).toBe("fixed");
    expect(menu.style.top).toBe("240px");
    // And still inside that element in the document, which is the half a portal
    // gives up — the reason `dialog.js` refuses to portal, kept here.
    expect(trigger.closest("div")?.contains(menu)).toBe(true);
    // The accessibility properties do not regress when an overlay is
    // positioned: the trigger still names an element that is in the document.
    expect(trigger.getAttribute("aria-controls")).toBe(menu.getAttribute("id"));
    expect(danglingReferences()).toEqual([]);
  });

  it("opens a menu upwards when there is no room below, and says so", async () => {
    render(<Clipped />);
    const trigger = screen.getByRole("button", { name: "File" });
    // The window here is 768 tall, so a trigger at 700 has 28 pixels under it.
    measure(trigger, { height: 40, left: 100, top: 700, width: 80 });
    await userEvent.click(trigger);

    const menu = screen.getByRole("menu");
    measure(menu, { height: 200, left: 0, top: 0, width: 120 });
    reflow();

    expect(menu).toHaveAttribute("data-side", "top");
    expect(menu.style.top).toBe("500px");
    expect(danglingReferences()).toEqual([]);
  });

  it("measures a menu again when the page scrolls under it", async () => {
    render(<Clipped />);
    const trigger = screen.getByRole("button", { name: "File" });
    measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
    await userEvent.click(trigger);
    const menu = screen.getByRole("menu");
    measure(menu, { height: 60, left: 0, top: 0, width: 120 });
    reflow();
    expect(menu.style.top).toBe("240px");

    // Capture, because a scroll event does not bubble: a trigger inside a
    // scrolling panel moves under an overlay that never hears a scroll of its
    // own, which is the case this listener exists for.
    measure(trigger, { height: 40, left: 100, top: 120, width: 80 });
    fireEvent.scroll(trigger.closest("div") ?? document);
    expect(menu.style.top).toBe("160px");
  });

  it("measures a menu again when its trigger changes size", async () => {
    const restore = installResizeObserver();
    try {
      render(<Clipped />);
      const trigger = screen.getByRole("button", { name: "File" });
      measure(trigger, { height: 40, left: 100, top: 200, width: 80 });
      await userEvent.click(trigger);
      const menu = screen.getByRole("menu");
      measure(menu, { height: 60, left: 0, top: 0, width: 120 });
      reflow();
      expect(menu.style.top).toBe("240px");

      // The trigger gained a second line. No scroll, no window resize, and
      // without the observer the menu would sit over the label it belongs to.
      measure(trigger, { height: 80, left: 100, top: 200, width: 80 });
      expect(resizeCallbacks.length).toBeGreaterThan(0);
      act(() => {
        for (const fire of resizeCallbacks) {
          fire();
        }
      });
      expect(menu.style.top).toBe("280px");
    } finally {
      restore();
    }
  });

  it("anchors a combobox list to the field it belongs to", async () => {
    render(
      <div style={{ overflow: "hidden" }}>
        <Combobox.Root>
          <Combobox.Label>Fruit</Combobox.Label>
          <Combobox.Input />
          <Combobox.List>
            <Combobox.Option value="apple">Apple</Combobox.Option>
          </Combobox.List>
        </Combobox.Root>
      </div>,
    );
    const field = screen.getByRole("combobox");
    measure(field, { height: 32, left: 40, top: 300, width: 220 });
    await userEvent.type(field, "a");

    const list = screen.getByRole("listbox");
    measure(list, { height: 120, left: 0, top: 0, width: 220 });
    reflow();

    expect(list.style.position).toBe("fixed");
    // Under the field and aligned to the edge its text starts at, which is what
    // `align="start"` means and the only alignment a list of options can have.
    expect(list.style.top).toBe("332px");
    expect(list.style.left).toBe("40px");
    expect(list).toHaveAttribute("data-align", "start");
    // The measurement a stylesheet cannot make: how wide the field was.
    expect(list.style.getPropertyValue("--uf-anchor-trigger-width")).toBe("220px");
    expect(danglingReferences()).toEqual([]);
  });

  it("anchors a select popup to its trigger and reports its width", async () => {
    render(
      <Select.Root>
        <Select.Label>Country</Select.Label>
        <Select.Trigger>
          <Select.Value placeholder="Choose" />
        </Select.Trigger>
        <Select.List>
          <Select.Option value="gb">United Kingdom</Select.Option>
          <Select.Option value="jp">Japan</Select.Option>
        </Select.List>
      </Select.Root>,
    );
    const trigger = screen.getByRole("combobox", { name: "Country" });
    measure(trigger, { height: 36, left: 500, top: 400, width: 180 });
    await userEvent.click(trigger);

    const list = screen.getByRole("listbox");
    measure(list, { height: 90, left: 0, top: 0, width: 180 });
    reflow();

    expect(list.style.position).toBe("fixed");
    expect(list.style.top).toBe("436px");
    expect(list).toHaveAttribute("data-side", "bottom");
    // A popup narrower than the button it came out of reads as a different
    // control, and this is the number that stops it being one.
    expect(list.style.getPropertyValue("--uf-anchor-trigger-width")).toBe("180px");
    expect(danglingReferences()).toEqual([]);
  });

  it("opens a submenu onto the inline end, which is the left in an Arabic page", async () => {
    render(
      <div dir="rtl">
        <Menu.Root defaultOpen>
          <Menu.Trigger>File</Menu.Trigger>
          <Menu.Body>
            <Menu.Sub defaultOpen>
              <Menu.SubTrigger>Export</Menu.SubTrigger>
              <Menu.Body>
                <Menu.Item>PNG</Menu.Item>
              </Menu.Body>
            </Menu.Sub>
          </Menu.Body>
        </Menu.Root>
      </div>,
    );
    const submenu = screen.getAllByRole("menu")[1];
    const opener = screen.getByRole("menuitem", { name: "Export" });
    measure(opener, { height: 32, left: 400, top: 200, width: 160 });
    measure(submenu, { height: 90, left: 0, top: 0, width: 200 });
    reflow();

    // `submenuKeys` already mirrors the key that opens a submenu; this is the
    // other half of the same sentence, and a hard-coded `side="right"` would
    // have opened it under the reader's own menu.
    expect(submenu).toHaveAttribute("data-side", "left");
    expect(submenu.style.left).toBe("200px");
  });
});

describe("Popover", () => {
  component Example() {
    return (
      <div>
        <p>Behind</p>
        <Popover.Root>
          <Popover.Trigger>Filters</Popover.Trigger>
          <Popover.Body>
            <button type="button">Only mine</button>
            <button type="button">Save</button>
          </Popover.Body>
        </Popover.Root>
        <button type="button">After</button>
      </div>
    );
  }

  it("is closed until it is opened, and says what it controls", async () => {
    render(<Example />);
    expect(screen.queryByRole("dialog")).toBe(null);
    const trigger = screen.getByRole("button", { name: "Filters" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
    // Nothing to control yet, so nothing is named.
    expect(trigger).not.toHaveAttribute("aria-controls");

    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(trigger.getAttribute("aria-controls")).toBe(screen.getByRole("dialog").id);
    expect(danglingReferences()).toEqual([]);
  });

  it("leaves the page behind it alone", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));

    // The inverse of the dialog's own case above, and the assertion that says
    // a popover is not a dialog: the page is still there, still readable and
    // still scrollable.
    const behind = screen.getByText("Behind");
    expect(behind).not.toHaveAttribute("inert");
    expect(behind).not.toHaveAttribute("aria-hidden");
    expect(screen.getByRole("dialog")).not.toHaveAttribute("aria-modal");
    expect(document.body.style.overflow).toBe("");
  });

  it("moves focus into it, and gives it back on Escape", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    await userEvent.click(trigger);
    expect(screen.getByRole("button", { name: "Only mine" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBe(null);
    // Back where the reader was, rather than at the top of the page.
    expect(trigger).toHaveFocus();
  });

  it("lets Tab leave", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    screen.getByRole("button", { name: "Save" }).focus();

    await userEvent.tab();
    // Out, rather than wrapped back to the first control inside. A trap here
    // is a hole in the page: the reader tabbed in and cannot tab out.
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
    // And the popover they left is dismissed rather than abandoned open behind
    // them, because Escape is answered where focus is.
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("closes on a press outside and leaves focus where the reader put it", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    const after = screen.getByRole("button", { name: "After" });

    await userEvent.click(after);
    expect(screen.queryByRole("dialog")).toBe(null);
    // Not dragged back to the trigger: the reader has already moved on.
    expect(after).toHaveFocus();
  });

  it("is named by the button that opened it, unless the caller names it", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Filters" });
    await userEvent.click(trigger);
    expect(screen.getByRole("dialog", { name: "Filters" })).toBeInTheDocument();
  });

  it("keeps the name a caller gave it", () => {
    render(
      <Popover.Root defaultOpen>
        <Popover.Trigger>Filters</Popover.Trigger>
        <Popover.Body aria-label="Filter options">
          <button type="button">Only mine</button>
        </Popover.Body>
      </Popover.Root>,
    );
    // An `aria-labelledby` added on top would win over the caller's label and
    // announce the button's text instead of theirs.
    const body = screen.getByRole("dialog");
    expect(body).not.toHaveAttribute("aria-labelledby");
    expect(body).toHaveAttribute("aria-label", "Filter options");
  });
});

describe("Tooltip", () => {
  afterEach(() => {
    uft.useRealTimers();
  });

  const advance = (millis: number) => {
    act(() => {
      uft.advanceTimersByTime(millis);
    });
  };

  component Example() {
    return (
      <div>
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
          <Tooltip.Body>Bold (⌘B)</Tooltip.Body>
        </Tooltip.Root>
        <button type="button">After</button>
      </div>
    );
  }

  it("waits for the pointer and does not wait for focus", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });

    fireEvent.pointerEnter(trigger);
    advance(699);
    // A pointer crossing a toolbar passes six triggers on its way elsewhere.
    expect(screen.queryByRole("tooltip")).toBe(null);
    advance(1);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    fireEvent.pointerLeave(trigger);
    advance(300);
    expect(screen.queryByRole("tooltip")).toBe(null);

    act(() => {
      trigger.focus();
    });
    // No wait at all: a reader who tabbed here has already said what they want.
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
  });

  it("stays while the pointer travels to it", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerEnter(trigger);
    advance(700);
    const tip = screen.getByRole("tooltip");

    fireEvent.pointerLeave(trigger);
    advance(100);
    fireEvent.pointerEnter(tip);
    advance(10_000);
    // WCAG 2.1 SC 1.4.13, hoverable: the trip across the gap must not take the
    // content away, and this is the clause every hand-written tooltip fails.
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    fireEvent.pointerLeave(tip);
    advance(300);
    expect(screen.queryByRole("tooltip")).toBe(null);
  });

  it("closes on Escape without moving the pointer", () => {
    uft.useFakeTimers();
    render(<Example />);
    fireEvent.pointerEnter(screen.getByRole("button", { name: "Bold" }));
    advance(700);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    // Nothing has focus, so the key is answered on the document — which is why
    // the hand-written version cannot answer it at all.
    fireEvent.keyDown(bodyOf(), { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBe(null);
  });

  it("stays dismissed while the pointer is still resting on the trigger", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerEnter(trigger);
    advance(700);
    fireEvent.keyDown(bodyOf(), { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBe(null);

    // Nothing has moved, so nothing may bring it back: a dismissal the pointer
    // undoes in the next instant is a key that does nothing a reader can see.
    advance(10_000);
    expect(screen.queryByRole("tooltip")).toBe(null);

    // Leaving and coming back is a fresh gesture, and gets a fresh answer.
    fireEvent.pointerLeave(trigger);
    fireEvent.pointerEnter(trigger);
    advance(700);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
  });

  it("describes its trigger only while it is there", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });
    expect(trigger).not.toHaveAttribute("aria-describedby");
    expect(danglingReferences()).toEqual([]);

    fireEvent.pointerEnter(trigger);
    advance(700);
    expect(trigger.getAttribute("aria-describedby")).toBe(screen.getByRole("tooltip").id);
    expect(danglingReferences()).toEqual([]);

    fireEvent.pointerLeave(trigger);
    advance(300);
    expect(trigger).not.toHaveAttribute("aria-describedby");
    expect(danglingReferences()).toEqual([]);
  });

  it("never takes focus", async () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });
    act(() => {
      trigger.focus();
    });
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    await userEvent.tab();
    // The next control in the page, not a stop inside the tooltip: a focusable
    // tooltip is a stop the reader cannot leave the way they expect.
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
    expect(screen.queryByRole("tooltip")).toBe(null);
  });

  it("says nothing about a popup, because it is not one", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerEnter(trigger);
    advance(700);
    // `aria-haspopup` promises something to interact with, and a reader sent to
    // a tooltip finds nothing there.
    expect(trigger).not.toHaveAttribute("aria-haspopup");
    expect(trigger).not.toHaveAttribute("aria-expanded");
  });

  it("does not open on a tap, and does not eat it either", () => {
    uft.useFakeTimers();
    const pressed = fn();
    render(
      <Tooltip.Root>
        <Tooltip.Trigger aria-label="Bold" onClick={pressed}>
          B
        </Tooltip.Trigger>
        <Tooltip.Body>Bold (⌘B)</Tooltip.Body>
      </Tooltip.Root>,
    );
    const trigger = screen.getByRole("button", { name: "Bold" });

    // A touch pointer, which is what a finger arriving looks like. There is no
    // hover on a phone, so opening here would either swallow the tap or show
    // something the next tap dismisses.
    const touch = new Event("pointerenter", { bubbles: false });
    (touch as $FlowFixMe).pointerType = "touch";
    act(() => {
      trigger.dispatchEvent(touch);
    });
    advance(10_000);
    expect(screen.queryByRole("tooltip")).toBe(null);

    fireEvent.click(trigger);
    expect(pressed).toHaveBeenCalledTimes(1);
  });

  it("refuses a trigger the keyboard cannot reach", () => {
    // A tooltip on a `<span>` is one only a mouse can find, and it looks
    // perfect in the markup. This package's answer to that is an error.
    expect(() =>
      render(
        <Tooltip.Root>
          <Tooltip.Trigger render={(props) => <span {...props}>B</span>} />
          <Tooltip.Body>Bold</Tooltip.Body>
        </Tooltip.Root>,
      ),
    ).toThrow(/keyboard can reach/);
  });

  it("goes on a trigger the caller renders", () => {
    uft.useFakeTimers();
    render(
      <Tooltip.Root>
        <Tooltip.Trigger
          render={(props) => (
            <a href="/bold" {...props}>
              Bold
            </a>
          )}
        />
        <Tooltip.Body>What bold does</Tooltip.Body>
      </Tooltip.Root>,
    );
    const trigger = screen.getByRole("link", { name: "Bold" });
    fireEvent.pointerEnter(trigger);
    advance(700);
    expect(trigger.getAttribute("aria-describedby")).toBe(screen.getByRole("tooltip").id);
  });
});

describe("Tooltip: one clock for a toolbar", () => {
  afterEach(() => {
    uft.useRealTimers();
  });

  const advance = (millis: number) => {
    act(() => {
      uft.advanceTimersByTime(millis);
    });
  };

  component Toolbar() {
    return (
      <Tooltip.Provider delayDuration={700} skipDelayDuration={300}>
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
          <Tooltip.Body>Bold</Tooltip.Body>
        </Tooltip.Root>
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Italic">I</Tooltip.Trigger>
          <Tooltip.Body>Italic</Tooltip.Body>
        </Tooltip.Root>
      </Tooltip.Provider>
    );
  }

  /** Read the first tooltip, then leave it, which is what opens the window. */
  const readTheFirst = () => {
    const first = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerEnter(first);
    advance(700);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Bold");
    fireEvent.pointerLeave(first);
    advance(300);
    expect(screen.queryByRole("tooltip")).toBe(null);
  };

  it("opens the second tooltip in a toolbar without waiting again", () => {
    uft.useFakeTimers();
    render(<Toolbar />);
    readTheFirst();

    fireEvent.pointerEnter(screen.getByRole("button", { name: "Italic" }));
    // In the same tick. A reader who has waited out the delay once has
    // established that they are reading tooltips.
    expect(screen.getByRole("tooltip")).toHaveTextContent("Italic");
  });

  it("waits again once the window has passed", () => {
    uft.useFakeTimers();
    render(<Toolbar />);
    readTheFirst();
    advance(301);

    fireEvent.pointerEnter(screen.getByRole("button", { name: "Italic" }));
    expect(screen.queryByRole("tooltip")).toBe(null);
    advance(700);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Italic");
  });

  it("leaves a tooltip outside a provider exactly as it is", () => {
    uft.useFakeTimers();
    render(
      <div>
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
          <Tooltip.Body>Bold</Tooltip.Body>
        </Tooltip.Root>
        <Tooltip.Root openDelay={50}>
          <Tooltip.Trigger aria-label="Italic">I</Tooltip.Trigger>
          <Tooltip.Body>Italic</Tooltip.Body>
        </Tooltip.Root>
      </div>,
    );
    const first = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerEnter(first);
    advance(699);
    expect(screen.queryByRole("tooltip")).toBe(null);
    advance(1);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Bold");
    fireEvent.pointerLeave(first);
    advance(300);

    // No group, so no skip window: the second one waits its own delay, and the
    // delay is its own rather than a provider's.
    fireEvent.pointerEnter(screen.getByRole("button", { name: "Italic" }));
    expect(screen.queryByRole("tooltip")).toBe(null);
    advance(50);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Italic");
  });

  it("lets one tooltip in a group keep a delay of its own", () => {
    uft.useFakeTimers();
    render(
      <Tooltip.Provider delayDuration={700} skipDelayDuration={300}>
        <Tooltip.Root openDelay={0}>
          <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
          <Tooltip.Body>Bold</Tooltip.Body>
        </Tooltip.Root>
      </Tooltip.Provider>,
    );
    fireEvent.pointerEnter(screen.getByRole("button", { name: "Bold" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Bold");
  });
});

describe("HoverCard", () => {
  afterEach(() => {
    uft.useRealTimers();
  });

  const advance = (millis: number) => {
    act(() => {
      uft.advanceTimersByTime(millis);
    });
  };

  component Example() {
    return (
      <div>
        <HoverCard.Root>
          <HoverCard.Trigger
            render={(props) => (
              <a href="/ada" {...props}>
                @ada
              </a>
            )}
          />
          <HoverCard.Body>
            <p>Ada Lovelace</p>
            <a href="/ada/notes">Notes</a>
          </HoverCard.Body>
        </HoverCard.Root>
        <button type="button">After</button>
      </div>
    );
  }

  it("does not pull focus back when a caller changes closeDelay", async () => {
    // The focus return used to live in the cleanup of the effect that attaches
    // the card's listeners, and that effect lists `closeDelay` — a caller's
    // prop. So a caller changing it while the card was open, with the reader's
    // focus on a link inside, ran the cleanup and dragged focus back to the
    // trigger. The card was not closing; nothing about the reader's position
    // had changed.
    component Changing(closeDelay: number) {
      return (
        <div>
          <HoverCard.Root closeDelay={closeDelay}>
            <HoverCard.Trigger
              render={(props) => (
                <a href="/ada" {...props}>
                  @ada
                </a>
              )}
            />
            <HoverCard.Body>
              <a href="/ada/notes">Notes</a>
            </HoverCard.Body>
          </HoverCard.Root>
        </div>
      );
    }

    const view = render(<Changing closeDelay={300} />);
    const trigger = screen.getByRole("link", { name: "@ada" });
    act(() => {
      trigger.focus();
    });
    const notes = screen.getByRole("link", { name: "Notes" });
    act(() => {
      fireEvent.focusIn(notes);
      notes.focus();
    });
    expect(document.activeElement).toBe(notes);

    view.rerender(<Changing closeDelay={900} />);

    expect(screen.getByRole("link", { name: "Notes" })).toBeInTheDocument();
    expect(document.activeElement).toBe(notes);
  });

  it("opens on hover after a wait, and on focus at once", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("link", { name: "@ada" });

    fireEvent.pointerEnter(trigger);
    advance(699);
    expect(screen.queryByText("Ada Lovelace")).toBe(null);
    advance(1);
    expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();

    fireEvent.pointerLeave(trigger);
    advance(300);
    expect(screen.queryByText("Ada Lovelace")).toBe(null);

    act(() => {
      trigger.focus();
    });
    expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();
  });

  it("is not a dialog and does not describe its trigger", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("link", { name: "@ada" });
    act(() => {
      trigger.focus();
    });

    // Nothing about it is modal, and a card of links flattened into a
    // description is a sentence nobody can act on.
    expect(screen.queryByRole("dialog")).toBe(null);
    expect(trigger).not.toHaveAttribute("aria-describedby");
    expect(danglingReferences()).toEqual([]);
  });

  it("holds links Tab can reach, and stays while focus is inside", async () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("link", { name: "@ada" });
    act(() => {
      trigger.focus();
    });

    await userEvent.tab();
    expect(screen.getByRole("link", { name: "Notes" })).toHaveFocus();
    advance(10_000);
    // Leaving the trigger scheduled a close; arriving in the card called it
    // off, which is the hoverable clause applied to the keyboard.
    expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("link", { name: "@ada" });
    act(() => {
      trigger.focus();
    });
    await userEvent.tab();
    expect(screen.getByRole("link", { name: "Notes" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByText("Ada Lovelace")).toBe(null);
    // The card took its own links away, so focus has to be put somewhere the
    // reader recognises rather than left on `<body>`.
    expect(trigger).toHaveFocus();
  });

  it("does not open on a tap", () => {
    uft.useFakeTimers();
    render(<Example />);
    const trigger = screen.getByRole("link", { name: "@ada" });
    const touch = new Event("pointerenter", { bubbles: false });
    (touch as $FlowFixMe).pointerType = "touch";
    act(() => {
      trigger.dispatchEvent(touch);
    });
    advance(10_000);
    // A hover card is an enrichment: the link under it goes somewhere useful on
    // its own, which is all a reader on a phone will ever get.
    expect(screen.queryByText("Ada Lovelace")).toBe(null);
  });
});

describe("the month a calendar shows", () => {
  // The arithmetic, called with dates. `internal/date-grid.js` is pure for the
  // same reason `internal/anchor.js`'s `placeOverlay` is: the cases worth being
  // exhaustive about are the month boundaries, and testing them through a
  // rendered grid tests the renderer instead.

  /** October 2026: it starts on a Thursday and ends on a Saturday. */
  const october = { month: 10, year: 2026 };

  it("lays a month out in weeks, with blanks where no day falls", () => {
    const weeks = weeksOf(october.year, october.month, 1);
    // Five rows, because a month that starts on a Thursday and has 31 days
    // needs five and not six.
    expect(weeks.length).toBe(5);
    expect(weeks.every((week) => week.length === 7)).toBe(true);
    // Monday first, so the 1st — a Thursday — is the fourth cell.
    expect(weeks[0].map((day) => day?.day ?? null)).toEqual([null, null, null, 1, 2, 3, 4]);
    expect(weeks[4].map((day) => day?.day ?? null)).toEqual([26, 27, 28, 29, 30, 31, null]);
  });

  it("lays the same month out differently for a week that starts on Sunday", () => {
    const weeks = weeksOf(october.year, october.month, 7);
    expect(weeks[0].map((day) => day?.day ?? null)).toEqual([null, null, null, null, 1, 2, 3]);
    expect(weeks[4].map((day) => day?.day ?? null)).toEqual([25, 26, 27, 28, 29, 30, 31]);
  });

  it("moves by a day, by a week, and off the end of the month", () => {
    const fourteenth = Temporal.PlainDate.from("2026-10-14");
    expect(moveDate(fourteenth, { by: 1, kind: "days" }, 1).toString()).toBe("2026-10-15");
    expect(moveDate(fourteenth, { by: 7, kind: "days" }, 1).toString()).toBe("2026-10-21");
    const last = Temporal.PlainDate.from("2026-10-31");
    expect(moveDate(last, { by: 1, kind: "days" }, 1).toString()).toBe("2026-11-01");
  });

  it("goes to the ends of the week the reader's week has", () => {
    const wednesday = Temporal.PlainDate.from("2026-10-14");
    expect(moveDate(wednesday, { kind: "week-edge", to: "start" }, 1).toString()).toBe(
      "2026-10-12",
    );
    expect(moveDate(wednesday, { kind: "week-edge", to: "end" }, 1).toString()).toBe("2026-10-18");
    // The same day, in a locale whose week starts on Sunday.
    expect(moveDate(wednesday, { kind: "week-edge", to: "start" }, 7).toString()).toBe(
      "2026-10-11",
    );
  });

  it("clamps the day when a month has fewer of them", () => {
    const january = Temporal.PlainDate.from("2026-01-31");
    // The 28th of February, not the 3rd of March: Temporal's `constrain`
    // overflow, and what a person means by "next month".
    expect(moveDate(january, { by: 1, kind: "months" }, 1).toString()).toBe("2026-02-28");
    const leapDay = Temporal.PlainDate.from("2028-02-29");
    expect(moveDate(leapDay, { by: 12, kind: "months" }, 1).toString()).toBe("2029-02-28");
  });

  it("mirrors the horizontal arrows for a reader who reads right to left", () => {
    expect(movementForDateKey({ key: "ArrowRight" }, "ltr")).toEqual({ by: 1, kind: "days" });
    expect(movementForDateKey({ key: "ArrowRight" }, "rtl")).toEqual({ by: -1, kind: "days" });
    // And nothing else: a page that reads right to left still reads top to
    // bottom, so a week later is a week later either way.
    expect(movementForDateKey({ key: "ArrowDown" }, "rtl")).toEqual({ by: 7, kind: "days" });
    expect(movementForDateKey({ key: "Home" }, "rtl")).toEqual({ kind: "week-edge", to: "start" });
  });

  it("leaves the keys that are not its own to the page", () => {
    // `Tab` is how a reader leaves the grid and `Escape` closes whatever the
    // calendar is inside; a grid that claimed either would be a trap.
    expect(movementForDateKey({ key: "Tab" }, "ltr")).toBe(null);
    expect(movementForDateKey({ key: "Escape" }, "ltr")).toBe(null);
    expect(movementForDateKey({ key: "a" }, "ltr")).toBe(null);
  });

  it("turns the page keys into a year when Shift is held", () => {
    expect(movementForDateKey({ key: "PageDown" }, "ltr")).toEqual({ by: 1, kind: "months" });
    expect(movementForDateKey({ key: "PageDown", shiftKey: true }, "ltr")).toEqual({
      by: 12,
      kind: "months",
    });
    expect(movementForDateKey({ key: "PageUp", shiftKey: true }, "ltr")).toEqual({
      by: -12,
      kind: "months",
    });
  });
});

describe("Calendar", () => {
  // A month of buttons in a grid looks finished from a screenshot, and every
  // assertion here is about something a screenshot cannot show: which cell the
  // keyboard is on, what a reader is told when the month changes, and which of
  // the two marks — today, and the chosen day — is on which cell.
  //
  // Every case names its own `today` and `weekStartsOn`, so that none of them
  // depends on the machine's clock or on which day the host's locale data
  // thinks a week starts on. The one case about the clock says so.

  component Booking(disabled?: (date: PlainDate) => boolean) {
    return (
      <Calendar.Root
        defaultValue="2026-10-14"
        isDateDisabled={disabled}
        locale="en-GB"
        today="2026-10-01"
        weekStartsOn={1}
      >
        <Calendar.Previous>Previous month</Calendar.Previous>
        <Calendar.Next>Next month</Calendar.Next>
        <Calendar.Month />
      </Calendar.Root>
    );
  }

  /** The cell the keyboard is on, which is the one thing a grid has one of. */
  const tabStop = (): HTMLElement | null =>
    screen.getAllByRole("gridcell").find((cell) => cell.getAttribute("tabindex") === "0") ?? null;

  it("is a grid of days with named columns", () => {
    render(<Booking />);
    expect(screen.getByRole("grid")).toBeInTheDocument();

    const columns = screen.getAllByRole("columnheader");
    expect(columns.length).toBe(7);
    // The full day name is what a reader is told; `Mo` is what is drawn. A
    // screen reader announcing "We" for a column is not announcing a day.
    expect(columns.map((column) => accessibleName(column))).toEqual([
      "Monday",
      "Tuesday",
      "Wednesday",
      "Thursday",
      "Friday",
      "Saturday",
      "Sunday",
    ]);

    // Exactly one, always: a set with two tab stops takes two `Tab` presses to
    // leave, and a set with none cannot be reached at all.
    const stops = screen
      .getAllByRole("gridcell")
      .filter((cell) => cell.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("14");
    expect(danglingReferences()).toEqual([]);
  });

  it("says which month it is showing, as the grid's own name", () => {
    render(<Booking />);
    expect(accessibleName(screen.getByRole("grid"))).toBe("October 2026");
  });

  it("moves by a day and by a week", async () => {
    render(<Booking />);
    act(() => {
      screen.getByRole("gridcell", { name: "14" }).focus();
    });
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("gridcell", { name: "15" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("gridcell", { name: "22" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("gridcell", { name: "14" })).toHaveFocus();
  });

  it("goes to the ends of the week, not of the month", async () => {
    render(<Booking />);
    act(() => {
      screen.getByRole("gridcell", { name: "14" }).focus();
    });
    await userEvent.keyboard("{Home}");
    // The Monday of that week, which is the 12th — `End` on a menu goes to the
    // last item and here it goes to the last day of the week.
    expect(screen.getByRole("gridcell", { name: "12" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("gridcell", { name: "18" })).toHaveFocus();
  });

  it("changes the month when the arrow runs off the end, and keeps focus on the day", async () => {
    render(<Booking />);
    act(() => {
      act(() => {
        screen.getByRole("gridcell", { name: "31" }).focus();
      });
    });
    await userEvent.keyboard("{ArrowRight}");

    expect(accessibleName(screen.getByRole("grid"))).toBe("November 2026");
    // The cell did not exist when the key was pressed: the grid was re-rendered
    // by the same update that asked for it, which is what `pendingFocus` is for.
    expect(screen.getByRole("gridcell", { name: "1" })).toHaveFocus();
    expect(tabStop()?.textContent).toBe("1");
  });

  it("jumps a month and a year", async () => {
    render(<Booking />);
    const start = screen.getByRole("gridcell", { name: "14" });
    start.focus();
    await userEvent.keyboard("{PageDown}");
    expect(accessibleName(screen.getByRole("grid"))).toBe("November 2026");
    expect(screen.getByRole("gridcell", { name: "14" })).toHaveFocus();

    // `Shift` is the one convention here a reader cannot discover by trying,
    // and a year is otherwise twelve presses.
    fireEvent.keyDown(screen.getByRole("gridcell", { name: "14" }), {
      key: "PageDown",
      shiftKey: true,
    });
    expect(accessibleName(screen.getByRole("grid"))).toBe("November 2027");
  });

  it("steps a month from the buttons without taking focus off them", async () => {
    render(<Booking />);
    const next = screen.getByRole("button", { name: "Next month" });
    next.focus();
    await userEvent.click(next);
    expect(accessibleName(screen.getByRole("grid"))).toBe("November 2026");
    // Focus stays on the button, which is what lets a reader step through
    // several months in a row.
    expect(next).toHaveFocus();
    expect(tabStop()?.textContent).toBe("14");

    await userEvent.click(screen.getByRole("button", { name: "Previous month" }));
    expect(accessibleName(screen.getByRole("grid"))).toBe("October 2026");
  });

  it("says which month it is showing when it changes", async () => {
    render(<Booking />);
    // Already in the document and empty: a live region added to the page in the
    // same commit as its text is not announced, because the technology watching
    // it had nothing to watch until it was too late. The shipped Combobox case
    // — "says how many options matched, in a region that was already there" —
    // is the same constraint.
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("");

    act(() => {
      screen.getByRole("gridcell", { name: "14" }).focus();
    });
    await userEvent.keyboard("{PageDown}");
    expect(screen.getByRole("status").textContent).toBe("November 2026");
  });

  it("keeps an unavailable date reachable", async () => {
    render(<Booking disabled={(date) => date.day === 15} />);
    const unavailable = screen.getByRole("gridcell", { name: "15" });
    expect(unavailable).toHaveAttribute("aria-disabled", "true");

    act(() => {
      screen.getByRole("gridcell", { name: "14" }).focus();
    });
    await userEvent.keyboard("{ArrowRight}");
    // *On* it rather than over it, which is the deliberate difference from the
    // menu and tab assertions elsewhere in this file: a reader arrowing through
    // October has to be able to pass over the days that cannot be booked, and a
    // grid that skipped them presents a month with holes in it.
    expect(unavailable).toHaveFocus();

    await userEvent.keyboard("{Enter}");
    expect(unavailable).not.toHaveAttribute("aria-selected");
    expect(screen.getByRole("gridcell", { name: "14" })).toHaveAttribute("aria-selected", "true");
  });

  it("marks today and the selection separately", () => {
    render(<Booking />);
    const today = screen.getByRole("gridcell", { name: "1" });
    const chosen = screen.getByRole("gridcell", { name: "14" });
    expect(today).toHaveAttribute("aria-current", "date");
    expect(today).not.toHaveAttribute("aria-selected");
    expect(chosen).toHaveAttribute("aria-selected", "true");
    expect(chosen).not.toHaveAttribute("aria-current");
  });

  it("chooses a day when it is pressed, and moves the tab stop to it", async () => {
    render(<Booking />);
    await userEvent.click(screen.getByRole("gridcell", { name: "20" }));
    expect(screen.getByRole("gridcell", { name: "20" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("gridcell", { name: "14" })).not.toHaveAttribute("aria-selected");
    expect(tabStop()?.textContent).toBe("20");
  });

  it("reads today from the clock seam rather than from the machine", () => {
    // Which is the whole reason `Temporal.Now` in this package goes through
    // `@uniflowed/core/clock`: a server and a browser disagree about the date,
    // and a test that had to wait until tomorrow to see this fail would not be
    // one. The clock is installed for this case only.
    const restore = setClock(fixedClock(Date.UTC(2026, 2, 9, 12, 0, 0)));
    try {
      render(
        <Calendar.Root locale="en-GB" weekStartsOn={1}>
          <Calendar.Month />
        </Calendar.Root>,
      );
      expect(accessibleName(screen.getByRole("grid"))).toBe("March 2026");
      expect(screen.getByRole("gridcell", { name: "9" })).toHaveAttribute("aria-current", "date");
    } finally {
      restore();
    }
  });

  it("walks the days the way an Arabic reader reads them", async () => {
    render(
      <div dir="rtl">
        <Booking />
      </div>,
    );
    act(() => {
      screen.getByRole("gridcell", { name: "14" }).focus();
    });
    // In a right-to-left page the next day is to the *left*, so `ArrowLeft` is
    // "next" — the same mirroring every other set in this package does, and the
    // one that renders identically when it is wrong.
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("gridcell", { name: "15" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("gridcell", { name: "14" })).toHaveFocus();
  });
});

describe("Date Picker", () => {
  // The composition, and the three joins that are this module's own: the
  // field's text and the chosen date, where focus goes when the calendar
  // closes, and what the calendar opens onto.

  component Trip() {
    return (
      <DatePicker.Root defaultValue="2026-10-14" locale="en-GB" today="2026-10-01" weekStartsOn={1}>
        <DatePicker.Input aria-label="Arrive on" />
        <DatePicker.Trigger>Choose a date</DatePicker.Trigger>
        <DatePicker.Calendar>
          <Calendar.Previous>Previous month</Calendar.Previous>
          <Calendar.Month />
        </DatePicker.Calendar>
      </DatePicker.Root>
    );
  }

  it("puts the date in a text field the reader can type into", () => {
    render(<Trip />);
    const field = screen.getByRole("textbox", { name: "Arrive on" });
    expect(field).toHaveValue("2026-10-14");
    // Not `aria-haspopup`, and not `aria-expanded`: the field does not open the
    // calendar, the button beside it does, and telling a reader otherwise is a
    // promise the field does not keep.
    expect(field).not.toHaveAttribute("aria-expanded");
    expect(screen.queryByRole("grid")).toBe(null);
    expect(danglingReferences()).toEqual([]);
  });

  it("opens the calendar onto the chosen date rather than onto a button", async () => {
    render(<Trip />);
    await userEvent.click(screen.getByRole("button", { name: "Choose a date" }));
    // The APG's date picker dialog puts focus on the date for the same reason:
    // the reader opened a calendar to find a day, not to step back a month.
    expect(screen.getByRole("gridcell", { name: "14" })).toHaveFocus();
    expect(danglingReferences()).toEqual([]);
  });

  it("closes on Escape and puts focus back in the field", async () => {
    render(<Trip />);
    await userEvent.click(screen.getByRole("button", { name: "Choose a date" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("grid")).toBe(null);
    // The field, not the button: it is the primary control, and a reader who
    // dismissed the calendar is back to typing.
    expect(screen.getByRole("textbox", { name: "Arrive on" })).toHaveFocus();
  });

  it("fills the field when a day is chosen, and closes", async () => {
    render(<Trip />);
    await userEvent.click(screen.getByRole("button", { name: "Choose a date" }));
    await userEvent.click(screen.getByRole("gridcell", { name: "20" }));
    expect(screen.getByRole("textbox", { name: "Arrive on" })).toHaveValue("2026-10-20");
    expect(screen.queryByRole("grid")).toBe(null);
    expect(screen.getByRole("textbox", { name: "Arrive on" })).toHaveFocus();
  });

  it("takes a date typed into the field", async () => {
    render(<Trip />);
    const field = screen.getByRole("textbox", { name: "Arrive on" });
    field.focus();
    replaceValue(field, "2026-11-05");
    await userEvent.keyboard("{Enter}");
    await userEvent.click(screen.getByRole("button", { name: "Choose a date" }));
    expect(accessibleName(screen.getByRole("grid"))).toBe("November 2026");
    expect(screen.getByRole("gridcell", { name: "5" })).toHaveAttribute("aria-selected", "true");
  });

  it("keeps text it could not read, and says it could not read it", async () => {
    render(<Trip />);
    const field = screen.getByRole("textbox", { name: "Arrive on" });
    field.focus();
    replaceValue(field, "next Tuesday");
    await userEvent.keyboard("{Enter}");
    // The text stays. Clearing it would throw away what the reader typed and
    // leave them nothing to correct — and `aria-invalid` is how they are told,
    // rather than a silent revert to the old date.
    expect(field).toHaveValue("next Tuesday");
    expect(field).toHaveAttribute("aria-invalid", "true");
  });
});

describe("Toast", () => {
  // The queue is one module-level value — that is what lets `toast()` be
  // called from a `catch` — so what one test queued is still there in the next
  // one unless something clears it.
  //
  // Inside `act`, because `render` cleans up the previous test's tree when the
  // next one starts rather than in an `afterEach`: the region from the test
  // before is still mounted here, and emptying the queue is an update to it.
  beforeEach(() => {
    act(() => {
      dismissAllToasts();
    });
  });

  // A leaked fake clock is the failure mode that matters most here: the next
  // file's `setTimeout` never fires and the run hangs with no explanation.
  afterEach(() => {
    uft.useRealTimers();
  });

  component Example(limit?: number = 3) {
    return (
      <Toast.Region limit={limit}>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>
    );
  }

  component WithUndo(onUndo?: () => void) {
    return (
      <Toast.Region>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Action onClick={onUndo}>Undo</Toast.Action>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>
    );
  }

  /** Queue a notification the way an event handler or a `catch` would. */
  const notify = (content: string, options?: $FlowFixMe) => {
    act(() => {
      toast(content, options);
    });
  };

  it("is watching before there is anything to announce", () => {
    render(<Example />);
    // The assertion the whole component exists for, and the one that fails for
    // every implementation that mounts the region together with the message:
    // a live region added in the same commit as its text is usually not
    // announced at all, because the technology watching it had nothing to
    // watch until it was already too late.
    const polite = screen.getByRole("status");
    expect(polite).toBeInTheDocument();
    expect(polite.textContent).toBe("");
    expect(screen.getByRole("alert").textContent).toBe("");
    expect(screen.queryAllByRole("group").length).toBe(0);
  });

  it("announces a failure assertively and a success politely", () => {
    render(<Example />);
    notify("Saved");
    notify("Could not save", { urgency: "assertive" });

    const polite = screen.getByRole("status");
    const assertive = screen.getByRole("alert");
    expect(polite).toHaveAttribute("aria-live", "polite");
    expect(polite).toHaveAttribute("aria-atomic", "true");
    expect(assertive).toHaveAttribute("aria-live", "assertive");
    expect(assertive).toHaveAttribute("aria-atomic", "true");
    // Which region a notification lands in is what decides whether it
    // interrupts, and it is chosen per notification rather than per
    // application: interrupting a reader mid-sentence to say "saved" is why
    // assertive is not the default, and waiting politely to say "could not
    // save" is why it has to be available.
    expect(polite.textContent).toContain("Saved");
    expect(polite.textContent).not.toContain("Could not save");
    expect(assertive.textContent).toContain("Could not save");
  });

  it("does not move focus when one appears", () => {
    render(
      <div>
        <input aria-label="Note" />
        <Example />
      </div>,
    );
    const field = screen.getByLabelText("Note");
    field.focus();
    notify("Saved");
    // Moving focus to a notification interrupts whatever the reader was
    // typing, and it is the single failure that makes people turn
    // notifications off.
    expect(field).toHaveFocus();
    expect(screen.getByRole("status").textContent).toContain("Saved");
  });

  it("names a notification after its title and describes it with the rest", () => {
    render(
      <Toast.Region>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Description>Two of three files.</Toast.Description>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>,
    );
    notify("Uploading");
    // The group is what turns a stack of three into three things a reader can
    // move between after F6, rather than one run of text.
    const group = screen.getByRole("group", { name: "Uploading" });
    const described = group.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(described)?.textContent).toBe("Two of three files.");
    expect(danglingReferences()).toEqual([]);
  });

  it("claims no name when nothing named it", () => {
    render(
      <Toast.Region>
        {() => (
          <Toast.Root>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>,
    );
    notify("Saved");
    const group = screen.getByRole("group");
    expect(group).not.toHaveAttribute("aria-labelledby");
    expect(group).not.toHaveAttribute("aria-describedby");
  });

  it("names its dismiss button", async () => {
    render(<Example />);
    notify("Saved");
    // An icon-only close with no name is announced as "button": a control a
    // reader can find and cannot identify.
    const dismiss = screen.getByRole("button", { name: /dismiss/i });
    await userEvent.click(dismiss);
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("takes the notification away when its action is taken", async () => {
    const onUndo = fn();
    render(<WithUndo onUndo={onUndo} />);
    notify("Deleted", { duration: null });
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(onUndo).toHaveBeenCalled();
    // A notification whose offer has been accepted is describing something
    // that is no longer true.
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("reaches the notifications from the keyboard", async () => {
    render(<WithUndo />);
    notify("Deleted", { duration: null });
    const region = screen.getByRole("region", { name: "Notifications" });
    fireEvent.keyDown(document, { key: "F6" });
    // Focus lands on the region rather than on the first button in it, which
    // is what makes a screen reader read the region's name and its contents
    // instead of skipping straight past both.
    expect(region).toHaveFocus();
    await userEvent.tab();
    // And from there the action is one Tab away, which is the whole point: an
    // Undo button that vanishes after four seconds is a control no keyboard
    // reader can operate.
    expect(screen.getByRole("button", { name: "Undo" })).toHaveFocus();
  });

  it("gives focus back when the key is pressed again", () => {
    render(
      <div>
        <input aria-label="Note" />
        <WithUndo />
      </div>,
    );
    const field = screen.getByLabelText("Note");
    field.focus();
    notify("Deleted", { duration: null });

    fireEvent.keyDown(document, { key: "F6" });
    expect(screen.getByRole("region", { name: "Notifications" })).toHaveFocus();
    fireEvent.keyDown(document, { key: "F6" });
    // A key that only goes one way strands the reader it was meant to help.
    expect(field).toHaveFocus();
  });

  it("says which part was used outside a region", () => {
    let message = "";
    try {
      render(<Toast.Title>orphan</Toast.Title>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Toast.Title must be rendered inside a Toast.Root");
  });
});

describe("Toast: the queue behind the stack", () => {
  beforeEach(() => {
    act(() => {
      dismissAllToasts();
    });
  });

  component Example(limit?: number = 2) {
    return (
      <Toast.Region limit={limit}>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>
    );
  }

  const notify = (content: string): string => {
    let id = "";
    act(() => {
      id = toast(content);
    });
    return id;
  };

  it("shows no more than the limit, oldest first", () => {
    render(<Example />);
    notify("One");
    notify("Two");
    notify("Three");
    // A queue rather than a pile: what arrived first is what a reader is shown
    // first, and what is over the limit is not rendered at all — which is also
    // what keeps its countdown from running before anyone has seen it.
    expect(screen.getAllByRole("group").length).toBe(2);
    expect(screen.getByRole("status").textContent).toContain("One");
    expect(screen.getByRole("status").textContent).not.toContain("Three");
  });

  it("promotes the one behind when a notification is dismissed", async () => {
    render(<Example />);
    notify("One");
    notify("Two");
    notify("Three");
    await userEvent.click(screen.getAllByRole("button", { name: /dismiss/i })[0]);
    expect(screen.getByRole("status").textContent).toContain("Three");
  });

  it("changes a notification in place rather than stacking a second one", () => {
    render(<Example />);
    const id = notify("Uploading…");
    act(() => {
      updateToast(id, { content: "Uploaded" });
    });
    // A reader told the second thing without the first disappearing has been
    // told the upload is both in progress and finished.
    expect(screen.getAllByRole("group").length).toBe(1);
    expect(screen.getByRole("status").textContent).toContain("Uploaded");
  });

  it("does not render a region's notification twice", () => {
    render(<Example />);
    notify("Saved");
    // One notification, in one of the two regions, and never mirrored into a
    // hidden announcer beside it — a reader browsing the page would find each
    // one again with no way to tell it is the same one.
    expect(screen.getAllByText("Saved").length).toBe(1);
  });
});

describe("Toast: timers that stop", () => {
  beforeEach(() => {
    act(() => {
      dismissAllToasts();
    });
  });

  afterEach(() => {
    uft.useRealTimers();
  });

  component Example() {
    return (
      <Toast.Region>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Action>Undo</Toast.Action>
          </Toast.Root>
        )}
      </Toast.Region>
    );
  }

  /** What the polite region is holding, which is what a reader would hear. */
  const announced = (): string => screen.getByRole("status").textContent ?? "";

  const advance = (millis: number) => {
    act(() => {
      uft.advanceTimersByTime(millis);
    });
  };

  const notify = (content: string, duration: number | null) => {
    act(() => {
      toast(content, { duration });
    });
  };

  it("goes away when its time is up", () => {
    uft.useFakeTimers();
    render(<Example />);
    notify("Saved", 4000);
    advance(3999);
    expect(announced()).toContain("Saved");
    advance(1);
    expect(announced()).toBe("");
  });

  it("stops the clock while the pointer is over it", () => {
    uft.useFakeTimers();
    render(<Example />);
    notify("Saved", 4000);
    advance(1000);

    const notification = screen.getByRole("group");
    fireEvent.pointerEnter(notification);
    advance(30_000);
    // WCAG 2.2.1, Timing Adjustable: anything that disappears on its own has
    // to be stoppable by the reader who is still reading it.
    expect(announced()).toContain("Saved");

    fireEvent.pointerLeave(notification);
    // What is left, not the whole duration again. The one-line version of
    // this component re-arms the timeout on every unpause, so a pointer
    // resting near the stack keeps a notification on screen for ever.
    advance(2999);
    expect(announced()).toContain("Saved");
    advance(1);
    expect(announced()).toBe("");
  });

  it("stops the clock while focus is inside it", () => {
    uft.useFakeTimers();
    render(<Example />);
    notify("Deleted", 4000);

    const undo = screen.getByRole("button", { name: "Undo" });
    act(() => {
      undo.focus();
    });
    advance(30_000);
    // Otherwise the control the notification exists to offer is taken away
    // from the reader in the middle of reaching for it.
    expect(announced()).toContain("Deleted");
    expect(undo).toHaveFocus();
  });

  it("stops the clock while the document is hidden", () => {
    uft.useFakeTimers();
    render(<Example />);
    notify("Saved", 4000);

    hideDocument(true);
    advance(30_000);
    // A reader who switches tabs for a minute should not come back to an
    // empty region and no idea what they missed.
    expect(announced()).toContain("Saved");

    hideDocument(false);
    advance(4000);
    expect(announced()).toBe("");
  });

  it("never expires when it was given no duration", () => {
    uft.useFakeTimers();
    render(<Example />);
    notify("Deleted", null);
    advance(30_000);
    // What anything carrying an action should be: the countdown stopping
    // while focus is inside makes the button reachable, and that is not a
    // promise that four seconds was enough time to decide.
    expect(announced()).toContain("Deleted");
  });

  it("starts the countdown again when the duration is changed under it", () => {
    uft.useFakeTimers();
    render(<Example />);
    let id = "";
    act(() => {
      id = toast("Uploading…", { duration: null });
    });
    advance(30_000);
    expect(announced()).toContain("Uploading…");

    act(() => {
      updateToast(id, { content: "Uploaded", duration: 4000 });
    });
    advance(3999);
    expect(announced()).toContain("Uploaded");
    advance(1);
    expect(announced()).toBe("");
  });
});

describe("Progress", () => {
  it("says how far along it is", () => {
    render(<Progress aria-label="Uploading" max={10} min={0} value={3} />);
    const bar = screen.getByRole("progressbar", { name: "Uploading" });
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "10");
    expect(bar).toHaveAttribute("aria-valuenow", "3");
  });

  it("says nothing about how far along it is when it does not know", () => {
    render(<Progress aria-label="Uploading" />);
    const bar = screen.getByRole("progressbar", { name: "Uploading" });
    // The whole component is this conditional. `aria-valuenow="0"` says
    // "nothing has happened yet", and a reader who asks again in ten seconds
    // and hears zero again concludes the operation is stuck. Omitting it says
    // "in progress, amount unknown", which is what is actually true.
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "100");
  });

  it("says what the number means when the percentage is not the answer", () => {
    render(<Progress aria-label="Uploading" max={10} value={3} valueText="3 of 10 files" />);
    const bar = screen.getByRole("progressbar", { name: "Uploading" });
    // The text is what a reader hears; the number is still there for anything
    // that draws a gauge from it.
    expect(bar).toHaveAttribute("aria-valuetext", "3 of 10 files");
    expect(bar).toHaveAttribute("aria-valuenow", "3");
  });

  it("never reports a value outside its own bounds", () => {
    render(<Progress aria-label="Uploading" max={10} value={40} />);
    // An `aria-valuenow` above `aria-valuemax` is a contradiction a screen
    // reader reads out loud.
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "10");
  });

  it("hands the progressbar contract to a caller-rendered element", () => {
    render(
      <Progress
        aria-label="Uploading"
        max={10}
        render={(props) => <section {...props} />}
        value={4}
      />,
    );
    const bar = screen.getByRole("progressbar", { name: "Uploading" });
    expect(bar.tagName).toBe("SECTION");
    expect(bar).toHaveAttribute("aria-valuenow", "4");
    expect(bar).toHaveAttribute("aria-valuemax", "10");
  });
});

describe("Slider", () => {
  component Example(
    defaultValue?: $ReadOnlyArray<number> = [20],
    disabled?: boolean = false,
    step?: number = 1,
    valueText?: (value: number, index: number) => string,
  ) {
    return (
      <Slider.Root
        defaultValue={defaultValue}
        disabled={disabled}
        step={step}
        valueText={valueText}
      >
        <Slider.Track data-testid="track">
          <Slider.Range />
        </Slider.Track>
        <Slider.Thumb aria-label="Volume" />
      </Slider.Root>
    );
  }

  const now = (thumb: HTMLElement): string | null | void => thumb.getAttribute("aria-valuenow");

  it("puts the slider role on the thumb and reaches it with Tab", async () => {
    render(<Example />);
    const thumb = screen.getByRole("slider", { name: "Volume" });
    // On the thumb, not on the track. The element carrying the role is the
    // element carrying `tabindex="0"`, and a track with the role is a track
    // nobody can focus with a thumb nobody can find.
    expect(thumb).toHaveAttribute("aria-valuemin", "0");
    expect(thumb).toHaveAttribute("aria-valuemax", "100");
    expect(thumb).toHaveAttribute("aria-valuenow", "20");
    expect(thumb).toHaveAttribute("aria-orientation", "horizontal");
    await userEvent.tab();
    expect(thumb).toHaveFocus();
  });

  it("moves by a step, by a page, and to the ends", async () => {
    render(<Example />);
    const thumb = screen.getByRole("slider");
    thumb.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(now(thumb)).toBe("21");
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(now(thumb)).toBe("19");
    await userEvent.keyboard("{PageUp}");
    // The key that makes a slider from 0 to 10,000 crossable without holding
    // another one down for a minute.
    expect(now(thumb)).toBe("29");
    await userEvent.keyboard("{PageDown}{PageDown}");
    expect(now(thumb)).toBe("9");
    await userEvent.keyboard("{End}");
    expect(now(thumb)).toBe("100");
    await userEvent.keyboard("{Home}");
    expect(now(thumb)).toBe("0");
  });

  it("stops at its ends rather than running past them", async () => {
    render(<Example />);
    const thumb = screen.getByRole("slider");
    thumb.focus();
    await userEvent.keyboard("{Home}{ArrowLeft}");
    // An `aria-valuenow` below `aria-valuemin` is a contradiction a screen
    // reader reads out loud.
    expect(now(thumb)).toBe("0");
    await userEvent.keyboard("{End}{ArrowRight}");
    expect(now(thumb)).toBe("100");
  });

  it("lands only on its own steps", async () => {
    render(<Example defaultValue={[20]} step={5} />);
    const thumb = screen.getByRole("slider");
    thumb.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(now(thumb)).toBe("25");
  });

  it("says what the value means when the number does not", async () => {
    const words = ["Off", "Low", "Medium", "High"];
    render(
      <Slider.Root defaultValue={[2]} max={3} valueText={(each) => words[each] ?? ""}>
        <Slider.Track>
          <Slider.Range />
        </Slider.Track>
        <Slider.Thumb aria-label="Fan" />
      </Slider.Root>,
    );
    const thumb = screen.getByRole("slider", { name: "Fan" });
    // "2" is the implementation. "Medium" is the meaning — and the number is
    // still there, so anything drawing a gauge still has it.
    expect(thumb).toHaveAttribute("aria-valuetext", "Medium");
    expect(thumb).toHaveAttribute("aria-valuenow", "2");
    thumb.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(thumb).toHaveAttribute("aria-valuetext", "High");
  });

  it("mirrors the horizontal keys in a right-to-left page", async () => {
    render(
      <div dir="rtl">
        <Example />
      </div>,
    );
    const thumb = screen.getByRole("slider");
    thumb.focus();
    await userEvent.keyboard("{ArrowRight}");
    // `ArrowRight` means "further along", and further along is to the left
    // here. A slider that ignores this looks identical and walks backwards.
    expect(now(thumb)).toBe("19");
    await userEvent.keyboard("{ArrowUp}");
    // The vertical axis is not mirrored by writing direction.
    expect(now(thumb)).toBe("20");
    await userEvent.keyboard("{Home}");
    // Nor are the ends: `Home` is the smallest value in both directions.
    expect(now(thumb)).toBe("0");
  });

  it("does nothing while disabled, and leaves the tab order", () => {
    render(<Example disabled />);
    const thumb = screen.getByRole("slider");
    expect(thumb).toHaveAttribute("tabindex", "-1");
    expect(thumb).toHaveAttribute("aria-disabled", "true");
    fireEvent.keyDown(thumb, { key: "ArrowRight" });
    expect(now(thumb)).toBe("20");
  });

  it("moves the nearest thumb when the track is pressed", () => {
    render(<Example />);
    const track = screen.getByTestId("track");
    measure(track, { left: 0, width: 200, top: 0, height: 10 });
    // WCAG 2.5.7, Dragging Movements: a control operated by dragging needs a
    // way that is not a drag, and a press on the track is the one a pointer
    // reader reaches for.
    fireEvent.pointerDown(track, { clientX: 150, clientY: 5 });
    expect(now(screen.getByRole("slider"))).toBe("75");
  });
});

describe("Slider: a range is two sliders", () => {
  component Example() {
    return (
      <Slider.Root defaultValue={[20, 60]}>
        <Slider.Track>
          <Slider.Range />
        </Slider.Track>
        <Slider.Thumb aria-label="Minimum" index={0} />
        <Slider.Thumb aria-label="Maximum" index={1} />
      </Slider.Root>
    );
  }

  it("bounds each thumb by its neighbour", async () => {
    render(<Example />);
    const lower = screen.getByRole("slider", { name: "Minimum" });
    const upper = screen.getByRole("slider", { name: "Maximum" });
    // Two names, told apart. Two identical "slider"s is the whole difference
    // between a control a reader can operate and one they cannot.
    expect(lower).toHaveAttribute("aria-valuenow", "20");
    expect(upper).toHaveAttribute("aria-valuenow", "60");
    // Announcing both as 0–100 while the behaviour stops them passing each
    // other is worse than not shipping the range: the reader is told they may
    // set the low thumb to 90, they try, and the control silently refuses.
    expect(lower).toHaveAttribute("aria-valuemax", "60");
    expect(upper).toHaveAttribute("aria-valuemin", "20");

    upper.focus();
    await userEvent.keyboard("{ArrowRight}");
    // The neighbour moved, so the bound moved with it.
    expect(screen.getByRole("slider", { name: "Minimum" })).toHaveAttribute("aria-valuemax", "61");
  });

  it("will not let one thumb pass the other", async () => {
    render(<Example />);
    const lower = screen.getByRole("slider", { name: "Minimum" });
    lower.focus();
    await userEvent.keyboard("{End}");
    // `End` on the lower thumb is its own end, which is its neighbour.
    expect(screen.getByRole("slider", { name: "Minimum" })).toHaveAttribute("aria-valuenow", "60");
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("slider", { name: "Minimum" })).toHaveAttribute("aria-valuenow", "60");
    expect(screen.getByRole("slider", { name: "Maximum" })).toHaveAttribute("aria-valuenow", "60");
  });

  it("keeps both thumbs in the tab order, in the order they were written", async () => {
    render(<Example />);
    await userEvent.tab();
    expect(screen.getByRole("slider", { name: "Minimum" })).toHaveFocus();
    await userEvent.tab();
    // The APG is explicit that a thumb passing another does not reorder them,
    // which is why the index is a prop rather than a position counted from the
    // page.
    expect(screen.getByRole("slider", { name: "Maximum" })).toHaveFocus();
  });
});

describe("Resizable", () => {
  component Example(
    defaultValue?: number = 50,
    disabled?: boolean = false,
    min?: number = 0,
    withPrimary?: boolean = true,
  ) {
    return (
      <Resizable.PanelGroup
        data-testid="group"
        defaultValue={defaultValue}
        disabled={disabled}
        min={min}
        step={10}
      >
        <Resizable.Panel primary={withPrimary}>Files</Resizable.Panel>
        <Resizable.Handle label="Resize the file list" />
        <Resizable.Panel>Editor</Resizable.Panel>
      </Resizable.PanelGroup>
    );
  }

  const handle = (): HTMLElement => screen.getByRole("separator", { name: "Resize the file list" });

  /**
   * Give the group a box, because this DOM gives every element a zero one.
   *
   * Two hundred wide and a hundred tall, so a percentage of it is two pixels
   * across and one down — round numbers in both orientations, which is what
   * keeps the arithmetic in each test readable.
   */
  const measureGroup = (): void => {
    measure(screen.getByTestId("group"), { height: 100, left: 0, top: 0, width: 200 });
  };

  it("resizes from the keyboard", async () => {
    render(<Example />);
    const splitter = handle();
    // Almost every resizable panel on the web is pointer-only, which is a
    // WCAG 2.1.1 failure. The keyboard is the feature.
    expect(splitter).toHaveAttribute("tabindex", "0");
    expect(splitter).toHaveAttribute("aria-valuenow", "50");
    await userEvent.tab();
    expect(splitter).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(handle()).toHaveAttribute("aria-valuenow", "60");
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(handle()).toHaveAttribute("aria-valuenow", "40");
  });

  it("names the pane it sizes", () => {
    render(<Example />);
    const controls = handle().getAttribute("aria-controls") ?? "";
    expect(document.getElementById(controls)?.textContent).toBe("Files");
    expect(danglingReferences()).toEqual([]);
  });

  it("names no pane when there is no primary one to name", () => {
    render(<Example withPrimary={false} />);
    // The rule every part of this package repeats: an `aria-controls` naming
    // an id nothing has is worse than saying nothing at all.
    expect(handle()).not.toHaveAttribute("aria-controls");
    expect(danglingReferences()).toEqual([]);
  });

  it("collapses the pane on Enter and restores it on the next one", async () => {
    render(<Example defaultValue={40} />);
    handle().focus();
    await userEvent.keyboard("{Enter}");
    expect(handle()).toHaveAttribute("aria-valuenow", "0");
    await userEvent.keyboard("{Enter}");
    // A collapse with no way back is a pane a keyboard reader has thrown away.
    expect(handle()).toHaveAttribute("aria-valuenow", "40");
  });

  it("goes to the ends of its range with Home and End", async () => {
    render(<Example min={10} />);
    handle().focus();
    await userEvent.keyboard("{Home}");
    expect(handle()).toHaveAttribute("aria-valuenow", "10");
    await userEvent.keyboard("{End}");
    expect(handle()).toHaveAttribute("aria-valuenow", "100");
  });

  it("says it is a vertical separator when the panes are side by side", () => {
    render(<Example />);
    // The inversion worth stating: `aria-orientation` on a separator describes
    // the separator, and two panes side by side are divided by a vertical
    // line. ARIA's default for the role is `horizontal`, so a vertical
    // splitter that says nothing is announced as a horizontal rule.
    expect(handle()).toHaveAttribute("aria-orientation", "vertical");
  });

  it("uses the vertical keys when the panes are stacked", async () => {
    render(
      <Resizable.PanelGroup defaultValue={50} orientation="vertical" step={10}>
        <Resizable.Panel primary>Top</Resizable.Panel>
        <Resizable.Handle />
        <Resizable.Panel>Bottom</Resizable.Panel>
      </Resizable.PanelGroup>,
    );
    const splitter = screen.getByRole("separator", { name: "Resize" });
    expect(splitter).toHaveAttribute("aria-orientation", "horizontal");
    splitter.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("separator")).toHaveAttribute("aria-valuenow", "60");
    // `ArrowRight` in a stacked group is the page's, and swallowing it takes a
    // key away from every reader who uses one.
    expect(fireEvent.keyDown(screen.getByRole("separator"), { key: "ArrowRight" })).toBe(true);
    expect(screen.getByRole("separator")).toHaveAttribute("aria-valuenow", "60");
  });

  it("moves under a pointer, and only while the pointer is down", () => {
    render(<Example />);
    measureGroup();
    const splitter = handle();
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    // Taking hold of the handle is not asking it to move. A splitter that also
    // jumped by the offset between the pointer and its own centre would shift
    // every time it was clicked.
    expect(handle()).toHaveAttribute("aria-valuenow", "50");
    fireEvent.pointerMove(splitter, { clientX: 150, clientY: 50, pointerId: 1 });
    // And what a reader is told follows the drag, which is the half a
    // pointer-only implementation leaves out.
    expect(handle()).toHaveAttribute("aria-valuenow", "75");
    fireEvent.pointerUp(splitter, { pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 20, clientY: 50, pointerId: 1 });
    expect(handle()).toHaveAttribute("aria-valuenow", "75");
  });

  it("captures the pointer, so a drag that wanders off the handle keeps arriving", () => {
    render(<Example />);
    measureGroup();
    const splitter = handle();
    let captured = null;
    (splitter as $FlowFixMe).setPointerCapture = (id: number) => {
      captured = id;
    };
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 7 });
    // Every drag leaves a bar a few pixels wide. Without the capture the moves
    // arrive at whatever the pointer wandered over and the splitter stops.
    expect(captured).toBe(7);
    // And the handle takes focus, because a reader who has just dragged it is
    // the reader most likely to reach for an arrow key next.
    expect(splitter).toHaveFocus();
  });

  it("holds a drag inside the range rather than mapping it across one", () => {
    render(<Example min={20} />);
    measureGroup();
    const splitter = handle();
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 10, clientY: 50, pointerId: 1 });
    // Five percent of the way along, and `min` is 20. A slider maps its
    // pointer across `min`–`max`, which would answer 24 here; these two are
    // bounds on how far the pane may be dragged rather than the ends of a
    // scale, so the value is the position clamped and the pane stops.
    expect(handle()).toHaveAttribute("aria-valuenow", "20");
  });

  it("moves the splitter one percent at a time, not one step", () => {
    render(<Example />);
    measureGroup();
    const splitter = handle();
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 133, clientY: 50, pointerId: 1 });
    // `step` is 10 and is the keyboard's: a drag that snapped to it would move
    // the bar in tenths under a pointer moving smoothly. The pointer's own
    // step is one, and it still snaps — 66.5 would reach `aria-valuenow` in
    // full and be read out in full.
    expect(handle()).toHaveAttribute("aria-valuenow", "67");
  });

  it("drags the other way in a right-to-left page", () => {
    render(
      <div dir="rtl">
        <Example />
      </div>,
    );
    measureGroup();
    const splitter = handle();
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 150, clientY: 50, pointerId: 1 });
    // The primary pane is the one before the handle, which in an RTL page is
    // the one on the right. Three quarters of the way from the left edge is a
    // quarter of the way along the pane, and a hard-coded reading renders
    // identically while dragging the wrong pane.
    expect(handle()).toHaveAttribute("aria-valuenow", "25");
  });

  it("reads a stacked group from the top, where its primary pane is", () => {
    render(
      <Resizable.PanelGroup data-testid="group" defaultValue={50} orientation="vertical" step={10}>
        <Resizable.Panel primary>Top</Resizable.Panel>
        <Resizable.Handle />
        <Resizable.Panel>Bottom</Resizable.Panel>
      </Resizable.PanelGroup>,
    );
    measureGroup();
    const splitter = screen.getByRole("separator", { name: "Resize" });
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 100, clientY: 30, pointerId: 1 });
    // `Slider.Track` reads a vertical drag from the *bottom*, because a
    // slider's minimum is there. A stacked group's primary pane is the top
    // one, so this reads from the top; taking the slider's line unchanged
    // would make the pane grow as the pointer went up.
    expect(screen.getByRole("separator")).toHaveAttribute("aria-valuenow", "30");
  });

  it("does not drag while disabled", () => {
    render(<Example disabled />);
    measureGroup();
    const splitter = handle();
    fireEvent.pointerDown(splitter, { clientX: 100, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 150, clientY: 50, pointerId: 1 });
    expect(handle()).toHaveAttribute("aria-valuenow", "50");
  });

  it("forgets where the pane was once a drag has moved it", async () => {
    render(<Example defaultValue={40} />);
    measureGroup();
    const splitter = handle();
    splitter.focus();
    await userEvent.keyboard("{Enter}");
    expect(handle()).toHaveAttribute("aria-valuenow", "0");
    fireEvent.pointerDown(splitter, { clientX: 0, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 120, clientY: 50, pointerId: 1 });
    fireEvent.pointerUp(splitter, { pointerId: 1 });
    expect(handle()).toHaveAttribute("aria-valuenow", "60");
    await userEvent.keyboard("{Enter}");
    // `Enter` collapses, because the drag is the reader's last word about
    // where the pane goes. Restoring 40 here would put it back to a size it
    // had before a gesture that said otherwise.
    expect(handle()).toHaveAttribute("aria-valuenow", "0");
  });

  it("tells a menu's separator and a splitter apart", () => {
    render(
      <div>
        <Menu.Root defaultOpen>
          <Menu.Trigger>File</Menu.Trigger>
          <Menu.Body>
            <Menu.Item>Open</Menu.Item>
            <Menu.Separator />
          </Menu.Body>
        </Menu.Root>
        <Example />
      </div>,
    );
    const [rule, splitter] = screen.getAllByRole("separator");
    // Same role, and only one of them is a control. `tabindex` and
    // `aria-valuenow` are the difference, and they are also how a screen
    // reader tells them apart.
    expect(rule).not.toHaveAttribute("tabindex");
    expect(rule).not.toHaveAttribute("aria-valuenow");
    expect(splitter).toHaveAttribute("tabindex", "0");
    expect(splitter).toHaveAttribute("aria-valuenow");
  });
});

describe("Table", () => {
  const PEOPLE = [
    { id: "ada", name: "Ada Lovelace", born: 1815 },
    { id: "alan", name: "Alan Turing", born: 1912 },
    { id: "grace", name: "Grace Hopper", born: 1906 },
  ];

  component Example(rowCount?: number | null = null, rowOffset?: number = 0) {
    const [sort, setSort] = useState(null);
    const [chosen, setChosen] = useState<$ReadOnlyArray<string>>([]);
    const all = chosen.length === PEOPLE.length ? true : chosen.length === 0 ? false : "mixed";

    return (
      <Table.Root onSortChange={setSort} rowCount={rowCount} rowOffset={rowOffset} sort={sort}>
        <Table.Caption>People</Table.Caption>
        <Table.Header>
          <Table.Row>
            <Table.Head>
              <Table.SelectAll
                checked={all}
                onCheckedChange={(on) => setChosen(on ? PEOPLE.map((each) => each.id) : [])}
              />
            </Table.Head>
            <Table.Head column="name">Name</Table.Head>
            <Table.Head column="born">Born</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {PEOPLE.map((person, at) => (
            <Table.Row index={at} key={person.id}>
              <Table.Cell>
                <Table.RowSelect
                  checked={chosen.includes(person.id)}
                  label={`Select ${person.name}`}
                  onCheckedChange={(on) =>
                    setChosen((held) =>
                      on ? [...held, person.id] : held.filter((each) => each !== person.id),
                    )
                  }
                />
              </Table.Cell>
              <Table.RowHeader>{person.name}</Table.RowHeader>
              <Table.Cell>{person.born}</Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    );
  }

  it("is a table with named columns and a caption", () => {
    render(<Example />);
    // The caption is what gives a `<table>` its accessible name. A heading
    // above the table looks the same and is not the table's name.
    expect(screen.getByRole("table", { name: "People" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Name" })).toHaveAttribute("scope", "col");
    // `scope="row"` is the other half: it is what lets a reader hear "Ada
    // Lovelace, 1815" instead of "1815" while moving down the year column.
    expect(screen.getByRole("rowheader", { name: "Ada Lovelace" })).toHaveAttribute("scope", "row");
  });

  it("marks only the sorted column", async () => {
    render(<Example />);
    const sortable = screen.getAllByRole("columnheader").filter((each) => each.textContent !== "");
    for (const header of sortable) {
      // Not `"none"` on the unsorted ones: eleven headers each announcing "not
      // sorted" is eleven announcements of nothing on every pass.
      expect(header).not.toHaveAttribute("aria-sort");
    }

    await userEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
    expect(document.querySelectorAll("[aria-sort]").length).toBe(1);

    await userEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toHaveAttribute(
      "aria-sort",
      "descending",
    );

    await userEvent.click(screen.getByRole("button", { name: "Born" }));
    expect(screen.getByRole("columnheader", { name: "Born" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
    expect(screen.getByRole("columnheader", { name: "Name" })).not.toHaveAttribute("aria-sort");
    expect(document.querySelectorAll("[aria-sort]").length).toBe(1);
  });

  it("puts the sort in a button, so a keyboard can reach it", async () => {
    render(<Example />);
    // A `<th>` with an `onClick` is a sort half the readers do not have. The
    // header's content is a real button, which is what puts it in the tab
    // order and what makes `Enter` and `Space` the browser's job rather than
    // this component's.
    const header = screen.getByRole("columnheader", { name: "Name" });
    expect(within(header).getByRole("button", { name: "Name" })).toBeInTheDocument();
    // Tab reaches it: the select-all checkbox first, then this.
    await userEvent.tab();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Name" })).toHaveFocus();
    await userEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
  });

  it("says that it re-sorted, in a region that was already there", async () => {
    render(<Example />);
    // The rows change places and a screen reader is told nothing, so the
    // sentence is the component's job — and the region has to have been in the
    // document before the first sort or it announces nothing at all.
    const status = screen.getByRole("status");
    expect(status).toBeInTheDocument();
    expect(status.textContent).toBe("");
    await userEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByRole("status").textContent).toBe("Sorted by Name, ascending.");
    await userEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByRole("status").textContent).toBe("Sorted by Name, descending.");
  });

  it("reports a partial selection as mixed and moves it to checked", async () => {
    render(<Example />);
    const all = screen.getByRole("checkbox", { name: "Select all rows" });
    expect(all).toHaveAttribute("aria-checked", "false");

    await userEvent.click(screen.getByRole("checkbox", { name: "Select Ada Lovelace" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Select Alan Turing" }));
    // `checkbox.js`'s documented case, finally asserted against the thing it
    // describes: two of three rows chosen is not "unchecked".
    expect(screen.getByRole("checkbox", { name: "Select all rows" })).toHaveAttribute(
      "aria-checked",
      "mixed",
    );

    await userEvent.click(screen.getByRole("checkbox", { name: "Select all rows" }));
    // A half-selected "select all" that clears itself on the first click is
    // the behaviour every table in every application gets wrong.
    expect(screen.getByRole("checkbox", { name: "Select Grace Hopper" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("checkbox", { name: "Select all rows" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("names each row's checkbox after its row", () => {
    render(<Example />);
    // "Select row" forty times is forty identical announcements, with no way
    // to tell which row a reader is on.
    expect(screen.getByRole("checkbox", { name: "Select Ada Lovelace" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Select Grace Hopper" })).toBeInTheDocument();
  });

  it("counts the rows it is not showing", () => {
    render(<Example rowCount={500} rowOffset={90} />);
    // Five hundred data rows and one header row. The caller said 500, which is
    // what an application knows; the header is the component's arithmetic.
    expect(screen.getByRole("table")).toHaveAttribute("aria-rowcount", "501");
    const rows = screen.getAllByRole("row");
    expect(rows[0]).toHaveAttribute("aria-rowindex", "1");
    // Row 91 of the data, after one header row, is row 92 of the table — and a
    // reader on page ten being told "row 1 of 10" looks exactly like a reader
    // being told the truth.
    expect(rows[1]).toHaveAttribute("aria-rowindex", "92");
    expect(rows[3]).toHaveAttribute("aria-rowindex", "94");
  });

  it("counts nothing when it is showing everything", () => {
    render(<Example />);
    // The browser counts the rows itself, and a second source of truth is one
    // that can disagree with the document.
    expect(screen.getByRole("table")).not.toHaveAttribute("aria-rowcount");
    expect(screen.getAllByRole("row")[1]).not.toHaveAttribute("aria-rowindex");
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<Table.Head>orphan</Table.Head>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Table.Head must be rendered inside a Table.Root");
  });
});

describe("Pagination", () => {
  component Example(page?: number = 4, pageCount?: number = 25) {
    return (
      <Pagination.Root page={page} pageCount={pageCount}>
        <Pagination.Content>
          <Pagination.Previous disabled={page === 1} href={`?page=${String(page - 1)}`}>
            ‹
          </Pagination.Previous>
          <Pagination.Item href="?page=3">3</Pagination.Item>
          <Pagination.Item current href="?page=4">
            4
          </Pagination.Item>
          <Pagination.Item href="?page=5">5</Pagination.Item>
          <Pagination.Next href={`?page=${String(page + 1)}`}>›</Pagination.Next>
        </Pagination.Content>
      </Pagination.Root>
    );
  }

  it("marks the current page and names the pagination", () => {
    render(<Example />);
    // A page has more than one `nav`, and an unnamed one is announced as
    // "navigation" with no way to tell it from the site's menu.
    const nav = screen.getByRole("navigation", { name: "Pagination" });
    expect(nav).toBeInTheDocument();
    // `aria-current="page"` and exactly one of it. Not a class, not bold text,
    // not `aria-selected` — `page` is the value ARIA defines for this and the
    // only one that tells a reader where they are. Asked as a role query,
    // because "exactly one control is current" is a fact about what is
    // announced; reading the attribute back off a link found by its name says
    // less, and was all this could say while `current` was an option
    // `getByRole` accepted and ignored (ubugeeei-prod/uf#359).
    expect(within(nav).getByRole("link", { current: "page" }).textContent).toBe("4");
    expect(within(nav).getAllByRole("link", { current: false }).length).toBe(4);
  });

  it("names previous and next in words rather than in chevrons", () => {
    render(<Example />);
    // "link, single left-pointing angle quotation mark" is not a thing anybody
    // can act on. The glyph stays; the name is words.
    expect(screen.getByRole("link", { name: "Previous page" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Next page" })).toBeInTheDocument();
  });

  it("is not a link at all when there is nowhere to go", () => {
    render(<Example page={1} />);
    // There is no such thing as a disabled link: an `<a>` with no `href` is
    // out of the tab order and is not announced as a link, which is exactly
    // what "there is no previous page" means.
    expect(screen.queryByRole("link", { name: "Previous page" })).toBe(null);
    expect(screen.getByRole("link", { name: "Next page" })).toBeInTheDocument();
  });

  it("says which page it moved to, in a region that was already there", () => {
    const { rerender } = render(<Example page={4} />);
    expect(screen.getByRole("status").textContent).toBe("Page 4 of 25.");
    rerender(<Example page={5} />);
    // Pressing "next" replaces the rows and moves nothing a reader is looking
    // at, so the sentence is the only thing that tells them it worked.
    expect(screen.getByRole("status").textContent).toBe("Page 5 of 25.");
  });

  it("is watching before it has anything to say", () => {
    render(
      <Pagination.Root>
        <Pagination.Content>
          <Pagination.Item href="?page=1">1</Pagination.Item>
        </Pagination.Content>
      </Pagination.Root>,
    );
    // Given no page to announce it is still in the document, empty, because a
    // live region that appears together with its text is not announced at all.
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("is a list, so a reader can skip it in one keystroke", () => {
    render(<Example />);
    expect(within(screen.getByRole("navigation")).getAllByRole("listitem").length).toBe(5);
  });
});

describe("Breadcrumb", () => {
  component Example() {
    return (
      <Breadcrumb.Root>
        <Breadcrumb.List>
          <Breadcrumb.Item>
            <Breadcrumb.Link href="/">Home</Breadcrumb.Link>
          </Breadcrumb.Item>
          <Breadcrumb.Separator>/</Breadcrumb.Separator>
          <Breadcrumb.Item>
            <Breadcrumb.Link href="/settings">Settings</Breadcrumb.Link>
          </Breadcrumb.Item>
          <Breadcrumb.Separator>/</Breadcrumb.Separator>
          <Breadcrumb.Item>
            <Breadcrumb.Page>Billing</Breadcrumb.Page>
          </Breadcrumb.Item>
        </Breadcrumb.List>
      </Breadcrumb.Root>
    );
  }

  it("reads a breadcrumb as a trail and not as punctuation", () => {
    render(<Example />);
    // A page has more than one `nav`, and an unnamed one is announced as
    // "navigation" with nothing to tell it from the site's menu. This is the
    // name assistive technology's own documentation tells readers to look for.
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
    // Three places and two slashes, and a reader is told about three things.
    // The separators have to be `<li>` because an `<ol>` may hold nothing else,
    // so without `role="presentation"` and `aria-hidden` on them the trail
    // would be announced as a list of five, two of them punctuation.
    expect(within(nav).getAllByRole("listitem").length).toBe(3);
    // Exactly one place the reader is at, and it is the last one.
    const current = nav.querySelectorAll('[aria-current="page"]');
    expect(current.length).toBe(1);
    expect(current[0].textContent).toBe("Billing");
    // And nothing announced as a link is a slash: the two crumbs that go
    // somewhere are the whole of what a reader can act on.
    expect(
      within(nav)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["Home", "Settings"]);
  });

  it("does not announce the page you are on as a link", () => {
    render(<Example />);
    // The shape this is usually copied with gives the current page
    // `role="link"` with `aria-disabled`, which announces "link, dimmed" — a
    // control the reader cannot use, rather than the place they arrived at.
    expect(screen.queryByRole("link", { name: "Billing" })).toBe(null);
  });

  it("is a list, so a reader can skip the whole trail in one keystroke", () => {
    render(<Example />);
    expect(within(screen.getByRole("navigation")).getAllByRole("list").length).toBe(1);
  });

  it("takes another name, for a page that is not written in English", () => {
    render(
      <Breadcrumb.Root label="Fil d'Ariane">
        <Breadcrumb.List>
          <Breadcrumb.Item>
            <Breadcrumb.Page>Facturation</Breadcrumb.Page>
          </Breadcrumb.Item>
        </Breadcrumb.List>
      </Breadcrumb.Root>,
    );
    expect(screen.getByRole("navigation", { name: "Fil d'Ariane" })).toBeInTheDocument();
  });

  it("hands the trail contract to caller-rendered elements", () => {
    render(
      <Breadcrumb.Root label="Trail" render={(props) => <section {...props} />}>
        <Breadcrumb.List render={(props) => <div {...props} data-testid="list" />}>
          <Breadcrumb.Item render={(props) => <div {...props} data-testid="home" />}>
            <Breadcrumb.Link href="/" render={(props) => <span {...props} />}>
              Home
            </Breadcrumb.Link>
          </Breadcrumb.Item>
          <Breadcrumb.Separator render={(props) => <span {...props} data-testid="slash" />}>
            /
          </Breadcrumb.Separator>
          <Breadcrumb.Item render={(props) => <div {...props} data-testid="billing" />}>
            <Breadcrumb.Page render={(props) => <strong {...props} />}>Billing</Breadcrumb.Page>
          </Breadcrumb.Item>
        </Breadcrumb.List>
      </Breadcrumb.Root>,
    );
    const trail = screen.getByRole("navigation", { name: "Trail" });
    expect(trail.tagName).toBe("SECTION");
    expect(screen.getByTestId("list")).toHaveAttribute("role", "list");
    expect(screen.getAllByRole("listitem").length).toBe(2);
    expect(screen.getByRole("link", { name: "Home" }).tagName).toBe("SPAN");
    expect(screen.getByText("Billing")).toHaveAttribute("aria-current", "page");
    expect(screen.getByTestId("slash")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("slash")).toHaveAttribute("role", "presentation");
  });
});

describe("Alert", () => {
  component Example(failed: boolean) {
    return (
      <div>
        <Alert.Root>
          <Alert.Title>Your trial ends on Friday</Alert.Title>
          <Alert.Description>Add a card to keep your projects.</Alert.Description>
        </Alert.Root>
        {failed ? (
          <Alert.Root live>
            <Alert.Title>Could not save</Alert.Title>
            <Alert.Description>The server said no.</Alert.Description>
          </Alert.Root>
        ) : null}
      </div>
    );
  }

  it("does not announce a callout that was always there", () => {
    const { rerender } = render(<Example failed={false} />);
    // `role="alert"` is a live region, and a live region on an element that was
    // in the document when the page loaded announces on insertion or not at
    // all. A permanently rendered "your trial ends soon" box carrying one is
    // therefore an interruption on every page load, or silence, and neither is
    // what anybody wanted — so the static callout has no live semantics of any
    // kind, not the assertive one and not the polite one.
    expect(screen.queryByRole("alert")).toBe(null);
    expect(screen.queryByRole("status")).toBe(null);
    // And the one that appeared because something happened does, which is the
    // whole distinction this component exists to make.
    rerender(<Example failed={true} />);
    expect(screen.getByRole("alert").textContent).toContain("Could not save");
    // Still exactly one: the callout that was always there did not acquire a
    // role by standing next to one that has it.
    expect(screen.getAllByRole("alert").length).toBe(1);
  });

  it("puts the callout in the document outline", () => {
    render(<Example failed={false} />);
    // A heading rather than a bold `<div>`, because a heading is how a screen
    // reader user reaches a region of a page without reading the page.
    expect(
      screen.getByRole("heading", { level: 3, name: "Your trial ends on Friday" }),
    ).toBeInTheDocument();
  });

  it("takes the heading level from the caller", () => {
    render(
      <Alert.Root>
        <Alert.Title level={2}>Your trial ends on Friday</Alert.Title>
      </Alert.Root>,
    );
    // The level that keeps a document outline true depends on what the callout
    // is inside, which is `Accordion.Header`'s argument for the same prop.
    expect(screen.getByRole("heading", { level: 2 })).toBeInTheDocument();
  });

  it("never renders a heading nobody recognises", () => {
    render(
      <Alert.Root>
        <Alert.Title level={9}>Your trial ends on Friday</Alert.Title>
      </Alert.Root>,
    );
    // `<h9>` is not an element, and a tag nobody recognises is announced as
    // nothing at all — which loses the heading rather than deepening it.
    expect(screen.getByRole("heading", { level: 6 })).toBeInTheDocument();
  });

  it("hands the callout contract to caller-rendered elements", () => {
    render(
      <Alert.Root live render={(props) => <section {...props} data-testid="callout" />}>
        <Alert.Title level={2} render={(props) => <span {...props} />}>
          Could not save
        </Alert.Title>
        <Alert.Description render={(props) => <div {...props} />}>
          The server said no.
        </Alert.Description>
      </Alert.Root>,
    );

    expect(screen.getByTestId("callout")).toHaveAttribute("role", "alert");
    expect(screen.getByRole("heading", { level: 2, name: "Could not save" })).toBeInTheDocument();
    expect(screen.getByText("The server said no.")).toBeInTheDocument();
  });
});

describe("Avatar", () => {
  component Example(src?: string | null = "/ada.png") {
    return (
      <Avatar.Root>
        <Avatar.Image src={src} />
        <Avatar.Fallback>AL</Avatar.Fallback>
      </Avatar.Root>
    );
  }

  /** The `<img>` this avatar is rendering, while it is still rendering one. */
  function pictureIn(container: Element): Element {
    const image = container.querySelector("img");
    if (image == null) {
      throw new Error("the avatar is rendering no image");
    }
    return image;
  }

  it("shows the fallback only when the image fails", () => {
    const { container } = render(<Example />);
    const image = pictureIn(container);
    // Loading: the fallback is not there. The two-state version renders it
    // whenever the image has not painted, which on a cached image is a flash of
    // somebody's initials on every navigation, for ever.
    expect(screen.queryByText("AL")).toBe(null);
    // Failed: it is.
    fireEvent.error(image);
    expect(screen.getByText("AL")).toBeInTheDocument();
    // And the element that failed has gone rather than staying to show the
    // browser's broken-image glyph beside the fallback that replaced it — which
    // a package shipping no styles cannot leave to a stylesheet, because
    // `hidden` loses to any `display` the caller sets.
    expect(container.querySelector("img")).toBe(null);
  });

  it("gives the image an empty alt unless the caller wrote one", () => {
    const { container, rerender } = render(<Example />);
    // An avatar sits beside the name of the person it is a picture of, and a
    // component that helpfully puts that name in `alt` makes every screen
    // reader say it twice. An empty `alt` is what takes the image out of the
    // accessibility tree, which is what "decorative" means and what this is.
    //
    // Asserted as the attribute rather than through `getByRole("img", { name })`
    // because this harness's accessible name does not read `alt` — the query
    // would find nothing whichever answer the component gave, which is a test
    // that cannot fail rather than one that passes.
    expect(pictureIn(container)).toHaveAttribute("alt", "");
    // And the caller whose picture is the only name there is says so and is
    // believed.
    rerender(
      <Avatar.Root>
        <Avatar.Image alt="Ada Lovelace" src="/ada.png" />
      </Avatar.Root>,
    );
    expect(pictureIn(container)).toHaveAttribute("alt", "Ada Lovelace");
  });

  it("hands the avatar state contract to caller-rendered elements", () => {
    render(
      <Avatar.Root render={(props) => <span {...props} data-testid="avatar-root" />}>
        <Avatar.Image
          alt="Ada Lovelace"
          render={(props) => <img {...props} data-testid="avatar-image" />}
          src="/ada.png"
        />
        <Avatar.Fallback
          delay={0}
          render={(props) => <strong {...props} data-testid="avatar-fallback" />}
        >
          AL
        </Avatar.Fallback>
      </Avatar.Root>,
    );

    const image = screen.getByTestId("avatar-image");
    expect(screen.getByTestId("avatar-root").contains(image)).toBe(true);
    expect(image).toHaveAttribute("alt", "Ada Lovelace");
    fireEvent.error(image);
    expect(screen.queryByTestId("avatar-image")).toBe(null);
    expect(screen.getByTestId("avatar-fallback").tagName).toBe("STRONG");
    expect(screen.getByTestId("avatar-fallback").textContent).toBe("AL");
  });

  it("takes the fallback away again once the image arrives", () => {
    const { container } = render(<Example />);
    fireEvent.load(pictureIn(container));
    expect(screen.queryByText("AL")).toBe(null);
  });

  it("shows the fallback at once when there is no image to wait for", () => {
    render(
      <Avatar.Root>
        <Avatar.Fallback>AL</Avatar.Fallback>
      </Avatar.Root>,
    );
    // Nothing is loading, so nothing is being waited for. A delay here would be
    // a hole in the page for everybody who has never uploaded a photograph.
    expect(screen.getByText("AL")).toBeInTheDocument();
  });

  it("gives up on an image that never arrives", () => {
    uft.useFakeTimers();
    render(<Example />);
    expect(screen.queryByText("AL")).toBe(null);
    act(() => {
      uft.advanceTimersByTime(300);
    });
    // The delay is how long "still loading" is allowed to last before the
    // fallback appears anyway: long enough that a cached image never flashes
    // initials, short enough that a slow one does not leave a hole.
    expect(screen.getByText("AL")).toBeInTheDocument();
    uft.useRealTimers();
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<Avatar.Fallback>AL</Avatar.Fallback>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Avatar.Fallback must be rendered inside an Avatar.Root");
  });
});

describe("Separator", () => {
  it("keeps a decorative rule out of the accessibility tree", () => {
    const { rerender } = render(<Separator decorative />);
    // The hairline under a heading is a border that happens to be an element.
    // Announced, it adds a "separator" to every reading of the page.
    expect(screen.queryAllByRole("separator")).toEqual([]);
    rerender(<Separator />);
    // The rule between two groups of content is the opposite: it is the only
    // way a reader who is not looking at the page is told the subject changed.
    const rules = screen.getAllByRole("separator");
    expect(rules.length).toBe(1);
    expect(rules[0]).toHaveAttribute("aria-orientation", "horizontal");
  });

  it("says which way it runs", () => {
    render(<Separator orientation="vertical" />);
    expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "vertical");
  });

  it("is announced by default, because the silent mistake is the worse one", () => {
    render(<Separator />);
    // A rule wrongly announced is noise a reader can hear and skip past. A
    // boundary wrongly silent is information that is simply not there, and
    // nobody finds out — so the default is the mistake that can be corrected.
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("hands the chosen separator semantics to a caller-rendered element", () => {
    const { rerender } = render(
      <Separator
        orientation="vertical"
        render={(props) => <span {...props} data-testid="rule" />}
      />,
    );
    const rule = screen.getByRole("separator");
    expect(rule.tagName).toBe("SPAN");
    expect(rule).toHaveAttribute("aria-orientation", "vertical");

    rerender(<Separator decorative render={(props) => <span {...props} data-testid="rule" />} />);
    expect(screen.queryByRole("separator")).toBe(null);
    expect(screen.getByTestId("rule")).toHaveAttribute("aria-hidden", "true");
  });
});

describe("Skeleton", () => {
  component Example(busy: boolean) {
    return (
      <Skeleton.Root busy={busy}>
        {busy ? <Skeleton.Box>Invoice</Skeleton.Box> : <p>Two invoices, both overdue.</p>}
        {busy ? <Skeleton.Box /> : null}
      </Skeleton.Root>
    );
  }

  it("says the page is loading rather than showing empty boxes", () => {
    const { container } = render(<Example busy={true} />);
    // A screen of skeletons is a screen of empty `<div>`s to everybody who is
    // not looking at it. Three attributes fix that and none of them is on the
    // grey box's class name.
    expect(container.querySelectorAll('[aria-hidden="true"]').length).toBe(2);
    expect(container.querySelectorAll('[aria-busy="true"]').length).toBe(1);
    // And something says so out loud, because `aria-busy` is a property a
    // reader may ask about rather than an announcement they are given.
    expect(screen.getByRole("status").textContent).toBe("Loading…");
  });

  it("is watching before it has anything to say", () => {
    const { rerender } = render(<Example busy={false} />);
    // ubugeeei-prod/uf#289's rule, met where it bites hardest. A live region
    // that appears together with its text is not announced, so the region is in
    // the document holding nothing…
    const region = screen.getByRole("status");
    expect(region.textContent).toBe("");
    rerender(<Example busy={true} />);
    // …and it is that same element that fills in, rather than a new one
    // arriving with its sentence already inside it.
    expect(screen.getByRole("status")).toBe(region);
    expect(region.textContent).toBe("Loading…");
  });

  it("says the wait is over", () => {
    const { rerender } = render(<Example busy={true} />);
    expect(screen.getByRole("status").textContent).toBe("Loading…");
    rerender(<Example busy={false} />);
    // The one thing this component asks of a caller: keep the root mounted
    // across the load. Unmounted, the region goes with it, and the reader is
    // left with the last thing they heard, which was "loading".
    expect(screen.getByRole("status").textContent).toBe("Loaded");
    expect(screen.getByText("Two invoices, both overdue.")).toBeInTheDocument();
  });

  it("does not tell a reader who never waited that it has loaded", () => {
    render(<Example busy={false} />);
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("stops saying it is busy once it is not", () => {
    const { container, rerender } = render(<Example busy={true} />);
    rerender(<Example busy={false} />);
    // `aria-busy="false"` and no `aria-busy` say the same thing, and the one
    // that is not written cannot be written wrong.
    expect(container.querySelector("[aria-busy]")).toBe(null);
  });

  it("takes the wording, for a page that is not written in English", () => {
    render(
      <Skeleton.Root busy={true} label="Chargement…">
        <Skeleton.Box />
      </Skeleton.Root>,
    );
    expect(screen.getByRole("status").textContent).toBe("Chargement…");
  });

  it("hands the busy region and hidden boxes to caller-rendered elements", () => {
    render(
      <Skeleton.Root busy={true} render={(props) => <section {...props} data-testid="loading" />}>
        <Skeleton.Box render={(props) => <span {...props} data-testid="placeholder" />}>
          Invoice
        </Skeleton.Box>
      </Skeleton.Root>,
    );

    const loading = screen.getByTestId("loading");
    const placeholder = screen.getByTestId("placeholder");
    expect(loading.tagName).toBe("SECTION");
    expect(loading).toHaveAttribute("aria-busy", "true");
    expect(loading.contains(placeholder)).toBe(true);
    expect(placeholder.tagName).toBe("SPAN");
    expect(placeholder).toHaveAttribute("aria-hidden", "true");
    expect(placeholder.textContent).toBe("Invoice");
    expect(screen.getByRole("status").textContent).toBe("Loading…");
  });
});

describe("the five that are one element, audited rather than asserted", () => {
  // Every other case in this file states one promise at a time — a role, a key,
  // an attribute — which is the right shape for a promise somebody made on
  // purpose. An audit is the other half: it reads the tree that was actually
  // rendered and finds the mistakes nobody thought to write a case for. An
  // `<ol>` holding a `<div>`, an `aria-*` on an element that may not carry it,
  // a live region announced twice.
  //
  // These five are where it is worth spending, because they are the components
  // whose whole content is `aria-*` — `packages/test/axe.test.js` proves the
  // matcher works and this is the matcher pointed at what it was built for.
  //
  // Inside a `<main>` with a heading because the rule set includes the
  // page-level rules: a fragment audited on its own is reported for having no
  // landmark, which is a fact about the fragment and not about the component.

  it("gives an engine nothing to report", async () => {
    const { container } = render(
      <main>
        <h1>Billing</h1>
        <Breadcrumb.Root>
          <Breadcrumb.List>
            <Breadcrumb.Item>
              <Breadcrumb.Link href="/">Home</Breadcrumb.Link>
            </Breadcrumb.Item>
            <Breadcrumb.Separator>/</Breadcrumb.Separator>
            <Breadcrumb.Item>
              <Breadcrumb.Page>Billing</Breadcrumb.Page>
            </Breadcrumb.Item>
          </Breadcrumb.List>
        </Breadcrumb.Root>
        <Alert.Root live>
          {/*
            `level={2}` because the callout is under the page's `<h1>`, and the
            default of 3 would skip a level. That the audit says so is the
            argument for the prop: a hard-coded heading level is an outline
            nobody can navigate, and only the caller knows what it is inside.
          */}
          <Alert.Title level={2}>Could not save</Alert.Title>
          <Alert.Description>The server said no.</Alert.Description>
        </Alert.Root>
        <Separator />
        <Separator decorative orientation="vertical" />
        <Avatar.Root>
          <Avatar.Image src="/ada.png" />
          <Avatar.Fallback>AL</Avatar.Fallback>
        </Avatar.Root>
        <Skeleton.Root busy={true}>
          <Skeleton.Box>Invoice</Skeleton.Box>
        </Skeleton.Root>
      </main>,
    );
    await expect(container).toHaveNoAxeViolations();
  });
});

describe("Switch and Checkbox", () => {
  it("announces a switch as a switch, not a checkbox", () => {
    render(<Switch aria-label="Notifications" />);
    // A screen reader says "on"/"off" for a switch and "checked"/"unchecked"
    // for a checkbox; the wrong role tells the reader the wrong thing.
    expect(screen.getByRole("switch")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).toBe(null);
  });

  it("toggles on click", async () => {
    render(<Switch aria-label="Notifications" />);
    const control = screen.getByRole("switch");
    expect(control).not.toBeChecked();
    await userEvent.click(control);
    expect(control).toBeChecked();
  });

  it("toggles a switch on Space and on Enter", async () => {
    render(<Switch aria-label="Notifications" />);
    const control = screen.getByRole("switch");
    await userEvent.click(control);
    await userEvent.keyboard(" ");
    expect(control).not.toBeChecked();
    await userEvent.keyboard("{Enter}");
    expect(control).toBeChecked();
  });

  it("toggles a checkbox on Space", async () => {
    render(<Checkbox aria-label="Subscribe" />);
    const control = screen.getByRole("checkbox");
    control.focus();
    await userEvent.keyboard(" ");
    expect(control).toBeChecked();
  });

  it("submits the form on Enter rather than toggling itself", async () => {
    const onSubmit = fn();
    render(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <Checkbox aria-label="Subscribe" />
        <button type="submit">Sign up</button>
      </form>,
    );
    const control = screen.getByRole("checkbox");
    // Focused, which is the only version of this that would ever have failed —
    // ubugeeei-prod/uf#324. The old case pressed Enter at `<body>` and then
    // fired a bare `keyDown`, and neither of those produces the browser's own
    // click, which is the default action of Enter on a focused `<button>` and
    // the thing that used to toggle this control.
    control.focus();
    // Prevented: the click, and the toggle behind it, do not happen.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(control).not.toBeChecked();
    // And the form is submitted, which is what "left to the form" always meant
    // and what a `<button type="button">` had never once done.
    expect(onSubmit).toHaveBeenCalled();
  });

  it("submits through the default button the form owns from outside it", () => {
    let submitter = null;
    render(
      <div>
        <form
          id="signup"
          onSubmit={(event) => {
            event.preventDefault();
            submitter = (event.nativeEvent as $FlowFixMe).submitter;
          }}
        >
          <Checkbox aria-label="Subscribe" />
        </form>
        <button form="signup" type="submit">
          Sign up
        </button>
      </div>,
    );
    const control = screen.getByRole("checkbox");
    control.focus();
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    // A form's default button is the first submit button *it owns*, which is
    // not the same as the first one inside it: a footer button beside the form
    // is the everyday spelling, and a subtree search never sees it. Missing it
    // falls through to a submission with no submitter, which is the one thing
    // passing the button was for.
    expect(submitter).toBe(screen.getByRole("button", { name: "Sign up" }));
  });

  it("leaves a buttonless form alone when two of its fields block submission", () => {
    const onSubmit = fn();
    render(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <input aria-label="Email" type="email" />
        <input aria-label="Password" type="password" />
        <Checkbox aria-label="Remember me" />
      </form>,
    );
    const control = screen.getByRole("checkbox");
    control.focus();
    // What the platform does, which is the whole claim: a form with no submit
    // button submits implicitly only while at most one field blocks it, and
    // `Enter` in either of these two text fields does nothing in any browser.
    // `requestSubmit()` does not know that rule, so this component applies it.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(onSubmit).not.toHaveBeenCalled();
    expect(control).not.toBeChecked();
  });

  it("submits a buttonless form with one blocking field, which is the search box", () => {
    const onSubmit = fn();
    render(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <input aria-label="Query" type="search" />
        <Checkbox aria-label="Match case" />
      </form>,
    );
    const control = screen.getByRole("checkbox");
    control.focus();
    // The other side of the same rule. A checkbox is not a blocking field, so
    // one search box and any number of checkboxes still submits.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(onSubmit).toHaveBeenCalled();
  });

  it("does nothing on Enter outside a form", () => {
    render(<Checkbox aria-label="Subscribe" />);
    const control = screen.getByRole("checkbox");
    control.focus();
    // Which is what a native `<input type="checkbox">` with no form around it
    // does with the key: implicit submission needs a form to submit.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(control).not.toBeChecked();
  });

  it("does not submit on Enter while it is disabled", () => {
    const onSubmit = fn();
    render(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <Checkbox aria-label="Subscribe" disabled />
        <button type="submit">Sign up</button>
      </form>,
    );
    const control = screen.getByRole("checkbox");
    fireEvent.keyDown(control, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("does not toggle while disabled", async () => {
    render(<Switch aria-label="Notifications" disabled />);
    const control = screen.getByRole("switch");
    await userEvent.click(control);
    expect(control).not.toBeChecked();
  });

  it("reports a checkbox's third state as mixed", () => {
    render(<Checkbox aria-label="Select all" indeterminate />);
    expect(screen.getByRole("checkbox")).toHaveAttribute("aria-checked", "mixed");
  });

  it("moves a mixed checkbox to checked rather than to its opposite", async () => {
    const onCheckedChange = fn();
    render(<Checkbox aria-label="Select all" indeterminate onCheckedChange={onCheckedChange} />);
    await userEvent.click(screen.getByRole("checkbox"));
    // A half-selected "select all" that clears itself on the first click is the
    // behaviour every table in every application gets wrong.
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("lets a parent own the value", async () => {
    component Controlled() {
      const [on, setOn] = useState(false);
      return (
        <div>
          <Switch aria-label="Notifications" checked={on} onCheckedChange={setOn} />
          <output>{on ? "on" : "off"}</output>
        </div>
      );
    }
    render(<Controlled />);
    await userEvent.click(screen.getByRole("switch"));
    expect(screen.getByText("on")).toBeInTheDocument();
  });

  it("keeps a parent that refuses a change in charge of it", async () => {
    component Refusing() {
      const [on, setOn] = useState(false);
      return (
        <Switch aria-label="Notifications" checked={on} onCheckedChange={() => setOn(false)} />
      );
    }
    render(<Refusing />);
    await userEvent.click(screen.getByRole("switch"));
    // A controlled component that also writes its own state moves anyway and is
    // moved back on the next render, which reads as a flicker and is a bug.
    expect(screen.getByRole("switch")).not.toBeChecked();
  });
});

describe("Toggle", () => {
  it("announces a toggle button as pressed, not as checked", () => {
    render(<Toggle aria-label="Bold" />);
    // The mirror of "announces a switch as a switch, not a checkbox", and the
    // pair is what documents why there are three of these. A reader told
    // "checkbox" believes they are answering a question; told "switch", that
    // they are configuring something. A toggle button does neither: it is an
    // action that stays applied.
    const control = screen.getByRole("button", { name: "Bold" });
    expect(control).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("switch")).toBe(null);
    expect(screen.queryByRole("checkbox")).toBe(null);
  });

  it("presses on click, on Space and on Enter", async () => {
    render(<Toggle aria-label="Bold" />);
    const control = screen.getByRole("button", { name: "Bold" });
    await userEvent.click(control);
    expect(control).toHaveAttribute("aria-pressed", "true");
    await userEvent.keyboard(" ");
    expect(control).toHaveAttribute("aria-pressed", "false");
    // A toggle button is a button, and a button activates on both keys.
    await userEvent.keyboard("{Enter}");
    expect(control).toHaveAttribute("aria-pressed", "true");
  });

  it("hands toggle-button behaviour to a caller-rendered element", async () => {
    render(<Toggle aria-label="Bold" render={(props) => <div {...props} tabIndex={0} />} />);
    const control = screen.getByRole("button", { name: "Bold" });
    expect(control.tagName).toBe("DIV");
    expect(control).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(control);
    expect(control).toHaveAttribute("aria-pressed", "true");
    control.focus();
    await userEvent.keyboard(" ");
    expect(control).toHaveAttribute("aria-pressed", "false");
  });

  it("does not press while disabled", async () => {
    render(<Toggle aria-label="Bold" disabled />);
    await userEvent.click(screen.getByRole("button", { name: "Bold" }));
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-pressed", "false");
  });

  it("lets a parent own the state, and refuse a change", async () => {
    component Refusing() {
      const [on, setOn] = useState(false);
      return <Toggle aria-label="Bold" onPressedChange={() => setOn(false)} pressed={on} />;
    }
    render(<Refusing />);
    await userEvent.click(screen.getByRole("button", { name: "Bold" }));
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-pressed", "false");
  });
});

describe("RadioGroup", () => {
  component Plans(orientation?: "horizontal" | "vertical" = "vertical") {
    return (
      <Field.Root>
        <Field.Label>Plan</Field.Label>
        <Field.Control
          render={(props) => (
            <RadioGroup.Root {...props} defaultValue="free" orientation={orientation}>
              <RadioGroup.Item value="free">
                Free
                <RadioGroup.Indicator>dot</RadioGroup.Indicator>
              </RadioGroup.Item>
              <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
              <RadioGroup.Item value="team">Team</RadioGroup.Item>
            </RadioGroup.Root>
          )}
        />
      </Field.Root>
    );
  }

  it("announces itself as a named radio group of radios", () => {
    render(<Plans />);
    // The name comes from `Field.Label` through `Field.Control`, which is the
    // wiring `field.js` exists to get right; a second spelling of it in
    // `radio-group.js` would be a second thing to keep in step.
    expect(screen.getByRole("radiogroup", { name: "Plan" })).toHaveAttribute(
      "aria-orientation",
      "vertical",
    );
    expect(screen.getAllByRole("radio").length).toBe(3);
    expect(danglingReferences()).toEqual([]);
  });

  it("checks as it moves, because that is what a radio group does", async () => {
    render(<Plans />);
    await userEvent.click(screen.getByRole("radio", { name: /Free/ }));
    await userEvent.keyboard("{ArrowDown}");
    // One key press, focus and the answer together. Arrows that only moved
    // focus would leave a reader believing they had answered when they had not.
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveFocus();
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /Free/ })).toHaveAttribute("aria-checked", "false");
  });

  it("wraps at the ends and jumps with Home and End", async () => {
    render(<Plans />);
    await userEvent.click(screen.getByRole("radio", { name: /Free/ }));
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("radio", { name: "Team" })).toHaveAttribute("aria-checked", "true");
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("radio", { name: /Free/ })).toHaveAttribute("aria-checked", "true");
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("radio", { name: "Team" })).toHaveAttribute("aria-checked", "true");
  });

  it("uses the horizontal arrows when it is horizontal, and says which", async () => {
    render(<Plans orientation="horizontal" />);
    expect(screen.getByRole("radiogroup")).toHaveAttribute("aria-orientation", "horizontal");
    await userEvent.click(screen.getByRole("radio", { name: /Free/ }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-checked", "true");
    // And the vertical pair is left to the page, which scrolls with it.
    expect(
      fireEvent.keyDown(screen.getByRole("radio", { name: "Pro" }), { key: "ArrowDown" }),
    ).toBe(true);
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-checked", "true");
  });

  it("checks the focused item on Space", async () => {
    render(<Plans />);
    const pro = screen.getByRole("radio", { name: "Pro" });
    act(() => pro.focus());
    await userEvent.keyboard(" ");
    expect(pro).toHaveAttribute("aria-checked", "true");
  });

  it("lets Tab reach a group where nothing is chosen", () => {
    render(
      <RadioGroup.Root aria-label="Plan">
        <RadioGroup.Item value="free">Free</RadioGroup.Item>
        <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
      </RadioGroup.Root>,
    );
    // With the tab stop derived from the selection alone, an unanswered group
    // has no `tabindex="0"` at all and is not reachable from the keyboard:
    // not awkward to reach — absent.
    const stops = screen
      .getAllByRole("radio")
      .filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("Free");
  });

  it("gives the tab stop to the chosen item once there is one", async () => {
    render(<Plans />);
    const stops = () =>
      screen.getAllByRole("radio").filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops().length).toBe(1);
    expect(stops()[0].textContent).toContain("Free");
    await userEvent.click(screen.getByRole("radio", { name: "Team" }));
    expect(stops().length).toBe(1);
    expect(stops()[0].textContent).toBe("Team");
  });

  it("steps over a disabled choice and still announces it", async () => {
    render(
      <RadioGroup.Root aria-label="Plan" defaultValue="free">
        <RadioGroup.Item value="free">Free</RadioGroup.Item>
        <RadioGroup.Item disabled value="pro">
          Pro
        </RadioGroup.Item>
        <RadioGroup.Item value="team">Team</RadioGroup.Item>
      </RadioGroup.Root>,
    );
    // `aria-disabled` rather than `disabled`: the answer exists and is
    // unavailable, which is a thing a reader can be told. A native `disabled`
    // leaves a gap they cannot ask about.
    expect(screen.getAllByRole("radio").length).toBe(3);
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(screen.getByRole("radio", { name: "Free" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: "Team" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("radio", { name: "Pro" }));
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-checked", "false");
  });

  it("puts the tab stop on the first choice a reader can take", () => {
    render(
      <RadioGroup.Root aria-label="Plan">
        <RadioGroup.Item disabled value="free">
          Free
        </RadioGroup.Item>
        <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
      </RadioGroup.Root>,
    );
    // A group whose first answer is unavailable must still be reachable.
    const stops = screen
      .getAllByRole("radio")
      .filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops.length).toBe(1);
    expect(stops[0].textContent).toBe("Pro");
  });

  it("shows the indicator only inside the chosen item", async () => {
    render(<Plans />);
    expect(screen.getAllByText("dot").length).toBe(1);
    // And it is decoration: the item already says `aria-checked`, so a reader
    // who heard the dot as well would hear the answer's state twice.
    expect(screen.getByText("dot")).toHaveAttribute("aria-hidden", "true");
    await userEvent.click(screen.getByRole("radio", { name: "Pro" }));
    expect(screen.queryByText("dot")).toBe(null);
  });

  it("reports the answer to a controlled parent, and takes its refusal", async () => {
    const onValueChange = fn();
    component Refusing() {
      const [plan, setPlan] = useState<string | null>("free");
      return (
        <RadioGroup.Root
          aria-label="Plan"
          onValueChange={(next) => {
            onValueChange(next);
            setPlan("free");
          }}
          value={plan}
        >
          <RadioGroup.Item value="free">Free</RadioGroup.Item>
          <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
        </RadioGroup.Root>
      );
    }
    render(<Refusing />);
    await userEvent.click(screen.getByRole("radio", { name: "Pro" }));
    expect(onValueChange).toHaveBeenCalledWith("pro");
    // A controlled component that also writes its own state moves anyway and
    // is moved back on the next render, which reads as a flicker and is a bug.
    expect(screen.getByRole("radio", { name: "Pro" })).toHaveAttribute("aria-checked", "false");
  });

  it("submits the chosen value", async () => {
    render(
      <form data-testid="signup">
        <RadioGroup.Root aria-label="Plan" defaultValue="free" name="plan">
          <RadioGroup.Item value="free">Free</RadioGroup.Item>
          <RadioGroup.Item value="pro">Pro</RadioGroup.Item>
        </RadioGroup.Root>
      </form>,
    );
    const form: $FlowFixMe = screen.getByTestId("signup");
    // The document's own `FormData`, not the global one: this suite runs on
    // Node, whose `FormData` takes no arguments at all, so `new FormData(form)`
    // there throws rather than reading the form.
    const submitted = () => new form.ownerDocument.defaultView.FormData(form).get("plan");
    expect(submitted()).toBe("free");
    await userEvent.click(screen.getByRole("radio", { name: "Pro" }));
    // A control a reader can operate and a form cannot read is half a control.
    expect(submitted()).toBe("pro");
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<RadioGroup.Item value="free">orphan</RadioGroup.Item>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("RadioGroup.Item must be rendered inside a RadioGroup.Root");
  });
});

describe("ToggleGroup", () => {
  it("keeps a single toggle group to one answer", async () => {
    render(
      <ToggleGroup.Root aria-label="Alignment" defaultValue={["left"]} type="single">
        <ToggleGroup.Item value="left">Left</ToggleGroup.Item>
        <ToggleGroup.Item value="centre">Centre</ToggleGroup.Item>
        <ToggleGroup.Item value="right">Right</ToggleGroup.Item>
      </ToggleGroup.Root>,
    );
    // A reader told "three pressed buttons" will reasonably believe they may
    // press all three, and they may not — so a single group is a radio group,
    // and says so.
    expect(screen.getByRole("radiogroup")).toBeInTheDocument();
    expect(screen.getAllByRole("radio").length).toBe(3);
    expect(document.querySelectorAll("[aria-pressed]").length).toBe(0);

    await userEvent.click(screen.getByRole("radio", { name: "Right" }));
    expect(screen.getByRole("radio", { name: "Right" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Left" })).toHaveAttribute("aria-checked", "false");
  });

  it("keeps two items of a multiple group pressed at once", async () => {
    const onValueChange = fn();
    render(
      <ToggleGroup.Root aria-label="Formatting" onValueChange={onValueChange} type="multiple">
        <ToggleGroup.Item value="bold">Bold</ToggleGroup.Item>
        <ToggleGroup.Item value="italic">Italic</ToggleGroup.Item>
      </ToggleGroup.Root>,
    );
    expect(screen.getByRole("group")).toHaveAttribute("aria-orientation", "horizontal");
    expect(screen.queryByRole("radiogroup")).toBe(null);
    await userEvent.click(screen.getByRole("button", { name: "Bold" }));
    await userEvent.click(screen.getByRole("button", { name: "Italic" }));
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Italic" })).toHaveAttribute("aria-pressed", "true");
    expect(onValueChange).toHaveBeenCalledWith(["bold", "italic"]);
  });

  it("takes one tab stop for the whole set and moves it with the arrows", async () => {
    render(
      <ToggleGroup.Root aria-label="Formatting" type="multiple">
        <ToggleGroup.Item value="bold">Bold</ToggleGroup.Item>
        <ToggleGroup.Item value="italic">Italic</ToggleGroup.Item>
        <ToggleGroup.Item value="underline">Underline</ToggleGroup.Item>
      </ToggleGroup.Root>,
    );
    const stops = () =>
      within(screen.getByRole("group"))
        .getAllByRole("button")
        .filter((item) => item.getAttribute("tabindex") === "0");
    expect(stops().length).toBe(1);
    expect(stops()[0].textContent).toBe("Bold");

    act(() => screen.getByRole("button", { name: "Bold" }).focus());
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("button", { name: "Italic" })).toHaveFocus();
    // Moving does not press: arrowing across a toolbar to reach one command
    // must not apply the five it passed on the way.
    expect(screen.getByRole("button", { name: "Italic" })).toHaveAttribute("aria-pressed", "false");
    expect(stops().length).toBe(1);
    expect(stops()[0].textContent).toBe("Italic");
  });

  it("presses the focused item on Space, and steps over a disabled one", async () => {
    render(
      <ToggleGroup.Root aria-label="Formatting" type="multiple">
        <ToggleGroup.Item value="bold">Bold</ToggleGroup.Item>
        <ToggleGroup.Item disabled value="italic">
          Italic
        </ToggleGroup.Item>
        <ToggleGroup.Item value="underline">Underline</ToggleGroup.Item>
      </ToggleGroup.Root>,
    );
    // Announced, not removed: the same choice `menu.js` and `tabs.js` make.
    expect(screen.getByRole("button", { name: "Italic" })).toHaveAttribute("aria-disabled", "true");
    act(() => screen.getByRole("button", { name: "Bold" }).focus());
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("button", { name: "Underline" })).toHaveFocus();
    await userEvent.keyboard(" ");
    expect(screen.getByRole("button", { name: "Underline" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("inherits the radio group's keyboard for its single mode", async () => {
    render(
      <ToggleGroup.Root aria-label="Alignment" defaultValue={["left"]} type="single">
        <ToggleGroup.Item value="left">Left</ToggleGroup.Item>
        <ToggleGroup.Item value="centre">Centre</ToggleGroup.Item>
      </ToggleGroup.Root>,
    );
    // The point of rendering through `radio-group.js`: this behaviour has one
    // implementation, and a change to it cannot fix one of the two and not the
    // other.
    await userEvent.click(screen.getByRole("radio", { name: "Left" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("radio", { name: "Centre" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Left" })).toHaveAttribute("aria-checked", "false");
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<ToggleGroup.Item value="bold">orphan</ToggleGroup.Item>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("ToggleGroup.Item must be rendered inside a ToggleGroup.Root");
  });
});

describe("Collapsible", () => {
  component Details() {
    return (
      <Collapsible.Root>
        <Collapsible.Trigger>Details</Collapsible.Trigger>
        <Collapsible.Content>the small print</Collapsible.Content>
      </Collapsible.Root>
    );
  }

  it("says that the button controls something, and whether it is showing", async () => {
    render(<Details />);
    const trigger = screen.getByRole("button", { name: "Details" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger.getAttribute("aria-controls")).toBe(
      screen.getByText("the small print").getAttribute("id"),
    );
    expect(danglingReferences()).toEqual([]);
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("hands trigger and content behaviour to caller-rendered elements", async () => {
    const clicked = fn();
    render(
      <Collapsible.Root>
        <Collapsible.Trigger
          onClick={clicked}
          render={(props) => <a href="#details" {...props} />}
        >
          Details
        </Collapsible.Trigger>
        <Collapsible.Content render={(props) => <section {...props} data-testid="panel" />}>
          the small print
        </Collapsible.Content>
      </Collapsible.Root>,
    );

    const trigger = screen.getByRole("link", { name: "Details" });
    const content = screen.getByTestId("panel");
    expect(trigger.tagName).toBe("A");
    expect(trigger).toHaveAttribute("href", "#details");
    expect(trigger).not.toHaveAttribute("type");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger.getAttribute("aria-controls")).toBe(content.getAttribute("id"));
    expect(content.tagName).toBe("SECTION");
    expect(content.textContent).toBe("the small print");
    expect(content).toHaveAttribute("hidden", "until-found");

    await userEvent.click(trigger);
    expect(clicked).toHaveBeenCalled();
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(content).not.toHaveAttribute("hidden");
    expect(danglingReferences()).toEqual([]);
  });

  it("does not open a caller-rendered trigger while disabled", async () => {
    render(
      <Collapsible.Root>
        <Collapsible.Trigger
          disabled
          render={(props) => <div {...props} role="button" tabIndex={0} />}
        >
          Details
        </Collapsible.Trigger>
        <Collapsible.Content render={(props) => <section {...props} data-testid="panel" />}>
          the small print
        </Collapsible.Content>
      </Collapsible.Root>,
    );

    const trigger = screen.getByRole("button", { name: "Details" });
    expect(trigger.tagName).toBe("DIV");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("panel")).toHaveAttribute("hidden", "until-found");
  });

  it("claims no aria-controls when there is no content to name", () => {
    render(
      <Collapsible.Root>
        <Collapsible.Trigger>Details</Collapsible.Trigger>
      </Collapsible.Root>,
    );
    // A caller may render the content conditionally, or not at all until data
    // arrives. An `aria-controls` pointing at an id nothing has tells a reader
    // there is somewhere to go and then has nowhere to send them.
    expect(screen.getByRole("button", { name: "Details" })).not.toHaveAttribute("aria-controls");
  });

  it("keeps the closed content in the document, where find-in-page can reach it", async () => {
    render(<Details />);
    const content = screen.getByText("the small print");
    // Not `null`, which is what `Tabs.Panel` returns and what would take the
    // text out of the browser's find-in-page. `until-found` rather than a bare
    // `hidden` is the whole point, and React cannot say it through the prop —
    // `<div hidden="until-found">` renders `hidden=""` — so an effect upgrades
    // the attribute React has already committed.
    expect(content).toHaveAttribute("hidden", "until-found");
    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(content).not.toHaveAttribute("hidden");
  });

  it("lets a parent own whether it is open", async () => {
    const onOpenChange = fn();
    render(
      <Collapsible.Root onOpenChange={onOpenChange} open={false}>
        <Collapsible.Trigger>Details</Collapsible.Trigger>
        <Collapsible.Content>the small print</Collapsible.Content>
      </Collapsible.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(onOpenChange).toHaveBeenCalledWith(true);
    expect(screen.getByRole("button", { name: "Details" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });
});

describe("Accordion", () => {
  component Faq(
    collapsible?: boolean = true,
    level?: number = 3,
    type?: "single" | "multiple" = "single",
  ) {
    return (
      <Accordion.Root collapsible={collapsible} defaultValue={["shipping"]} type={type}>
        <Accordion.Item value="shipping">
          <Accordion.Header level={level}>
            <Accordion.Trigger>Shipping</Accordion.Trigger>
          </Accordion.Header>
          <Accordion.Content>ships in two days</Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="returns">
          <Accordion.Header level={level}>
            <Accordion.Trigger>Returns</Accordion.Trigger>
          </Accordion.Header>
          <Accordion.Content>thirty days</Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="warranty">
          <Accordion.Header level={level}>
            <Accordion.Trigger>Warranty</Accordion.Trigger>
          </Accordion.Header>
          <Accordion.Content>two years</Accordion.Content>
        </Accordion.Item>
      </Accordion.Root>
    );
  }

  // The panels a reader is actually told about, by their text. Every panel
  // keeps its role and its place in the document whether it is open or closed
  // — that is the point of `hidden="until-found"` — and a closed one is not
  // announced, so this is a role query and nothing else. It used to read the
  // `hidden` attribute back off the result, because the query returned the
  // closed panels too (ubugeeei-prod/uf#323).
  const showing = () => screen.getAllByRole("region").map((panel) => panel.textContent);

  it("names the region after the trigger that opens it", async () => {
    render(<Faq />);
    const region = screen.getAllByRole("region")[0];
    const named = region.getAttribute("aria-labelledby") ?? "";
    expect(document.getElementById(named)?.textContent).toBe("Shipping");
    // A region with no name is a landmark that says "region" and nothing else.
    expect(danglingReferences()).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Shipping" }));
    // And still true with everything closed, which is when a name pointing at
    // an unmounted trigger would have gone stale.
    expect(danglingReferences()).toEqual([]);
  });

  it("puts the trigger in a heading at the level the caller asked for", () => {
    render(<Faq level={3} />);
    // An accordion inside a section titled by an `<h2>` needs `<h3>`, and a
    // component that hard-codes one produces an outline nobody can navigate.
    // Asked by level rather than by tag name, because the level is the thing a
    // reader is told; the tag is how it happens to be spelt.
    const headings = screen.getAllByRole("heading", { level: 3 });
    expect(headings.length).toBe(3);
    expect(within(headings[0]).getByRole("button", { name: "Shipping" })).toBeInTheDocument();
    expect(screen.queryAllByRole("heading", { level: 2 })).toEqual([]);
  });

  it("takes a different heading level without changing anything else", () => {
    render(<Faq level={2} />);
    const headings = screen.getAllByRole("heading", { level: 2 });
    expect(headings.length).toBe(3);
    expect(within(headings[0]).getByRole("button", { name: "Shipping" })).toBeInTheDocument();
    expect(screen.queryAllByRole("heading", { level: 3 })).toEqual([]);
  });

  it("keeps a single accordion to one open item", async () => {
    render(<Faq />);
    expect(showing()).toEqual(["ships in two days"]);
    // All three are in the document and one of them is announced, which is the
    // difference `hidden` makes and the difference a role query has to see.
    expect(screen.getAllByRole("region", { hidden: true }).length).toBe(3);
    await userEvent.click(screen.getByRole("button", { name: "Returns" }));
    expect(showing()).toEqual(["thirty days"]);
    expect(screen.getByRole("button", { name: "Shipping" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("lets a multiple accordion hold two open at once", async () => {
    render(<Faq type="multiple" />);
    await userEvent.click(screen.getByRole("button", { name: "Returns" }));
    expect(showing()).toEqual(["ships in two days", "thirty days"]);
  });

  it("says why the open one cannot be closed, and keeps it announced", async () => {
    render(<Faq collapsible={false} />);
    const open = screen.getByRole("button", { name: "Shipping" });
    // `aria-disabled` rather than `disabled`: a reader is told "pressing this
    // does nothing" instead of finding that a header they can see has left the
    // accessibility tree.
    expect(open).toHaveAttribute("aria-disabled", "true");
    expect(open).toBeInTheDocument();
    await userEvent.click(open);
    expect(open).toHaveAttribute("aria-expanded", "true");
    // And the constraint is only about closing: another section still opens,
    // and the first trigger is then an ordinary one again.
    await userEvent.click(screen.getByRole("button", { name: "Returns" }));
    expect(showing()).toEqual(["thirty days"]);
    expect(screen.getByRole("button", { name: "Shipping" })).not.toHaveAttribute("aria-disabled");
  });

  it("leaves every header in the page's tab order", () => {
    render(<Faq />);
    // The inverse of the tab list's "keeps exactly one tab in the page's tab
    // order", and the pair is what documents that an accordion is a stack of
    // ordinary buttons rather than one control.
    const headers = screen.getAllByRole("button");
    expect(headers.length).toBe(3);
    for (const header of headers) {
      expect(header).not.toHaveAttribute("tabindex");
    }
  });

  it("moves between headers with the arrows, Home and End", async () => {
    render(<Faq />);
    act(() => screen.getByRole("button", { name: "Shipping" }).focus());
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: "Returns" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("button", { name: "Warranty" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("button", { name: "Shipping" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("button", { name: "Warranty" })).toHaveFocus();
  });

  it("lands the arrows on a header that cannot be pressed, rather than over it", async () => {
    render(<Faq collapsible={false} />);
    act(() => screen.getByRole("button", { name: "Returns" }).focus());
    // Every other set in this package steps over an `aria-disabled` item. This
    // one must not: `Tab` reaches all three headers, and arrows that skipped
    // the open one would disagree with `Tab` about which headers exist.
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("button", { name: "Shipping" })).toHaveFocus();
  });

  it("keeps the closed sections findable", async () => {
    render(<Faq />);
    const closed = screen.getByText("thirty days");
    expect(closed).toHaveAttribute("hidden", "until-found");
    await userEvent.click(screen.getByRole("button", { name: "Returns" }));
    expect(closed).not.toHaveAttribute("hidden");
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<Accordion.Trigger>orphan</Accordion.Trigger>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("Accordion.Trigger must be rendered inside an Accordion.Item");
  });
});

describe("the height a closed disclosure would have", () => {
  // ubugeeei-prod/uf#330. `height: 0 → var(--uf-collapsible-height)` is the
  // whole of animating a disclosure, and the number in that property is wanted
  // while the panel is still closed, because a transition has to know its
  // destination before it starts.

  /**
   * A panel with a height, and none while it is hidden.
   *
   * This DOM computes no layout, so `getBoundingClientRect` answers zero for
   * everything — which is *also* what a browser answers for an element that is
   * `display: none`, and the difference between those two zeroes is the whole
   * of what the measuring pass has to do. So the stub answers the way a browser
   * does: nothing while the element is hidden and nobody has overridden its
   * display, and the real height once it has been laid out. A component that
   * measured the hidden element would read `0px` here, exactly as it would in a
   * page.
   */
  function laidOut(element: HTMLElement, height: number): void {
    (element as $FlowFixMe).getBoundingClientRect = () => {
      const painted = !element.hasAttribute("hidden") || element.style.display !== "";
      const box = painted ? height : 0;
      return { bottom: box, height: box, left: 0, right: 0, top: 0, width: 0, x: 0, y: 0 };
    };
  }

  /**
   * A panel with the documented stylesheet on it, `height: 0` and all.
   *
   * `.panel { height: 0 }` with `.panel:not([hidden]) { height: var(…) }` is the
   * rule the property exists for, and the first half of it is *in force while
   * the panel is closed* — which is the moment the pass runs. So laying the
   * panel out is not enough on its own: an author `height: 0` that nothing
   * overrides measures zero however visible the box has been made. The stub
   * answers the way a browser would, which means only an inline `height` beats
   * it.
   */
  function styledClosed(element: HTMLElement, height: number): void {
    (element as $FlowFixMe).getBoundingClientRect = () => {
      const laidOut = !element.hasAttribute("hidden") || element.style.display !== "";
      const flattened = element.style.height === "" || element.style.height === "0px";
      const box = laidOut && !flattened ? height : 0;
      return { bottom: box, height: box, left: 0, right: 0, top: 0, width: 0, x: 0, y: 0 };
    };
  }

  /**
   * A panel whose text wraps, so its height depends on the width it is given.
   *
   * The other half of laying a hidden panel out: `position: absolute` makes a
   * box shrink-to-fit against its containing block rather than against its
   * parent, so the same paragraph is `unwrapped` tall out of flow and `wrapped`
   * tall where the panel actually lives.
   */
  function wrapsAt(
    element: HTMLElement,
    at: string,
    heights: { unwrapped: number, wrapped: number },
  ): void {
    (element as $FlowFixMe).getBoundingClientRect = () => {
      const laidOut = !element.hasAttribute("hidden") || element.style.display !== "";
      const box = laidOut ? (element.style.width === at ? heights.wrapped : heights.unwrapped) : 0;
      return { bottom: box, height: box, left: 0, right: 0, top: 0, width: 0, x: 0, y: 0 };
    };
  }

  /** A parent with a width, in a DOM that computes none. */
  function widthOfEveryParent(width: string): () => void {
    const host: $FlowFixMe = window;
    const previous = host.getComputedStyle;
    host.getComputedStyle = () => ({ width });
    return () => {
      host.getComputedStyle = previous;
    };
  }

  /** A `ResizeObserver` this file can fire by hand; there is none in this DOM. */
  const resizeCallbacks: Array<() => void> = [];
  function installResizeObserver(): () => void {
    const host: $FlowFixMe = window;
    const previous = host.ResizeObserver;
    host.ResizeObserver = function (callback: () => void) {
      resizeCallbacks.push(callback);
      return { disconnect: () => {}, observe: () => {} };
    };
    return () => {
      host.ResizeObserver = previous;
      resizeCallbacks.length = 0;
    };
  }

  const remeasure = () => {
    act(() => {
      for (const fire of resizeCallbacks) {
        fire();
      }
    });
  };

  const heightOf = (element: HTMLElement) =>
    element.style.getPropertyValue("--uf-collapsible-height");

  component Details(measure?: boolean = false) {
    return (
      <Collapsible.Root measure={measure}>
        <Collapsible.Trigger>Details</Collapsible.Trigger>
        <Collapsible.Content>the small print</Collapsible.Content>
      </Collapsible.Root>
    );
  }

  it("reports the height while the panel is still closed", () => {
    const restore = installResizeObserver();
    try {
      render(<Details measure />);
      const content = screen.getByText("the small print");
      expect(content).toHaveAttribute("hidden", "until-found");
      laidOut(content, 120);
      remeasure();

      // Not `0px`, which is what `ResizeObserver`, `getBoundingClientRect` and
      // `scrollHeight` all answer for a panel with no box — and which is
      // exactly the moment the number is wanted.
      expect(heightOf(content)).toBe("120px");
      // And the pass put everything back: still hidden, still findable, and no
      // inline layout left behind for a stylesheet to fight.
      expect(content).toHaveAttribute("hidden");
      expect(content.style.boxSizing).toBe("");
      expect(content.style.display).toBe("");
      expect(content.style.height).toBe("");
      expect(content.style.position).toBe("");
      expect(content.style.visibility).toBe("");
      expect(content.style.width).toBe("");
    } finally {
      restore();
    }
  });

  it("beats the `height: 0` the stylesheet has on while the panel is closed", () => {
    const restore = installResizeObserver();
    try {
      render(<Details measure />);
      const content = screen.getByText("the small print");
      styledClosed(content, 120);
      remeasure();

      // Laying the panel out is half the job. The other half is the author
      // declaration this property exists to replace: a panel measured with
      // `height: 0` still applying reports zero and writes back the `0px` the
      // caller asked it to fill, which is the whole bug wearing the rule it was
      // written for. Defeating `display` and not `height` is the same mistake
      // as defeating `display` and not `content-visibility`.
      expect(heightOf(content)).toBe("120px");
      expect(content.style.height).toBe("");
    } finally {
      restore();
    }
  });

  it("measures at the width the panel has in flow, not shrink-to-fit", () => {
    const restore = installResizeObserver();
    const restoreWidths = widthOfEveryParent("300px");
    try {
      render(<Details measure />);
      const content = screen.getByText("the small print");
      wrapsAt(content, "300px", { unwrapped: 40, wrapped: 120 });
      remeasure();

      // Out of flow a box is as wide as its content wants to be, bounded by its
      // containing block — the nearest positioned ancestor, which on most pages
      // is the viewport. A paragraph that wraps to four lines in a sidebar
      // measures one line there, and the property then holds a height the panel
      // never has.
      expect(heightOf(content)).toBe("120px");
      expect(content.style.width).toBe("");
      expect(content.style.boxSizing).toBe("");
    } finally {
      restoreWidths();
      restore();
    }
  });

  it("keeps up with content that changes size while the panel is open", async () => {
    const restore = installResizeObserver();
    try {
      render(<Details measure />);
      const content = screen.getByText("the small print");
      laidOut(content, 120);
      remeasure();
      await userEvent.click(screen.getByRole("button", { name: "Details" }));
      expect(content).not.toHaveAttribute("hidden");

      // A caller rendered a list into a panel that is already open.
      laidOut(content, 320);
      remeasure();
      expect(heightOf(content)).toBe("320px");
    } finally {
      restore();
    }
  });

  it("measures nothing at all without the opt-in", () => {
    const restore = installResizeObserver();
    try {
      render(<Details />);
      const content = screen.getByText("the small print");
      laidOut(content, 120);
      // No observer, no property, and no forced layout: a page with forty
      // collapsibles and no animation pays nothing for this.
      expect(resizeCallbacks.length).toBe(0);
      expect(heightOf(content)).toBe("");
    } finally {
      restore();
    }
  });

  it("gives each accordion panel its own height", () => {
    const restore = installResizeObserver();
    try {
      render(
        <Accordion.Root measure type="multiple">
          <Accordion.Item value="shipping">
            <Accordion.Header level={3}>
              <Accordion.Trigger>Shipping</Accordion.Trigger>
            </Accordion.Header>
            <Accordion.Content>ships in two days</Accordion.Content>
          </Accordion.Item>
          <Accordion.Item value="returns">
            <Accordion.Header level={3}>
              <Accordion.Trigger>Returns</Accordion.Trigger>
            </Accordion.Header>
            <Accordion.Content>thirty days</Accordion.Content>
          </Accordion.Item>
        </Accordion.Root>,
      );
      const shipping = screen.getByText("ships in two days");
      const returns = screen.getByText("thirty days");
      laidOut(shipping, 90);
      laidOut(returns, 240);
      remeasure();

      expect(heightOf(shipping)).toBe("90px");
      expect(heightOf(returns)).toBe("240px");
      expect(shipping).toHaveAttribute("hidden");
      expect(returns).toHaveAttribute("hidden");
    } finally {
      restore();
    }
  });
});

describe("Navigation menu", () => {
  component Site() {
    return (
      <NavigationMenu.Root aria-label="Main">
        <NavigationMenu.List>
          <NavigationMenu.Item value="docs">
            <NavigationMenu.Trigger>Docs</NavigationMenu.Trigger>
            <NavigationMenu.Body>
              <NavigationMenu.Link href="/guide">Guide</NavigationMenu.Link>
              <NavigationMenu.Link href="/reference">Reference</NavigationMenu.Link>
            </NavigationMenu.Body>
          </NavigationMenu.Item>
          <NavigationMenu.Item value="blog">
            <NavigationMenu.Trigger>Blog</NavigationMenu.Trigger>
            <NavigationMenu.Body>
              <NavigationMenu.Link href="/blog/latest">Latest</NavigationMenu.Link>
            </NavigationMenu.Body>
          </NavigationMenu.Item>
        </NavigationMenu.List>
      </NavigationMenu.Root>
    );
  }

  it("is a list of links and not a menu", async () => {
    render(<Site />);
    await userEvent.click(screen.getByRole("button", { name: "Docs" }));
    // `menu`, `menubar` and `menuitem` are for application commands. A reader
    // told "menu, five items" expected a list of links, and a `menuitem` is not
    // announced as a link, is not in the list of links they can pull up, and
    // brings a whole keyboard map with it that this is not implementing.
    expect(screen.queryByRole("menu")).toBe(null);
    expect(screen.queryByRole("menubar")).toBe(null);
    expect(screen.queryAllByRole("menuitem").length).toBe(0);
    expect(screen.getAllByRole("link").map((link) => link.textContent)).toEqual([
      "Guide",
      "Reference",
    ]);
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
  });

  it("says whether an entry is open, and names the group only while it is", async () => {
    render(<Site />);
    const trigger = screen.getByRole("button", { name: "Docs" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).not.toHaveAttribute("aria-controls");
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const group = screen.getAllByRole("list")[1];
    expect(trigger.getAttribute("aria-controls")).toBe(group.getAttribute("id"));
    expect(group.getAttribute("aria-labelledby")).toBe(trigger.getAttribute("id"));
    expect(danglingReferences()).toEqual([]);
  });

  it("closes the group on Escape and gives focus back to its button", async () => {
    render(<Site />);
    const trigger = screen.getByRole("button", { name: "Docs" });
    await userEvent.click(trigger);
    act(() => screen.getByRole("link", { name: "Guide" }).focus());
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("link", { name: "Guide" })).toBe(null);
    // Leaving focus on the `<li>` the group was removed from drops the reader
    // at the top of the page.
    expect(trigger).toHaveFocus();
  });

  it("keeps one group open at a time", async () => {
    render(<Site />);
    await userEvent.click(screen.getByRole("button", { name: "Docs" }));
    await userEvent.click(screen.getByRole("button", { name: "Blog" }));
    expect(screen.queryByRole("link", { name: "Guide" })).toBe(null);
    expect(screen.getByRole("link", { name: "Latest" })).toBeInTheDocument();
  });

  it("closes the group when a link in it is chosen", async () => {
    render(<Site />);
    await userEvent.click(screen.getByRole("button", { name: "Docs" }));
    await userEvent.click(screen.getByRole("link", { name: "Guide" }));
    expect(screen.getByRole("button", { name: "Docs" })).toHaveAttribute("aria-expanded", "false");
  });

  it("says which part was used outside a root", () => {
    let message = "";
    try {
      render(<NavigationMenu.Trigger>orphan</NavigationMenu.Trigger>);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain(
      "NavigationMenu.Trigger must be rendered inside a NavigationMenu.Item",
    );
  });
});

describe("Carousel", () => {
  afterEach(() => {
    uft.useRealTimers();
    answerMediaQueries(null);
  });

  component Example(autoplay?: number | null = null) {
    return (
      <Carousel.Root autoplay={autoplay} count={3} label="Featured">
        <Carousel.Pause />
        <Carousel.Content>
          <Carousel.Item index={0}>
            <a href="/one">One</a>
          </Carousel.Item>
          <Carousel.Item index={1}>
            <a href="/two">Two</a>
          </Carousel.Item>
          <Carousel.Item index={2}>
            <a href="/three">Three</a>
          </Carousel.Item>
        </Carousel.Content>
        <Carousel.Previous />
        <Carousel.Next />
      </Carousel.Root>
    );
  }

  /** The carousel, which is the group the slides are in rather than one of them. */
  const carouselElement = (): HTMLElement => screen.getByRole("group", { name: "Featured" });

  /** Every slide, in document order. */
  const slides = (): Array<HTMLElement> =>
    screen
      .getAllByRole("group")
      .filter((element) => element.getAttribute("aria-roledescription") === "slide");

  /** Which one is showing, by the text in it. */
  const showing = (): string =>
    slides()
      .filter((slide) => slide.getAttribute("data-state") === "active")
      .map((slide) => slide.textContent ?? "")
      .join("");

  const advance = (millis: number) => {
    act(() => {
      uft.advanceTimersByTime(millis);
    });
  };

  it("says it is a carousel and where each slide is", () => {
    render(<Example />);
    // Without these a reader is told "group, group" and has no way to know
    // what they are in or how much of it there is.
    expect(carouselElement()).toHaveAttribute("aria-roledescription", "carousel");
    const all = slides();
    expect(all.length).toBe(3);
    expect(all[0]).toHaveAttribute("aria-label", "1 of 3");
    expect(all[2]).toHaveAttribute("aria-label", "3 of 3");
    expect(danglingReferences()).toEqual([]);
  });

  it("does not let Tab into a slide nobody can see", () => {
    render(<Example />);
    // The slides that are not showing are still in the document, so without
    // `inert` their links are still focus stops and a reader tabs into content
    // the page is not showing.
    expect(slides()[1]).toHaveAttribute("inert");
    expect(
      focusable(carouselElement())
        .filter((element) => element.tagName.toLowerCase() === "a")
        .map((element) => element.textContent),
    ).toEqual(["One"]);
  });

  it("can be stopped, and stays stopped", () => {
    uft.useFakeTimers();
    render(<Example autoplay={5000} />);
    const pause = screen.getByRole("button", { name: "Stop the carousel" });
    // WCAG 2.2.2's mechanism, and the APG's placement for it: a pause control
    // a reader reaches after the slides is one they reach after the thing they
    // wanted to stop.
    expect(focusable(carouselElement())[0]).toBe(pause);

    advance(5000);
    expect(showing()).toBe("Two");

    fireEvent.click(pause);
    expect(pause).toHaveAttribute("aria-pressed", "true");
    // Not merely paused by the pointer or by focus: both have gone, and it is
    // still stopped, because that was a decision rather than a hover.
    fireEvent.pointerLeave(carouselElement());
    fireEvent.blur(carouselElement());
    advance(50_000);
    expect(showing()).toBe("Two");
  });

  it("turns the live region on only when it is not rotating", () => {
    uft.useFakeTimers();
    render(<Example autoplay={5000} />);
    const live = (): Element | null => carouselElement().querySelector("[aria-live]");
    // Announcing every slide of an auto-rotating carousel is unusable; never
    // announcing anything makes the Next button silent. So it is one while it
    // moves and the other while it does not.
    expect(live()).toHaveAttribute("aria-live", "off");

    fireEvent.click(screen.getByRole("button", { name: "Stop the carousel" }));
    expect(live()).toHaveAttribute("aria-live", "polite");
  });

  it("does not rotate for a reader who asked for less motion", () => {
    answerMediaQueries(true);
    uft.useFakeTimers();
    render(<Example autoplay={5000} />);
    advance(50_000);
    expect(showing()).toBe("One");
  });

  it("refuses to rotate behind a pause control nobody reaches in time", () => {
    expect(() =>
      render(
        <Carousel.Root autoplay={5000} count={2} label="Featured">
          <Carousel.Previous />
          <Carousel.Pause />
          <Carousel.Content>
            <Carousel.Item index={0}>One</Carousel.Item>
            <Carousel.Item index={1}>Two</Carousel.Item>
          </Carousel.Content>
        </Carousel.Root>,
      ),
    ).toThrow("first focusable element");
  });
});

describe("Scroll area", () => {
  component Example(children: React.Node) {
    return (
      <ScrollArea.Root label="Release notes">
        <ScrollArea.Viewport>{children}</ScrollArea.Viewport>
        <ScrollArea.Scrollbar orientation="vertical" />
      </ScrollArea.Root>
    );
  }

  it("can be scrolled from the keyboard", async () => {
    render(
      <Example>
        <p>The bottom of this is only reachable by scrolling.</p>
      </Example>,
    );
    // A scroll container is focusable in Firefox and not in Chromium, so a
    // region that must be scrolled to be read needs the tab stop, the role and
    // the name — or a keyboard reader can see the top of it and nothing else.
    const region = screen.getByRole("region", { name: "Release notes" });
    expect(region).toHaveAttribute("tabindex", "0");

    await userEvent.tab();
    expect(region).toHaveFocus();
  });

  it("does not take the scrolling keys away from the reader", () => {
    render(
      <Example>
        <p>Long.</p>
      </Example>,
    );
    const region = screen.getByRole("region", { name: "Release notes" });
    // Nothing here handles a key. Every key that scrolls a native overflow
    // container scrolls this one, because it is one.
    expect(fireEvent.keyDown(region, { key: "PageDown" })).toBe(true);
    expect(fireEvent.keyDown(region, { key: "ArrowDown" })).toBe(true);
    expect(fireEvent.keyDown(region, { key: "End" })).toBe(true);
  });

  it("keeps the reader's place when the content is replaced", () => {
    const { rerender } = render(
      <Example>
        <p>First.</p>
      </Example>,
    );
    const region = screen.getByRole("region", { name: "Release notes" });
    region.scrollTop = 120;
    fireEvent.scroll(region);

    // What a browser does when a scroll container's content is replaced by
    // something shorter: the offset is clamped, and it is not put back when
    // the content grows again.
    region.scrollTop = 0;
    rerender(
      <Example>
        <p>Second.</p>
      </Example>,
    );
    expect(region.scrollTop).toBe(120);
  });

  it("leaves the reader's own scroll to the top alone", () => {
    const { rerender } = render(
      <Example>
        <p>First.</p>
      </Example>,
    );
    const region = screen.getByRole("region", { name: "Release notes" });
    region.scrollTop = 120;
    fireEvent.scroll(region);
    // Their scroll, not the browser's clamp: it fires a `scroll` event, so the
    // remembered position is theirs and nothing puts them back.
    region.scrollTop = 0;
    fireEvent.scroll(region);

    rerender(
      <Example>
        <p>Second.</p>
      </Example>,
    );
    expect(region.scrollTop).toBe(0);
  });
});

describe("Input OTP", () => {
  component Example(length?: number = 6) {
    return (
      <InputOtp.Root label="One-time code" length={length} name="code">
        <InputOtp.Group>
          <InputOtp.Slot index={0} />
          <InputOtp.Slot index={1} />
          <InputOtp.Slot index={2} />
        </InputOtp.Group>
        <InputOtp.Separator>-</InputOtp.Separator>
        <InputOtp.Group>
          <InputOtp.Slot index={3} />
          <InputOtp.Slot index={4} />
          <InputOtp.Slot index={5} />
        </InputOtp.Group>
      </InputOtp.Root>
    );
  }

  /** What each box is showing, in order. */
  const boxes = (): Array<string> =>
    Array.from(document.querySelectorAll("[data-index]")).map(
      (element) => element.textContent ?? "",
    );

  /** Which box is lit. */
  const lit = (): string | null =>
    document.querySelector('[data-active="true"]')?.getAttribute("data-index") ?? null;

  it("asks the platform for the code", () => {
    render(<Example />);
    const field = screen.getByRole("textbox", { name: "One-time code" });
    // The single most valuable thing about the component, and the first thing
    // a six-input version loses: without it the operating system has no field
    // to offer the code it just received by SMS to.
    expect(field).toHaveAttribute("autocomplete", "one-time-code");
    expect(field).toHaveAttribute("inputmode", "numeric");
  });

  it("fills every box from one paste", () => {
    render(<Example />);
    const field = screen.getByRole("textbox", { name: "One-time code" });
    replaceValue(field, "123456");

    expect(boxes()).toEqual(["1", "2", "3", "4", "5", "6"]);
    // One value, under one name, rather than six. A form reads the field the
    // reader typed into, so there is no hidden input here at all.
    expect(document.querySelectorAll("input").length).toBe(1);
    expect(field).toHaveAttribute("name", "code");
    expect((field as $FlowFixMe).value).toBe("123456");
  });

  it("keeps everything that is not the code out", () => {
    render(<Example />);
    const field = screen.getByRole("textbox", { name: "One-time code" });
    // A code pasted out of a message arrives with whatever was around it.
    replaceValue(field, "Your code is 12-34-56, do not share it");
    expect((field as $FlowFixMe).value).toBe("123456");
  });

  it("moves back on Backspace", async () => {
    render(<Example />);
    const field = screen.getByRole("textbox", { name: "One-time code" });
    await userEvent.type(field, "12");
    expect(boxes()).toEqual(["1", "2", "", "", "", ""]);
    expect(lit()).toBe("2");

    replaceValue(field, "1");
    // The character is gone and the lit box has moved back with it, which is
    // the half of `Backspace` this component is responsible for; the deletion
    // is the browser's, because there is one input for it to happen in.
    expect(boxes()).toEqual(["1", "", "", "", "", ""]);
    expect(lit()).toBe("1");
  });

  it("says the code is finished once, when it is", async () => {
    const onComplete = fn();
    render(
      <InputOtp.Root label="One-time code" length={4} name="code" onComplete={onComplete}>
        <InputOtp.Slot index={0} />
        <InputOtp.Slot index={1} />
        <InputOtp.Slot index={2} />
        <InputOtp.Slot index={3} />
      </InputOtp.Root>,
    );
    const field = screen.getByRole("textbox", { name: "One-time code" });
    await userEvent.type(field, "123");
    expect(onComplete.mock.calls.length).toBe(0);
    await userEvent.type(field, "4");
    expect(onComplete).toHaveBeenCalledWith("1234");
  });
});

describe("caller props never disable the component", () => {
  it("keeps the focus trap when the caller passes a ref", async () => {
    // The ref used to replace the dialog's own, leaving it null — so the Tab
    // handler returned early and the trap was off while the dialog still
    // announced aria-modal="true".
    const seen = { current: null };
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body ref={seen}>
          <Dialog.Title>Title</Dialog.Title>
          <button type="button">first</button>
          <button type="button">last</button>
        </Dialog.Body>
      </Dialog.Root>,
    );
    const dialog = screen.getByRole("dialog");
    // The caller's ref is set too, not instead.
    expect(seen.current).toBe(dialog);

    const last = within(dialog).getByRole("button", { name: "last" });
    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(within(dialog).getByRole("button", { name: "first" })).toHaveFocus();
  });

  it("keeps Escape closing the dialog when the caller passes onKeyDown", async () => {
    const theirs = fn();
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body onKeyDown={theirs}>
          <Dialog.Title>Title</Dialog.Title>
        </Dialog.Body>
      </Dialog.Root>,
    );
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    // Both ran: the caller's handler and the component's behaviour.
    expect(theirs.mock.calls.length).toBe(1);
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("lets a caller handler stop the component's behaviour deliberately", () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body onKeyDown={(event) => event.preventDefault()}>
          <Dialog.Title>Title</Dialog.Title>
        </Dialog.Body>
      </Dialog.Root>,
    );
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    // `preventDefault` is how the DOM says "I handled this", so the dialog
    // does not also act on it.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("keeps a tab selectable when the caller passes onClick", async () => {
    const theirs = fn();
    render(
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab onClick={theirs} value="two">
            Two
          </Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
      </Tabs.Root>,
    );
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(theirs.mock.calls.length).toBe(1);
    expect(screen.getByRole("tabpanel").textContent).toBe("second");
  });

  it("keeps a menu item runnable when the caller passes onClick", async () => {
    const theirs = fn();
    const onSelect = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item onClick={theirs} onSelect={onSelect}>
            Open
          </Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Open" }));
    expect(theirs.mock.calls.length).toBe(1);
    expect(onSelect).toHaveBeenCalled();
  });

  it("keeps the menu's arrow keys when the caller passes onKeyDown", async () => {
    const theirs = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body onKeyDown={theirs}>
          <Menu.Item>Open</Menu.Item>
          <Menu.Item>Save</Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    await userEvent.keyboard("{ArrowDown}");
    expect(theirs.mock.calls.length).toBe(1);
    expect(screen.getByRole("menuitem", { name: "Save" })).toHaveFocus();
  });

  it("keeps a switch toggling when the caller passes onClick", async () => {
    const theirs = fn();
    render(<Switch aria-label="Notifications" onClick={theirs} />);
    await userEvent.click(screen.getByRole("switch"));
    expect(theirs.mock.calls.length).toBe(1);
    expect(screen.getByRole("switch")).toBeChecked();
  });

  it("keeps the combobox's keys when the caller passes onKeyDown", async () => {
    const theirs = fn();
    render(
      <Combobox.Root defaultOpen>
        <Combobox.Label>Fruit</Combobox.Label>
        <Combobox.Input onKeyDown={theirs} />
        <Combobox.List>
          <Combobox.Option value="apple">Apple</Combobox.Option>
        </Combobox.List>
      </Combobox.Root>,
    );
    const input = screen.getByRole("combobox");
    input.focus();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(theirs.mock.calls.length).toBe(1);
    expect(input).toHaveAttribute("aria-activedescendant");
  });

  it("keeps the select's keys when the caller passes onKeyDown", async () => {
    const theirs = fn();
    render(
      <Select.Root>
        <Select.Label>Country</Select.Label>
        <Select.Trigger onKeyDown={theirs}>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Option value="FR">France</Select.Option>
        </Select.List>
      </Select.Root>,
    );
    const trigger = screen.getByRole("combobox");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(theirs.mock.calls.length).toBe(1);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps the select's trigger findable when the caller passes a ref", async () => {
    const seen = { current: null };
    render(
      <Select.Root>
        <Select.Label>Country</Select.Label>
        <Select.Trigger ref={seen}>
          <Select.Value placeholder="Choose one" />
        </Select.Trigger>
        <Select.List>
          <Select.Option value="FR">France</Select.Option>
        </Select.List>
      </Select.Root>,
    );
    // The caller's ref is set too, not instead: the component's own copy is
    // what an option's click restores focus to, and a replaced one would leave
    // the reader's next keystroke arriving at the document.
    expect(seen.current).toBe(screen.getByRole("combobox"));
    await userEvent.click(screen.getByRole("combobox"));
    await userEvent.click(screen.getByRole("option", { name: "France" }));
    expect(screen.getByRole("combobox")).toHaveFocus();
  });

  it("keeps a notification dismissable when the caller passes onClick", async () => {
    const theirs = fn();
    act(() => {
      dismissAllToasts();
    });
    render(
      <Toast.Region>
        {(each) => (
          <Toast.Root>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Close onClick={theirs} />
          </Toast.Root>
        )}
      </Toast.Region>,
    );
    act(() => {
      toast("Saved");
    });
    await userEvent.click(screen.getByRole("button", { name: /dismiss/i }));
    expect(theirs.mock.calls.length).toBe(1);
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("keeps the field's ids authoritative", () => {
    render(
      <Field.Root>
        <Field.Label id="theirs">Name</Field.Label>
        <Field.Control render={(props) => <input {...props} />} />
      </Field.Root>,
    );
    const control = screen.getByLabelText("Name");
    const label = screen.getByText("Name");
    // A caller id used to win, and the control then pointed at an id that no
    // longer existed.
    expect(label.getAttribute("id")).toBe(control.getAttribute("aria-labelledby"));
    expect(label.getAttribute("for")).toBe(control.getAttribute("id"));
  });

  it("does not treat a control inside aria-hidden as a focus stop", () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title>Title</Dialog.Title>
          <div aria-hidden="true">
            <button type="button">concealed</button>
          </div>
          <button type="button">real</button>
        </Dialog.Body>
      </Dialog.Root>,
    );
    // Focus goes to the first stop a reader can actually reach.
    expect(screen.getByRole("button", { name: "real" })).toHaveFocus();
  });

  it("carries the attributes a caller styles and finds the element by", () => {
    // The other half of narrowing `Rest`: the names a caller actually spreads
    // are open-ended — a class, an id, `data-*` for a test or a stylesheet,
    // `aria-*` the component does not set itself — and all of them still have
    // to arrive. This is why `Rest` kept its indexer instead of becoming a
    // written-out list of element props.
    render(
      <Switch
        aria-describedby="hint"
        className="knob"
        data-testid="notifications"
        id="notify"
        title="Notifications"
      />,
    );
    const control = screen.getByTestId("notifications");
    expect(control.getAttribute("class")).toBe("knob");
    expect(control.getAttribute("id")).toBe("notify");
    expect(control.getAttribute("title")).toBe("Notifications");
    expect(control.getAttribute("aria-describedby")).toBe("hint");
    // And the component's own semantics are still on top of them.
    expect(control.getAttribute("role")).toBe("switch");
  });

  it("hands the field's control attributes to a render function that spreads them", () => {
    // `Field.Control` passes props the other way — the field gives them to the
    // caller to spread onto whatever element they render — so it is typed with
    // the same `Rest`, and a consumer's `<input {...props} />` has to keep
    // working.
    render(
      <Field.Root>
        <Field.Label>Name</Field.Label>
        <Field.Control render={(props) => <input {...props} className="control" />} />
      </Field.Root>,
    );
    const control = screen.getByLabelText("Name");
    expect(control.getAttribute("class")).toBe("control");
    expect(control.getAttribute("id")).not.toBe(null);
  });
});

// What `uf check` says about this package, and the two promises only it can
// hold: that no part makes React's `key` a `mixed`, and that a misused `side`
// or `align` is an error at the call rather than an overlay in the wrong place.
// Both blocks below run the checker; `./type-tests.js` holds what it takes to
// run it — the checkout, the binary `uf test` named, and the marker harness
// the three fixture blocks share with five other suites.

describe("the escape hatch: which part hands its element to the caller", () => {
  // ubugeeei-prod/uf#303. This package has no copy step, and the thing a copy
  // step is *for* is changing the markup — so `render` is what has to answer
  // "I need this to be an `<a>`", and the answer is only worth writing down
  // where it exists. It existed on `Field.Control` alone when #303 was filed,
  // which is why the issue calls a documented escape hatch that is not there
  // worse than an undocumented one that is.
  //
  // The table below is the whole surface, part by part, in one of three states.
  // It is a list held to the files, the way `hook_descriptors()` is held to
  // `@uniflowed/hooks` in `crates/uf_lib/src/tests.rs` — and the shape matters
  // more than the contents: a part added to `packages/ui/index.js` is in none
  // of the three lists, so the first test below fails and whoever added it has
  // to say which state it is in. That is the guard. Nothing else in this
  // repository would have noticed.
  //
  //   * **`RENDER`** — takes `render?: RenderProp` and hands over the props it
  //     would have put on its own element. Checked: the source declares the
  //     prop and calls it.
  //   * **`NO_ELEMENT`** — renders no element of its own. Two kinds live here,
  //     and the difference is worth knowing: a context-only part like
  //     `Dialog.Root` has nothing to hand over at all, while `Sheet.Title`-shaped
  //     parts render *another part of this package* and the escape hatch is that
  //     part's to offer. Every delegating part whose delegate has `render` today
  //     forwards it, and is in `RENDER` rather than here; the ones left here
  //     delegate to something that has not got one yet. Checked: no intrinsic
  //     element anywhere in the body, so neither kind can be hiding a fixed
  //     `<button>`.
  //   * **`FIXED`** — still renders an element the caller cannot change. This
  //     is the remainder of #303 and it is a closed list that only shrinks:
  //     the third test fails if a part named here has grown a `render`, so
  //     landing the hatch on one means moving its name, in the same change.
  //
  // What the escape hatch does *not* cost is the constraint: `Menu.Body`'s
  // `renders*` still rejects a `<div>` where a `Menu.Item` belongs, because a
  // `Menu.Item` rendered as an `<a>` is still a `Menu.Item`. That is the half a
  // copied source cannot keep, and it is why "no copy step" is a trade rather
  // than a loss.

  /** Parts that take `render` and hand their props to the caller. */
  const RENDER: $ReadOnlyArray<string> = [
    "Alert.Description",
    "Alert.Root",
    "Alert.Title",
    "AlertDialog.Action",
    "AlertDialog.Body",
    "AlertDialog.Cancel",
    "AlertDialog.Description",
    "AlertDialog.Footer",
    "AlertDialog.Header",
    "AlertDialog.Overlay",
    "AlertDialog.Title",
    "AlertDialog.Trigger",
    "Avatar.Fallback",
    "Avatar.Image",
    "Avatar.Root",
    "Breadcrumb.Item",
    "Breadcrumb.Link",
    "Breadcrumb.List",
    "Breadcrumb.Page",
    "Breadcrumb.Root",
    "Breadcrumb.Separator",
    "Checkbox",
    "Collapsible.Content",
    "Collapsible.Trigger",
    "ContextMenu.Body",
    "ContextMenu.CheckboxItem",
    "ContextMenu.Group",
    "ContextMenu.Item",
    "ContextMenu.Label",
    "ContextMenu.RadioGroup",
    "ContextMenu.RadioItem",
    "ContextMenu.Separator",
    "ContextMenu.SubTrigger",
    "ContextMenu.Trigger",
    "Dialog.Body",
    "Dialog.Close",
    "Dialog.Description",
    "Dialog.Footer",
    "Dialog.Header",
    "Dialog.Overlay",
    "Dialog.Title",
    "Dialog.Trigger",
    "Drawer.Body",
    "Drawer.Close",
    "Drawer.Description",
    "Drawer.Footer",
    "Drawer.Handle",
    "Drawer.Header",
    "Drawer.Overlay",
    "Drawer.Title",
    "Drawer.Trigger",
    "Field.Control",
    "HoverCard.Trigger",
    "Menu.Body",
    "Menu.CheckboxItem",
    "Menu.Group",
    "Menu.Item",
    "Menu.Label",
    "Menu.RadioGroup",
    "Menu.RadioItem",
    "Menu.Separator",
    "Menu.SubTrigger",
    "Menu.Trigger",
    "Menubar.Body",
    "Menubar.CheckboxItem",
    "Menubar.Group",
    "Menubar.Item",
    "Menubar.Label",
    "Menubar.RadioGroup",
    "Menubar.RadioItem",
    "Menubar.Root",
    "Menubar.Separator",
    "Menubar.SubTrigger",
    "Menubar.Trigger",
    "Progress",
    "Separator",
    "Sheet.Body",
    "Sheet.Close",
    "Sheet.Description",
    "Sheet.Footer",
    "Sheet.Header",
    "Sheet.Overlay",
    "Sheet.Title",
    "Sheet.Trigger",
    "Sidebar.Item",
    "Skeleton.Box",
    "Skeleton.Root",
    "Switch",
    "Tabs.List",
    "Tabs.Panel",
    "Tabs.Root",
    "Tabs.Tab",
    "Toggle",
    "Tooltip.Trigger",
  ];

  /** Parts with no element of their own: a context, or another part of this package. */
  const NO_ELEMENT: $ReadOnlyArray<string> = [
    "Accordion.Header",
    "AlertDialog.Root",
    "Calendar.Next",
    "Calendar.Previous",
    "Carousel.Next",
    "Carousel.Previous",
    "Collapsible.Root",
    "ContextMenu.Root",
    "ContextMenu.Sub",
    "DatePicker.Calendar",
    "DatePicker.Trigger",
    "Dialog.Root",
    "Drawer.Root",
    "HoverCard.Root",
    "Menu.Root",
    "Menu.Sub",
    "Menubar.Menu",
    "Menubar.Sub",
    "Pagination.Item",
    "Pagination.Next",
    "Pagination.Previous",
    "Popover.Root",
    "Sheet.Root",
    "Sidebar.Root",
    "Table.RowSelect",
    "Table.SelectAll",
    "Tooltip.Provider",
    "Tooltip.Root",
  ];

  /** Parts whose element a caller still cannot change. #303's remainder; it only shrinks. */
  const FIXED: $ReadOnlyArray<string> = [
    "Accordion.Content",
    "Accordion.Item",
    "Accordion.Root",
    "Accordion.Trigger",
    "Calendar.Day",
    "Calendar.Month",
    "Calendar.Root",
    "Carousel.Content",
    "Carousel.Item",
    "Carousel.Pause",
    "Carousel.Root",
    "Combobox.Empty",
    "Combobox.Group",
    "Combobox.GroupLabel",
    "Combobox.Input",
    "Combobox.Label",
    "Combobox.List",
    "Combobox.Option",
    "Combobox.Root",
    "Combobox.Status",
    "DatePicker.Input",
    "DatePicker.Root",
    "Field.Description",
    "Field.Error",
    "Field.Label",
    "Field.Root",
    "HoverCard.Body",
    "InputOtp.Group",
    "InputOtp.Root",
    "InputOtp.Separator",
    "InputOtp.Slot",
    "NavigationMenu.Body",
    "NavigationMenu.Item",
    "NavigationMenu.Link",
    "NavigationMenu.List",
    "NavigationMenu.Root",
    "NavigationMenu.Trigger",
    "Pagination.Content",
    "Pagination.Root",
    "Popover.Body",
    "Popover.Trigger",
    "RadioGroup.Indicator",
    "RadioGroup.Item",
    "RadioGroup.Root",
    "Resizable.Handle",
    "Resizable.Panel",
    "Resizable.PanelGroup",
    "ScrollArea.Root",
    "ScrollArea.Scrollbar",
    "ScrollArea.Viewport",
    "Select.Group",
    "Select.GroupLabel",
    "Select.Label",
    "Select.List",
    "Select.Option",
    "Select.Root",
    "Select.Separator",
    "Select.Trigger",
    "Select.Value",
    "Sidebar.Body",
    "Sidebar.Footer",
    "Sidebar.Header",
    "Sidebar.Trigger",
    "Slider.Range",
    "Slider.Root",
    "Slider.Thumb",
    "Slider.Track",
    "Table.Body",
    "Table.Caption",
    "Table.Cell",
    "Table.Head",
    "Table.Header",
    "Table.Root",
    "Table.Row",
    "Table.RowHeader",
    "Toast.Action",
    "Toast.Close",
    "Toast.Description",
    "Toast.Region",
    "Toast.Root",
    "Toast.Title",
    "ToggleGroup.Item",
    "ToggleGroup.Root",
    "Tooltip.Body",
  ];

  /** The parts `packages/ui/index.js` names, and the component behind each. */
  function partsOfTheBarrel(): Map<string, string> {
    const source = fs.readFileSync(path.join(repository, "packages", "ui", "index.js"), "utf8");
    const parts = new Map<string, string>();
    for (const namespace of source.matchAll(/^export const (\w+) = \{\n([\s\S]*?)^\};$/gm)) {
      for (const part of namespace[2].matchAll(/^ {2}(\w+): (\w+),$/gm)) {
        parts.set(`${namespace[1]}.${part[1]}`, part[2]);
      }
    }
    // The five that are one component rather than a namespace of parts. They
    // are exported by name and `the five that are one element` above is the
    // suite that covers them.
    for (const alone of ["Checkbox", "Progress", "Separator", "Switch", "Toggle"]) {
      parts.set(alone, alone);
    }
    return parts;
  }

  /** Every `export component`'s source text, by component name. */
  function componentSources(): Map<string, string> {
    const directory = path.join(repository, "packages", "ui");
    const sources = new Map<string, string>();
    for (const file of fs.readdirSync(directory)) {
      if (!file.endsWith(".js") || file === "index.js") {
        continue;
      }
      const lines = fs.readFileSync(path.join(directory, file), "utf8").split("\n");
      for (let at = 0; at < lines.length; at += 1) {
        const declared = /^export component (\w+)\(/.exec(lines[at]);
        if (declared == null) {
          continue;
        }
        // To the closing brace in column one, which is where this package's
        // formatter puts the end of a top-level declaration.
        let end = at;
        while (end < lines.length && lines[end] !== "}") {
          end += 1;
        }
        sources.set(declared[1], lines.slice(at, end + 1).join("\n"));
      }
    }
    return sources;
  }

  /** The source of the component behind a part, or a failure naming it. */
  function sourceOf(part: string): string {
    const component = partsOfTheBarrel().get(part);
    if (component == null) {
      throw new Error(`${part} is in the table and is not exported by packages/ui/index.js`);
    }
    const source = componentSources().get(component);
    if (source == null) {
      throw new Error(`${part} names ${component}, and no module declares that component`);
    }
    return source;
  }

  it("names every part the package exports, and nothing else", () => {
    const tabled = [...RENDER, ...NO_ELEMENT, ...FIXED];
    const duplicated = tabled.filter((part, at) => tabled.indexOf(part) !== at);
    expect(duplicated).toEqual([]);

    const exported = [...partsOfTheBarrel().keys()];
    const untabled = exported.filter((part) => !tabled.includes(part));
    const invented = tabled.filter((part) => !exported.includes(part));
    expect({ untabled, invented }).toEqual({ untabled: [], invented: [] });

    // A floor as well as an equality, so a barrel that stopped parsing into
    // anything cannot make two empty lists agree. `crates/uf_lib/src/tests.rs`
    // guards its hook table the same way and for the same reason.
    expect(exported.length).toBeGreaterThan(150);
  });

  it("gives every part it calls an escape hatch a real one", () => {
    const missing = RENDER.filter((part) => {
      const source = sourceOf(part);
      return !/\brender\??: RenderProp[,)]/.test(source) || !source.includes("render(");
    });
    // `render={render}` is how a part that delegates to another part passes it
    // on, and that spelling contains `render(` nowhere — so those are named by
    // the forwarding form instead.
    const unforwarded = missing.filter((part) => !sourceOf(part).includes("render={render}"));
    expect(unforwarded).toEqual([]);
  });

  it("keeps the fixed list shrinking rather than growing", () => {
    const hatched = FIXED.filter((part) => /\brender\??: RenderProp[,)]/.test(sourceOf(part)));
    // A part that has grown the escape hatch belongs in `RENDER`. Moving it is
    // the point: the two lists are what #303's remainder is counted from, and
    // a stale one is a remainder nobody can trust.
    expect(hatched).toEqual([]);
  });

  it("keeps the parts that render nothing rendering nothing", () => {
    const withElements = NO_ELEMENT.filter((part) => {
      const source = sourceOf(part);
      const body = source.slice(source.indexOf(") {"));
      return /<[a-z][a-zA-Z0-9]*[\s/>]/.test(body);
    });
    expect(withElements).toEqual([]);
  });
});

describe("the escape hatch, exercised", () => {
  // The other half of the guard above: that the props a part hands over are the
  // props it would have used, so the element a caller renders is not a weaker
  // one. Every case here fails without the `render` prop the part now takes —
  // the part renders its own element and the assertion about the caller's is
  // about something that is not there.

  it("renders a menu item as a link, and it is still a menu item", async () => {
    const onSelect = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item onSelect={onSelect} render={(props) => <a href="/settings" {...props} />}>
            Settings
          </Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );

    // The role is the part's and the element is the caller's, which is the
    // whole claim: a menu of links is announced as a menu of menu items and
    // still has the middle click, the context menu and the status bar.
    const item = screen.getByRole("menuitem", { name: "Settings" });
    expect(item.tagName).toBe("A");
    expect(item.getAttribute("href")).toBe("/settings");
    // The children were written between the tags and the caller spread the
    // props onto a self-closing element; they arrived anyway.
    expect(item.textContent).toBe("Settings");

    await userEvent.click(item);
    expect(onSelect).toHaveBeenCalled();
    // And choosing it still closes the tree, which is the behaviour the item
    // owns rather than the element.
    expect(screen.queryByRole("menu")).toBe(null);
  });

  it("keeps the roving tab stop on a menu item the caller rendered", async () => {
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item render={(props) => <a href="/one" {...props} />}>One</Menu.Item>
          <Menu.Item>Two</Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    const [first, second] = screen.getAllByRole("menuitem");
    expect(first.tagName).toBe("A");
    expect(first).toHaveFocus();
    expect(first.getAttribute("tabindex")).toBe("0");
    expect(second.getAttribute("tabindex")).toBe("-1");

    fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
    expect(second).toHaveFocus();
  });

  it("renders a dialog title as the heading the page around it needs", () => {
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body>
          <Dialog.Title render={(props) => <h3 {...props} />}>Rename project</Dialog.Title>
        </Dialog.Body>
      </Dialog.Root>,
    );
    const heading = screen.getByRole("heading", { level: 3, name: "Rename project" });
    // The dialog still names itself after it, which is the reason the id has to
    // survive the change of element. ubugeeei-prod/uf#276 is the same
    // observation about an accordion's header.
    expect(screen.getByRole("dialog").getAttribute("aria-labelledby")).toBe(heading.id);
    expect(danglingReferences()).toEqual([]);
  });

  it("gives a dialog trigger's ref to whatever the caller rendered", async () => {
    render(
      <Dialog.Root>
        <Dialog.Trigger render={(props) => <span {...props} tabIndex={0} />}>Open</Dialog.Trigger>
        <Dialog.Body>
          <Dialog.Title>Title</Dialog.Title>
          <Dialog.Close>Done</Dialog.Close>
        </Dialog.Body>
      </Dialog.Root>,
    );
    const trigger = screen.getByText("Open");
    expect(trigger.tagName).toBe("SPAN");
    expect(trigger.getAttribute("aria-haspopup")).toBe("dialog");

    await userEvent.click(trigger);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    // Focus goes back to the caller's element, which only works because the
    // composed ref went across with the rest of the props.
    expect(trigger).toHaveFocus();
  });

  it("renders a tab as a link without losing the roving tab stop", async () => {
    render(
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab render={(props) => <a href="#one" {...props} />} value="one">
            One
          </Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">first</Tabs.Panel>
        <Tabs.Panel value="two">second</Tabs.Panel>
      </Tabs.Root>,
    );
    const first = screen.getByRole("tab", { name: "One" });
    expect(first.tagName).toBe("A");
    expect(first.getAttribute("aria-selected")).toBe("true");
    expect(first.getAttribute("tabindex")).toBe("0");
    expect(screen.getByRole("tab", { name: "Two" }).getAttribute("tabindex")).toBe("-1");

    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(screen.getByRole("tabpanel").textContent).toBe("second");
  });

  it("keeps a switch a switch when the caller renders a div", async () => {
    const changed = fn();
    render(
      <Switch onCheckedChange={changed} render={(props) => <div {...props} tabIndex={0} />}>
        Notifications
      </Switch>,
    );
    const control = screen.getByRole("switch", { name: "Notifications" });
    expect(control.tagName).toBe("DIV");
    expect(control.getAttribute("aria-checked")).toBe("false");

    control.focus();
    await userEvent.keyboard(" ");
    expect(control.getAttribute("aria-checked")).toBe("true");
    expect(changed).toHaveBeenCalledWith(true);
  });

  it("keeps a checkbox's mixed state when the caller renders a span", () => {
    render(
      <Checkbox indeterminate render={(props) => <span {...props} tabIndex={0} />}>
        Select all
      </Checkbox>,
    );
    const control = screen.getByRole("checkbox", { name: "Select all" });
    expect(control.tagName).toBe("SPAN");
    expect(control.getAttribute("aria-checked")).toBe("mixed");
  });

  it("puts the part's own semantics on top of the caller's props", async () => {
    const theirs = fn();
    render(
      <Menu.Root defaultOpen>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body>
          <Menu.Item onClick={theirs} render={(props) => <a href="/open" role="link" {...props} />}>
            Open
          </Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    // The caller wrote `role="link"` *before* the spread, so the part's
    // `role="menuitem"` is what survives — the same rule
    // `internal/merge-props.js` states for a caller's props on a part's own
    // element, applied where the element is the caller's.
    const item = screen.getByRole("menuitem", { name: "Open" });
    expect(item.tagName).toBe("A");
    expect(item.getAttribute("role")).toBe("menuitem");

    // And the caller's handler still runs beside the part's rather than
    // instead of it.
    await userEvent.click(item);
    expect(theirs.mock.calls.length).toBe(1);
  });

  it("carries the hatch through a sheet to the dialog underneath it", async () => {
    render(
      <Sheet.Root defaultOpen>
        <Sheet.Body>
          <Sheet.Title render={(props) => <h4 {...props} />}>Filters</Sheet.Title>
          <Sheet.Close render={(props) => <a href="#close" {...props} />}>Done</Sheet.Close>
        </Sheet.Body>
      </Sheet.Root>,
    );
    const heading = screen.getByRole("heading", { level: 4, name: "Filters" });
    expect(screen.getByRole("dialog").getAttribute("aria-labelledby")).toBe(heading.id);

    const close = screen.getByRole("link", { name: "Done" });
    expect(close.tagName).toBe("A");
    await userEvent.click(close);
    // A sheet part forwards `render` to the dialog part it is made of, so the
    // close still closes.
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("renders a menubar trigger through the hatch and keeps the bar's keys", async () => {
    render(
      <Menubar.Root aria-label="Application">
        <Menubar.Menu value="file">
          <Menubar.Trigger render={(props) => <a href="#file" {...props} />}>File</Menubar.Trigger>
          <Menubar.Body>
            <Menubar.Item>New</Menubar.Item>
          </Menubar.Body>
        </Menubar.Menu>
        <Menubar.Menu value="edit">
          <Menubar.Trigger>Edit</Menubar.Trigger>
          <Menubar.Body>
            <Menubar.Item>Undo</Menubar.Item>
          </Menubar.Body>
        </Menubar.Menu>
      </Menubar.Root>,
    );
    const file = screen.getByRole("menuitem", { name: "File" });
    expect(file.tagName).toBe("A");

    file.focus();
    fireEvent.keyDown(screen.getByRole("menubar"), { key: "ArrowRight" });
    expect(screen.getByRole("menuitem", { name: "Edit" })).toHaveFocus();
  });
});

describe("the props a part spreads onto its element", () => {
  // A type is a promise the same way a role is, and this is the only test here
  // that can hold one to it.
  //
  // Every part takes `...rest: Rest` and spreads it onto an intrinsic. `Rest`
  // — `packages/ui/internal/merge-props.js` — names `key` out of its indexer,
  // because React's `key` is `string | number` and an indexer answers `mixed`
  // for every name. Widen it back to a bare `{ readonly [string]: mixed }` and
  // `uf check` reports "Cannot create button element because in property key"
  // once for every element the package renders: thirty-two of them, which is
  // what #206 was.
  //
  // Scoped to that one family on purpose. `packages/ui` still reports
  // `value-as-type` errors for `React.Node` and `React.Context`, because
  // nothing resolves a module for `@uniflowed/react` and the import is typed
  // `any` — a different bug, with a different fix, and not one this test
  // should start failing over.
  //
  // And scoped to what the package *ships*. `uf check packages/ui` now reads
  // this file too, because this file is in `packages/ui` — that is what
  // co-locating the suite means. The claim above is about the elements the
  // package renders, so a diagnostic against a test file is not evidence for
  // or against it, and counting one would make the suite's own call sites the
  // subject. There is one today: the `Plans` component below spreads
  // `Field.Control`'s render props onto a `RadioGroup.Root`, and the checker
  // has an opinion about the `key` in them that nothing had asked for until
  // this file moved. That is a real finding about the API and it belongs in
  // an issue about `Field.Control`, not in an assertion about `merge-props.js`.

  it("does not make React's key mixed", () => {
    const run = spawnSync(UF, ["check", "packages/ui", "--json"], {
      cwd: repository,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    // A non-zero status is expected: the package still has the `value-as-type`
    // errors above. The answer is on stdout either way — and when it is not,
    // this says so. `JSON.parse("")` reports `Unexpected end of JSON input`
    // and names neither the command, the directory, nor what the command said
    // instead, which is the half of #313 that made a wrong directory take an
    // afternoon to find rather than a minute.
    if (run.stdout === "") {
      throw new Error(
        `\`uf check packages/ui --json\` in ${repository} printed nothing: ` +
          `status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
      );
    }
    const report: CheckReport = JSON.parse(run.stdout);
    // Without this the test would pass just as happily on a run that checked
    // nothing at all.
    expect(report.typeCheck.status).toBe("checked");
    expect(report.typeCheck.filesChecked).toBeGreaterThan(0);

    const keyed = report.typeCheck.diagnostics
      .filter((diagnostic) => !diagnostic.primary.path.endsWith(".test.js"))
      .map((diagnostic) => ({
        at: `${diagnostic.primary.path}:${String(diagnostic.primary.start.line)}`,
        said: diagnostic.message.map((span) => span.text).join(""),
      }))
      .filter((diagnostic) => diagnostic.said.includes("in property key"));
    expect(keyed).toEqual([]);
    // And the checker read the package rather than only this file, which is
    // the way the filter above could have emptied the list it is asserting on.
    expect(
      report.typeCheck.diagnostics.some(
        (diagnostic) => !diagnostic.primary.path.endsWith(".test.js"),
      ),
    ).toBe(true);
  });
});

describe("a side and an alignment are unions, not strings", () => {
  // The other promise a type makes, and the other one no amount of rendering
  // can check. `internal/anchor.js` says a side is one of four names and an
  // alignment one of three; the claim that follows is that a consumer who
  // misspells one is stopped by the checker rather than by a reader finding an
  // overlay in the wrong place.
  //
  // `tests/type-tests/anchoring.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "anchoring.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
    });
  });
});

describe("a wrong child is a type error and not a review comment", () => {
  // The strongest claim `packages/ui/index.js` makes, and the one nothing here
  // held: `Tabs.List` declares `renders* Tabs.Tab`, so a `<button>` in a tab
  // list does not compile. Thirteen containers in this package state a constraint
  // like that — `Menu.Body`, `Combobox.List`, `Select.List`, `Toast.Region`,
  // `Pagination.Content` and the rest — and every one of them was an unverified
  // promise: they were checked by hand against a scratch file while `select.js`
  // and `toast.js` were written, and a scratch file survives no refactor. That
  // is ubugeeei-prod/uf#358.
  //
  // It is checked here rather than by rendering anything because there is
  // nothing to render. The failure a `renders*` prevents does not reach a
  // browser: it is a `<button>` announced as "button" where the reader expected
  // "tab, 2 of 5", in a build that never happened.
  //
  // Both directions are in the fixture, which is the part worth keeping. A
  // constraint that stopped rejecting a `<div>` would take the guarantee away
  // and nothing else would notice; one that started rejecting the parts it
  // exists to admit would take the library away, and this test would name which
  // container did it.
  //
  // `tests/type-tests/composition.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "composition.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
    });
  });
});

describe("an edge, a role, an alphabet and an orientation are unions too", () => {
  // The same claim, for the dialog-shaped components, the three that replace
  // something the browser already does, and the rule between two of them. A
  // sheet's `side`, a sidebar's — which has two members rather than four,
  // because a sidebar is never along the top — a modal's `role`, what a
  // one-time code is made of, and which way a `Separator` runs: five unions
  // whose misuse has no symptom at run time and none in a screenshot.
  //
  // `tests/type-tests/overlays.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "overlays.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
    });
  });
});
