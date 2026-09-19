"use client";
// @flow
import * as React from "@uniflowed/react";
import { Suspense, useState } from "@uniflowed/react";
import { RelayEnvironmentProvider } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";
import type {
  SnsScreenQuery$variables,
  SnsScreenQuery$data,
} from "./__generated__/SnsScreenQuery.graphql.js";
import query from "./__generated__/SnsScreenQuery.graphql.js";
import { environment } from "./relay-environment.js";
import { SocialFrame } from "./social-frame.js";
import { Timeline } from "./timeline.client.js";
import { AccountForm, SettingsForm, Inbox } from "./forms.client.js";
import { Clips } from "./clips/clips-client.js";
import { CLIPS } from "./clips/clip-model.js";
import { LoadingState } from "./ui.js";
import type { FeedFilter, Session, View } from "./social-model.js";

type QueryRef = PreloadedQueryRef<SnsScreenQuery$variables, SnsScreenQuery$data>;

/** One environment per mounted tree, including during concurrent SSR renders. */
export component Screen(view: View, filter: FeedFilter, queryRef: QueryRef) {
  const [client] = useState(() => environment("/graphql"));
  return (
    <RelayEnvironmentProvider environment={client}>
      <Suspense fallback={<LoadingState kind="feed" />}>
        <Content view={view} filter={filter} queryRef={queryRef} />
      </Suspense>
    </RelayEnvironmentProvider>
  );
}

component Content(view: View, filter: FeedFilter, queryRef: QueryRef) {
  const data = useQueryFromServer(query, queryRef);
  const session: Session =
    data.viewer == null ? { kind: "guest" } : { kind: "authenticated", user: data.viewer };
  return (
    <SocialFrame active={view} session={session} aside={view === "timeline"}>
      {view === "timeline" ? <Timeline data={data} filter={filter} /> : null}
      {view === "messages" ? <Inbox data={data} /> : null}
      {view === "settings" ? <SettingsForm settings={data.settings ?? null} /> : null}
      {view === "login" || view === "signup" ? <AccountForm register={view === "signup"} /> : null}
      {view === "clips" ? <Clips clips={CLIPS} /> : null}
    </SocialFrame>
  );
}
