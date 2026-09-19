// @flow
import * as React from "@uniflowed/react";
import { relay } from "./relay.server.js";
import query from "./__generated__/SnsScreenQuery.graphql.js";
import { Screen } from "./screen.client.js";
import { feedFilter, type View } from "./social-model.js";

/** Begin the upstream read in the RSC render, then stream its promise to Relay. */
export function renderScreen(view: View, search: { readonly [string]: mixed }): React.Node {
  const filter = feedFilter(
    String(search.topic ?? "all"),
    String(search.q ?? ""),
    String(search.page ?? "1"),
  );
  const variables = {
    topic: filter.topic,
    search: filter.query,
    page: filter.page,
    thread: String(search.thread ?? "thread-mika"),
    feed: view === "timeline",
    messages: view === "messages",
    settings: view === "settings",
  };
  return (
    <Screen view={view} filter={filter} queryRef={relay.serverPreloadQuery(query, variables)} />
  );
}
