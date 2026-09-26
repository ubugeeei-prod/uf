// @flow
import * as React from "@uniflowed/react";
import { ListBox } from "./list-box.js";
const items = [
  { key: "alpha", textValue: "Alpha" },
  { key: "beta", textValue: "Beta" },
  { key: "gamma", textValue: "Gamma", disabled: true },
];

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return <ListBox aria-label="Options" items={items} selectionMode="multiple" />;
}
