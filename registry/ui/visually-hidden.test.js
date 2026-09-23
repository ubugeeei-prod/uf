// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen } from "@uniflowed/react-testing";
import { Example } from "./visually-hidden.example.js";
afterEach(cleanup);
it("names the icon button and shows the skip link only while it has focus", () => {
  render(<Example />);
  expect(screen.getByRole("button", { name: "Close the panel" })).not.toBeNull();
  const link = screen.getByRole("link", { name: "Skip to content" });
  const block = link.parentElement;
  if (!(link instanceof HTMLElement) || block == null)
    throw new Error("expected a link in a block");
  expect(block.style.position).toBe("absolute");
  act(() => link.focus());
  expect(block.style.position).toBe("");
  expect(block.getAttribute("class")).not.toBeNull();
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  await expect(container).toHaveNoAxeViolations();
});
