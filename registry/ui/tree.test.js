// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./tree.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const control = screen.getByRole("tree", { name: "Files" });
  fireEvent.keyDown(control, { key: "ArrowRight" });
  fireEvent.keyDown(control, { key: "ArrowDown" });
  fireEvent.keyDown(control, { key: " " });
  expect(screen.getByRole("treeitem", { name: "Child" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("treeitem", { name: "Child" })).toHaveAttribute("aria-level", "2");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
