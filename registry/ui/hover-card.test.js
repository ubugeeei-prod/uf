// @flow
//
// The hover card `uf ui add hover-card` writes. `@uniflowed/ui`'s own suite
// holds the timing; these hold that dressing it kept the trigger a link, the
// card reachable by `Tab`, and the dressing accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";

import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card.js";
import { Example } from "./hover-card.example.js";

afterEach(() => {
  cleanup();
});

/**
 * The element a query found, as the element `userEvent` presses.
 *
 * `screen` answers with an `Element` and `userEvent` asks for an `HTMLElement`,
 * which is ubugeeei-prod/uf#1017 rather than anything about this component.
 */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("HoverCard", () => {
  it("makes its trigger a link that goes somewhere on its own", () => {
    render(<Example />);
    expect(screen.getByRole("link", { name: "Ada Lovelace" })).toHaveAttribute(
      "href",
      "/people/ada",
    );
    expect(screen.queryByText("Writes the notes that outlive the engine.")).toBeNull();
  });

  // How long it waits is `@uniflowed/ui`'s to hold, and its suite does; this
  // asks for no wait so that what is checked here is the dressing.
  it("opens for a pointer", () => {
    render(
      <HoverCard openDelay={0}>
        <HoverCardTrigger href="/people/ada">Ada Lovelace</HoverCardTrigger>
        <HoverCardContent>
          <p>A preview.</p>
        </HoverCardContent>
      </HoverCard>,
    );
    const trigger = html(screen.getByRole("link", { name: "Ada Lovelace" }));

    fireEvent.pointerEnter(trigger);
    expect(screen.getByText("A preview.")).toBeInTheDocument();
  });

  it("opens on focus at once, and lets Tab walk into it", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("link", { name: "Ada Lovelace" }));
    act(() => {
      trigger.focus();
    });
    expect(screen.getByText("Writes the notes that outlive the engine.")).toBeInTheDocument();

    await userEvent.tab();
    expect(screen.getByRole("link", { name: "Read her notes" })).toHaveFocus();
  });

  it("is dismissed by Escape, and gives focus back to its link", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("link", { name: "Ada Lovelace" }));
    act(() => {
      trigger.focus();
    });
    await userEvent.tab();
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByText("Writes the notes that outlive the engine.")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("dresses the link and the card", () => {
    render(<Example />);
    const trigger = html(screen.getByRole("link", { name: "Ada Lovelace" }));
    expect(trigger.getAttribute("class")).not.toBeNull();
    act(() => {
      trigger.focus();
    });
    const card = document.querySelector('[data-state="open"]');
    expect(card).not.toBeNull();
    expect(card?.getAttribute("class") ?? null).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    act(() => {
      html(screen.getByRole("link", { name: "Ada Lovelace" })).focus();
    });
    await expect(container).toHaveNoAxeViolations();
  });
});
