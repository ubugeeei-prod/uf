// @flow
import * as React from "@uniflowed/react";

import {
  Select,
  SelectGroup,
  SelectGroupLabel,
  SelectLabel,
  SelectList,
  SelectOption,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./select.js";

/** A country, in two named groups, with one option unavailable. */
export component Example() {
  return (
    <Select name="country">
      <SelectLabel>Country</SelectLabel>
      <SelectTrigger>
        <SelectValue placeholder="Choose a country" />
      </SelectTrigger>
      <SelectList>
        <SelectGroup>
          <SelectGroupLabel>Europe</SelectGroupLabel>
          <SelectOption value="fr">France</SelectOption>
          <SelectOption value="de">Germany</SelectOption>
          <SelectOption value="gb">United Kingdom</SelectOption>
        </SelectGroup>
        <SelectSeparator />
        <SelectGroup>
          <SelectGroupLabel>Asia</SelectGroupLabel>
          <SelectOption value="jp">Japan</SelectOption>
          <SelectOption disabled value="kr">
            South Korea
          </SelectOption>
        </SelectGroup>
      </SelectList>
    </Select>
  );
}
