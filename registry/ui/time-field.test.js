// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./time-field.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const hour = screen.getByRole("spinbutton", { name: "hour" });
  fireEvent.keyDown(hour, { key: "ArrowUp" });
  fireEvent.keyDown(hour, { key: "Enter" });
  expect(String((hour as $FlowFixMe).value)).toBe("15");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
