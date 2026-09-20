"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsClipsPageQuery$variables,
  SnsClipsPageQuery$data,
} from "./__generated__/SnsClipsPageQuery.graphql.js";

import { SocialFrame } from "../_shared/social-frame.js";
import { Clips } from "./clips.client.js";
import { CLIPS } from "./clip-model.js";

const query = graphql`
  query SnsClipsPageQuery {
    ...SnsSocialFrame_query
  }
`;

export component Screen(
  queryRef: PreloadedQueryRef<SnsClipsPageQuery$variables, SnsClipsPageQuery$data>,
) {
  const data = useQueryFromServer(query, queryRef);

  return (
    <SocialFrame active={"clips"} queryRef={data} aside={false}>
      <Clips clips={CLIPS} />
    </SocialFrame>
  );
}
