// @flow
//
// The notices `uf ui add toast` writes: a named region, a notice named by its
// title that leaves focus where it was, dismissal from the close button and from
// an action, and the whole accessible empty and with a notice.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./toast.example.js";
import { dismissAllToasts, toast } from "./toast.js";
import * as Toast from "./toast.js";

afterEach(() => {
  act(() => {
    dismissAllToasts();
  });
  cleanup();
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

/** Render the example and raise its notice. */
async function raised(): Promise<HTMLElement> {
  render(<Example />);
  const save = html(screen.getByRole("button", { name: "Save draft" }));
  await userEvent.click(save);
  return save;
}

describe("Toast", () => {
  it("shows a notice named by its title, and leaves focus where it was", async () => {
    const save = await raised();
    expect(screen.getByRole("region", { name: "Notifications" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Draft saved" })).toHaveTextContent(
      "Your changes are kept on this device.",
    );
    expect(save).toHaveFocus();
  });

  it("dismisses a notice from its close button", async () => {
    await raised();
    await userEvent.click(html(screen.getByRole("button", { name: "Dismiss" })));
    expect(screen.queryByRole("group", { name: "Draft saved" })).toBeNull();
  });

  it("dismisses a notice from its action", async () => {
    render(<Example />);
    act(() => {
      toast(
        <>
          <Toast.Title>Export ready</Toast.Title>
          <Toast.Action>Open</Toast.Action>
        </>,
        { duration: null },
      );
    });
    await userEvent.click(html(screen.getByRole("button", { name: "Open" })));
    expect(screen.queryByRole("group", { name: "Export ready" })).toBeNull();
  });

  it("dresses the region, the notice and its close button", async () => {
    await raised();
    expect(
      screen.getByRole("region", { name: "Notifications" }).getAttribute("class"),
    ).not.toBeNull();
    expect(screen.getByRole("group", { name: "Draft saved" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Dismiss" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, empty or with a notice", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Save draft" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
