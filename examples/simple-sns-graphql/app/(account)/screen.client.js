"use client";
// @flow
import * as React from "@uniflowed/react";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";
import type {
  SnsAccountPageQuery$variables,
  SnsAccountPageQuery$data,
} from "./__generated__/SnsAccountPageQuery.graphql.js";
import { SocialFrame } from "../_shared/social-frame.js";
import { AccountForm } from "./account.client.js";
const query = graphql`
  query SnsAccountPageQuery {
    ...SnsSocialFrame_query
  }
`;
export component Screen(
  queryRef: PreloadedQueryRef<SnsAccountPageQuery$variables, SnsAccountPageQuery$data>,
  mode: "login" | "signup",
) {
  const data = useQueryFromServer(query, queryRef);
  return (
    <SocialFrame active={mode} queryRef={data} aside={false}>
      <AccountForm register={mode === "signup"} />
    </SocialFrame>
  );
}
