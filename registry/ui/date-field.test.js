// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";
import { Example } from "./date-field.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", async () => {
  render(<Example />);
  const day = screen.getByRole("spinbutton", { name: "day" });
  day.focus();
  await userEvent.keyboard("{ArrowUp}{Enter}");
  expect(String((day as $FlowFixMe).value)).toBe("15");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
