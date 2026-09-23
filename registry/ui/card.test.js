// @flow
//
// The card `uf ui add card` writes, held to what its header promises: no role
// of its own, a title that is a real heading at the level it is given, the
// actions after the content, and nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import * as Card from "./card.js";
import { Example } from "./card.example.js";

afterEach(() => {
  cleanup();
});

describe("Card", () => {
  it("adds no landmark to the page", () => {
    const { container } = render(<Example />);
    expect(screen.queryByRole("region")).toBeNull();
    expect(screen.queryByRole("article")).toBeNull();
    expect(container.querySelector("[role]")).toBeNull();
  });

  it("titles the card with a heading at the level it is given, 3 by default", () => {
    render(<Example />);
    expect(screen.getByRole("heading", { level: 2, name: "Weekly digest" }).tagName).toBe("H2");

    cleanup();
    render(
      <Card.Root>
        <Card.Title>Plain</Card.Title>
      </Card.Root>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "Plain" }).tagName).toBe("H3");
  });

  it("reaches the actions after the content they act on", () => {
    const { container } = render(<Example />);
    const text = container.textContent ?? "";
    expect(text.indexOf("A summary")).toBeLessThan(text.indexOf("Subscribe"));
  });

  it("puts every attribute it does not name on the element", () => {
    render(
      <Card.Root aria-labelledby="t" data-kind="plan" id="plan">
        <Card.Content data-part="body">Body</Card.Content>
      </Card.Root>,
    );
    const body = screen.getByText("Body");
    expect(body).toHaveAttribute("data-part", "body");
    const card = body.parentElement;
    expect(card?.getAttribute("id") ?? null).toBe("plan");
    expect(card?.getAttribute("data-kind") ?? null).toBe("plan");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
