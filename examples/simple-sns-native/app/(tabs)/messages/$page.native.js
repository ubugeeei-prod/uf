// @flow

import { ScrollView, Text, View } from "react-native";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../../router";

import type { Thread } from "../../_shared/social.js";

import { AsyncRegion } from "../../_shared/async-region.native.js";
import { useResource } from "../../_shared/client.js";
import { styles } from "../../_shared/commonplace.stylex.js";
import {
  Avatar,
  LoadingState,
  PageHeading,
  Screen,
  SignInPrompt,
  Topbar,
} from "../../_shared/ui.native.js";

component ThreadLink(thread: Thread) renders Link {
  return (
    <Link
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
  );
}

component ThreadList(children: renders* ThreadLink) {
  return (
    <View accessibilityLabel="Conversations" {...stylex.props(local.list)}>
      {children}
    </View>
  );
}

/**
 * The web inbox shows the thread list beside the open conversation. A phone has room for one,
 * so the list is this tab and a conversation is pushed onto the stack above it.
 */

export component Page() {
  const { resource, retry } = useResource("threads", (service) => service.threads());

  return (
    <Screen>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <Topbar section="Inbox" />
        <PageHeading title="Inbox">Your conversations, one at a time.</PageHeading>
        <AsyncRegion
          resource={resource}
          label="conversations"
          retry={retry}
          pending={<LoadingState kind="threads" />}
        >
          {(threads) =>
            match (threads) {
              {kind: "unauthenticated"} => <SignInPrompt title="Sign in to read your messages" />,
              {kind: "ready", value: const value} =>
                <ThreadList>
                  {value.map((thread) => (
                    <ThreadLink key={thread.id} thread={thread} />
                  ))}
                </ThreadList>,
            }}
        </AsyncRegion>
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
