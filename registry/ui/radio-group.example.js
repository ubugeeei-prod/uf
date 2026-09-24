// @flow
import * as React from "@uniflowed/react";

import * as RadioGroup from "./radio-group.js";

/** A plan, one of which is chosen and one of which is unavailable. */
export component Example() {
  return (
    <RadioGroup.Root aria-label="Plan" defaultValue="team" name="plan">
      <RadioGroup.Item value="personal">Personal</RadioGroup.Item>
      <RadioGroup.Item value="team">Team</RadioGroup.Item>
      <RadioGroup.Item disabled value="enterprise">
        Enterprise
      </RadioGroup.Item>
    </RadioGroup.Root>
  );
}
