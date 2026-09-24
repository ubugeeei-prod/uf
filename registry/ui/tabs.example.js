// @flow
import * as React from "@uniflowed/react";

import * as Tabs from "./tabs.js";

/** Three sections of a project, one of them unavailable. */
export component Example() {
  return (
    <Tabs.Root defaultValue="overview">
      <Tabs.List aria-label="Project">
        <Tabs.Tab value="overview">Overview</Tabs.Tab>
        <Tabs.Tab value="activity">Activity</Tabs.Tab>
        <Tabs.Tab disabled value="billing">
          Billing
        </Tabs.Tab>
      </Tabs.List>
      <Tabs.Panel value="overview">Everything the project is doing, at a glance.</Tabs.Panel>
      <Tabs.Panel value="activity">The last thirty days of changes.</Tabs.Panel>
      <Tabs.Panel value="billing">Invoices, and the plan they are for.</Tabs.Panel>
    </Tabs.Root>
  );
}
