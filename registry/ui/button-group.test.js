// @flow
//
// The button group `uf ui add button-group` writes, held to what its header
// promises: a named group of real buttons that are each a tab stop, which
// submit nothing unless asked, and nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import * as ButtonGroup from "./button-group.js";
import { Example } from "./button-group.example.js";

afterEach(() => {
  cleanup();
});

describe("ButtonGroup", () => {
  it("is a group named by its label, holding real buttons", () => {
    render(<Example />);
    const group = screen.getByRole("group", { name: "Message actions" });
    const buttons = Array.from(group.querySelectorAll("button"));
    expect(buttons.map((button) => button.textContent)).toEqual(["Archive", "Report", "Snooze"]);
    for (const button of buttons) {
      expect(button).toHaveAttribute("type", "button");
    }
    expect(screen.getByRole("button", { name: "Snooze" })).toBeDisabled();
  });

  it("leaves every enabled button its own tab stop", async () => {
    render(<Example />);
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Archive" }));
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Report" }));
  });

  it("passes a press, a submit type and every attribute through to the button", async () => {
    const pressed: Array<string> = [];
    render(
      <ButtonGroup.Root className="mine" data-testid="bar" label="Save">
        <ButtonGroup.Item onClick={() => pressed.push("draft")}>Save draft</ButtonGroup.Item>
        <ButtonGroup.Item aria-describedby="hint" name="intent" type="submit" value="publish">
          Publish
        </ButtonGroup.Item>
      </ButtonGroup.Root>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Save draft" }));
    expect(pressed).toEqual(["draft"]);
    const publish = screen.getByRole("button", { name: "Publish" });
    expect(publish).toHaveAttribute("type", "submit");
    expect(publish).toHaveAttribute("name", "intent");
    expect(publish).toHaveAttribute("aria-describedby", "hint");
    expect((screen.getByTestId("bar").getAttribute("class") ?? "").split(" ")).toContain("mine");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
