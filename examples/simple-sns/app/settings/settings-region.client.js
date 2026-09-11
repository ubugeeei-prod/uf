"use client";
// @flow
import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";
import { settingsData } from "../social-queries.js";
import { AsyncRegion, useRetryableResource } from "../async-region.client.js";
import { LoadingState, SignInPrompt } from "../ui.js";
import { SettingsClient } from "./settings-client.js";
import type { Settings, Protected } from "../social-model.js";
component Profile(data: Promise<Protected<Settings>>) {
  return match (use(data)) {
    {kind: "unauthenticated"} => <SignInPrompt title="Sign in to manage your account" />,
    {kind: "ready", value: const settings} => <SettingsClient initial={settings} />,
  };
}

export component SettingsRegion(initial: Promise<Protected<Settings>>) {
  const { resource, generation, retry } = useRetryableResource(initial, settingsData);
  return (
    <AsyncRegion
      generation={generation}
      retry={retry}
      label="profile"
      pending={<LoadingState kind="profile" />}
    >
      <Profile data={resource} />
    </AsyncRegion>
  );
}
