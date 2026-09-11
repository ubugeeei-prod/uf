// @flow

import * as React from "@uniflowed/react";

import { sessionData, settingsData } from "../social-queries.js";
import { SocialFrame } from "../social-frame.js";

import { SettingsRegion } from "./settings-region.client.js";
import type { Settings, Session, Protected } from "../social-model.js";

/** Resolved shell identity and a deferred, authorized private profile. */
export type Data = {| readonly session: Session, readonly profile: Promise<Protected<Settings>> |};

/**
 * Start shell identity and private profile reads together without blocking on the editor data.
 */
export async function loader(): Promise<Data> {
  const session = sessionData();
  const profile = settingsData();

  return { session: await session, profile };
}

/** Render the account shell with its own retryable profile region. */
export component Page(data: Data) {
  return (
    <SocialFrame active="settings" session={data.session} aside={false}>
      <header className="page-heading">
        <div>
          <h1>Settings</h1>
          <p>Manage your profile and account.</p>
        </div>
      </header>
      <SettingsRegion initial={data.profile} />
    </SocialFrame>
  );
}
