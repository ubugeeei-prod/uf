// @flow
//
// The alert `uf ui add alert` writes: silent while it is a notice that was
// always there, a live alert when it says so, a real heading, and every tone
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import * as Alert from "./alert.js";
import { Example } from "./alert.example.js";

afterEach(() => {
  cleanup();
});

describe("Alert", () => {
  it("is not a live region while it is a notice that was on the page", () => {
    render(<Example />);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Your trial ends in 3 days").tagName).toBe("H2");
  });

  it("announces itself when it is live", () => {
    render(
      <Alert.Root live tone="danger">
        <Alert.Title>Could not save</Alert.Title>
        <Alert.Description>The connection dropped.</Alert.Description>
      </Alert.Root>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Could not save");
  });

  it("dresses every tone", () => {
    render(<Example />);
    for (const title of ["Your trial ends in 3 days", "The last export failed"]) {
      const box = screen.getByText(title).parentElement;
      expect(box?.getAttribute("class") ?? null).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
