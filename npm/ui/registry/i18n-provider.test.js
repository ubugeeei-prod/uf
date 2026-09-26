// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";
import { Example } from "./i18n-provider.example.js";
afterEach(cleanup);
it("preserves the primitive's keyboard behavior", () => {
  render(<Example />);
  const region = screen.getByRole("region", { name: "Arabic settings" });
  expect(region).toHaveAttribute("lang", "ar-EG");
  expect(region).toHaveAttribute("dir", "rtl");
  const input = screen.getByRole("spinbutton", { name: "Quantity" });
  fireEvent.keyDown(input, { key: "ArrowUp" });
  expect(input).toHaveAttribute("aria-valuenow", "3");
  expect(String((input as $FlowFixMe).value)).toBe("٣");
});
it("ships a styled example without accessibility violations", async () => {
  const { container } = render(<Example />);
  expect(container.querySelector("[class]")).not.toBeNull();
  await expect(container).toHaveNoAxeViolations();
});
