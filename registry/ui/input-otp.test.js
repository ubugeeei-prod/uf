// @flow
//
// The code field `uf ui add input-otp` writes: one named field under the boxes,
// characters a numeric code refuses dropped, the boxes showing what the field
// holds, the next box marked only while the field has focus, the code reported
// once it is whole, and the row accessible empty and filled.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { InputOtp, InputOtpSlot } from "./input-otp.js";
import { Example } from "./input-otp.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` types into. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

/** What each box shows, in order. */
function boxes(container: Element): $ReadOnlyArray<string> {
  return Array.from(container.querySelectorAll("[data-index]")).map((box) => box.textContent ?? "");
}

describe("InputOtp", () => {
  it("is one field named by its label, offered the code a phone received", () => {
    render(<Example />);
    const field = screen.getByRole("textbox", { name: "Verification code" });
    expect(field).toHaveAttribute("autocomplete", "one-time-code");
    expect(field).toHaveAttribute("maxlength", "6");
  });

  it("draws what the field holds, and drops what a numeric code refuses", async () => {
    const { container } = render(<Example />);
    await userEvent.type(html(screen.getByRole("textbox", { name: "Verification code" })), "12a3");
    expect(boxes(container)).toEqual(["1", "2", "3", "", "", ""]);
    expect(container.querySelector('[data-index="2"]')).toHaveAttribute("data-filled", "true");
    expect(container.querySelector('[data-index="3"]')).not.toHaveAttribute("data-filled");
  });

  it("marks the box the next character goes into, only while the field has focus", async () => {
    const { container } = render(<Example />);
    expect(container.querySelector("[data-active]")).toBeNull();
    await userEvent.type(html(screen.getByRole("textbox", { name: "Verification code" })), "12");
    expect(container.querySelector("[data-active]")).toHaveAttribute("data-index", "2");
  });

  it("reports the code once every box is full", async () => {
    const codes: Array<string> = [];
    render(
      <InputOtp
        label="Code"
        length={4}
        onComplete={(code) => {
          codes.push(code);
        }}
      >
        <InputOtpSlot index={0} />
        <InputOtpSlot index={1} />
        <InputOtpSlot index={2} />
        <InputOtpSlot index={3} />
      </InputOtp>,
    );
    await userEvent.type(html(screen.getByRole("textbox", { name: "Code" })), "4821");
    expect(codes).toEqual(["4821"]);
  });

  it("dresses the boxes, and keeps them out of what a reader hears", () => {
    const { container } = render(<Example />);
    const all = Array.from(container.querySelectorAll("[data-index]"));
    expect(all).toHaveLength(6);
    for (const box of all) {
      expect(box.getAttribute("class")).not.toBeNull();
      expect(box).toHaveAttribute("aria-hidden", "true");
    }
  });

  it("has no accessibility violations, empty or filled", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.type(
      html(screen.getByRole("textbox", { name: "Verification code" })),
      "123456",
    );
    await expect(container).toHaveNoAxeViolations();
  });
});
