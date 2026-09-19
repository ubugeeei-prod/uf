"use client";
// @flow
import * as React from "@uniflowed/react";
import { Suspense, useState } from "@uniflowed/react";
import { graphql, useFragment, RelayEnvironmentProvider } from "@uniflowed/relay";
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
import { AccountForm } from "./account.client.js";
import { SettingsForm } from "./settings.client.js";
import { Inbox } from "./inbox.client.js";
import { Clips } from "./clips/clips-client.js";
import { CLIPS } from "./clips/clip-model.js";
import { LoadingState } from "./ui.js";
import type { FeedFilter, View } from "./social-model.js";

const screenFragment = graphql`
  fragment SnsScreen_query on Query
  @argumentDefinitions(
    topic: { type: "String!" }
    search: { type: "String!" }
    page: { type: "Int!" }
    thread: { type: "ID!" }
    feed: { type: "Boolean!" }
    messages: { type: "Boolean!" }
    settings: { type: "Boolean!" }
  ) {
    ...SnsSocialFrame_query
    ...SnsTimeline_query
      @arguments(topic: $topic, search: $search, page: $page)
      @include(if: $feed)
      @alias(as: "timeline")
    ...SnsInbox_query @arguments(thread: $thread) @include(if: $messages) @alias(as: "inbox")
    ...SnsSettings_query @include(if: $settings) @alias(as: "settings")
  }
`;

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
  const root = useQueryFromServer(query, queryRef);
  const data = useFragment(screenFragment, root);
  return (
    <SocialFrame active={view} queryRef={data} aside={view === "timeline"}>
      {view === "timeline" && data.timeline ? (
        <Timeline queryRef={data.timeline} filter={filter} />
      ) : null}
      {view === "messages" && data.inbox ? <Inbox queryRef={data.inbox} /> : null}
      {view === "settings" && data.settings ? <SettingsForm queryRef={data.settings} /> : null}
      {view === "login" || view === "signup" ? <AccountForm register={view === "signup"} /> : null}
      {view === "clips" ? <Clips clips={CLIPS} /> : null}
    </SocialFrame>
  );
}
