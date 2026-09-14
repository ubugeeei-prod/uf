// @flow
import * as React from "@uniflowed/react";

import { Tabs, TabsList, TabsPanel, TabsTab } from "./tabs.js";

/** Three sections of a project, one of them unavailable. */
export component Example() {
  return (
    <Tabs defaultValue="overview">
      <TabsList aria-label="Project">
        <TabsTab value="overview">Overview</TabsTab>
        <TabsTab value="activity">Activity</TabsTab>
        <TabsTab disabled value="billing">
          Billing
        </TabsTab>
      </TabsList>
      <TabsPanel value="overview">Everything the project is doing, at a glance.</TabsPanel>
      <TabsPanel value="activity">The last thirty days of changes.</TabsPanel>
      <TabsPanel value="billing">Invoices, and the plan they are for.</TabsPanel>
    </Tabs>
  );
}
