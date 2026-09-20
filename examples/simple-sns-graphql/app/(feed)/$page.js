// @flow

import * as React from "@uniflowed/react";

import type { SearchParams } from "@uniflowed/router";

import { relay } from "../_server/relay.server.js";
import { RelayRoot } from "../_shared/relay-root.client.js";
import query from "./__generated__/SnsFeedPageQuery.graphql.js";
import { Screen } from "./screen.client.js";
import { feedFilter } from "../_shared/social-model.js";

export const dynamic = "force-dynamic";

export component Page(searchParams: SearchParams) {
  const filter = feedFilter(
    String(searchParams.topic ?? "all"),
    String(searchParams.q ?? ""),
    String(searchParams.page ?? "1"),
  );

  return (
    <RelayRoot>
      <Screen
        queryRef={relay.serverPreloadQuery(query, {
          topic: filter.topic,
          search: filter.query,
          page: filter.page,
        })}
        filter={filter}
      />
    </RelayRoot>
  );
}
