// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./date-range-picker.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const trigger = screen.getByRole("button", { name: "Choose range" });
  fireEvent.click(trigger);
  expect(screen.getByRole("grid")).toHaveAttribute("aria-multiselectable", "true");
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
  fireEvent.click(screen.getByRole("button", { name: "Choose range" }));
  await expect(container).toHaveNoAxeViolations();
});
