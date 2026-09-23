// @flow
// A page written against @uniflowed/ui 0.2, which exported every part twice.
"use client";

import * as React from "@uniflowed/react";
import { Switch, toast, Dialog, Tabs } from "@uniflowed/ui";
import type { DialogRole } from "@uniflowed/ui";

/** A settings screen; the comment's DialogRoot is prose and stays. */
export component Settings(role: DialogRole) renders Dialog.Root {
  return (
    <Dialog.Root>
      <Dialog.Trigger>Settings</Dialog.Trigger>
      <Dialog.Body role={role}>
        <Dialog.Title>Settings</Dialog.Title>
        <Tabs.Root defaultValue="general">
          <Tabs.List aria-label="Sections">
            <Tabs.Tab value="general">General</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel value="general">
            <Switch onCheckedChange={() => toast("Saved")}>Notifications</Switch>
          </Tabs.Panel>
        </Tabs.Root>
        <Dialog.Close>Done</Dialog.Close>
      </Dialog.Body>
    </Dialog.Root>
  );
}
