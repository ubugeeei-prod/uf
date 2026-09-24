// @flow
import * as React from "@uniflowed/react";

import * as Select from "./select.js";

/** A country, in two named groups, with one option unavailable. */
export component Example() {
  return (
    <Select.Root name="country">
      <Select.Label>Country</Select.Label>
      <Select.Trigger>
        <Select.Value placeholder="Choose a country" />
      </Select.Trigger>
      <Select.List>
        <Select.Group>
          <Select.GroupLabel>Europe</Select.GroupLabel>
          <Select.Option value="fr">France</Select.Option>
          <Select.Option value="de">Germany</Select.Option>
          <Select.Option value="gb">United Kingdom</Select.Option>
        </Select.Group>
        <Select.Separator />
        <Select.Group>
          <Select.GroupLabel>Asia</Select.GroupLabel>
          <Select.Option value="jp">Japan</Select.Option>
          <Select.Option disabled value="kr">
            South Korea
          </Select.Option>
        </Select.Group>
      </Select.List>
    </Select.Root>
  );
}
