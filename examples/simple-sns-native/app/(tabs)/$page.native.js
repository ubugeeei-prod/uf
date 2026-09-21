// @flow

import { useOptimistic, useState, useTransition } from "react";
import { ScrollView, Text } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { FeedFilter } from "../_shared/social.js";

import { AsyncRegion } from "../_shared/async-region.native.js";
import { useResource } from "../_shared/client.js";
import { styles } from "../_shared/commonplace.stylex.js";
import { LoadingState, PageHeading, Screen, Topbar } from "../_shared/ui.native.js";
import { ChannelTabs } from "./channel-tabs.native.js";
import { SearchNotes } from "./search.native.js";
import { Timeline } from "./timeline.native.js";

/**
 * The web feed's URL state — channel and search — is this screen's own. Changing either is a
 * transition: the notes already shown stay, dimmed, until the ones asked for have arrived, and
 * only the first load of the screen shows its skeleton. The tab itself moves at once, because
 * what the person pressed is not something to wait for.
 */

export component Page() {
  const [filter, setFilter] = useState<FeedFilter>({ topic: "all", search: "" });
  const [shown, show] = useOptimistic<FeedFilter, FeedFilter>(filter, (_, next) => next);
  const [switching, startSwitch] = useTransition();
  const { resource, retry } = useResource(`feed:${filter.topic}:${filter.search}`, (service) =>
    // The composer needs the viewer and the list needs the notes; both reads start together.
    Promise.all([service.session(), service.feed(filter)]).then(([session, posts]) => ({
      session,
      posts,
    })),
  );
  const change = (next: FeedFilter) =>
    startSwitch(() => {
      show(next);
      setFilter(next);
    });

  return (
    <Screen>
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={stylex.props(styles.content).style}
      >
        <Topbar section="Feed" />
        <PageHeading title="Feed">Notes from the people in your community.</PageHeading>
        <ChannelTabs topic={shown.topic} onChange={(topic) => change({ ...filter, topic })} />
        <SearchNotes
          key={filter.search}
          search={filter.search}
          onSearch={(search) => change({ ...filter, search })}
        />
        {filter.search !== "" ? (
          <Text {...stylex.props(local.results)}>Results for “{filter.search}”</Text>
        ) : null}
        <AsyncRegion
          resource={resource}
          label="notes"
          retry={retry}
          pending={<LoadingState kind="feed" />}
        >
          {({ session, posts }) => (
            <Timeline session={session} posts={posts} topic={filter.topic} stale={switching} />
          )}
        </AsyncRegion>
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  results: { fontSize: 12, color: "#707070", paddingTop: 14 },
});
