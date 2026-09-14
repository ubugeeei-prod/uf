// @flow
//
// The button `uf ui add button` writes, held to what its header promises: the
// attributes reach the element, the default does not submit a form, both
// spellings of disabled stay what they are, and nothing about it fails an
// accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Button } from "./button.js";
import { Example } from "./button.example.js";

afterEach(() => {
  cleanup();
});

describe("Button", () => {
  it("submits nothing unless it says it is a submit button", () => {
    render(
      <form>
        <Button>Cancel</Button>
        <Button type="submit">Save</Button>
      </form>,
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveAttribute("type", "button");
    expect(screen.getByRole("button", { name: "Save" })).toHaveAttribute("type", "submit");
  });

  it("puts every attribute it does not name on the element", () => {
    render(
      <Button aria-describedby="hint" form="settings" formAction="/save" name="intent" value="save">
        Save
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toHaveAttribute("aria-describedby", "hint");
    expect(button).toHaveAttribute("form", "settings");
    expect(button).toHaveAttribute("formaction", "/save");
    expect(button).toHaveAttribute("name", "intent");
    expect(button).toHaveAttribute("value", "save");
  });

  it("hands its ref to the element", () => {
    let element: HTMLButtonElement | null = null;
    render(
      <Button
        ref={(node: HTMLButtonElement | null) => {
          element = node;
        }}
      >
        Save
      </Button>,
    );
    expect(element).toBe(screen.getByRole("button", { name: "Save" }));
  });

  it("keeps a caller's class beside its own", () => {
    render(<Button className="wide">Save</Button>);
    const classes = (
      screen.getByRole("button", { name: "Save" }).getAttribute("class") ?? ""
    ).split(" ");
    expect(classes).toContain("wide");
    expect(classes.length > 1).toBe(true);
  });

  it("stays in the tab order when it is aria-disabled, and leaves it when it is disabled", async () => {
    render(
      <>
        <Button disabled>Saving…</Button>
        <Button aria-disabled="true">Not yet</Button>
      </>,
    );
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Not yet" })).toHaveFocus();
  });

  it("has no accessibility violations in any tone or size", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
