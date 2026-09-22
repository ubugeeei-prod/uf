// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./number-field.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const input = screen.getByRole("spinbutton", { name: "Quantity" });
  fireEvent.keyDown(input, { key: "End" });
  expect(input).toHaveAttribute("aria-valuenow", "5");
  expect(screen.getByRole("button", { name: "Increase" })).toBeDisabled();
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
