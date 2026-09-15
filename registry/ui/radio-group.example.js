// @flow
import * as React from "@uniflowed/react";

import { RadioGroup, RadioGroupItem } from "./radio-group.js";

/** A plan, one of which is chosen and one of which is unavailable. */
export component Example() {
  return (
    <RadioGroup aria-label="Plan" defaultValue="team" name="plan">
      <RadioGroupItem value="personal">Personal</RadioGroupItem>
      <RadioGroupItem value="team">Team</RadioGroupItem>
      <RadioGroupItem disabled value="enterprise">
        Enterprise
      </RadioGroupItem>
    </RadioGroup>
  );
}
