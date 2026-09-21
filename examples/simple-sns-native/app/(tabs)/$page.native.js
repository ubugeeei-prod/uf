// @flow

import { useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { TopicFilter } from "../_shared/social.js";

import { styles } from "../_shared/commonplace.stylex.js";
import { useSocial } from "../_shared/social.js";
import { ActionLink, EmptyState, PageHeading, Screen, Topbar } from "../_shared/ui.native.js";
import { ChannelTabs } from "./channel-tabs.native.js";
import { Composer } from "./composer.native.js";
import { PostCard } from "./post-card.native.js";
import { SearchNotes } from "./search.native.js";

/**
 * The web feed's URL state — channel and search — is this screen's own state here: a tab keeps
 * its place while the person visits the others, which is what the URL did for the web page.
 */

export component Page() {
  const { viewer, feed } = useSocial();
  const [topic, setTopic] = useState<TopicFilter>("all");
  const [search, setSearch] = useState("");
  const posts = feed(topic, search);

  return (
    <Screen>
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={stylex.props(styles.content).style}
      >
        <Topbar section="Feed" />
        <PageHeading title="Feed">Notes from the people in your community.</PageHeading>
        <ChannelTabs topic={topic} onChange={setTopic} />
        <SearchNotes key={search} search={search} onSearch={setSearch} />
        {search !== "" ? (
          <Text {...stylex.props(local.results)}>Results for “{search}”</Text>
        ) : null}
        {viewer != null ? (
          <Composer viewer={viewer} topic={topic} />
        ) : (
          <View {...stylex.props(styles.spread, styles.rule, local.invitation)}>
            <View {...stylex.props(styles.grow)}>
              <Text {...stylex.props(styles.sectionTitle)}>What are you working on?</Text>
              <Text {...stylex.props(styles.hint)}>
                Sign in to post an update or ask a question.
              </Text>
            </View>
            <ActionLink href={route("/join")}>Create account</ActionLink>
          </View>
        )}
        {posts.length > 0 ? (
          <View accessibilityLabel="Timeline posts">
            {posts.map((post) => (
              <PostCard key={post.id} post={post} />
            ))}
          </View>
        ) : (
          <EmptyState title="No notes here yet">Try another channel or search.</EmptyState>
        )}
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  results: { fontSize: 12, color: "#707070", paddingTop: 14 },
  invitation: { paddingTop: 22, paddingBottom: 22 },
});
