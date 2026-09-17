// @flow
//
// Correct uses of `userEvent` with elements found by `screen`.
//
// `screen` queries return `Element`, because SVG and other non-HTML elements
// can have roles too. A component test should still be able to press, type
// into, clear, blur and select controls directly from the query that found
// them.

import * as React from "../../packages/react/index.js";
import { render, screen, userEvent } from "../../packages/react-testing/index.js";

component Controls() {
  return (
    <form>
      <button type="button">Save</button>

      <label htmlFor="name">Name</label>
      <input id="name" />

      <label htmlFor="plan">Plan</label>
      <select id="plan" defaultValue="free">
        <option value="free">Free</option>
        <option value="pro">Pro</option>
      </select>
    </form>
  );
}

export async function interactsWithQueriedControls(): Promise<void> {
  render(<Controls />);

  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "Ada");
  await userEvent.clear(screen.getByRole("textbox", { name: "Name" }));
  await userEvent.selectOptions(screen.getByRole("combobox", { name: "Plan" }), "pro");
  await userEvent.tabAway(screen.getByRole("textbox", { name: "Name" }));
}
