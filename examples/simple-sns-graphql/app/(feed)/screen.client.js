"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsFeedPageQuery$variables,
  SnsFeedPageQuery$data,
} from "./__generated__/SnsFeedPageQuery.graphql.js";

import { SocialFrame } from "../_shared/social-frame.js";
import { Timeline } from "./timeline.client.js";

import type { FeedFilter } from "../_shared/social-model.js";

const query = graphql`
  query SnsFeedPageQuery($topic: String!, $search: String!, $page: Int!) {
    ...SnsSocialFrame_query
    ...SnsTimeline_query @arguments(topic: $topic, search: $search, page: $page)
  }
`;

export component Screen(
  queryRef: PreloadedQueryRef<SnsFeedPageQuery$variables, SnsFeedPageQuery$data>,
  filter: FeedFilter,
) {
  const data = useQueryFromServer(query, queryRef);

  return (
    <SocialFrame active={"timeline"} queryRef={data} aside={true}>
      <Timeline queryRef={data} filter={filter} />
    </SocialFrame>
  );
}
