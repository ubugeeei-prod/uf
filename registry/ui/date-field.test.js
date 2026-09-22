// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./date-field.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const day = screen.getByRole("spinbutton", { name: "day" });
  fireEvent.keyDown(day, { key: "ArrowUp" });
  fireEvent.keyDown(day, { key: "Enter" });
  expect(String((day as $FlowFixMe).value)).toBe("15");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
