// @flow
//
// The avatar `uf ui add avatar` writes: initials shown when the picture is not,
// a picture that is decoration beside a name, and the circle drawn.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, waitFor } from "@uniflowed/react-testing";

import * as Avatar from "./avatar.js";
import { Example } from "./avatar.example.js";

afterEach(() => {
  cleanup();
});

describe("Avatar", () => {
  it("shows the initials while there is no picture to show", async () => {
    render(<Example />);
    await waitFor(() => {
      expect(screen.getByText("AL")).toBeInTheDocument();
    });
  });

  it("draws the circle and the initials", async () => {
    render(
      <Avatar.Root size="lg">
        <Avatar.Fallback delay={0}>GH</Avatar.Fallback>
      </Avatar.Root>,
    );
    await waitFor(() => {
      expect(screen.getByText("GH")).toBeInTheDocument();
    });
    const initials = screen.getByText("GH");
    expect(initials.getAttribute("class")).not.toBeNull();
    expect(initials.parentElement?.getAttribute("class") ?? null).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await waitFor(() => {
      expect(screen.getByText("AL")).toBeInTheDocument();
    });
    await expect(container).toHaveNoAxeViolations();
  });
});
