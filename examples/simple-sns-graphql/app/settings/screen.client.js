"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsSettingsPageQuery$variables,
  SnsSettingsPageQuery$data,
} from "./__generated__/SnsSettingsPageQuery.graphql.js";

import { SocialFrame } from "../_shared/social-frame.js";
import { SettingsForm } from "./settings.client.js";

const query = graphql`
  query SnsSettingsPageQuery {
    ...SnsSocialFrame_query
    ...SnsSettings_query
  }
`;

export component Screen(
  queryRef: PreloadedQueryRef<SnsSettingsPageQuery$variables, SnsSettingsPageQuery$data>,
) {
  const data = useQueryFromServer(query, queryRef);

  return (
    <SocialFrame active={"settings"} queryRef={data} aside={false}>
      <SettingsForm queryRef={data} />
    </SocialFrame>
  );
}
