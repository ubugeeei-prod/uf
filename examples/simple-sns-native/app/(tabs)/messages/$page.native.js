// @flow

import { ScrollView, Text, View } from "react-native";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../../router";

import { styles } from "../../_shared/commonplace.stylex.js";
import { useSocial } from "../../_shared/social.js";
import { Avatar, PageHeading, Screen, SignInPrompt, Topbar } from "../../_shared/ui.native.js";

/**
 * The web inbox shows the thread list beside the open conversation. A phone has room for one,
 * so the list is this tab and a conversation is pushed onto the stack above it.
 */

export component Page() {
  const { viewer, threads } = useSocial();

  return (
    <Screen>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <Topbar section="Inbox" />
        <PageHeading title="Inbox">Your conversations, one at a time.</PageHeading>
        {viewer == null ? (
          <SignInPrompt title="Sign in to read your messages" />
        ) : (
          <View accessibilityLabel="Conversations" {...stylex.props(local.list)}>
            {threads.map((thread) => (
              <Link
                key={thread.id}
                href={route("/messages/:thread", { thread: thread.id })}
                accessibilityLabel={`Conversation with ${thread.participant.name}`}
              >
                <View {...stylex.props(styles.rule, local.thread)}>
                  <Avatar user={thread.participant} small />
                  <View {...stylex.props(styles.grow)}>
                    <Text {...stylex.props(local.name)}>{thread.participant.name}</Text>
                    <Text numberOfLines={1} {...stylex.props(local.preview)}>
                      {thread.lastMessage}
                    </Text>
                  </View>
                  <Text {...stylex.props(local.chevron)}>→</Text>
                </View>
              </Link>
            ))}
          </View>
        )}
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  list: { marginTop: 18 },
  thread: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    paddingTop: 16,
    paddingBottom: 16,
  },
  name: { fontSize: 13, fontWeight: "600", color: "#242424" },
  preview: { fontSize: 12, lineHeight: 18, color: "#707070", marginTop: 2 },
  chevron: { fontSize: 14, color: "#868686" },
});
