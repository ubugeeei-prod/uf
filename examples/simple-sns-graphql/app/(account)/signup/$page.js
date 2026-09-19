// @flow
import * as React from "@uniflowed/react";
import type { SearchParams } from "@uniflowed/router";
import { relay } from "../../_server/relay.server.js";
import { RelayRoot } from "../../_shared/relay-root.client.js";
import query from "../__generated__/SnsAccountPageQuery.graphql.js";
import { Screen } from "../screen.client.js";

export const dynamic = "force-dynamic";
export component Page(searchParams: SearchParams) {
  return (
    <RelayRoot>
      <Screen queryRef={relay.serverPreloadQuery(query, {})} mode="signup" />
    </RelayRoot>
  );
}
