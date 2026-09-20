"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsMessagesPageQuery$variables,
  SnsMessagesPageQuery$data,
} from "./__generated__/SnsMessagesPageQuery.graphql.js";

import { SocialFrame } from "../_shared/social-frame.js";
import { Inbox } from "./inbox.client.js";

const query = graphql`
  query SnsMessagesPageQuery($thread: ID!) {
    ...SnsSocialFrame_query
    ...SnsInbox_query @arguments(thread: $thread)
  }
`;

export component Screen(
  queryRef: PreloadedQueryRef<SnsMessagesPageQuery$variables, SnsMessagesPageQuery$data>,
) {
  const data = useQueryFromServer(query, queryRef);

  return (
    <SocialFrame active={"messages"} queryRef={data} aside={false}>
      <Inbox queryRef={data} />
    </SocialFrame>
  );
}
