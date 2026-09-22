// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./color-picker.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  fireEvent.keyDown(screen.getByRole("slider", { name: "Red" }), { key: "ArrowRight" });
  expect(String((screen.getByRole("textbox", { name: "Hex color" }) as $FlowFixMe).value)).toBe(
    "#346699",
  );
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
