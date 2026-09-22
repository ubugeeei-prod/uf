// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./list-box.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const control = screen.getByRole("listbox", { name: "Options" });
  fireEvent.keyDown(control, { key: "ArrowDown" });
  fireEvent.keyDown(control, { key: " " });
  const selected = screen.getByRole("option", { name: "Beta" });
  expect(selected).toHaveAttribute("aria-selected", "true");
  expect(selected).toHaveAttribute("data-active", "true");
  expect(selected.querySelector("[data-selected=true][data-active=true]")).not.toBeNull();
  expect(control).toHaveAttribute("aria-activedescendant", selected.id);
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
