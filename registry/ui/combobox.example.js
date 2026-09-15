"use client";
// @flow
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";

import {
  Combobox,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxLabel,
  ComboboxList,
  ComboboxOption,
  ComboboxStatus,
} from "./combobox.js";

const COUNTRIES: $ReadOnlyArray<string> = ["France", "Germany", "Japan", "Kenya", "Norway"];

/** A country, filtered by the page as the reader types. */
export component Example() {
  const [query, setQuery] = useState("");
  const typed = query.trim().toLowerCase();
  const matches = COUNTRIES.filter((country) => country.toLowerCase().includes(typed));
  return (
    <Combobox inputValue={query} name="country" onInputValueChange={setQuery}>
      <ComboboxLabel>Country</ComboboxLabel>
      <ComboboxInput placeholder="Start typing a country" />
      <ComboboxList>
        {matches.map((country) => (
          <ComboboxOption key={country} value={country}>
            {country}
          </ComboboxOption>
        ))}
      </ComboboxList>
      <ComboboxEmpty>No country matches.</ComboboxEmpty>
      <ComboboxStatus />
    </Combobox>
  );
}
