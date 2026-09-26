// @flow
import * as React from "@uniflowed/react";

import * as ButtonGroup from "./button-group.js";

/** Three actions on one message, joined into a named bar. */
export component Example() {
  return (
    <ButtonGroup.Root label="Message actions">
      <ButtonGroup.Item>Archive</ButtonGroup.Item>
      <ButtonGroup.Item>Report</ButtonGroup.Item>
      <ButtonGroup.Item disabled>Snooze</ButtonGroup.Item>
    </ButtonGroup.Root>
  );
}
