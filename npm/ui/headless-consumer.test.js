// @flow
//
// A project that does not compile StyleX still uses `@uniflowed/ui`.
//
// Registry copies — `uf ui add`, the files under `registry/` — are StyleX, and
// the command does not rewrite one into plain CSS. This file is the other
// half. It imports a part from `@uniflowed/ui`, puts a class the project owns
// on it, and never imports `@uniflowed/stylex` or calls `stylex.create`.
//
// The preset's classes are `npm/stylex/stylex.test.js`. The keys with no class
// at all are `ui.test.js`, in "toggles a switch on Space and on Enter". The
// class spread in that file's prop block is about `Rest`, not about a project
// whose styles are a stylesheet. This one is that project.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Switch } from "@uniflowed/ui";

afterEach(() => {
  cleanup();
});

describe("a consumer that does not compile StyleX", () => {
  it("styles a switch with its own class and keeps the control", async () => {
    render(
      <Switch className="notify" defaultChecked={false}>
        Notifications
      </Switch>,
    );

    const control = screen.getByRole("switch", { name: "Notifications" });
    expect(control.getAttribute("class")).toBe("notify");
    expect(control.getAttribute("aria-checked")).toBe("false");

    await userEvent.click(control);
    expect(control.getAttribute("aria-checked")).toBe("true");
    expect(control.getAttribute("class")).toBe("notify");
  });
});
