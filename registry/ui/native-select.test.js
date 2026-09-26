// @flow
//
// The select `uf ui add native-select` writes, held to what its header
// promises: a real `<select>` named by its label, the caller's own options and
// groups, every attribute and the ref on the element, a chevron nobody hears,
// and nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { NativeSelect } from "./native-select.js";
import { Example } from "./native-select.example.js";

afterEach(() => {
  cleanup();
});

describe("NativeSelect", () => {
  it("is a real select, named by its label, holding the caller's options and groups", () => {
    render(<Example />);
    const region = screen.getByRole("combobox", { name: "Region" });
    expect(region.tagName).toBe("SELECT");
    expect(region).toHaveValue("eu-west");
    expect(region.querySelectorAll("optgroup")).toHaveLength(2);
    expect(screen.getByRole("option", { name: "EU North (full)" })).toBeDisabled();
  });

  it("changes its value the platform's way and hands its ref to the element", async () => {
    let element: HTMLSelectElement | null = null;
    const changes: Array<string> = [];
    render(
      <NativeSelect
        aria-label="Size"
        defaultValue="s"
        name="size"
        onChange={(event: { readonly currentTarget: HTMLSelectElement, ... }) => {
          changes.push(event.currentTarget.value);
        }}
        ref={(node: HTMLSelectElement | null) => {
          element = node;
        }}
      >
        <option value="s">Small</option>
        <option value="m">Medium</option>
      </NativeSelect>,
    );
    const size = screen.getByRole("combobox", { name: "Size" });
    expect(element).toBe(size);
    expect(size).toHaveAttribute("name", "size");
    await userEvent.selectOptions(size, "m");
    expect(size).toHaveValue("m");
    expect(changes).toEqual(["m"]);
  });

  it("says it is invalid and disabled through the attributes its look follows", () => {
    render(<Example />);
    const plan = screen.getByRole("combobox", { name: "Plan" });
    expect(plan).toHaveAttribute("aria-invalid", "true");
    expect(plan).toBeRequired();
    expect(screen.getByRole("combobox", { name: "Currency" })).toBeDisabled();
  });

  it("draws a chevron nobody hears, and none for a list box", () => {
    const { container } = render(<Example />);
    const chevrons = Array.from(container.querySelectorAll("svg"));
    expect(chevrons).toHaveLength(3);
    for (const chevron of chevrons) {
      expect(chevron.getAttribute("aria-hidden")).toBe("true");
    }

    cleanup();
    const list = render(
      <NativeSelect aria-label="Tags" multiple>
        <option value="a">A</option>
      </NativeSelect>,
    );
    expect(screen.getByRole("listbox", { name: "Tags" })).toHaveAttribute("multiple");
    expect(list.container.querySelector("svg")).toBeNull();
  });

  it("puts a caller's class on the select beside its own", () => {
    render(
      <NativeSelect aria-label="Mode" className="mine">
        <option value="a">A</option>
      </NativeSelect>,
    );
    const classes = (
      screen.getByRole("combobox", { name: "Mode" }).getAttribute("class") ?? ""
    ).split(" ");
    expect(classes).toContain("mine");
    expect(classes.length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
