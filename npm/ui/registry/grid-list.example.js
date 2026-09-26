// @flow
import * as React from "@uniflowed/react";
import { GridList } from "./grid-list.js";
const items = [
  { key: "alpha", textValue: "Alpha" },
  { key: "beta", textValue: "Beta" },
  { key: "gamma", textValue: "Gamma", disabled: true },
];

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return <GridList aria-label="Options" items={items} selectionMode="multiple" />;
}
