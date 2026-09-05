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

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import { act, fireEvent, render, screen, userEvent, within } from "@uniflowed/react-testing";
import { Checkbox, Combobox, Dialog, Field, Menu, Switch, Tabs } from "@uniflowed/ui";

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
});
