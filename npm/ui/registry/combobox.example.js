"use client";
// @flow
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";

import * as Combobox from "./combobox.js";

const COUNTRIES: $ReadOnlyArray<string> = ["France", "Germany", "Japan", "Kenya", "Norway"];

/** A country, filtered by the page as the reader types. */
export component Example() {
  const [query, setQuery] = useState("");
  const typed = query.trim().toLowerCase();
  const matches = COUNTRIES.filter((country) => country.toLowerCase().includes(typed));
  return (
    <Combobox.Root inputValue={query} name="country" onInputValueChange={setQuery}>
      <Combobox.Label>Country</Combobox.Label>
      <Combobox.Input placeholder="Start typing a country" />
      <Combobox.List>
        {matches.map((country) => (
          <Combobox.Option key={country} value={country}>
            {country}
          </Combobox.Option>
        ))}
      </Combobox.List>
      <Combobox.Empty>No country matches.</Combobox.Empty>
      <Combobox.Status />
    </Combobox.Root>
  );
}
