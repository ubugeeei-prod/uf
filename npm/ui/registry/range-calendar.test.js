// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./range-calendar.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const grid = screen.getByRole("grid");
  expect(grid).toHaveAttribute("aria-multiselectable", "true");
  const day = screen.getByRole("gridcell", { name: "14" });
  fireEvent.keyDown(day, { key: "ArrowRight" });
  expect(screen.getByRole("gridcell", { name: "15" })).toHaveAttribute("tabindex", "0");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
