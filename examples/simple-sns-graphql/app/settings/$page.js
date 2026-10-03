// @flow

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";
import { props } from "@uniflowed/stylex";

import type { SearchParams } from "@uniflowed/router";

import { preloadSession, relay } from "../_server/relay.server.js";
import { SignedIn } from "../_shared/session.client.js";
import { SocialFrame } from "../_shared/social-frame.js";
import { LoadingState, SignInPrompt, styles as uiStyles } from "../_shared/ui.js";
import settingsQuery from "./__generated__/SnsSettingsQuery.graphql.js";
import { SettingsForm } from "./settings.client.js";

export const dynamic = "force-dynamic";

/** Only the form is a client island; an anonymous request's preload carries no private fields. */

export component Page(searchParams: SearchParams) {
  const session = preloadSession();

  return (
    <SocialFrame active="settings" aside={false} session={session}>
      <Suspense fallback={<LoadingState kind="profile" />}>
        <SignedIn queryRef={session} guest={<SignInPrompt />}>
          <section {...props(uiStyles.settingsPanel)}>
            <header {...props(uiStyles.pageHeading)}>
              <div>
                <h1 {...props(uiStyles.pageTitle)}>Settings</h1>
                <p {...props(uiStyles.pageSummary)}>Your profile and account.</p>
              </div>
            </header>
            <SettingsForm queryRef={relay.serverPreloadQuery(settingsQuery, {})} />
          </section>
        </SignedIn>
      </Suspense>
    </SocialFrame>
  );
}
