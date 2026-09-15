// @flow
//
// The carousel `uf ui add carousel` writes: a named carousel of slides named by
// their place, one showing at a time, buttons that move between them and wrap
// around, a pause button that says whether it is stopped, and the whole
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Carousel, CarouselContent, CarouselItem, CarouselPause } from "./carousel.js";
import { Example } from "./carousel.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Carousel", () => {
  it("is a named carousel of slides named by their place, the first showing", () => {
    render(<Example />);
    expect(screen.getByRole("group", { name: "Guides" })).toHaveAttribute(
      "aria-roledescription",
      "carousel",
    );
    const first = screen.getByRole("group", { name: "1 of 3" });
    expect(first).toHaveAttribute("aria-roledescription", "slide");
    expect(first).toHaveAttribute("data-state", "active");
    expect(screen.getByRole("group", { name: "2 of 3" })).toHaveAttribute("data-state", "inactive");
  });

  it("moves to the next slide and back", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Next slide" })));
    expect(screen.getByRole("group", { name: "2 of 3" })).toHaveAttribute("data-state", "active");
    await userEvent.click(html(screen.getByRole("button", { name: "Previous slide" })));
    expect(screen.getByRole("group", { name: "1 of 3" })).toHaveAttribute("data-state", "active");
  });

  it("wraps around from the first slide to the last", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Previous slide" })));
    expect(screen.getByRole("group", { name: "3 of 3" })).toHaveAttribute("data-state", "active");
  });

  it("stops turning on its own from the pause button, and says so", async () => {
    render(
      <Carousel autoplay={60000} count={2} label="Tips">
        <CarouselContent>
          <CarouselItem index={0}>Keep a lockfile.</CarouselItem>
          <CarouselItem index={1}>Pin your toolchain.</CarouselItem>
        </CarouselContent>
        <CarouselPause />
      </Carousel>,
    );
    const pause = html(screen.getByRole("button", { name: "Stop the carousel" }));
    expect(pause).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(pause);
    expect(pause).toHaveAttribute("aria-pressed", "true");
  });

  it("dresses the buttons", () => {
    render(<Example />);
    expect(screen.getByRole("button", { name: "Next slide" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
