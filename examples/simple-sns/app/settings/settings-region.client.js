"use client";
// @flow

import * as React from "@uniflowed/react";
import { settingsData } from "../social-queries.js";
import { AsyncRegion, useRetryableResource } from "../async-region.client.js";
import { LoadingState, SignInPrompt } from "../ui.js";
import { SettingsClient } from "./settings-client.js";
import type { Settings, Protected } from "../social-model.js";

component Profile(data: Protected<Settings>) {
  return match (data) {
    {kind: "unauthenticated"} => <SignInPrompt title="Sign in to manage your account" />,
    {kind: "ready", value: const settings} => <SettingsClient initial={settings} />,
  };
}

/** Load and retry the private profile independently, then hand resolved data to the editor. */
export component SettingsRegion(initial: Promise<Protected<Settings>>) {
  const { resource, retry } = useRetryableResource(initial, settingsData);

  return (
    <AsyncRegion
      resource={resource}
      retry={retry}
      label="profile"
      pending={<LoadingState kind="profile" />}
    >
      {(data) => <Profile data={data} />}
    </AsyncRegion>
  );
}
