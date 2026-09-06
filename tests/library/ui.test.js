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
import path from "node:path";

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { afterEach, beforeEach, describe, expect, fn, it, uft } from "@uniflowed/test";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  within,
} from "@uniflowed/react-testing";
import {
  Checkbox,
  Combobox,
  Dialog,
  Field,
  Menu,
  Select,
  Switch,
  Tabs,
  Toast,
  dismissAllToasts,
  toast,
  updateToast,
} from "@uniflowed/ui";

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
    fireEvent.pointerDown(document.body);
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
    fireEvent.pointerDown(document.body);
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
    fireEvent.pointerDown(document.body);
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
    fireEvent.pointerDown(document.body);
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

  it("toggles a checkbox on Space and leaves Enter to the form", async () => {
    render(<Checkbox aria-label="Subscribe" />);
    const control = screen.getByRole("checkbox");
    await userEvent.keyboard("{Enter}");
    expect(control).not.toBeChecked();
    control.focus();
    // Not prevented, so a checkbox inside a form still submits it.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(true);
    await userEvent.keyboard(" ");
    expect(control).toBeChecked();
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

  // The repository, two levels up from the project this worker runs in.
  // `uf test` names that project in `UF_PROJECT_ROOT` and starts the worker
  // there, so it is `tests/library` whichever directory the command was typed
  // in. `import.meta.url` would say it more directly, and
  // `fileURLToPath(import.meta.url)` is itself one of the type errors
  // `uf check` reports against this suite today — see `story.test.js` — which
  // is a poor thing for a test about type errors to add another of.
  const repository = path.resolve(process.env.UF_PROJECT_ROOT ?? process.cwd(), "..", "..");

  // The binary running this suite, the way `lsp.test.js` names it: `uf test`
  // puts its own path in `UF_BINARY`, so this checks *this* build rather than
  // whatever `uf` is on PATH.
  const UF: string = (() => {
    const binary = process.env.UF_BINARY;
    if (binary == null || binary === "") {
      throw new Error("UF_BINARY is not set: this test runs `uf check`, and `uf test` names it");
    }
    return binary;
  })();

  // The part of `uf check --json` this reads. A message arrives as spans
  // rather than a string so that a renderer can mark the code inside it, which
  // is why the filter below joins it back together first.
  type Diagnostic = {
    primary: { path: string, start: { line: number, column: number } },
    message: Array<{ kind: string, text: string }>,
  };
  type Report = {
    typeCheck: { status: string, filesChecked: number, diagnostics: Array<Diagnostic> },
  };

  it("does not make React's key mixed", () => {
    const run = spawnSync(UF, ["check", "packages/ui", "--json"], {
      cwd: repository,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    // A non-zero status is expected: the package still has the `value-as-type`
    // errors above. The answer is on stdout either way.
    const report: Report = JSON.parse(run.stdout);
    // Without this the test would pass just as happily on a run that checked
    // nothing at all.
    expect(report.typeCheck.status).toBe("checked");
    expect(report.typeCheck.filesChecked).toBeGreaterThan(0);

    const keyed = report.typeCheck.diagnostics
      .map((diagnostic) => ({
        at: `${diagnostic.primary.path}:${String(diagnostic.primary.start.line)}`,
        said: diagnostic.message.map((span) => span.text).join(""),
      }))
      .filter((diagnostic) => diagnostic.said.includes("in property key"));
    expect(keyed).toEqual([]);
  });
});
