"use client";
// @flow

import * as React from "@uniflowed/react";

import { settingsData } from "../_server/social-queries.js";
import { AsyncRegion, useRetryableResource } from "../_shared/async-region.client.js";
import { LoadingState, SignInPrompt } from "../_shared/ui.js";
import { SettingsClient } from "./settings.client.js";

import type { Settings, Protected } from "../_shared/social-model.js";

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
