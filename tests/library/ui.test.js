// @flow
//
// `@uniflowed/ui`.
//
// These test the part that is invisible: what a screen reader is told, and what
// the keyboard does. A snapshot of the markup would pass while every one of
// these was broken.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import { fireEvent, render, screen, userEvent, within } from "@uniflowed/react-testing";
import { Checkbox, Dialog, Field, Switch, Tabs } from "@uniflowed/ui";

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
    // An id naming a missing element makes a screen reader announce nothing at
    // all, which is worse than leaving the attribute off.
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
  component Example() {
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
    const stops = screen
      .getAllByRole("tab")
      .filter((tab) => tab.getAttribute("tabindex") === "0");
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

  it("ties each tab to the panel it controls", () => {
    render(<Example />);
    const tab = screen.getByRole("tab", { name: "One" });
    const panel = screen.getByRole("tabpanel");
    expect(tab.getAttribute("aria-controls")).toBe(panel.getAttribute("id"));
    expect(panel.getAttribute("aria-labelledby")).toBe(tab.getAttribute("id"));
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

describe("Dialog", () => {
  component Example() {
    return (
      <Dialog.Root>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Content>
          <Dialog.Title>Are you sure?</Dialog.Title>
          <button type="button">Confirm</button>
          <Dialog.Close>Cancel</Dialog.Close>
        </Dialog.Content>
      </Dialog.Root>
    );
  }

  it("is closed until it is opened", () => {
    render(<Example />);
    expect(screen.queryByRole("dialog")).toBe(null);
    expect(screen.getByRole("button", { name: "Open" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("opens, and says the rest of the page is unavailable", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    // Its accessible name comes from the title, not from a guess.
    expect(dialog.getAttribute("aria-labelledby")).toBe(
      screen.getByRole("heading").getAttribute("id"),
    );
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
        <Dialog.Content>
          <Dialog.Title>Nothing to do</Dialog.Title>
        </Dialog.Content>
      </Dialog.Root>,
    );
    const dialog = screen.getByRole("dialog");
    // The dialog itself takes focus, and Tab has nowhere to go.
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(dialog).toHaveFocus();
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
    // Otherwise focus restarts at the top of the document and the reader has
    // to find their place again.
    expect(trigger).toHaveFocus();
  });

  it("closes from its own close button", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBe(null);
  });

  it("reports opening and closing to a controlled parent", async () => {
    const onOpenChange = fn();
    render(
      <Dialog.Root onOpenChange={onOpenChange}>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Content>
          <Dialog.Title>Title</Dialog.Title>
        </Dialog.Content>
      </Dialog.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(onOpenChange).toHaveBeenCalledWith(true);
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

  it("toggles on Space and on Enter", async () => {
    render(<Switch aria-label="Notifications" />);
    const control = screen.getByRole("switch");
    await userEvent.click(control);
    await userEvent.keyboard(" ");
    expect(control).not.toBeChecked();
    await userEvent.keyboard("{Enter}");
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
});
