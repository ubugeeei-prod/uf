// @flow
// A page written against @uniflowed/ui 0.2, which exported every part twice.
"use client";

import * as React from "@uniflowed/react";
import {
  DialogBody,
  DialogClose,
  DialogRoot,
  DialogTitle,
  DialogTrigger as OpenButton,
  Switch,
  TabsList,
  TabsPanel,
  TabsRoot,
  TabsTab,
  toast,
} from "@uniflowed/ui";
import type { DialogRole } from "@uniflowed/ui";

/** A settings screen; the comment's DialogRoot is prose and stays. */
export component Settings(role: DialogRole) renders DialogRoot {
  return (
    <DialogRoot>
      <OpenButton>Settings</OpenButton>
      <DialogBody role={role}>
        <DialogTitle>Settings</DialogTitle>
        <TabsRoot defaultValue="general">
          <TabsList aria-label="Sections">
            <TabsTab value="general">General</TabsTab>
          </TabsList>
          <TabsPanel value="general">
            <Switch onCheckedChange={() => toast("Saved")}>Notifications</Switch>
          </TabsPanel>
        </TabsRoot>
        <DialogClose>Done</DialogClose>
      </DialogBody>
    </DialogRoot>
  );
}
