// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./tag-group.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  fireEvent.keyDown(screen.getByRole("grid", { name: "Tags" }), { key: "Delete" });
  expect(screen.queryByText("Alpha")).toBeNull();
  expect(screen.getByText("Beta")).toBeInTheDocument();
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
