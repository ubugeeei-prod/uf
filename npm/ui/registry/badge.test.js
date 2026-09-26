// @flow
//
// The badge `uf ui add badge` writes, held to what its header promises: a
// `<span>` that passes through every attribute and is read as its words, with
// every tone drawn and nothing in it failing an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Badge } from "./badge.js";
import { Example } from "./badge.example.js";

afterEach(() => {
  cleanup();
});

describe("Badge", () => {
  it("is a span read as its words, with no role and nothing to focus", () => {
    render(<Badge tone="danger">Failed</Badge>);
    const badge = screen.getByText("Failed");
    expect(badge.tagName).toBe("SPAN");
    expect(badge.getAttribute("role")).toBeNull();
    expect(badge.getAttribute("tabindex")).toBeNull();
  });

  it("puts every attribute it does not name on the element", () => {
    render(
      <Badge aria-label="3 unread" data-state="unread" id="count" title="Unread">
        3
      </Badge>,
    );
    const badge = screen.getByText("3");
    expect(badge).toHaveAttribute("aria-label", "3 unread");
    expect(badge).toHaveAttribute("data-state", "unread");
    expect(badge).toHaveAttribute("id", "count");
    expect(badge).toHaveAttribute("title", "Unread");
  });

  it("draws each tone differently and keeps a caller's class beside its own", () => {
    render(<Example />);
    const classes = ["Draft", "Beta", "New", "Failed", "v0.1.0"].map(
      (text) => screen.getByText(text).getAttribute("class") ?? "",
    );
    expect(new Set(classes).size).toBe(classes.length);

    cleanup();
    render(<Badge className="mine">Beta</Badge>);
    const own = screen.getByText("Beta").getAttribute("class") ?? "";
    expect(own.split(" ")).toContain("mine");
    expect(own.split(" ").length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
