// @flow
//
// The date picker `uf ui add date-picker` writes: a named field holding the
// date, a named button that opens the calendar on the chosen month, a day
// chosen there written into the field, a field that says when it does not hold
// a date, and the whole accessible closed and open.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./date-picker.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

/** What a text field holds. */
function valueOf(element: Element): string {
  return String((element as $FlowFixMe).value);
}

describe("DatePicker", () => {
  it("is a named field holding the date, and a named button", () => {
    render(<Example />);
    expect(valueOf(screen.getByRole("textbox", { name: "Start date" }))).toBe("2026-09-14");
    expect(screen.getByRole("button", { name: "Choose a date" })).toBeInTheDocument();
  });

  it("opens the calendar on the chosen month, and writes a day chosen there", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Choose a date" })));
    expect(screen.getByRole("grid", { name: "September 2026" })).toBeInTheDocument();
    await userEvent.click(html(screen.getByRole("gridcell", { name: "16" })));
    expect(valueOf(screen.getByRole("textbox", { name: "Start date" }))).toBe("2026-09-16");
  });

  it("says when the field does not hold a date", async () => {
    render(<Example />);
    const field = html(screen.getByRole("textbox", { name: "Start date" }));
    await userEvent.clear(field);
    await userEvent.type(field, "someday");
    // Leaving the field is what commits it. Dispatched on the field itself
    // rather than through `field.blur()`, which does nothing unless the field
    // still has focus, and whether it does after typing depends on what else
    // the worker ran first.
    fireEvent.blur(field);
    expect(field).toHaveAttribute("aria-invalid", "true");
  });

  it("dresses the field", () => {
    render(<Example />);
    expect(
      screen.getByRole("textbox", { name: "Start date" }).getAttribute("class"),
    ).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Choose a date" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
